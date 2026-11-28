//! Every configured server, connected once and shared by all chats.
//!
//! The pool reads the connectors file, connects each enabled server in the
//! background and keeps its status (`connecting`, `connected`, `needs-auth`,
//! `failed`, `off`), its tools and prompts, and the last lines of its log.
//! Chats see a connected server's tools as `mcp__<server>__<tool>`; a
//! tool's permission (`allow` / `ask` / `block`) defaults to `allow` for
//! tools the server marks read-only and `ask` for the rest. Every change
//! calls `on_change`, so the host can tell its UI and its chats.
//!
//! Plugin servers have a lifecycle (docs/features/plugin-settings/spec.md,
//! FR-11 – FR-13): `Always` connects at once; `OnDemand` sleeps until a call
//! needs it — its tools are offered meanwhile from the last list it gave —
//! and goes back to sleep when idle; `Off` never starts. [`Pool::tick`]
//! puts idle servers to sleep and restarts crashed ones, giving up after
//! [`MAX_CRASHES`] crashes in a minute.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::auth;
use crate::client::{self, ConnectError, Connection};
use crate::config::{valid_name, ConfigFile, Permission, ServerConfig};

/// Log lines kept per server.
const LOG_LINES: usize = 200;
/// A tool result's text beyond this many characters is cut.
const MAX_RESULT_CHARS: usize = 100_000;
/// Model APIs limit tool names to 64 characters.
const MAX_TOOL_NAME: usize = 64;
/// Crashes within [`CRASH_WINDOW`] after which a server is left stopped.
pub const MAX_CRASHES: usize = 5;
const CRASH_WINDOW: Duration = Duration::from_secs(60);
/// Why an app server cannot be removed or switched like a connector.
const APP_MANAGED: &str = "this connector is managed by Connected apps: change it there";

/// A server supplied by the app's own code rather than the person's
/// file (connected apps: docs/features/connected-apps/spec.md, FR-7,
/// NFR-1), for [`Pool::set_app_server`]. Its key is `app:<name>`, its tools
/// `mcp__<name>__<tool>`. Its config (URL and headers, which may hold a
/// key) lives only in memory; its tool permissions and last tool list are
/// saved under `appServers.<name>` in the connectors file.
#[derive(Debug, Clone)]
pub struct AppServer {
    pub name: String,
    pub config: ServerConfig,
    /// Permissions for tools the person has not chosen one for (else the
    /// usual default: `allow` for read-only tools, `ask` for the rest).
    pub defaults: BTreeMap<String, Permission>,
}

/// When a plugin server runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lifecycle {
    /// Connected at once and kept running.
    #[default]
    Always,
    /// Started by a call; stopped after `idle` without one.
    OnDemand { idle: Duration },
    /// Never started.
    Off,
}

/// A plugin's server for [`Pool::set_plugin_servers`].
#[derive(Debug, Clone)]
pub struct PluginServer {
    pub plugin: String,
    pub server: String,
    pub config: ServerConfig,
    pub lifecycle: Lifecycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Off,
    Connecting,
    Connected,
    NeedsAuth,
    Failed,
    /// Not running; its last tool list is offered and a call starts it.
    Sleeping,
    /// Stopped after crashing too often; started again by hand.
    Crashed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Off => "off",
            Status::Connecting => "connecting",
            Status::Connected => "connected",
            Status::NeedsAuth => "needs-auth",
            Status::Failed => "failed",
            Status::Sleeping => "sleeping",
            Status::Crashed => "crashed",
        }
    }
}

struct Entry {
    /// The server's name as people and models see it (tool names use it).
    label: String,
    /// The project root whose `.mcp.json` it comes from; `None`: your own.
    project: Option<String>,
    /// The plugin that brought it; `None`: not a plugin's.
    plugin: Option<String>,
    /// The app server it is ([`AppServer::name`]); `None`: not one.
    app: Option<String>,
    config: ServerConfig,
    status: Status,
    error: Option<String>,
    tools: Vec<Value>,
    prompts: Vec<Value>,
    resources: Vec<Value>,
    info: Value,
    conn: Option<Arc<Connection>>,
    log: VecDeque<String>,
    /// Bumped on every (re)connect, so a slow old attempt cannot win.
    generation: u64,
    /// When it runs (plugin servers; others are always connected).
    lifecycle: Lifecycle,
    /// The last call through it.
    last_used: Option<Instant>,
    /// Recent crashes, oldest first.
    crashes: VecDeque<Instant>,
}

impl Entry {
    fn new(label: String, config: ServerConfig, project: Option<String>) -> Self {
        Self {
            label,
            project,
            plugin: None,
            app: None,
            resources: Vec::new(),
            status: if config.disabled {
                Status::Off
            } else {
                Status::Connecting
            },
            config,
            error: None,
            tools: Vec::new(),
            prompts: Vec::new(),
            info: Value::Null,
            conn: None,
            log: VecDeque::new(),
            generation: 0,
            lifecycle: Lifecycle::Always,
            last_used: None,
            crashes: VecDeque::new(),
        }
    }
}

type OnChange = Arc<dyn Fn() + Send + Sync>;
/// Receives plugin servers' log lines: `(plugin, server label, line)`, with
/// their `env` and `headers` values hidden.
pub type LogSink = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;

struct Shared {
    path: PathBuf,
    /// Where stdio servers run when their config names no folder.
    cwd: Option<PathBuf>,
    servers: Mutex<BTreeMap<String, Entry>>,
    on_change: OnChange,
    log_sink: Mutex<Option<LogSink>>,
}

#[derive(Clone)]
pub struct Pool {
    shared: Arc<Shared>,
}

/// Which server and tool a chat's tool name stands for: the server's key,
/// the tool, its connection (none while it sleeps), its permission, and
/// whether a call may start it.
type Resolved = (String, String, Option<Arc<Connection>>, Permission, bool);

/// A plugin server's config and log as the windows may see them: its `env`
/// and `headers` values hidden (they may hold the plugin's secrets, filled
/// in from its vault), and those values hidden in its log lines too
/// (docs/features/plugin-settings/spec.md, NFR-1).
fn hide_plugin_values(config: &ServerConfig, log: Vec<String>) -> (Value, Vec<String>) {
    let mut values: Vec<&String> = config
        .env
        .values()
        .chain(config.headers.values())
        .filter(|v| v.len() >= 4)
        .collect();
    values.sort_by_key(|v| std::cmp::Reverse(v.len()));
    let mut shown = serde_json::to_value(config).unwrap_or(Value::Null);
    for field in ["env", "headers"] {
        if let Some(map) = shown.get_mut(field).and_then(Value::as_object_mut) {
            for value in map.values_mut() {
                *value = Value::String("(hidden)".into());
            }
        }
    }
    let log = log
        .into_iter()
        .map(|line| {
            values.iter().fold(line, |line, secret| {
                line.replace(secret.as_str(), "(hidden)")
            })
        })
        .collect();
    (shown, log)
}

/// `mcp__<server>__<tool>` with characters models accept, at most 64 long.
pub fn exposed_name(server: &str, tool: &str) -> String {
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    };
    let mut name = format!("mcp__{}__{}", clean(server), clean(tool));
    name.truncate(MAX_TOOL_NAME);
    name
}

fn tool_name(tool: &Value) -> &str {
    tool.get("name").and_then(Value::as_str).unwrap_or("")
}

/// `allow` for tools the server marks read-only, else `ask`.
fn default_permission(tool: &Value) -> Permission {
    let read_only = tool
        .pointer("/annotations/readOnlyHint")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if read_only {
        Permission::Allow
    } else {
        Permission::Ask
    }
}

fn permission_of(config: &ServerConfig, tool: &Value) -> Permission {
    config
        .tool_permissions
        .get(tool_name(tool))
        .copied()
        .unwrap_or_else(|| default_permission(tool))
}

/// Cut very long text parts of a tool result, with a note.
fn trim_result(mut result: Value) -> Value {
    if let Some(parts) = result.get_mut("content").and_then(Value::as_array_mut) {
        for part in parts {
            if let Some(text) = part.get_mut("text") {
                if let Some(s) = text.as_str() {
                    if s.chars().count() > MAX_RESULT_CHARS {
                        let cut: String = s.chars().take(MAX_RESULT_CHARS).collect();
                        *text = Value::String(format!("{cut}\n\n[cut: the tool returned more than {MAX_RESULT_CHARS} characters]"));
                    }
                }
            }
        }
    }
    result
}

/// The folder whose `.mcp.json` applies to `cwd`: the nearest one from
/// `cwd` up to the repository root.
pub fn project_root(cwd: &Path) -> Option<PathBuf> {
    for dir in cwd.ancestors() {
        if dir.join(".mcp.json").is_file() {
            return Some(dir.to_path_buf());
        }
        if dir.join(".git").exists() {
            return None;
        }
    }
    None
}

/// A project root as a stable key (forward slashes; lower case on Windows).
fn root_key(root: &Path) -> String {
    let text = root.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// Your own connectors first, then the servers of `root`'s project whose
/// names your own do not take.
fn ordered<'a>(
    servers: &'a BTreeMap<String, Entry>,
    root: Option<&str>,
) -> Vec<(&'a String, &'a Entry)> {
    let mut list: Vec<(&String, &Entry)> = servers
        .iter()
        .filter(|(_, e)| e.project.is_none())
        .collect();
    for (key, entry) in servers.iter() {
        let Some(project) = entry.project.as_deref() else {
            continue;
        };
        if Some(project) == root && !list.iter().any(|(_, e)| e.label == entry.label) {
            list.push((key, entry));
        }
    }
    list
}

/// The pool key of a project server.
fn project_entry_key(label: &str, root: &str) -> String {
    format!("{label}#{root}")
}

impl Pool {
    pub fn new(
        path: PathBuf,
        cwd: Option<PathBuf>,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                path,
                cwd,
                servers: Mutex::new(BTreeMap::new()),
                on_change: Arc::new(on_change),
                log_sink: Mutex::new(None),
            }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.shared.path
    }

    /// Where sign-in tokens live: `mcp-auth.json` next to the connectors file.
    fn auth_path(&self) -> PathBuf {
        self.shared.path.with_file_name("mcp-auth.json")
    }

    fn changed(&self) {
        (self.shared.on_change)();
    }

    /// Read the file and connect every enabled server (in the background).
    pub fn start(&self) -> Result<(), String> {
        let file = ConfigFile::load(&self.shared.path)?;
        let names: Vec<String> = {
            let mut servers = self.shared.servers.lock().unwrap();
            // The app server is the app's, not the file's: it stays.
            servers.retain(|_, e| e.app.is_some());
            for (name, config) in file.servers {
                servers.insert(name.clone(), Entry::new(name, config, None));
            }
            servers
                .iter()
                .filter(|(_, e)| !e.config.disabled)
                .map(|(n, _)| n.clone())
                .collect()
        };
        self.changed();
        for name in names {
            self.spawn_connect(name);
        }
        Ok(())
    }

    fn spawn_connect(&self, name: String) {
        let pool = self.clone();
        tokio::spawn(async move { pool.connect(&name).await });
    }

    /// (Re)connect `name` now; returns once it is connected or has failed.
    pub async fn connect(&self, name: &str) {
        let (transport, generation, old, disabled) = {
            let mut servers = self.shared.servers.lock().unwrap();
            let Some(entry) = servers.get_mut(name) else {
                return;
            };
            entry.generation += 1;
            let old = entry.conn.take();
            let disabled = entry.config.disabled;
            entry.error = None;
            if disabled {
                entry.status = Status::Off;
                entry.tools.clear();
                entry.prompts.clear();
            } else {
                entry.status = Status::Connecting;
            }
            (entry.config.transport(), entry.generation, old, disabled)
        };
        self.changed();
        if let Some(old) = old.and_then(|c| Arc::try_unwrap(c).ok()) {
            old.close().await;
        }
        if disabled {
            return;
        }
        let transport = match transport {
            Ok(transport) => transport,
            Err(error) => return self.finish(name, generation, Err(ConnectError::Failed(error))),
        };
        let log_pool = self.clone();
        let log_name = name.to_string();
        let log = Arc::new(move |line: String| log_pool.log(&log_name, line));
        // A URL server someone signed in to gets its token (refreshed as needed).
        let auth = match &transport {
            crate::config::Transport::Http { url, .. }
            | crate::config::Transport::Sse { url, .. } => {
                match auth::authorized_client(url, &self.auth_path(), name).await {
                    Ok(client) => client,
                    Err(error) => {
                        self.log(name, format!("[evo] saved sign-in unusable: {error}"));
                        None
                    }
                }
            }
            _ => None,
        };
        let result = client::connect(&transport, self.shared.cwd.as_deref(), log, auth).await;
        let result = match result {
            Ok(conn) => {
                let tools = conn.tools().await;
                let prompts = conn.prompts().await.unwrap_or_default();
                let resources = conn.resources().await.unwrap_or_default();
                match tools {
                    Ok(tools) => Ok((conn, tools, prompts, resources)),
                    Err(error) => Err(ConnectError::Failed(format!("tools/list failed: {error}"))),
                }
            }
            Err(error) => Err(error),
        };
        self.finish(name, generation, result);
    }

    fn finish(
        &self,
        name: &str,
        generation: u64,
        result: Result<(Connection, Vec<Value>, Vec<Value>, Vec<Value>), ConnectError>,
    ) {
        let mut cache = None;
        let mut app_cache = None;
        {
            let mut servers = self.shared.servers.lock().unwrap();
            let Some(entry) = servers.get_mut(name) else {
                return;
            };
            if entry.generation != generation {
                return;
            }
            match result {
                Ok((conn, tools, prompts, resources)) => {
                    entry.status = Status::Connected;
                    entry.error = None;
                    entry.info = conn.info.clone();
                    entry.tools = tools;
                    entry.prompts = prompts;
                    entry.resources = resources;
                    entry.conn = Some(Arc::new(conn));
                    entry.last_used = Some(Instant::now());
                    if let Some(plugin) = entry.plugin.clone() {
                        cache = Some((plugin, entry.label.clone(), entry.tools.clone()));
                    }
                    if let Some(app) = entry.app.clone() {
                        app_cache = Some((app, entry.tools.clone()));
                    }
                }
                Err(ConnectError::NeedsAuth(error)) => {
                    entry.status = Status::NeedsAuth;
                    entry.error = Some(error);
                }
                Err(ConnectError::Failed(error)) => {
                    entry.status = Status::Failed;
                    entry.error = Some(error);
                }
            }
        }
        // A plugin server's tools are kept, to offer them while it sleeps.
        if let Some((plugin, label, tools)) = cache {
            let server = label
                .strip_prefix(&format!("plugin_{plugin}_"))
                .unwrap_or(&label)
                .to_string();
            let _ = self.save(|file| {
                let all = file
                    .extra
                    .entry("pluginServers")
                    .or_insert_with(|| json!({}));
                if !all.is_object() {
                    *all = json!({});
                }
                let entry = all
                    .as_object_mut()
                    .unwrap()
                    .entry(plugin)
                    .or_insert_with(|| json!({}));
                if !entry.is_object() {
                    *entry = json!({});
                }
                let server = entry
                    .as_object_mut()
                    .unwrap()
                    .entry(server)
                    .or_insert_with(|| json!({}));
                if !server.is_object() {
                    *server = json!({});
                }
                server["tools"] = Value::Array(tools);
            });
        }
        if let Some((app, tools)) = app_cache {
            let _ = self.save_app(&app, |entry| entry["tools"] = Value::Array(tools));
        }
        self.changed();
    }

    fn log(&self, name: &str, line: String) {
        let mut sink = None;
        {
            let mut servers = self.shared.servers.lock().unwrap();
            if let Some(entry) = servers.get_mut(name) {
                if let Some(plugin) = &entry.plugin {
                    let (_, hidden) = hide_plugin_values(&entry.config, vec![line.clone()]);
                    sink = Some((
                        plugin.clone(),
                        entry.label.clone(),
                        hidden.into_iter().next().unwrap_or_default(),
                    ));
                }
                if entry.log.len() >= LOG_LINES {
                    entry.log.pop_front();
                }
                entry.log.push_back(line);
            }
        }
        if let Some((plugin, label, line)) = sink {
            if let Some(sink) = self.shared.log_sink.lock().unwrap().clone() {
                sink(&plugin, &label, &line);
            }
        }
    }

    /// Where plugin servers' log lines go besides the pool (the app's log).
    pub fn set_log_sink(&self, sink: LogSink) {
        *self.shared.log_sink.lock().unwrap() = Some(sink);
    }

    /// The keys of `plugin`'s servers.
    pub fn plugin_keys(&self, plugin: &str) -> Vec<String> {
        let servers = self.shared.servers.lock().unwrap();
        servers
            .iter()
            .filter(|(_, e)| e.plugin.as_deref() == Some(plugin))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Start `key` if it is not running (a sleeping, stopped or crashed
    /// plugin server); the connection, or why it could not start.
    pub async fn wake(&self, key: &str) -> Result<Arc<Connection>, String> {
        let running = {
            let mut servers = self.shared.servers.lock().unwrap();
            let entry = servers
                .get_mut(key)
                .ok_or_else(|| format!("`{key}` is not a server"))?;
            if entry.lifecycle == Lifecycle::Off {
                return Err(format!("{} is set not to run", entry.label));
            }
            entry.last_used = Some(Instant::now());
            entry.conn.clone().filter(|c| !c.is_closed())
        };
        if let Some(conn) = running {
            return Ok(conn);
        }
        self.connect(key).await;
        let servers = self.shared.servers.lock().unwrap();
        let entry = servers
            .get(key)
            .ok_or_else(|| format!("`{key}` is not a server"))?;
        entry.conn.clone().ok_or_else(|| {
            format!(
                "{} could not start: {}",
                entry.label,
                entry
                    .error
                    .clone()
                    .unwrap_or_else(|| entry.status.as_str().into())
            )
        })
    }

    /// Stop `key` and let it sleep: its tools stay offered, the next call
    /// starts it again.
    pub async fn sleep(&self, key: &str) {
        let old = {
            let mut servers = self.shared.servers.lock().unwrap();
            let Some(entry) = servers.get_mut(key) else {
                return;
            };
            entry.generation += 1;
            entry.status = if entry.tools.is_empty() {
                Status::Off
            } else {
                Status::Sleeping
            };
            entry.error = None;
            entry.conn.take()
        };
        self.changed();
        if let Some(old) = old.and_then(|c| Arc::try_unwrap(c).ok()) {
            old.close().await;
        }
    }

    /// Call `tool` of server `key` itself, starting it when it sleeps (a
    /// plugin's own view calling its backend: plugin-settings FR-14).
    pub async fn call_server(
        &self,
        key: &str,
        tool: &str,
        arguments: Option<Map<String, Value>>,
    ) -> Result<Value, String> {
        {
            let servers = self.shared.servers.lock().unwrap();
            let entry = servers
                .get(key)
                .ok_or_else(|| format!("`{key}` is not a server"))?;
            let found = entry
                .tools
                .iter()
                .find(|t| tool_name(t) == tool)
                .ok_or_else(|| format!("{} has no tool `{tool}`", entry.label))?;
            if permission_of(&entry.config, found) == Permission::Block {
                return Err(format!("`{tool}` is blocked in Customize → Connectors"));
            }
        }
        let conn = self.wake(key).await?;
        conn.call_tool(tool, arguments).await.map(trim_result)
    }

    /// One look at the plugin servers at `now`: those idle past their time
    /// sleep; those whose process ended while connected count a crash and
    /// start again, or stay stopped after [`MAX_CRASHES`] in a minute.
    pub async fn tick(&self, now: Instant) {
        let mut idle = Vec::new();
        let mut restart = Vec::new();
        let mut gave_up = false;
        {
            let mut servers = self.shared.servers.lock().unwrap();
            for (key, entry) in servers
                .iter_mut()
                .filter(|(_, e)| e.plugin.is_some() && e.status == Status::Connected)
            {
                let closed = entry.conn.as_ref().is_none_or(|c| c.is_closed());
                if closed {
                    entry.crashes.push_back(now);
                    while entry
                        .crashes
                        .front()
                        .is_some_and(|at| now.duration_since(*at) > CRASH_WINDOW)
                    {
                        entry.crashes.pop_front();
                    }
                    entry.conn = None;
                    if entry.crashes.len() >= MAX_CRASHES {
                        entry.status = Status::Crashed;
                        entry.error =
                            Some(format!("stopped after {MAX_CRASHES} crashes in a minute"));
                        gave_up = true;
                    } else {
                        restart.push(key.clone());
                    }
                    continue;
                }
                if let Lifecycle::OnDemand { idle: limit } = entry.lifecycle {
                    if entry
                        .last_used
                        .is_none_or(|used| now.duration_since(used) >= limit)
                    {
                        idle.push(key.clone());
                    }
                }
            }
        }
        if gave_up {
            self.changed();
        }
        for key in idle {
            self.sleep(&key).await;
        }
        for key in restart {
            self.spawn_connect(key);
        }
    }

    /// Run [`Pool::tick`] every `every`, for as long as the pool lives.
    pub fn start_lifecycles(&self, every: Duration) {
        let pool = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                pool.tick(Instant::now()).await;
            }
        });
    }

    fn save(&self, change: impl FnOnce(&mut ConfigFile)) -> Result<(), String> {
        let mut file = ConfigFile::load(&self.shared.path)?;
        change(&mut file);
        file.save(&self.shared.path)
    }

    /// Add or replace a server (renaming it when `previous` differs) and connect it.
    pub async fn upsert(
        &self,
        name: &str,
        config: ServerConfig,
        previous: Option<&str>,
    ) -> Result<(), String> {
        if !valid_name(name) {
            return Err("the name may use letters, digits, - and _ (at most 48)".into());
        }
        config.transport()?;
        let renamed = previous.filter(|p| *p != name);
        {
            let servers = self.shared.servers.lock().unwrap();
            let clash = servers.contains_key(name) && previous != Some(name);
            if clash || servers.values().any(|e| e.app.is_some() && e.label == name) {
                return Err(format!("a connector named `{name}` already exists"));
            }
        }
        self.save(|file| {
            if let Some(old) = renamed {
                file.servers.remove(old);
            }
            file.servers.insert(name.to_string(), config.clone());
        })?;
        let old = {
            let mut servers = self.shared.servers.lock().unwrap();
            let old = renamed.and_then(|old| servers.remove(old));
            let entry = servers
                .entry(name.to_string())
                .or_insert_with(|| Entry::new(name.to_string(), config.clone(), None));
            entry.config = config;
            old
        };
        if let Some(conn) = old
            .and_then(|e| e.conn)
            .and_then(|c| Arc::try_unwrap(c).ok())
        {
            conn.close().await;
        }
        self.connect(name).await;
        Ok(())
    }

    pub async fn remove(&self, name: &str) -> Result<(), String> {
        if self.app_of_key(name).is_some() {
            return Err(APP_MANAGED.into());
        }
        if self.plugin_of_key(name).is_some() {
            return Err(
                "this connector comes with a plugin: turn the plugin off or remove it".into(),
            );
        }
        if let Some((label, root)) = self.project_key(name) {
            return self.set_project_approval_at(&root, &label, false).await;
        }
        self.save(|file| {
            file.servers.remove(name);
        })?;
        let _ = auth::sign_out(&self.auth_path(), name);
        let entry = self.shared.servers.lock().unwrap().remove(name);
        self.changed();
        if let Some(conn) = entry
            .and_then(|e| e.conn)
            .and_then(|c| Arc::try_unwrap(c).ok())
        {
            conn.close().await;
        }
        Ok(())
    }

    pub async fn set_enabled(&self, name: &str, on: bool) -> Result<(), String> {
        if self.app_of_key(name).is_some() {
            return Err(APP_MANAGED.into());
        }
        if self.plugin_of_key(name).is_some() {
            return Err("this connector comes with a plugin: turn the plugin off instead".into());
        }
        if let Some((label, root)) = self.project_key(name) {
            return self.set_project_approval_at(&root, &label, on).await;
        }
        self.save(|file| {
            if let Some(server) = file.servers.get_mut(name) {
                server.disabled = !on;
            }
        })?;
        {
            let mut servers = self.shared.servers.lock().unwrap();
            let entry = servers
                .get_mut(name)
                .ok_or_else(|| format!("no connector `{name}`"))?;
            entry.config.disabled = !on;
        }
        self.connect(name).await;
        Ok(())
    }

    /// Sign in to the URL server `name` (`open` shows the provider's page in
    /// the browser), then connect with the new token.
    pub async fn sign_in(&self, name: &str, open: impl FnOnce(String)) -> Result<(), String> {
        let url = {
            let servers = self.shared.servers.lock().unwrap();
            let entry = servers
                .get(name)
                .ok_or_else(|| format!("no connector `{name}`"))?;
            match entry.config.transport()? {
                crate::config::Transport::Http { url, .. }
                | crate::config::Transport::Sse { url, .. } => url,
                _ => return Err("only URL connectors sign in".into()),
            }
        };
        self.log(name, "[evo] signing in…".into());
        let result = auth::sign_in(&url, &self.auth_path(), name, open).await;
        if let Err(error) = &result {
            self.log(name, format!("[evo] sign-in failed: {error}"));
            self.changed();
            return result;
        }
        self.connect(name).await;
        Ok(())
    }

    /// Forget `name`'s sign-in and reconnect without it.
    pub async fn sign_out(&self, name: &str) -> Result<(), String> {
        auth::sign_out(&self.auth_path(), name)?;
        self.connect(name).await;
        Ok(())
    }

    pub fn set_permission(
        &self,
        name: &str,
        tool: &str,
        permission: Permission,
    ) -> Result<(), String> {
        if let Some(app) = self.app_of_key(name) {
            self.save_app(&app, |entry| {
                if entry.get("toolPermissions").is_none_or(|p| !p.is_object()) {
                    entry["toolPermissions"] = json!({});
                }
                entry["toolPermissions"][tool] = json!(permission);
            })?;
        } else if let Some((plugin, server)) = self.plugin_of_key(name) {
            self.save(|file| {
                let all = file
                    .extra
                    .entry("pluginServers")
                    .or_insert_with(|| json!({}));
                if !all.is_object() {
                    *all = json!({});
                }
                let perms = &mut all[&plugin][&server]["toolPermissions"];
                if !perms.is_object() {
                    *perms = json!({});
                }
                perms[tool] = json!(permission);
            })?;
        } else if let Some((label, root)) = self.project_key(name) {
            self.save_approval(&root, &label, |entry| {
                entry["toolPermissions"][tool] = json!(permission);
            })?;
        } else {
            self.save(|file| {
                if let Some(server) = file.servers.get_mut(name) {
                    server.tool_permissions.insert(tool.to_string(), permission);
                }
            })?;
        }
        {
            let mut servers = self.shared.servers.lock().unwrap();
            let entry = servers
                .get_mut(name)
                .ok_or_else(|| format!("no connector `{name}`"))?;
            entry
                .config
                .tool_permissions
                .insert(tool.to_string(), permission);
        }
        self.changed();
        Ok(())
    }

    /// Every server for the Connectors view.
    pub fn snapshot(&self) -> Value {
        let auth_path = self.auth_path();
        let servers = self.shared.servers.lock().unwrap();
        let list: Vec<Value> = servers
            .iter()
            .map(|(key, entry)| {
                let name = &entry.label;
                let log: Vec<String> = entry.log.iter().cloned().collect();
                let shown = if entry.plugin.is_some() || entry.app.is_some() {
                    hide_plugin_values(&entry.config, log)
                } else {
                    (serde_json::to_value(&entry.config).unwrap_or(Value::Null), log)
                };
                let transport = entry.config.transport();
                let kind = match &transport {
                    Ok(crate::config::Transport::Stdio { .. }) => "stdio",
                    Ok(crate::config::Transport::Http { .. }) => "http",
                    Ok(crate::config::Transport::Sse { .. }) => "sse",
                    Err(_) => "invalid",
                };
                let tools: Vec<Value> = entry
                    .tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "name": tool_name(tool),
                            "title": tool.get("title").or_else(|| tool.pointer("/annotations/title")),
                            "description": tool.get("description"),
                            "readOnly": tool.pointer("/annotations/readOnlyHint").and_then(Value::as_bool).unwrap_or(false),
                            "permission": permission_of(&entry.config, tool),
                            "exposedName": exposed_name(name, tool_name(tool)),
                        })
                    })
                    .collect();
                let resources: Vec<Value> = entry
                    .resources
                    .iter()
                    .map(|r| json!({
                        "uri": r.get("uri"),
                        "name": r.get("name"),
                        "description": r.get("description"),
                        "mimeType": r.get("mimeType"),
                    }))
                    .collect();
                json!({
                    "key": key,
                    "name": name,
                    "project": entry.project,
                    "plugin": entry.plugin,
                    "app": entry.app,
                    "resources": resources,
                    "kind": kind,
                    "summary": entry.config.summary(),
                    "config": shown.0,
                    "status": entry.status.as_str(),
                    "signedIn": kind != "stdio" && auth::signed_in(&auth_path, key),
                    "error": entry.error.clone().or_else(|| transport.err()),
                    "server": entry.info.get("serverInfo").cloned().unwrap_or(Value::Null),
                    "instructions": entry.info.get("instructions").cloned().unwrap_or(Value::Null),
                    "tools": tools,
                    "prompts": entry.prompts,
                    "log": shown.1,
                })
            })
            .collect();
        json!({ "servers": list, "path": self.shared.path })
    }

    /// The tools a chat working in `cwd` gets: every connected server's tools
    /// but blocked ones — your own connectors, plus the approved servers of
    /// that project's `.mcp.json` (yours win on a name clash) — as
    /// `{name, description, inputSchema, permission, server, tool}`.
    pub fn chat_tools(&self, cwd: Option<&Path>) -> Vec<Value> {
        let root = cwd.and_then(project_root).map(|r| root_key(&r));
        let servers = self.shared.servers.lock().unwrap();
        let mut out = Vec::new();
        for (key, entry) in ordered(&servers, root.as_deref())
            .into_iter()
            .filter(|(_, e)| e.status == Status::Connected || e.status == Status::Sleeping)
        {
            let name = &entry.label;
            for tool in &entry.tools {
                let permission = permission_of(&entry.config, tool);
                if permission == Permission::Block {
                    continue;
                }
                let description = tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                out.push(json!({
                    "name": exposed_name(name, tool_name(tool)),
                    "description": format!("[{name}] {description}").trim_end().to_string(),
                    "inputSchema": tool.get("inputSchema").cloned().unwrap_or_else(|| json!({"type": "object"})),
                    "permission": permission,
                    "server": key,
                    "tool": tool_name(tool),
                    // The server marks it as only reading (a read-only chat
                    // gets no other: docs/features/session-roles-and-mail/spec.md, FR-13).
                    "readOnly": tool.pointer("/annotations/readOnlyHint").and_then(Value::as_bool).unwrap_or(false),
                }));
            }
        }
        out
    }

    /// Which server and tool a chat's tool name stands for.
    fn resolve(&self, exposed: &str, cwd: Option<&Path>) -> Option<Resolved> {
        let root = cwd.and_then(project_root).map(|r| root_key(&r));
        let servers = self.shared.servers.lock().unwrap();
        for (key, entry) in ordered(&servers, root.as_deref()) {
            for tool in &entry.tools {
                if exposed_name(&entry.label, tool_name(tool)) == exposed {
                    // A plugin server may sleep: the call starts it.
                    let wakes = entry.plugin.is_some() && entry.lifecycle != Lifecycle::Off;
                    if entry.conn.is_none() && !wakes {
                        return None;
                    }
                    return Some((
                        key.clone(),
                        tool_name(tool).to_string(),
                        entry.conn.clone(),
                        permission_of(&entry.config, tool),
                        wakes,
                    ));
                }
            }
        }
        None
    }

    /// Call a tool by the name chats see. A blocked tool is refused; asking
    /// the person first is the host's job (see `chat_tools`' `permission`).
    pub async fn call(
        &self,
        exposed: &str,
        arguments: Option<Map<String, Value>>,
        cwd: Option<&Path>,
    ) -> Result<Value, String> {
        let (server, tool, conn, permission, wakes) = self
            .resolve(exposed, cwd)
            .ok_or_else(|| format!("`{exposed}` is not available (is its connector connected?)"))?;
        if permission == Permission::Block {
            return Err(format!(
                "`{tool}` of {server} is blocked in Customize → Connectors"
            ));
        }
        let conn = match conn {
            Some(conn) if !conn.is_closed() => {
                if wakes {
                    if let Some(entry) = self.shared.servers.lock().unwrap().get_mut(&server) {
                        entry.last_used = Some(Instant::now());
                    }
                }
                conn
            }
            _ if wakes => self.wake(&server).await?,
            _ => {
                self.spawn_connect(server.clone());
                return Err(format!(
                    "{server} disconnected; reconnecting — try again in a moment"
                ));
            }
        };
        conn.call_tool(&tool, arguments).await.map(trim_result)
    }

    /// A prompt's messages as one text (for the composer's `/server:prompt`).
    pub async fn get_prompt(
        &self,
        server: &str,
        prompt: &str,
        arguments: Map<String, Value>,
    ) -> Result<String, String> {
        let conn = {
            let servers = self.shared.servers.lock().unwrap();
            servers
                .get(server)
                .or_else(|| {
                    servers
                        .values()
                        .find(|e| e.label == server && e.project.is_none())
                })
                .and_then(|e| e.conn.clone())
                .ok_or_else(|| format!("{server} is not connected"))?
        };
        let result = conn.get_prompt(prompt, arguments).await?;
        let texts: Vec<String> = result
            .get("messages")
            .and_then(Value::as_array)
            .map(|messages| {
                messages
                    .iter()
                    .filter_map(|m| {
                        let content = m.get("content")?;
                        let parts = match content {
                            Value::Array(parts) => parts.clone(),
                            other => vec![other.clone()],
                        };
                        let text: Vec<String> = parts
                            .iter()
                            .filter_map(|p| {
                                p.get("text")
                                    .and_then(Value::as_str)
                                    .map(str::to_string)
                                    .or_else(|| {
                                        p.pointer("/resource/text")
                                            .and_then(Value::as_str)
                                            .map(str::to_string)
                                    })
                            })
                            .collect();
                        (!text.is_empty()).then(|| text.join("\n"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(texts.join("\n\n"))
    }

    /// `(plugin, server)` when `key` names a plugin's server.
    fn plugin_of_key(&self, key: &str) -> Option<(String, String)> {
        let rest = key.strip_prefix("plugin:")?;
        let (plugin, server) = rest.split_once(':')?;
        Some((plugin.to_string(), server.to_string()))
    }

    /// Make the pool's plugin servers exactly `servers` (those of the
    /// plugins that are on), with their lifecycles: new or changed ones start
    /// (or sleep, offering the tools they gave last time), gone ones close
    /// (plugin-settings FR-11). Their tools reach every chat as
    /// `mcp__plugin_<plugin>_<server>__<tool>`.
    pub async fn set_plugin_servers(&self, servers: Vec<PluginServer>) {
        let saved: Map<String, Value> = ConfigFile::load(&self.shared.path)
            .ok()
            .and_then(|f| {
                f.extra
                    .get("pluginServers")
                    .and_then(Value::as_object)
                    .cloned()
            })
            .unwrap_or_default();
        let wanted: Vec<(String, String, String, ServerConfig, Lifecycle, Vec<Value>)> = servers
            .into_iter()
            .map(
                |PluginServer {
                     plugin,
                     server,
                     mut config,
                     lifecycle,
                 }| {
                    config.disabled = false;
                    let saved_server = saved.get(&plugin).and_then(|p| p.get(&server));
                    if let Some(perms) = saved_server
                        .and_then(|s| s.get("toolPermissions"))
                        .and_then(Value::as_object)
                    {
                        for (tool, p) in perms {
                            if let Ok(p) = serde_json::from_value::<Permission>(p.clone()) {
                                config.tool_permissions.insert(tool.clone(), p);
                            }
                        }
                    }
                    let cached = saved_server
                        .and_then(|s| s.get("tools"))
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    (
                        format!("plugin:{plugin}:{server}"),
                        format!("plugin_{plugin}_{server}"),
                        plugin,
                        config,
                        lifecycle,
                        cached,
                    )
                },
            )
            .collect();
        let (connect, close) = {
            let mut entries = self.shared.servers.lock().unwrap();
            let mut close = Vec::new();
            let stale: Vec<String> = entries
                .iter()
                .filter(|(key, e)| e.plugin.is_some() && !wanted.iter().any(|(k, ..)| k == *key))
                .map(|(key, _)| key.clone())
                .collect();
            for key in stale {
                if let Some(conn) = entries.remove(&key).and_then(|e| e.conn) {
                    close.push(conn);
                }
            }
            let mut connect = Vec::new();
            for (key, label, plugin, config, lifecycle, cached) in wanted {
                let same = entries.get(&key).is_some_and(|e| e.config == config);
                if same {
                    let entry = entries.get_mut(&key).expect("present");
                    if entry.lifecycle != lifecycle {
                        entry.lifecycle = lifecycle;
                        match lifecycle {
                            Lifecycle::Off => {
                                if let Some(conn) = entry.conn.take() {
                                    close.push(conn);
                                }
                                entry.generation += 1;
                                entry.status = Status::Off;
                            }
                            Lifecycle::Always if entry.conn.is_none() => connect.push(key.clone()),
                            Lifecycle::OnDemand { .. } if entry.conn.is_none() => {
                                if entry.tools.is_empty() {
                                    connect.push(key.clone());
                                } else {
                                    entry.status = Status::Sleeping;
                                }
                            }
                            _ => {}
                        }
                    }
                    continue;
                }
                let mut entry = Entry::new(label, config, None);
                entry.plugin = Some(plugin);
                entry.lifecycle = lifecycle;
                match lifecycle {
                    // Not offered while off; kept for when it is turned back on.
                    Lifecycle::Off => {
                        entry.status = Status::Off;
                        entry.tools = cached;
                    }
                    // Asleep with the tools it gave last time; never run yet: run once to learn them.
                    Lifecycle::OnDemand { .. } if !cached.is_empty() => {
                        entry.status = Status::Sleeping;
                        entry.tools = cached;
                    }
                    _ => connect.push(key.clone()),
                }
                if let Some(old) = entries.insert(key.clone(), entry).and_then(|e| e.conn) {
                    close.push(old);
                }
            }
            (connect, close)
        };
        self.changed();
        for conn in close {
            if let Ok(conn) = Arc::try_unwrap(conn) {
                conn.close().await;
            }
        }
        for key in connect {
            self.spawn_connect(key);
        }
    }

    /// The pool key of the app server `name`.
    pub fn app_key(&self, name: &str) -> String {
        format!("app:{name}")
    }

    /// The app server's name when `key` names one.
    fn app_of_key(&self, key: &str) -> Option<String> {
        key.strip_prefix("app:").map(str::to_string)
    }

    /// The tools the app server `name` gave last (kept while it reconnects
    /// or fails); empty when there is none.
    pub fn app_tools(&self, name: &str) -> Vec<Value> {
        let servers = self.shared.servers.lock().unwrap();
        servers
            .get(&self.app_key(name))
            .map(|e| e.tools.clone())
            .unwrap_or_default()
    }

    /// The permission the person (or the app's defaults) set for `tool`
    /// of the app server `name` — a tool of the server, or a name the app
    /// keeps its own choices under; `None`: not set.
    pub fn app_permission(&self, name: &str, tool: &str) -> Option<Permission> {
        let servers = self.shared.servers.lock().unwrap();
        servers
            .get(&self.app_key(name))
            .and_then(|e| e.config.tool_permissions.get(tool).copied())
    }

    /// Change the saved part of the app server `name` (`appServers[name]`).
    fn save_app(&self, name: &str, change: impl FnOnce(&mut Value)) -> Result<(), String> {
        self.save(|file| {
            let all = file.extra.entry("appServers").or_insert_with(|| json!({}));
            if !all.is_object() {
                *all = json!({});
            }
            let entry = all
                .as_object_mut()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| json!({}));
            if !entry.is_object() {
                *entry = json!({});
            }
            change(entry);
        })
    }

    /// Make the pool's app server exactly `server` (or none), and connect it
    /// before returning when it is new or changed. Its saved permissions
    /// win over its defaults; its last tool list is known at once (not
    /// offered until it connects). Its config is never written.
    pub async fn set_app_server(&self, server: Option<AppServer>) {
        let saved: Map<String, Value> = ConfigFile::load(&self.shared.path)
            .ok()
            .and_then(|f| {
                f.extra
                    .get("appServers")
                    .and_then(Value::as_object)
                    .cloned()
            })
            .unwrap_or_default();
        let wanted = server.map(
            |AppServer {
                 name,
                 mut config,
                 defaults,
             }| {
                config.disabled = false;
                let mut permissions = defaults;
                permissions.extend(std::mem::take(&mut config.tool_permissions));
                let saved_server = saved.get(&name);
                if let Some(perms) = saved_server
                    .and_then(|s| s.get("toolPermissions"))
                    .and_then(Value::as_object)
                {
                    for (tool, p) in perms {
                        if let Ok(p) = serde_json::from_value::<Permission>(p.clone()) {
                            permissions.insert(tool.clone(), p);
                        }
                    }
                }
                config.tool_permissions = permissions;
                let cached = saved_server
                    .and_then(|s| s.get("tools"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                (self.app_key(&name), name, config, cached)
            },
        );
        let (connect, close) = {
            let mut entries = self.shared.servers.lock().unwrap();
            let same = wanted.as_ref().is_some_and(|(key, _, config, _)| {
                entries.get(key).is_some_and(|e| &e.config == config)
            });
            let stale: Vec<String> = entries
                .iter()
                .filter(|(key, e)| {
                    e.app.is_some() && !(same && wanted.as_ref().is_some_and(|(k, ..)| k == *key))
                })
                .map(|(key, _)| key.clone())
                .collect();
            let mut close = Vec::new();
            for key in stale {
                if let Some(conn) = entries.remove(&key).and_then(|e| e.conn) {
                    close.push(conn);
                }
            }
            let mut connect = None;
            if let Some((key, name, config, cached)) = wanted.filter(|_| !same) {
                let mut entry = Entry::new(name.clone(), config, None);
                entry.app = Some(name);
                entry.tools = cached;
                entries.insert(key.clone(), entry);
                connect = Some(key);
            }
            (connect, close)
        };
        self.changed();
        for conn in close {
            if let Ok(conn) = Arc::try_unwrap(conn) {
                conn.close().await;
            }
        }
        if let Some(key) = connect {
            self.connect(&key).await;
        }
    }

    /// `(label, root)` when `key` names a project server.
    fn project_key(&self, key: &str) -> Option<(String, String)> {
        let servers = self.shared.servers.lock().unwrap();
        let entry = servers.get(key)?;
        Some((entry.label.clone(), entry.project.clone()?))
    }

    /// Change a project server's saved decision (`projectServers[root][name]`
    /// in the connectors file).
    fn save_approval(
        &self,
        root: &str,
        name: &str,
        change: impl FnOnce(&mut Value),
    ) -> Result<(), String> {
        self.save(|file| {
            let all = file
                .extra
                .entry("projectServers")
                .or_insert_with(|| json!({}));
            if !all.is_object() {
                *all = json!({});
            }
            let project = all
                .as_object_mut()
                .unwrap()
                .entry(root.to_string())
                .or_insert_with(|| json!({}));
            if !project.is_object() {
                *project = json!({});
            }
            let entry = project
                .as_object_mut()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| json!({}));
            if !entry.is_object() {
                *entry = json!({});
            }
            if entry.get("toolPermissions").is_none_or(|p| !p.is_object()) {
                entry["toolPermissions"] = json!({});
            }
            change(entry);
        })
    }

    fn approvals(&self, root: &str) -> Map<String, Value> {
        ConfigFile::load(&self.shared.path)
            .ok()
            .and_then(|file| {
                file.extra
                    .get("projectServers")?
                    .get(root)?
                    .as_object()
                    .cloned()
            })
            .unwrap_or_default()
    }

    /// The servers in the `.mcp.json` that applies to `cwd`, each with your
    /// decision: `approved`, `denied` or `pending`.
    pub fn project_state(&self, cwd: &Path) -> Value {
        let Some(root) = project_root(cwd) else {
            return json!({ "root": null, "servers": [] });
        };
        let key = root_key(&root);
        let file = ConfigFile::load(&root.join(".mcp.json"));
        let approvals = self.approvals(&key);
        let servers = self.shared.servers.lock().unwrap();
        let list: Vec<Value> = match &file {
            Ok(file) => file
                .servers
                .iter()
                .map(|(name, config)| {
                    let decision = approvals.get(name).and_then(|a| a.get("approved")).and_then(Value::as_bool);
                    json!({
                        "name": name,
                        "summary": config.summary(),
                        "kind": if config.url.is_some() { "http" } else { "stdio" },
                        "approval": match decision { Some(true) => "approved", Some(false) => "denied", None => "pending" },
                        "key": project_entry_key(name, &key),
                        "shadowed": servers.values().any(|e| e.project.is_none() && &e.label == name),
                    })
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        json!({
            "root": root,
            "file": root.join(".mcp.json"),
            "error": file.err(),
            "servers": list,
        })
    }

    /// Load and connect the approved servers of the project `cwd` is in
    /// (when a chat starts there). Already loaded ones are left alone.
    pub async fn open_project(&self, cwd: &Path) -> Result<(), String> {
        let Some(root) = project_root(cwd) else {
            return Ok(());
        };
        let key = root_key(&root);
        let file = ConfigFile::load(&root.join(".mcp.json"))?;
        let approvals = self.approvals(&key);
        let mut start = Vec::new();
        {
            let mut servers = self.shared.servers.lock().unwrap();
            for (name, config) in file.servers {
                let approval = approvals.get(&name);
                if approval
                    .and_then(|a| a.get("approved"))
                    .and_then(Value::as_bool)
                    != Some(true)
                {
                    continue;
                }
                let entry_key = project_entry_key(&name, &key);
                if servers.contains_key(&entry_key) {
                    continue;
                }
                let mut config = config;
                config.disabled = false;
                if let Some(perms) = approval
                    .and_then(|a| a.get("toolPermissions"))
                    .and_then(Value::as_object)
                {
                    for (tool, p) in perms {
                        if let Ok(p) = serde_json::from_value::<Permission>(p.clone()) {
                            config.tool_permissions.insert(tool.clone(), p);
                        }
                    }
                }
                if config.cwd.is_none() && config.command.is_some() {
                    config.cwd = Some(root.to_string_lossy().into_owned());
                }
                servers.insert(
                    entry_key.clone(),
                    Entry::new(name, config, Some(key.clone())),
                );
                start.push(entry_key);
            }
        }
        if !start.is_empty() {
            self.changed();
        }
        for key in start {
            self.spawn_connect(key);
        }
        Ok(())
    }

    /// Approve (connect) or deny (disconnect) a server of the project `cwd` is in.
    pub async fn set_project_approval(
        &self,
        cwd: &Path,
        name: &str,
        approved: bool,
    ) -> Result<(), String> {
        let root = project_root(cwd).ok_or("no .mcp.json applies to this folder")?;
        self.set_project_approval_at(&root_key(&root), name, approved)
            .await?;
        if approved {
            self.open_project(cwd).await?;
        }
        Ok(())
    }

    async fn set_project_approval_at(
        &self,
        root: &str,
        name: &str,
        approved: bool,
    ) -> Result<(), String> {
        self.save_approval(root, name, |entry| entry["approved"] = json!(approved))?;
        if !approved {
            let entry = self
                .shared
                .servers
                .lock()
                .unwrap()
                .remove(&project_entry_key(name, root));
            self.changed();
            if let Some(conn) = entry
                .and_then(|e| e.conn)
                .and_then(|c| Arc::try_unwrap(c).ok())
            {
                conn.close().await;
            }
        }
        Ok(())
    }

    /// Read one of `server`'s resources: its text parts joined.
    pub async fn read_resource(&self, server: &str, uri: &str) -> Result<String, String> {
        let conn = {
            let servers = self.shared.servers.lock().unwrap();
            servers
                .get(server)
                .or_else(|| {
                    servers
                        .values()
                        .find(|e| e.label == server && e.project.is_none())
                })
                .and_then(|e| e.conn.clone())
                .ok_or_else(|| format!("{server} is not connected"))?
        };
        let result = conn.read_resource(uri).await?;
        let texts: Vec<String> = result
            .get("contents")
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .map(|p| match p.get("text").and_then(Value::as_str) {
                        Some(text) => text.to_string(),
                        None => format!(
                            "[{} content, {} bytes base64]",
                            p.get("mimeType")
                                .and_then(Value::as_str)
                                .unwrap_or("binary"),
                            p.get("blob").and_then(Value::as_str).map_or(0, str::len)
                        ),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let text = texts.join("\n\n");
        Ok(if text.chars().count() > MAX_RESULT_CHARS {
            text.chars().take(MAX_RESULT_CHARS).collect::<String>() + "\n[cut]"
        } else {
            text
        })
    }

    /// Close every connection (the app is quitting).
    pub async fn shutdown(&self) {
        let conns: Vec<Arc<Connection>> = {
            let mut servers = self.shared.servers.lock().unwrap();
            servers.values_mut().filter_map(|e| e.conn.take()).collect()
        };
        for conn in conns {
            if let Ok(conn) = Arc::try_unwrap(conn) {
                conn.close().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // docs/features/plugin-settings/spec.md (FR-11, FR-12; T13).
    fn broken(lifecycle: Lifecycle) -> PluginServer {
        PluginServer {
            plugin: "notes".into(),
            server: "backend".into(),
            config: ServerConfig {
                command: Some("evo-no-such-mcp-server-xyz".into()),
                ..Default::default()
            },
            lifecycle,
        }
    }

    const KEY: &str = "plugin:notes:backend";

    fn pool_with_cached_tools(dir: &Path) -> Pool {
        let path = dir.join("mcp.json");
        std::fs::write(
            &path,
            serde_json::to_string(&json!({
                "mcpServers": {},
                "pluginServers": { "notes": { "backend": { "tools": [{ "name": "echo", "description": "Echo", "inputSchema": { "type": "object" } }] } } }
            }))
            .unwrap(),
        )
        .unwrap();
        let pool = Pool::new(path, None, || {});
        pool.start().unwrap();
        pool
    }

    #[tokio::test]
    async fn a_sleeping_plugin_server_offers_its_last_tools_and_a_call_starts_it() {
        let dir = tempfile::tempdir().unwrap();
        let pool = pool_with_cached_tools(dir.path());
        pool.set_plugin_servers(vec![broken(Lifecycle::OnDemand {
            idle: Duration::from_secs(600),
        })])
        .await;
        assert_eq!(
            pool.snapshot()["servers"][0]["status"],
            "sleeping",
            "no process for a server not in use"
        );
        let names: Vec<String> = pool
            .chat_tools(None)
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["mcp__plugin_notes_backend__echo"]);
        // The call starts it; this one cannot start, and says so.
        let error = pool
            .call("mcp__plugin_notes_backend__echo", None, None)
            .await
            .unwrap_err();
        assert!(error.contains("could not start"), "{error}");
        assert_eq!(pool.snapshot()["servers"][0]["status"], "failed");
        // Its own view calling it gets the same answer.
        assert!(pool
            .call_server(KEY, "echo", None)
            .await
            .unwrap_err()
            .contains("could not start"));
        assert!(pool
            .call_server(KEY, "nope", None)
            .await
            .unwrap_err()
            .contains("no tool `nope`"));
    }

    #[tokio::test]
    async fn an_off_server_never_starts_and_offers_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let pool = pool_with_cached_tools(dir.path());
        pool.set_plugin_servers(vec![broken(Lifecycle::Off)]).await;
        assert_eq!(pool.snapshot()["servers"][0]["status"], "off");
        assert!(pool.chat_tools(None).is_empty());
        assert!(pool
            .wake(KEY)
            .await
            .err()
            .unwrap()
            .contains("set not to run"));
        // Changing only the lifecycle keeps the entry.
        pool.set_plugin_servers(vec![broken(Lifecycle::OnDemand {
            idle: Duration::from_secs(60),
        })])
        .await;
        assert_eq!(pool.plugin_keys("notes"), [KEY]);
        assert_eq!(
            pool.snapshot()["servers"][0]["status"],
            "sleeping",
            "back on demand, it sleeps"
        );
        pool.set_plugin_servers(vec![]).await;
        assert!(
            pool.plugin_keys("notes").is_empty(),
            "a plugin turned off loses its servers"
        );
    }

    #[tokio::test]
    async fn a_server_that_keeps_crashing_is_left_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let pool = pool_with_cached_tools(dir.path());
        pool.set_plugin_servers(vec![broken(Lifecycle::OnDemand {
            idle: Duration::from_secs(600),
        })])
        .await;
        let start = Instant::now();
        for n in 0..MAX_CRASHES {
            // As if it had been running and its process ended.
            pool.shared
                .servers
                .lock()
                .unwrap()
                .get_mut(KEY)
                .unwrap()
                .status = Status::Connected;
            pool.tick(start + Duration::from_secs(n as u64)).await;
        }
        let snap = pool.snapshot();
        assert_eq!(snap["servers"][0]["status"], "crashed");
        assert!(snap["servers"][0]["error"]
            .as_str()
            .unwrap()
            .contains("5 crashes"));
        // Old crashes leave the window.
        pool.shared
            .servers
            .lock()
            .unwrap()
            .get_mut(KEY)
            .unwrap()
            .status = Status::Connected;
        pool.tick(start + Duration::from_secs(120)).await;
        assert_ne!(
            pool.snapshot()["servers"][0]["status"],
            "crashed",
            "one crash after a quiet minute restarts it"
        );
    }

    // docs/features/plugin-settings/spec.md (NFR-1).
    #[test]
    fn plugin_server_secrets_stay_out_of_what_windows_see() {
        let config: ServerConfig = serde_json::from_value(json!({
            "command": "run", "args": ["--space", "team"],
            "env": { "TOKEN": "tok-SECRET-42", "N": "1" },
            "headers": { "Authorization": "Bearer hdr-SECRET-7" }
        }))
        .unwrap();
        let (shown, log) = hide_plugin_values(
            &config,
            vec![
                "using tok-SECRET-42 now".into(),
                "auth Bearer hdr-SECRET-7".into(),
                "plain".into(),
            ],
        );
        assert_eq!(shown["env"]["TOKEN"], "(hidden)");
        assert_eq!(shown["headers"]["Authorization"], "(hidden)");
        assert_eq!(shown["args"], json!(["--space", "team"]));
        assert_eq!(log, ["using (hidden) now", "auth (hidden)", "plain"]);
    }

    #[test]
    fn exposed_names_are_safe_and_short() {
        assert_eq!(
            exposed_name("github", "create_issue"),
            "mcp__github__create_issue"
        );
        assert_eq!(exposed_name("my server", "a.b"), "mcp__my_server__a_b");
        assert_eq!(exposed_name("s", &"x".repeat(100)).len(), 64);
    }

    #[test]
    fn read_only_tools_are_allowed_by_default() {
        let read = json!({"name": "list", "annotations": {"readOnlyHint": true}});
        let write = json!({"name": "delete"});
        assert_eq!(default_permission(&read), Permission::Allow);
        assert_eq!(default_permission(&write), Permission::Ask);
        let mut config = ServerConfig::default();
        config
            .tool_permissions
            .insert("list".into(), Permission::Block);
        assert_eq!(permission_of(&config, &read), Permission::Block);
    }

    #[tokio::test]
    async fn a_bad_server_fails_and_the_config_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        let changes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = changes.clone();
        let pool = Pool::new(path.clone(), None, move || {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        pool.start().unwrap();
        let config = ServerConfig {
            command: Some("evo-no-such-mcp-server-xyz".into()),
            ..Default::default()
        };
        pool.upsert("broken", config, None).await.unwrap();
        let snap = pool.snapshot();
        assert_eq!(snap["servers"][0]["name"], "broken");
        assert_eq!(snap["servers"][0]["status"], "failed");
        assert!(snap["servers"][0]["error"]
            .as_str()
            .unwrap()
            .contains("could not start"));
        assert!(changes.load(std::sync::atomic::Ordering::SeqCst) > 0);
        // Saved, and turning it off is remembered.
        pool.set_enabled("broken", false).await.unwrap();
        let file = ConfigFile::load(&path).unwrap();
        assert!(file.servers["broken"].disabled);
        assert_eq!(pool.snapshot()["servers"][0]["status"], "off");
        assert!(pool
            .upsert("bad name", ServerConfig::default(), None)
            .await
            .is_err());
        pool.remove("broken").await.unwrap();
        assert!(ConfigFile::load(&path).unwrap().servers.is_empty());
    }

    // docs/features/connected-apps/spec.md (FR-7, NFR-1; T2).
    const SECRET: &str = "ak_app-SECRET-key-123";

    fn app(defaults: &[(&str, Permission)]) -> AppServer {
        let mut headers = BTreeMap::new();
        headers.insert("x-api-key".to_string(), SECRET.to_string());
        AppServer {
            name: "connected_apps".into(),
            config: ServerConfig {
                url: Some("http://127.0.0.1:1/tool_router/trs_1/mcp".into()),
                headers,
                ..Default::default()
            },
            defaults: defaults.iter().map(|(t, p)| (t.to_string(), *p)).collect(),
        }
    }

    /// A connection to an in-process server (its own tools do not matter:
    /// the test hands the pool the list).
    async fn in_process() -> Connection {
        use rmcp::handler::server::ServerHandler;
        use rmcp::service::ServiceExt;
        struct Nothing;
        impl ServerHandler for Nothing {
            fn get_info(&self) -> rmcp::model::ServerConfig {
                rmcp::model::ServerConfig::new(
                    rmcp::model::ServerCapabilities::builder()
                        .enable_tools()
                        .build(),
                )
            }
        }
        let (client_end, server_end) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            let server = Nothing.serve(server_end).await.expect("server starts");
            let _ = server.waiting().await;
        });
        client::connect_transport(client_end)
            .await
            .expect("handshake")
    }

    fn meta_tools() -> Vec<Value> {
        vec![
            json!({"name": "COMPOSIO_SEARCH_TOOLS", "description": "Search", "inputSchema": {"type": "object"}, "annotations": {"readOnlyHint": true}}),
            json!({"name": "COMPOSIO_MULTI_EXECUTE_TOOL", "description": "Run", "inputSchema": {"type": "object"}}),
            json!({"name": "COMPOSIO_REMOTE_BASH_TOOL", "description": "Bash", "inputSchema": {"type": "object"}}),
        ]
    }

    #[tokio::test]
    async fn app_server_config_and_headers_are_never_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        let pool = Pool::new(path.clone(), None, || {});
        pool.start().unwrap();
        pool.set_app_server(Some(app(&[]))).await;
        let snap = pool.snapshot();
        let server = &snap["servers"][0];
        assert_eq!(server["key"], "app:connected_apps");
        assert_eq!(server["name"], "connected_apps");
        assert_eq!(server["app"], "connected_apps");
        assert_eq!(server["status"], "failed", "nothing listens there");
        pool.start().unwrap();
        assert_eq!(
            pool.snapshot()["servers"][0]["key"],
            "app:connected_apps",
            "reading the file again keeps it"
        );
        assert!(!snap.to_string().contains(SECRET), "{snap}");
        // Its permissions are saved apart from the person's servers…
        pool.set_permission(
            "app:connected_apps",
            "COMPOSIO_SEARCH_TOOLS",
            Permission::Block,
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(SECRET), "{text}");
        assert!(!text.contains("127.0.0.1"), "{text}");
        let file = ConfigFile::load(&path).unwrap();
        assert!(file.servers.is_empty());
        assert_eq!(
            file.extra["appServers"]["connected_apps"]["toolPermissions"]["COMPOSIO_SEARCH_TOOLS"],
            "block"
        );
        // …and it is not the person's to remove, switch or shadow.
        assert!(pool.remove("app:connected_apps").await.is_err());
        assert!(pool.set_enabled("app:connected_apps", false).await.is_err());
        assert!(pool
            .upsert(
                "connected_apps",
                ServerConfig {
                    command: Some("x".into()),
                    ..Default::default()
                },
                None
            )
            .await
            .unwrap_err()
            .contains("already exists"));
        // Gone: no entry, the saved permissions stay for next time.
        pool.set_app_server(None).await;
        assert!(pool.snapshot()["servers"].as_array().unwrap().is_empty());
        let file = ConfigFile::load(&path).unwrap();
        assert!(file.extra["appServers"]["connected_apps"]["toolPermissions"].is_object());
    }

    #[tokio::test]
    async fn app_server_tools_are_cached_with_default_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        let pool = Pool::new(path.clone(), None, || {});
        pool.start().unwrap();
        pool.set_app_server(Some(app(&[(
            "COMPOSIO_REMOTE_BASH_TOOL",
            Permission::Block,
        )])))
        .await;
        let key = pool.app_key("connected_apps");
        assert_eq!(key, "app:connected_apps");
        let generation = pool.shared.servers.lock().unwrap()[&key].generation;
        pool.finish(
            &key,
            generation,
            Ok((in_process().await, meta_tools(), vec![], vec![])),
        );
        let names: Vec<String> = pool
            .chat_tools(None)
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            [
                "mcp__connected_apps__COMPOSIO_SEARCH_TOOLS",
                "mcp__connected_apps__COMPOSIO_MULTI_EXECUTE_TOOL"
            ],
            "the blocked-by-default tool is not offered"
        );
        let tools = pool.chat_tools(None);
        assert_eq!(tools[0]["permission"], "allow");
        assert_eq!(tools[1]["permission"], "ask");
        assert_eq!(tools[1]["server"], "app:connected_apps");
        // Tools cached for next time, without the config.
        let file = ConfigFile::load(&path).unwrap();
        assert_eq!(
            file.extra["appServers"]["connected_apps"]["tools"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(pool.app_tools("connected_apps").len(), 3);
        // A saved choice wins over the default, and survives a new session.
        pool.set_permission(&key, "COMPOSIO_REMOTE_BASH_TOOL", Permission::Ask)
            .unwrap();
        assert_eq!(
            pool.app_permission("connected_apps", "COMPOSIO_REMOTE_BASH_TOOL"),
            Some(Permission::Ask)
        );
        assert_eq!(
            pool.app_permission("connected_apps", "GMAIL_SEND_EMAIL"),
            None
        );
        let mut changed = app(&[("COMPOSIO_REMOTE_BASH_TOOL", Permission::Block)]);
        changed.config.url = Some("http://127.0.0.1:1/tool_router/trs_2/mcp".into());
        pool.set_app_server(Some(changed)).await;
        let snap = pool.snapshot();
        assert_eq!(
            snap["servers"][0]["config"]["toolPermissions"]["COMPOSIO_REMOTE_BASH_TOOL"],
            "ask"
        );
        assert_eq!(
            pool.app_tools("connected_apps").len(),
            3,
            "the last list is known while it connects"
        );
    }

    #[tokio::test]
    async fn project_servers_wait_for_approval_and_stay_in_their_project() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("proj");
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(
            project.join(".mcp.json"),
            r#"{"mcpServers": {"tooling": {"command": "evo-no-such-mcp-server-xyz"}}}"#,
        )
        .unwrap();
        let pool = Pool::new(dir.path().join("agent").join("mcp.json"), None, || {});
        pool.start().unwrap();
        // Found from a subfolder; pending until approved, and not loaded.
        let state = pool.project_state(&project.join("src"));
        assert_eq!(state["servers"][0]["name"], "tooling");
        assert_eq!(state["servers"][0]["approval"], "pending");
        pool.open_project(&project).await.unwrap();
        assert_eq!(pool.snapshot()["servers"].as_array().unwrap().len(), 0);
        // Approved: loaded (it fails to start here, which is fine), remembered.
        pool.set_project_approval(&project, "tooling", true)
            .await
            .unwrap();
        assert_eq!(
            pool.project_state(&project)["servers"][0]["approval"],
            "approved"
        );
        let snap = pool.snapshot();
        assert_eq!(snap["servers"][0]["name"], "tooling");
        assert!(snap["servers"][0]["project"].is_string());
        // Not offered to chats in other folders (nor here, as it is not connected).
        assert!(pool.chat_tools(Some(dir.path())).is_empty());
        // Denied: unloaded and remembered.
        pool.set_project_approval(&project, "tooling", false)
            .await
            .unwrap();
        assert_eq!(
            pool.project_state(&project)["servers"][0]["approval"],
            "denied"
        );
        assert_eq!(pool.snapshot()["servers"].as_array().unwrap().len(), 0);
        // Outside any project: nothing.
        assert!(pool.project_state(dir.path())["root"].is_null());
    }
}
