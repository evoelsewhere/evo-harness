//! Plugin AI in the shell (docs/features/plugin-ai/spec.md; plan 3.2, 3.5;
//! ADR 0023): the gate between plugins and the plugin-AI engine.
//!
//! - **The engine** (`evo-engine --mode plugin-ai`) starts on the first call,
//!   stays while calls come and stops after [`IDLE_STOP`] with no run. It
//!   chooses the model and calls it; the shell never talks to a provider.
//! - **Gates:** the plugin is on and has `ai`; the person's caps per day and
//!   month are not reached (none by default); at most [`PER_PLUGIN`] runs of
//!   a plugin and [`OVERALL`] in all go at once, others wait up to
//!   [`QUEUE_WAIT`].
//! - **Runs** belong to the plugin, not to a window: events go to every
//!   follower (the view that started it, a backend's stream, a view that
//!   follows it later); a run goes on when its view closes (FR-24), and its
//!   result and events are kept by the engine for [`follow`].
//!
//! Views reach this through the bridge (`ai.*`), backends and hooks through
//! the host API (`/v1/ai/*`), the plugin page through the commands below.

mod context;
pub mod host;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use evo_plugins::store::AiSettings;
use evo_plugins::{Installed, Store};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{mpsc, oneshot, Notify};

pub const PER_PLUGIN: usize = 3;
pub const OVERALL: usize = 6;
pub const QUEUE_WAIT: Duration = Duration::from_secs(30);
pub const IDLE_STOP: Duration = Duration::from_secs(5 * 60);
/// The most a run's request may weigh (messages with images).
pub const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;

/// Why a call was refused: `(code, message)`.
pub type Refusal = (&'static str, String);

/// Where a run's events go.
#[derive(Clone)]
pub enum Follower {
    /// A plugin view, by its webview label.
    View(String),
    /// A backend's or hook's open stream.
    Stream(mpsc::UnboundedSender<Value>),
}

/// Where a tool the model calls runs (plan-ai FR-10, FR-11).
#[derive(Debug, Clone, PartialEq)]
pub enum ToolRoute {
    /// A function of the view that started the run.
    View,
    /// A tool of one of the plugin's servers: pool key and tool name.
    Backend { key: String, tool: String },
    /// One of evo's app tools (browser, terminal), run by the shell.
    App,
}

/// What a run with evo's app tools needs in the shell.
#[derive(Clone)]
pub struct AppsRun {
    plugin_name: String,
    folder: std::path::PathBuf,
    /// Cancelled when the run ends: its app tool calls stop.
    stop: tokio_util::sync::CancellationToken,
}

/// Most tools a run may offer.
pub const MAX_TOOLS: usize = 32;

struct RunEntry {
    plugin_id: String,
    /// The view that started it, which runs its view tools.
    view: Option<String>,
    /// The thread it continues (one run per thread at a time).
    thread: Option<String>,
    routes: HashMap<String, ToolRoute>,
    followers: Vec<Follower>,
    started: Instant,
    /// What the plugin had spent today and this month when the run started.
    spent: (f64, f64),
    caps: (Option<f64>, Option<f64>),
    cost: f64,
    model: Option<String>,
    cap_hit: bool,
    /// Set when the run may use evo's app tools.
    apps: Option<AppsRun>,
}

struct Engine {
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    _child: Child,
    generation: u64,
}

#[derive(Default)]
struct State {
    engine: tokio::sync::Mutex<Option<Engine>>,
    ready: Notify,
    runs: Mutex<HashMap<String, RunEntry>>,
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    next: AtomicU64,
    generation: AtomicU64,
    last_used: Mutex<Option<Instant>>,
    /// Questions to the person about evo's tools in runs, open now.
    questions: Mutex<Vec<Value>>,
    /// The shell's own questions (app tools), answered here, not by the engine.
    local: Mutex<HashMap<String, oneshot::Sender<i32>>>,
}

static STATE: OnceLock<State> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();

fn state() -> &'static State {
    STATE.get_or_init(State::default)
}

fn store() -> Store {
    Store::in_agent_dir(&crate::mcp::agent_dir())
}

fn touch() {
    *state().last_used.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
}

/// Keep the app; stop the engine when it has been idle.
pub fn setup(app: &AppHandle) {
    let _ = APP.set(app.clone());
    if let Ok(conn) = evo_store::open() {
        let _ = evo_store::plugin_ai::end_orphans(&conn, crate::supervisor::now_ms() as i64);
    }
    tauri::async_runtime::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let idle = state().runs.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
                && state().last_used.lock().unwrap_or_else(|e| e.into_inner()).is_some_and(|at| at.elapsed() >= IDLE_STOP);
            if idle {
                stop_engine().await;
            }
        }
    });
}

async fn stop_engine() {
    let mut engine = state().engine.lock().await;
    if let Some(running) = engine.take() {
        let mut stdin = running.stdin.lock().await;
        let _ = stdin.write_all(b"{\"type\":\"stop\"}\n").await;
        let _ = stdin.flush().await;
    }
}

/// Start the engine when none runs; wait for its `ready`.
async fn ensure_engine() -> Result<Arc<tokio::sync::Mutex<ChildStdin>>, Refusal> {
    let mut engine = state().engine.lock().await;
    if let Some(running) = engine.as_ref() {
        return Ok(running.stdin.clone());
    }
    let mut command = tokio::process::Command::new(crate::engine::engine_binary());
    command.args(["--mode", "plugin-ai"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    // As for chat engines: a command that would stop the app always asks.
    command.env("EVO_HOST_PID", std::process::id().to_string());
    if let Some(name) = std::env::current_exe().ok().and_then(|exe| exe.file_name().map(|name| name.to_string_lossy().into_owned())) {
        command.env("EVO_HOST_EXE", name);
    }
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|e| ("engine", format!("the AI engine could not start: {e}")))?;
    let stdin = Arc::new(tokio::sync::Mutex::new(child.stdin.take().ok_or(("engine", "no stdin".to_string()))?));
    let stdout = child.stdout.take().ok_or(("engine", "no stdout".to_string()))?;
    let generation = state().generation.fetch_add(1, Ordering::Relaxed) + 1;
    let ready = state().ready.notified();
    tokio::pin!(ready);
    ready.as_mut().enable();
    tauri::async_runtime::spawn(read_engine(stdout, generation));
    *engine = Some(Engine { stdin: stdin.clone(), _child: child, generation });
    drop(engine);
    if tokio::time::timeout(Duration::from_secs(30), ready).await.is_err() {
        stop_engine().await;
        return Err(("engine", "the AI engine did not start in time".into()));
    }
    Ok(stdin)
}

async fn read_engine(stdout: tokio::process::ChildStdout, generation: u64) {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
        match value["type"].as_str() {
            Some("ready") => state().ready.notify_waiters(),
            Some("response") => {
                let id = value["id"].as_str().unwrap_or_default().to_string();
                if let Some(tx) = state().pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
                    let _ = tx.send(value);
                }
            }
            Some("ai_event") => {
                let run_id = value["runId"].as_str().unwrap_or_default().to_string();
                route(&run_id, value["event"].clone());
            }
            Some("ai_question") => {
                question_opened(value);
            }
            Some("ai_tool_call") => {
                tauri::async_runtime::spawn(tool_call(value));
            }
            _ => {}
        }
    }
    // The engine ended: its runs end with it.
    let mut engine = state().engine.lock().await;
    if engine.as_ref().is_some_and(|e| e.generation == generation) {
        *engine = None;
    }
    drop(engine);
    let ended: Vec<String> = state().runs.lock().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect();
    for run_id in ended {
        route(&run_id, json!({ "type": "done", "result": { "text": "", "usage": { "input": 0, "output": 0, "costUsd": 0.0 }, "stopped": "error", "error": "the AI engine stopped" } }));
    }
}

// ── Questions about evo's tools (FR-16) ───────────────────────────────────────

/// A run asks the person before one of evo's tools: the main window shows
/// it, with an OS notice when no window is in front.
fn question_opened(value: Value) -> bool {
    let run_id = value["runId"].as_str().unwrap_or_default().to_string();
    let plugin = state().runs.lock().unwrap_or_else(|e| e.into_inner()).get(&run_id).map(|r| r.plugin_id.clone());
    let Some(plugin) = plugin else {
        // The run is over: nobody is asked.
        let id = value["id"].clone();
        tauri::async_runtime::spawn(async move { send_to_running(json!({ "type": "ai_answer", "id": id, "index": -1 })).await });
        return false;
    };
    let question = json!({ "id": value["id"], "runId": run_id, "plugin": plugin, "title": value["title"], "detail": value["detail"], "options": value["options"] });
    state().questions.lock().unwrap_or_else(|e| e.into_inner()).push(question.clone());
    if let Some(app) = APP.get() {
        let _ = app.emit_to("main", "plugin-ai://question", question);
        if !crate::agent_tools::user::any_window_focused(app) && !crate::agent_tools::user::muted() {
            let detail = value["detail"].as_str().unwrap_or_default().lines().last().unwrap_or_default().to_string();
            crate::agent_tools::user::send_os(app, value["title"].as_str().unwrap_or("A plugin asks"), Some(&detail), None);
        }
    }
    true
}

/// Ask the person for an app tool of run `run_id` (the shell's own gate):
/// the plugin-AI dialog, answered here. -1 when the call or the run ends.
pub async fn ask_in_run(run_id: &str, title: &str, detail: &str, options: Vec<String>, cancel: &tokio_util::sync::CancellationToken) -> i32 {
    let id = format!("s{}", state().next.fetch_add(1, Ordering::Relaxed));
    let (tx, rx) = oneshot::channel();
    state().local.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), tx);
    if !question_opened(json!({ "runId": run_id, "id": id, "title": title, "detail": detail, "options": options })) {
        // The run is over: nobody is asked.
        state().local.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        return -1;
    }
    let answer = tokio::select! {
        answer = rx => answer.unwrap_or(-1),
        _ = cancel.cancelled() => -1,
    };
    state().local.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    questions_closed(|q| q["id"] == id.as_str());
    answer
}

/// Close the open questions `which` picks (their run ended, or answered).
fn questions_closed(which: impl Fn(&Value) -> bool) {
    let closed: Vec<Value> = {
        let mut open = state().questions.lock().unwrap_or_else(|e| e.into_inner());
        let (closed, kept): (Vec<Value>, Vec<Value>) = open.drain(..).partition(|q| which(q));
        *open = kept;
        closed
    };
    if let Some(app) = APP.get() {
        for question in closed {
            let _ = app.emit_to("main", "plugin-ai://question-closed", json!({ "id": question["id"] }));
        }
    }
}

/// The groups of evo's tools a request asks for, each allowed by its
/// `ai.tools:<group>` permission (FR-16).
pub fn evo_groups(body: &Map<String, Value>, approved: &[String]) -> Result<Vec<String>, Refusal> {
    let Some(asked) = body.get("evoTools").filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let list = asked.as_array().ok_or(("bad_request", "`evoTools` must be a list of groups: read, edit, command, web".to_string()))?;
    let mut groups: Vec<String> = Vec::new();
    for group in list {
        let group = group.as_str().unwrap_or_default();
        if !EVO_GROUPS.contains(&group) {
            return Err(("bad_request", format!("`{group}` is not a group of evo's tools: read, edit, command, web, apps")));
        }
        if !approved.iter().any(|p| p == &format!("ai.tools:{group}")) {
            return Err(("no_permission", format!("using evo's {group} tools needs the `ai.tools:{group}` permission")));
        }
        if !groups.iter().any(|g| g == group) {
            groups.push(group.to_string());
        }
    }
    Ok(groups)
}

/// Groups of evo's tools a plugin's run may use.
pub const EVO_GROUPS: [&str; 5] = ["read", "edit", "command", "web", "apps"];

/// Run a tool the engine asked for and send it the answer.
async fn tool_call(call: Value) {
    let text = |key: &str| call[key].as_str().unwrap_or_default().to_string();
    let (run_id, call_id, tool) = (text("runId"), text("callId"), text("tool"));
    let (route, apps) = {
        let runs = state().runs.lock().unwrap_or_else(|e| e.into_inner());
        let entry = runs.get(&run_id);
        (entry.map(|entry| (entry.routes.get(&tool).cloned(), entry.view.clone())), entry.and_then(|e| e.apps.clone()))
    };
    let (result, is_error) = match route {
        None => (json!("the run is over"), true),
        Some((None, _)) => (json!(format!("no tool `{tool}`")), true),
        Some((Some(ToolRoute::View), view)) => {
            let open = view.as_ref().zip(APP.get()).is_some_and(|(label, app)| tauri::Manager::get_webview(app, label).is_some());
            if open {
                // The view answers through the bridge (`ai.toolResult`).
                let (app, label) = (APP.get().expect("app"), view.expect("view"));
                crate::plugin_ui::views::dispatch(app, &label, &json!({ "kind": "ai.tool", "runId": run_id, "callId": call_id, "tool": tool, "input": call["input"] }));
                return;
            }
            (json!("the view that offered this tool is closed"), true)
        }
        Some((Some(ToolRoute::App), _)) => match (APP.get(), apps) {
            (Some(app), Some(apps)) => {
                let run = crate::agent_tools::PluginRun { id: run_id.clone(), folder: apps.folder.clone() };
                let result = crate::agent_tools::call_for_plugin_run(app, run, &apps.plugin_name, &tool, call["input"].clone(), apps.stop.child_token()).await;
                let is_error = result["isError"] == true;
                (json!({ "content": result["content"] }), is_error)
            }
            _ => (json!("the app's tools are not available to this run"), true),
        },
        Some((Some(ToolRoute::Backend { key, tool }), _)) => match APP.get() {
            Some(app) => {
                let pool = tauri::Manager::state::<crate::mcp::McpState>(app).0.clone();
                let input = call["input"].as_object().cloned();
                match pool.call_server(&key, &tool, input).await {
                    Ok(result) => (json!(mcp_text(&result)), result["isError"] == true),
                    Err(error) => (json!(error), true),
                }
            }
            None => (json!("the app is not ready"), true),
        },
    };
    send_to_running(json!({ "type": "ai_tool_result", "runId": run_id, "callId": call_id, "result": result, "isError": is_error })).await;
}

/// An MCP tool result as text for the model.
pub fn mcp_text(result: &Value) -> String {
    let mut parts: Vec<String> = result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|part| match part["type"].as_str() {
            Some("text") => part["text"].as_str().unwrap_or_default().to_string(),
            _ => part.to_string(),
        })
        .collect();
    if let Some(structured) = result.get("structuredContent").filter(|s| s.is_object()) {
        parts.push(structured.to_string());
    }
    parts.join("\n")
}

/// A view's answer to one of its tools (bridge `ai.toolResult`).
pub async fn view_tool_result(plugin_id: &str, run_id: &str, call_id: &str, result: Value, is_error: bool) -> Result<(), Refusal> {
    let owner = state().runs.lock().unwrap_or_else(|e| e.into_inner()).get(run_id).map(|r| r.plugin_id.clone());
    if owner.as_deref() != Some(plugin_id) {
        return Err(("not_found", "that run is not going".into()));
    }
    send_to_running(json!({ "type": "ai_tool_result", "runId": run_id, "callId": call_id, "result": result, "isError": is_error })).await;
    Ok(())
}

/// The tools a run offers the model and where each runs: a view's own
/// (`tools`, only from a view) and its plugin's server tools
/// (`backendTools`: `true` or names), from `server_tools` (the pool's tool
/// list of this plugin's servers).
pub fn tools_for(body: &Map<String, Value>, from_view: bool, plugin_name: &str, server_tools: &[Value]) -> Result<(Vec<Value>, HashMap<String, ToolRoute>), Refusal> {
    let bad = |m: String| ("bad_request", m);
    let mut offered = Vec::new();
    let mut routes = HashMap::new();
    let valid = |name: &str| !name.is_empty() && name.len() <= 64 && name != "respond" && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if let Some(tools) = body.get("tools").filter(|t| !t.is_null()) {
        if !from_view {
            return Err(bad("a backend offers its own tools with `backendTools`".into()));
        }
        for tool in tools.as_array().ok_or_else(|| bad("`tools` must be a list".into()))? {
            let name = tool["name"].as_str().unwrap_or_default();
            if !valid(name) {
                return Err(bad(format!("tool name `{name}` must be 1 to 64 letters, digits, - or _ (and not `respond`)")));
            }
            if routes.insert(name.to_string(), ToolRoute::View).is_some() {
                return Err(bad(format!("tool `{name}` is offered twice")));
            }
            offered.push(json!({ "name": name, "description": tool["description"].as_str().unwrap_or_default(), "inputSchema": tool.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object" })), "origin": "view" }));
        }
    }
    let wanted: Option<Vec<String>> = match body.get("backendTools") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => Some(Vec::new()),
        Some(Value::Bool(true)) => None,
        Some(Value::Array(names)) => Some(names.iter().filter_map(Value::as_str).map(str::to_string).collect()),
        Some(_) => return Err(bad("`backendTools` must be true or a list of tool names".into())),
    };
    let prefix = format!("plugin:{plugin_name}:");
    if wanted.as_ref().is_none_or(|names| !names.is_empty()) {
        for tool in server_tools.iter().filter(|t| t["server"].as_str().is_some_and(|s| s.starts_with(&prefix))) {
            let name = tool["tool"].as_str().unwrap_or_default();
            if wanted.as_ref().is_some_and(|names| !names.iter().any(|n| n == name)) || !valid(name) || routes.contains_key(name) {
                continue;
            }
            routes.insert(name.to_string(), ToolRoute::Backend { key: tool["server"].as_str().unwrap_or_default().to_string(), tool: name.to_string() });
            let description = tool["description"].as_str().unwrap_or_default();
            let description = description.strip_prefix('[').and_then(|d| d.split_once("] ")).map_or(description, |(_, rest)| rest);
            offered.push(json!({ "name": name, "description": description, "inputSchema": tool.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object" })), "origin": "backend" }));
        }
        if let Some(names) = &wanted {
            if let Some(missing) = names.iter().find(|n| !routes.contains_key(n.as_str())) {
                return Err(bad(format!("{plugin_name}'s servers have no tool `{missing}`")));
            }
        }
    }
    if offered.len() > MAX_TOOLS {
        return Err(bad(format!("a run offers at most {MAX_TOOLS} tools")));
    }
    Ok((offered, routes))
}

/// Send `event` of `run_id` to its followers; enforce caps on usage; forget
/// the run when it is done.
fn route(run_id: &str, event: Value) {
    let kind = event["type"].as_str().unwrap_or_default().to_string();
    let (followers, cancel, ended_apps) = {
        let mut runs = state().runs.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = runs.get_mut(run_id) else { return };
        if kind == "started" {
            entry.model = event["model"].as_str().map(str::to_string);
        }
        let mut cancel = false;
        if kind == "usage" {
            entry.cost = event["usage"]["costUsd"].as_f64().unwrap_or(entry.cost);
            cancel = !entry.cap_hit && over_cap(entry.spent, entry.cost, entry.caps);
            entry.cap_hit |= cancel;
        }
        let followers = entry.followers.clone();
        let mut ended_apps = None;
        if kind == "done" {
            ended_apps = runs.remove(run_id).and_then(|entry| entry.apps);
        }
        (followers, cancel, ended_apps)
    };
    if kind == "done" {
        questions_closed(|q| q["runId"] == run_id);
        if let (Some(apps), Some(app)) = (ended_apps, APP.get()) {
            apps.stop.cancel();
            crate::agent_tools::end_plugin_run(app, run_id);
        }
    }
    for follower in followers {
        deliver(&follower, run_id, &event);
    }
    if cancel {
        let run_id = run_id.to_string();
        tauri::async_runtime::spawn(async move { send_line(json!({ "type": "ai_cancel", "runId": run_id })).await });
    }
    if kind == "done" || kind == "started" {
        if let Some(app) = APP.get() {
            let _ = app.emit("plugin-ai://runs", ());
        }
    }
}

fn deliver(follower: &Follower, run_id: &str, event: &Value) {
    match follower {
        Follower::View(label) => {
            if let Some(app) = APP.get() {
                crate::plugin_ui::views::dispatch(app, label, &json!({ "kind": "ai.event", "runId": run_id, "event": event }));
            }
        }
        Follower::Stream(tx) => {
            let _ = tx.send(event.clone());
        }
    }
}

/// Whether a run's cost so far takes the plugin past a cap.
pub fn over_cap(spent: (f64, f64), cost: f64, caps: (Option<f64>, Option<f64>)) -> bool {
    caps.0.is_some_and(|cap| spent.0 + cost >= cap) || caps.1.is_some_and(|cap| spent.1 + cost >= cap)
}

/// Send `value` to the engine only if it is running (a tool's answer
/// matters only to the engine that asked).
async fn send_to_running(value: Value) {
    let stdin = state().engine.lock().await.as_ref().map(|e| e.stdin.clone());
    let Some(stdin) = stdin else { return };
    let mut stdin = stdin.lock().await;
    let mut line = value.to_string();
    line.push('\n');
    let _ = stdin.write_all(line.as_bytes()).await;
    let _ = stdin.flush().await;
}

async fn send_line(value: Value) {
    let Ok(stdin) = ensure_engine().await else { return };
    let mut stdin = stdin.lock().await;
    let mut line = value.to_string();
    line.push('\n');
    let _ = stdin.write_all(line.as_bytes()).await;
    let _ = stdin.flush().await;
}

/// The installed plugin `id` when it may use AI.
fn allowed_plugin(id: &str) -> Result<(Installed, String), Refusal> {
    let store = store();
    let installed = store.installed().into_iter().find(|i| i.id == id).ok_or(("no_permission", format!("`{id}` is not installed")))?;
    if !installed.enabled {
        return Err(("no_permission", format!("{} is off", installed.name)));
    }
    if !installed.approved_permissions.iter().any(|p| p == "ai") {
        return Err(("no_permission", format!("{} may not use your AI models: allow it on its Permissions tab", installed.name)));
    }
    let name = store.load(&installed).name;
    Ok((installed, name))
}

/// The start of today and of this month, local time, in ms.
fn periods() -> (i64, i64) {
    use chrono::{Datelike, Local, TimeZone};
    let now = Local::now();
    let day = Local.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0).single().map(|d| d.timestamp_millis()).unwrap_or(0);
    let month = Local.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0).single().map(|d| d.timestamp_millis()).unwrap_or(0);
    (day, month)
}

/// What `plugin` spent today and this month.
pub fn spent(plugin: &str) -> (f64, f64) {
    let (day, month) = periods();
    let Ok(conn) = evo_store::open() else { return (0.0, 0.0) };
    (
        evo_store::plugin_ai::spend(&conn, plugin, day).unwrap_or(0.0),
        evo_store::plugin_ai::spend(&conn, plugin, month).unwrap_or(0.0),
    )
}

/// The engine's `allowed`, default model and pins from the person's rules.
pub fn rules(settings: &AiSettings) -> Map<String, Value> {
    let mut out = Map::new();
    let mut allowed = Map::new();
    if let Some(providers) = &settings.providers {
        allowed.insert("providers".into(), json!(providers));
    }
    if let Some(models) = &settings.models {
        allowed.insert("models".into(), json!(models));
    }
    out.insert("allowed".into(), Value::Object(allowed));
    if let Some(model) = &settings.default_model {
        out.insert("defaultModel".into(), json!(model));
    }
    out.insert("pins".into(), json!(settings.tiers));
    out
}

/// The engine's request for a plugin's `body` (what the view or backend
/// asked); only the fields a plugin may set are taken.
pub fn build_request(run_id: &str, plugin_id: &str, plugin_name: &str, settings: &AiSettings, body: &Map<String, Value>) -> Result<Value, Refusal> {
    let messages = body.get("messages").filter(|m| m.as_array().is_some_and(|a| !a.is_empty())).ok_or(("bad_request", "`messages` must be a list with at least one message".to_string()))?;
    let mut request = rules(settings);
    request.insert("type".into(), json!("ai_run"));
    request.insert("runId".into(), json!(run_id));
    request.insert("plugin".into(), json!({ "id": plugin_id, "name": plugin_name }));
    request.insert("messages".into(), messages.clone());
    for key in ["model", "need", "budget", "system", "schema", "thread", "maxSteps"] {
        if let Some(value) = body.get(key).filter(|v| !v.is_null()) {
            request.insert(key.into(), value.clone());
        }
    }
    if let Some(thread) = request.get("thread").and_then(Value::as_str) {
        if thread.is_empty() || thread.chars().count() > 128 {
            return Err(("bad_request", "`thread` must have 1 to 128 characters".into()));
        }
    }
    let value = Value::Object(request);
    if value.to_string().len() > MAX_REQUEST_BYTES {
        return Err(("bad_request", format!("the request is larger than {} MB", MAX_REQUEST_BYTES / 1024 / 1024)));
    }
    Ok(value)
}

/// Runs going now: `(this plugin's, all)`.
fn running(plugin: &str) -> (usize, usize) {
    let runs = state().runs.lock().unwrap_or_else(|e| e.into_inner());
    (runs.values().filter(|r| r.plugin_id == plugin).count(), runs.len())
}

/// Start a run of `plugin_id` for `follower`; its id.
pub async fn start_run(plugin_id: &str, body: Map<String, Value>, follower: Follower) -> Result<String, Refusal> {
    touch();
    let id = plugin_id.to_string();
    let (installed, name) = tauri::async_runtime::spawn_blocking(move || allowed_plugin(&id)).await.map_err(|e| ("engine", e.to_string()))??;
    let spent_now = {
        let id = plugin_id.to_string();
        tauri::async_runtime::spawn_blocking(move || spent(&id)).await.unwrap_or((0.0, 0.0))
    };
    let caps = (installed.ai.cap_day_usd, installed.ai.cap_month_usd);
    if over_cap(spent_now, 0.0, caps) {
        return Err(("budget", format!("{name} reached its AI budget; you can change it on its Models tab")));
    }
    let waited = Instant::now();
    loop {
        let (mine, all) = running(plugin_id);
        if mine < PER_PLUGIN && all < OVERALL {
            break;
        }
        if waited.elapsed() >= QUEUE_WAIT {
            return Err(("busy", format!("{name} has {mine} AI runs going; try again when one ends")));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let run_id = format!("r{}-{}", crate::supervisor::now_ms(), state().next.fetch_add(1, Ordering::Relaxed));
    let mut request = build_request(&run_id, plugin_id, &name, &installed.ai, &body)?;
    let server_tools = APP.get().map(|app| tauri::Manager::state::<crate::mcp::McpState>(app).0.chat_tools(None)).unwrap_or_default();
    let view = match &follower {
        Follower::View(label) => Some(label.clone()),
        Follower::Stream(_) => None,
    };
    let (tools, mut routes) = tools_for(&body, view.is_some(), &name, &server_tools)?;
    let mut apps: Option<AppsRun> = None;
    if !tools.is_empty() {
        request["tools"] = Value::Array(tools);
    }
    let asked = context::asked(body.get("context"), &installed.approved_permissions)?;
    let groups = evo_groups(&body, &installed.approved_permissions)?;
    let window = APP.get().and_then(|app| view.as_ref().and_then(|label| tauri::Manager::get_webview(app, label))).map_or_else(|| "main".to_string(), |w| w.window().label().to_string());
    if let Some(app) = APP.get().filter(|_| !groups.is_empty()) {
        let folder = match body.get("folder").and_then(Value::as_str) {
            Some(folder) if std::path::Path::new(folder).is_dir() => std::path::PathBuf::from(folder),
            Some(folder) => return Err(("bad_request", format!("{folder} is not a folder"))),
            None => match crate::plugin_ui::bridge::active_chat(app, &window) {
                Some(chat) => crate::agent_tools::chat_folder(app, &chat),
                None => std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(std::path::PathBuf::from).unwrap_or_else(|| ".".into()),
            },
        };
        let db = tauri::Manager::state::<crate::db::Db>(app);
        if groups.iter().any(|g| g == "apps") {
            // evo's app tools run here; the engine relays the model's calls.
            let mut offered = request["tools"].as_array().cloned().unwrap_or_default();
            for definition in crate::agent_tools::plugin_run_tools(app) {
                let name = definition["name"].as_str().unwrap_or_default().to_string();
                offered.retain(|t| t["name"] != name.as_str());
                routes.insert(name, ToolRoute::App);
                offered.push(json!({ "name": definition["name"], "description": definition["description"], "inputSchema": definition["inputSchema"], "origin": "app" }));
            }
            request["tools"] = Value::Array(offered);
            apps = Some(AppsRun { plugin_name: name.clone(), folder: folder.clone(), stop: tokio_util::sync::CancellationToken::new() });
        }
        request["evoTools"] = json!(groups);
        request["folder"] = json!(folder.to_string_lossy());
        request["approvalMode"] = json!(crate::engine::approval_mode(&db));
        request["sandbox"] = crate::engine::sandbox_config(&db);
    }
    if let Some(app) = APP.get().filter(|_| asked != context::Asked::default()) {
        let skills = match &asked.skill {
            Some(_) => {
                let installed = installed.clone();
                tauri::async_runtime::spawn_blocking(move || store().load(&installed).skills).await.unwrap_or_default()
            }
            None => Vec::new(),
        };
        if let Some(attached) = context::gather(app, asked, &installed.approved_permissions, &skills, &name, &window).await? {
            request["context"] = attached;
        }
    }
    let thread = body.get("thread").and_then(Value::as_str).map(str::to_string);
    {
        let mut runs = state().runs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(thread) = &thread {
            if runs.values().any(|r| r.plugin_id == plugin_id && r.thread.as_ref() == Some(thread)) {
                return Err(("busy", format!("thread `{thread}` has a run going; follow it or wait for its end")));
            }
        }
        runs.insert(
            run_id.clone(),
            RunEntry { plugin_id: plugin_id.to_string(), view, thread, routes, followers: vec![follower], started: Instant::now(), spent: spent_now, caps, cost: 0.0, model: None, cap_hit: false, apps },
        );
    }
    if let Err(error) = ensure_engine().await {
        state().runs.lock().unwrap_or_else(|e| e.into_inner()).remove(&run_id);
        return Err(error);
    }
    send_line(request).await;
    Ok(run_id)
}

/// Stop a run of `plugin_id` (a plugin may stop only its own).
pub async fn cancel(plugin_id: Option<&str>, run_id: &str) -> Result<(), Refusal> {
    let known = state().runs.lock().unwrap_or_else(|e| e.into_inner()).get(run_id).map(|r| r.plugin_id.clone());
    match (known, plugin_id) {
        (None, _) => Err(("not_found", "that run is not going".into())),
        (Some(owner), Some(caller)) if owner != caller => Err(("not_found", "that run is not going".into())),
        _ => {
            send_line(json!({ "type": "ai_cancel", "runId": run_id })).await;
            Ok(())
        }
    }
}

/// Follow a run: live when it is going (the answer says so); its kept
/// result and events when it ended.
pub fn follow(plugin_id: &str, run_id: &str, follower: Follower) -> Result<Value, Refusal> {
    {
        let mut runs = state().runs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = runs.get_mut(run_id).filter(|r| r.plugin_id == plugin_id) {
            entry.followers.push(follower);
            return Ok(json!({ "live": true }));
        }
    }
    let conn = evo_store::open().map_err(|e| ("engine", e.to_string()))?;
    let (run, result, events) = evo_store::plugin_ai::run_detail(&conn, plugin_id, run_id).map_err(|e| ("engine", e.to_string()))?.ok_or(("not_found", "no such run".to_string()))?;
    let parse = |text: Option<String>| text.and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or(Value::Null);
    Ok(json!({ "live": false, "run": run, "result": parse(result), "events": parse(events) }))
}

/// A plugin's recent runs, the going ones marked.
pub fn runs(plugin_id: &str) -> Value {
    let going: Vec<Value> = state()
        .runs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(_, r)| r.plugin_id == plugin_id)
        .map(|(id, r)| json!({ "id": id, "model": r.model, "costUsd": r.cost, "seconds": r.started.elapsed().as_secs() }))
        .collect();
    let recent = evo_store::open().ok().and_then(|conn| evo_store::plugin_ai::runs(&conn, plugin_id, 20).ok()).unwrap_or_default();
    json!({ "going": going, "recent": recent })
}

/// A tool's answer in a thread as the person reads it, at most.
const THREAD_RESULT: usize = 2_000;

/// A thread's kept messages as a conversation to read (FR-19): the person's
/// and the plugin's words, the model's answers with the tools they called
/// and what came back. Reasoning and images are not kept.
pub fn thread_view(rows: &[evo_store::plugin_ai::ThreadMessage]) -> Vec<Value> {
    let text_of = |content: &Value| -> String {
        match content {
            Value::String(text) => text.clone(),
            Value::Array(blocks) => blocks
                .iter()
                .filter_map(|b| match b["type"].as_str() {
                    Some("text") => b["text"].as_str().map(str::to_string),
                    Some("image") => Some("[image]".into()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        }
    };
    let mut out: Vec<Value> = Vec::new();
    for row in rows {
        let Ok(message) = serde_json::from_str::<Value>(&row.content_json) else { continue };
        match row.role.as_str() {
            "user" => out.push(json!({ "role": "user", "text": text_of(&message["content"]), "at": row.at_ms })),
            "assistant" => {
                let tools: Vec<Value> = message["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|b| b["type"] == "toolCall")
                    .map(|b| json!({ "id": b["id"], "name": b["name"], "input": b["arguments"], "result": Value::Null, "isError": false }))
                    .collect();
                out.push(json!({ "role": "assistant", "text": text_of(&message["content"]), "tools": tools, "provider": row.provider_id, "model": row.model_id, "at": row.at_ms }));
            }
            "toolResult" => {
                let id = &message["toolCallId"];
                let result: String = text_of(&message["content"]).chars().take(THREAD_RESULT).collect();
                let is_error = message["isError"] == true;
                if let Some(tool) = out.iter_mut().rev().filter(|m| m["role"] == "assistant").find_map(|m| m["tools"].as_array_mut().and_then(|tools| tools.iter_mut().find(|t| &t["id"] == id))) {
                    tool["result"] = json!(result);
                    tool["isError"] = json!(is_error);
                }
            }
            _ => {}
        }
    }
    out
}

fn thread_going(plugin_id: &str, thread: &str) -> bool {
    state().runs.lock().unwrap_or_else(|e| e.into_inner()).values().any(|r| r.plugin_id == plugin_id && r.thread.as_deref() == Some(thread))
}

/// A plugin's threads, most recently used first (FR-18, FR-19).
pub async fn threads(plugin_id: &str) -> Result<Value, Refusal> {
    let id = plugin_id.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = evo_store::open().map_err(|e| ("engine", e.to_string()))?;
        let list = evo_store::plugin_ai::threads(&conn, &id).map_err(|e| ("engine", e.to_string()))?;
        Ok(json!(list.into_iter().map(|t| { let going = thread_going(&id, &t.id); json!({ "id": t.id, "title": t.title, "createdAt": t.created_at_ms, "updatedAt": t.updated_at_ms, "messages": t.messages, "going": going }) }).collect::<Vec<_>>()))
    })
    .await
    .map_err(|e| ("engine", e.to_string()))?
}

/// One thread of a plugin as a conversation to read.
pub async fn thread(plugin_id: &str, thread_id: &str) -> Result<Value, Refusal> {
    let (id, thread) = (plugin_id.to_string(), thread_id.to_string());
    tauri::async_runtime::spawn_blocking(move || {
        let conn = evo_store::open().map_err(|e| ("engine", e.to_string()))?;
        let info = evo_store::plugin_ai::threads(&conn, &id).map_err(|e| ("engine", e.to_string()))?.into_iter().find(|t| t.id == thread).ok_or(("not_found", format!("no thread `{thread}`")))?;
        let rows = evo_store::plugin_ai::thread_messages(&conn, &id, &thread, 2_000).map_err(|e| ("engine", e.to_string()))?;
        Ok(json!({ "id": info.id, "title": info.title, "createdAt": info.created_at_ms, "updatedAt": info.updated_at_ms, "going": thread_going(&id, &thread), "messages": thread_view(&rows) }))
    })
    .await
    .map_err(|e| ("engine", e.to_string()))?
}

/// Delete a plugin's thread (not while a run continues it).
pub async fn delete_thread(plugin_id: &str, thread_id: &str) -> Result<(), Refusal> {
    if thread_going(plugin_id, thread_id) {
        return Err(("busy", format!("thread `{thread_id}` has a run going")));
    }
    let (id, thread) = (plugin_id.to_string(), thread_id.to_string());
    let found = tauri::async_runtime::spawn_blocking(move || {
        let conn = evo_store::open().map_err(|e| ("engine", e.to_string()))?;
        evo_store::plugin_ai::delete_thread(&conn, &id, &thread).map_err(|e| ("engine", e.to_string()))
    })
    .await
    .map_err(|e| ("engine", e.to_string()))??;
    if !found {
        return Err(("not_found", format!("no thread `{thread_id}`")));
    }
    if let Some(app) = APP.get() {
        let _ = app.emit("plugin-ai://runs", ());
    }
    Ok(())
}

/// The models a plugin may use (`all`: every connected one, for the person
/// choosing on the Models tab).
pub async fn models(plugin_id: &str, all: bool) -> Result<Value, Refusal> {
    touch();
    let id = plugin_id.to_string();
    let settings = if all {
        AiSettings::default()
    } else {
        tauri::async_runtime::spawn_blocking(move || allowed_plugin(&id)).await.map_err(|e| ("engine", e.to_string()))??.0.ai
    };
    let stdin = ensure_engine().await?;
    let request_id = format!("m{}", state().next.fetch_add(1, Ordering::Relaxed));
    let (tx, rx) = oneshot::channel();
    state().pending.lock().unwrap_or_else(|e| e.into_inner()).insert(request_id.clone(), tx);
    let line = json!({ "type": "ai_models", "id": request_id, "allowed": rules(&settings)["allowed"] }).to_string() + "\n";
    {
        let mut stdin = stdin.lock().await;
        let _ = stdin.write_all(line.as_bytes()).await;
        let _ = stdin.flush().await;
    }
    let answer = tokio::time::timeout(Duration::from_secs(30), rx).await.map_err(|_| ("engine", "the AI engine did not answer".to_string()))?.map_err(|_| ("engine", "the AI engine stopped".to_string()))?;
    if let Some(error) = answer["error"].as_str() {
        return Err(("engine", error.to_string()));
    }
    Ok(answer["data"].clone())
}

/// Credentials or providers changed: the engine reads them again.
pub fn reload_providers() {
    tauri::async_runtime::spawn(async {
        if state().engine.lock().await.is_some() {
            send_line(json!({ "type": "reload_providers" })).await;
        }
    });
}

/// Stop the runs of plugins `keep` refuses (turned off, `ai` taken back,
/// uninstalled).
pub fn retain(keep: impl Fn(&str) -> bool) {
    let stop: Vec<String> = state().runs.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|(_, r)| !keep(&r.plugin_id)).map(|(id, _)| id.clone()).collect();
    for run_id in stop {
        tauri::async_runtime::spawn(async move { send_line(json!({ "type": "ai_cancel", "runId": run_id })).await });
    }
}

// ── Commands (the plugin page's Models tab) ────────────────────────────────────

#[tauri::command]
pub async fn plugin_ai_state(id: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let installed = store().installed().into_iter().find(|i| i.id == id).ok_or("not installed")?;
        let (day, month) = periods();
        let conn = evo_store::open().map_err(|e| e.to_string())?;
        let spend = |since| evo_store::plugin_ai::spend(&conn, &id, since).unwrap_or(0.0);
        let (calls, input, output) = evo_store::plugin_ai::calls(&conn, &id, month).unwrap_or((0, 0, 0));
        Ok(json!({
            "settings": installed.ai,
            "spentToday": spend(day),
            "spentMonth": spend(month),
            "monthCalls": calls, "monthInput": input, "monthOutput": output,
            "runs": runs(&id),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn plugin_ai_settings_save(app: AppHandle, id: String, settings: AiSettings) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || store().edit(&id, |installed| installed.ai = settings).map(|_| ())).await.map_err(|e| e.to_string())??;
    let _ = app.emit("plugins://changed", ());
    Ok(())
}

#[tauri::command]
pub async fn plugin_ai_models(id: String) -> Result<Value, String> {
    models(&id, true).await.map_err(|(_, message)| message)
}

#[tauri::command]
pub async fn plugin_ai_cancel(run_id: String) -> Result<(), String> {
    cancel(None, &run_id).await.map_err(|(_, message)| message)
}

#[tauri::command]
pub fn plugin_ai_questions() -> Vec<Value> {
    state().questions.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub async fn plugin_ai_answer(id: String, index: i32) -> Result<(), String> {
    let found = state().questions.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|q| q["id"] == id.as_str());
    if !found {
        return Err("that question is no longer open".into());
    }
    questions_closed(|q| q["id"] == id.as_str());
    let local = state().local.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    match local {
        Some(tx) => {
            let _ = tx.send(index);
        }
        None => send_to_running(json!({ "type": "ai_answer", "id": id, "index": index })).await,
    }
    Ok(())
}

#[tauri::command]
pub async fn plugin_ai_threads(id: String) -> Result<Value, String> {
    threads(&id).await.map_err(|(_, message)| message)
}

#[tauri::command]
pub async fn plugin_ai_thread(id: String, thread: String) -> Result<Value, String> {
    self::thread(&id, &thread).await.map_err(|(_, message)| message)
}

#[tauri::command]
pub async fn plugin_ai_thread_delete(id: String, thread: String) -> Result<(), String> {
    delete_thread(&id, &thread).await.map_err(|(_, message)| message)
}
