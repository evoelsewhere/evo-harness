//! The plugin host API (docs/features/plugin-settings/spec.md, FR-6, FR-26,
//! NFR-1; plan 3.3 "Host API"; ADR 0022): a loopback HTTP server on a
//! random port where a plugin's running code — its backend, its hooks — asks
//! evo for an account's token or cookies, its settings, or to log a line.
//!
//! Each caller holds a bearer key that names one plugin: one key for its
//! backend and one for its hooks, made on first need and kept while the app
//! runs (a backend's key in its config, so a new key would restart it). A key
//! stops working when its plugin is turned off or removed, or loses the
//! permission its part needs. Keys reach backends in their environment and
//! hooks through the engine's `set_plugin_host` — never the engine's own
//! environment, which the agent's shell commands inherit.
//!
//! Tokens and cookies need the `secrets` permission; cookies only for the
//! account's declared domains. Nothing here logs a secret: `/v1/log` lines
//! have the plugin's vault values hidden.

pub(crate) mod http;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use evo_plugins::accounts::{Account, Method};
use evo_plugins::store::AccountRecord;
use evo_plugins::{Installed, Plugin};
use evo_vault::{account_name, setting_name};
use serde_json::{json, Map, Value};

/// Calls one key may make in [`RATE_WINDOW`].
const RATE_LIMIT: u32 = 200;
const RATE_WINDOW: Duration = Duration::from_secs(10);
/// The longest `/v1/log` message kept.
const LOG_CHARS: usize = 4000;

/// Which part of a plugin holds a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Caller {
    Backend,
    Hooks,
}

impl Caller {
    /// The permission the part runs under.
    fn permission(self) -> &'static str {
        match self {
            Caller::Backend => "runtime",
            Caller::Hooks => "hooks",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Grant {
    plugin: String,
    caller: Caller,
}

/// What the host reads and does outside itself (the shell's store, vaults
/// and logs; a fake in tests).
pub trait Backing: Send + Sync {
    /// Plugin `id` with its parts gated, when it is installed and on.
    fn plugin(&self, id: &str) -> Option<(Installed, Plugin)>;
    /// The plugin's vault.
    fn secrets(&self, id: &str) -> Result<Map<String, Value>, String>;
    fn log(&self, plugin: &str, level: &str, message: &str);
    /// The plugin's code says `account`'s sign-in `record` was refused.
    fn signed_out(&self, id: &str, account: &Account, record: Option<&str>);
    /// Sign-in `record`'s session, renewed first when it is due (FR-25);
    /// `kept` is what the vault holds now.
    fn fresh(&self, _plugin: &str, _account: &Account, _record: &str, kept: Option<Map<String, Value>>) -> Result<Map<String, Value>, crate::plugin_auth::oauth::Failure> {
        kept.ok_or_else(|| crate::plugin_auth::oauth::Failure::Refused("not signed in".into()))
    }
}

/// One request, as [`Host::handle`] reads it.
#[derive(Debug, Default)]
pub struct Request {
    pub method: String,
    /// Without the query.
    pub path: String,
    pub query: HashMap<String, String>,
    /// The bearer key, if any.
    pub key: Option<String>,
    /// A browser page's request.
    pub origin: bool,
    pub body: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: Option<Value>,
}

fn ok(body: Value) -> Response {
    Response { status: 200, body: Some(body) }
}

fn no_content() -> Response {
    Response { status: 204, body: None }
}

fn error(status: u16, code: &str, message: impl Into<String>) -> Response {
    Response { status, body: Some(json!({ "code": code, "message": message.into() })) }
}

/// The keys and the port.
#[derive(Default)]
pub struct Host {
    keys: Mutex<HashMap<String, Grant>>,
    calls: Mutex<HashMap<String, (Instant, u32)>>,
    port: OnceLock<u16>,
}

static HOST: LazyLock<Host> = LazyLock::new(Host::default);

/// The app's host.
pub fn host() -> &'static Host {
    &HOST
}

impl Host {
    /// `http://127.0.0.1:<port>`, once it listens.
    pub fn url(&self) -> Option<String> {
        self.port().map(|port| format!("http://127.0.0.1:{port}"))
    }

    pub fn port(&self) -> Option<u16> {
        self.port.get().copied()
    }

    /// The key of `plugin`'s `caller`, made on first need.
    pub fn key(&self, plugin: &str, caller: Caller) -> Result<String, String> {
        let mut keys = self.keys.lock().unwrap_or_else(|e| e.into_inner());
        let wanted = Grant { plugin: plugin.to_string(), caller };
        if let Some((key, _)) = keys.iter().find(|(_, grant)| **grant == wanted) {
            return Ok(key.clone());
        }
        let key = evo_vault::random_key()?;
        keys.insert(key.clone(), wanted);
        Ok(key)
    }

    /// Keep only the keys `keep` allows (plugins still on, parts still approved).
    pub fn retain(&self, keep: impl Fn(&str, Caller) -> bool) {
        let mut keys = self.keys.lock().unwrap_or_else(|e| e.into_inner());
        keys.retain(|_, grant| keep(&grant.plugin, grant.caller));
        let live: Vec<String> = keys.keys().cloned().collect();
        drop(keys);
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).retain(|key, _| live.contains(key));
    }

    /// The plugin and part a key names (plugin AI's async routes).
    pub fn grant_of(&self, key: &str) -> Option<(String, Caller)> {
        self.grant(key).map(|grant| (grant.plugin, grant.caller))
    }

    /// Count a call of `key` against its rate.
    pub fn admit_key(&self, key: &str) -> bool {
        self.admit(key, Instant::now())
    }

    fn grant(&self, key: &str) -> Option<Grant> {
        self.keys.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned()
    }

    /// Count a call of `key`; false once it made too many.
    fn admit(&self, key: &str, now: Instant) -> bool {
        let mut calls = self.calls.lock().unwrap_or_else(|e| e.into_inner());
        let entry = calls.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= RATE_WINDOW {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= RATE_LIMIT
    }

    /// Answer one request.
    pub fn handle(&self, backing: &dyn Backing, request: &Request) -> Response {
        if request.origin {
            return error(403, "no_permission", "browser pages may not call the plugin host");
        }
        let Some(key) = request.key.as_deref() else {
            return error(401, "unauthorized", "send `Authorization: Bearer $EVO_PLUGIN_KEY`");
        };
        let Some(grant) = self.grant(key) else {
            return error(401, "unauthorized", "this key is not valid (the plugin was turned off, removed or changed)");
        };
        if !self.admit(key, Instant::now()) {
            return error(429, "rate_limited", "too many calls; wait a few seconds");
        }
        let Some((installed, plugin)) = backing.plugin(&grant.plugin) else {
            return error(401, "unauthorized", "the plugin is off or removed");
        };
        let approved = |p: &str| installed.approved_permissions.iter().any(|a| a == p);
        if !approved(grant.caller.permission()) {
            return error(403, "no_permission", format!("the plugin may not run its {} now", grant.caller.permission()));
        }
        let call = Call { backing, installed: &installed, plugin: &plugin, request };
        let parts: Vec<&str> = request.path.trim_matches('/').split('/').collect();
        match (request.method.as_str(), parts.as_slice()) {
            ("GET", ["v1", "accounts"]) => call.accounts(),
            ("GET", ["v1", "accounts", id, "token"]) => call.with_account(id, |account| call.token(account)),
            ("GET", ["v1", "accounts", id, "cookies"]) => call.with_account(id, |account| call.cookies(account)),
            ("POST", ["v1", "accounts", id, "signed-out"]) => call.with_account(id, |account| call.signed_out(account)),
            ("GET", ["v1", "settings"]) => call.settings(),
            ("POST", ["v1", "log"]) => call.log(),
            _ => error(404, "not_found", format!("no {} {}", request.method, request.path)),
        }
    }
}

/// One authorised request of one plugin.
struct Call<'a> {
    backing: &'a dyn Backing,
    installed: &'a Installed,
    plugin: &'a Plugin,
    request: &'a Request,
}

impl Call<'_> {
    fn workspace(&self) -> Option<&str> {
        self.request.query.get("workspace").map(String::as_str).filter(|w| !w.is_empty())
    }

    fn approved(&self, permission: &str) -> bool {
        self.installed.approved_permissions.iter().any(|a| a == permission)
    }

    fn secrets(&self) -> Result<Map<String, Value>, Response> {
        if !self.approved("secrets") {
            return Err(error(403, "no_permission", "the plugin was not allowed its secrets"));
        }
        self.backing
            .secrets(&self.plugin.id)
            .map_err(|e| error(503, "vault_unavailable", format!("the plugin's secrets cannot be read: {e}")))
    }

    fn with_account(&self, id: &str, then: impl FnOnce(&Account) -> Response) -> Response {
        match self.plugin.accounts.iter().find(|a| a.id == *id) {
            Some(account) => then(account),
            None => error(404, "unknown_account", format!("{} declares no account `{id}`", self.plugin.name)),
        }
    }

    /// The sign-in used for `account` here: the workspace's own, else the
    /// default, else the first.
    fn record(&self, account: &str) -> Option<&AccountRecord> {
        let mine = || self.installed.accounts.iter().filter(move |r| r.account == account);
        self.workspace()
            .and_then(|w| mine().find(|r| r.workspaces.iter().any(|x| x == w)))
            .or_else(|| mine().find(|r| r.default))
            .or_else(|| mine().next())
    }

    /// The session kept for a signed-in record.
    fn session(&self, account: &Account, secrets: &Map<String, Value>) -> Result<Map<String, Value>, Response> {
        let not_signed_in = || error(409, "not_signed_in", format!("{} is not signed in to {}", self.plugin.name, account.service));
        let record = self.record(&account.id).ok_or_else(not_signed_in)?;
        if record.state == "needsSignIn" {
            return Err(not_signed_in());
        }
        let kept = secrets.get(&account_name(&record.id)).and_then(Value::as_object).cloned();
        match self.backing.fresh(&self.plugin.id, account, &record.id, kept) {
            Ok(session) => Ok(session),
            Err(crate::plugin_auth::oauth::Failure::Refused(_)) => Err(not_signed_in()),
            Err(crate::plugin_auth::oauth::Failure::Offline(why)) => Err(error(503, "offline", format!("{} cannot be reached now: {why}", account.service))),
        }
    }

    fn accounts(&self) -> Response {
        let list: Vec<Value> = self
            .plugin
            .accounts
            .iter()
            .map(|account| {
                let (state, label) = match &account.method {
                    Method::Token { setting, .. } => {
                        let set = self.installed.secrets_set.iter().any(|s| s == setting || s.starts_with(&format!("{setting}@")));
                        (if set { "signedIn" } else { "signedOut" }.to_string(), String::new())
                    }
                    _ => match self.record(&account.id) {
                        Some(record) => (record.state.clone(), record.label.clone()),
                        None => ("signedOut".into(), String::new()),
                    },
                };
                json!({ "id": account.id, "service": account.service, "state": state, "label": label })
            })
            .collect();
        ok(Value::Array(list))
    }

    /// An account's access token. Renewing it ahead of expiry comes with
    /// sign-in (FR-25); until then the caller gets what is kept.
    fn token(&self, account: &Account) -> Response {
        let secrets = match self.secrets() {
            Ok(secrets) => secrets,
            Err(response) => return response,
        };
        match &account.method {
            Method::Token { setting, .. } => {
                let value = self
                    .workspace()
                    .and_then(|w| secrets.get(&setting_name(setting, Some(w))))
                    .or_else(|| secrets.get(&setting_name(setting, None)))
                    .and_then(Value::as_str);
                match value {
                    Some(token) => ok(json!({ "accessToken": token, "expiresAt": null })),
                    None => error(409, "not_signed_in", format!("{} has no {} key set", self.plugin.name, account.service)),
                }
            }
            Method::Oauth(_) | Method::Device(_) => match self.session(account, &secrets) {
                Ok(session) => match session.get("accessToken").and_then(Value::as_str) {
                    Some(token) => ok(json!({ "accessToken": token, "expiresAt": session.get("expiresAt").cloned().unwrap_or(Value::Null) })),
                    None => error(409, "not_signed_in", format!("{} is not signed in to {}", self.plugin.name, account.service)),
                },
                Err(response) => response,
            },
            Method::Web(_) => error(400, "bad_request", "a web sign-in keeps cookies, not a token: ask /cookies?url=…"),
        }
    }

    /// The `Cookie` header for `?url=` from a web sign-in, for its declared
    /// domains only.
    fn cookies(&self, account: &Account) -> Response {
        let Method::Web(web) = &account.method else {
            return error(400, "bad_request", "only a web sign-in keeps cookies: ask /token");
        };
        let Some(url) = self.request.query.get("url") else {
            return error(400, "bad_request", "`url` is required");
        };
        let Some((host, path)) = host_and_path(url) else {
            return error(400, "bad_request", "`url` must be an https URL");
        };
        if !web.domains.iter().any(|d| domain_matches(&host, d)) {
            return error(403, "no_permission", format!("{host} is not one of {}'s domains", account.service));
        }
        let secrets = match self.secrets() {
            Ok(secrets) => secrets,
            Err(response) => return response,
        };
        let session = match self.session(account, &secrets) {
            Ok(session) => session,
            Err(response) => return response,
        };
        let cookies = session.get("cookies").and_then(Value::as_array).cloned().unwrap_or_default();
        let header: Vec<String> = cookies
            .iter()
            .filter(|cookie| {
                let domain = cookie.get("domain").and_then(Value::as_str).unwrap_or_default();
                let at = cookie.get("path").and_then(Value::as_str).unwrap_or("/");
                domain_matches(&host, domain) && path.starts_with(at)
            })
            .filter_map(|cookie| Some(format!("{}={}", cookie.get("name")?.as_str()?, cookie.get("value")?.as_str()?)))
            .collect();
        ok(json!({ "header": header.join("; ") }))
    }

    fn signed_out(&self, account: &Account) -> Response {
        let record = self.record(&account.id).map(|r| r.id.clone());
        self.backing.signed_out(&self.plugin.id, account, record.as_deref());
        no_content()
    }

    /// Plain settings, the workspace's own where it set them (no secrets).
    fn settings(&self) -> Response {
        let workspace = self.workspace().and_then(|w| self.installed.workspace_config.get(w));
        let values = evo_plugins::settings::effective(&self.plugin.settings, &self.installed.config, workspace);
        ok(json!({ "values": values }))
    }

    fn log(&self) -> Response {
        let Ok(body) = serde_json::from_slice::<Value>(&self.request.body) else {
            return error(400, "bad_request", "send JSON: {\"level\": \"info\", \"message\": \"…\"}");
        };
        let Some(message) = body.get("message").and_then(Value::as_str) else {
            return error(400, "bad_request", "`message` is required");
        };
        let level = body
            .get("level")
            .and_then(Value::as_str)
            .filter(|level| matches!(*level, "debug" | "info" | "warn" | "error"))
            .unwrap_or("info");
        let mut message: String = message.chars().take(LOG_CHARS).collect();
        // Never log a secret the plugin echoes (NFR-1).
        if let Ok(secrets) = self.backing.secrets(&self.plugin.id) {
            message = hide(&message, &secrets);
        }
        self.backing.log(&self.plugin.name, level, &message);
        no_content()
    }
}

/// `text` with every string of `secrets` (4 characters or more) hidden.
pub fn hide(text: &str, secrets: &Map<String, Value>) -> String {
    fn strings<'v>(value: &'v Value, out: &mut Vec<&'v str>) {
        match value {
            Value::String(s) if s.chars().count() >= 4 => out.push(s),
            Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
            Value::Object(map) => map.values().for_each(|v| strings(v, out)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    secrets.values().for_each(|v| strings(v, &mut found));
    found.sort_by_key(|s| std::cmp::Reverse(s.len()));
    found.into_iter().fold(text.to_string(), |text, secret| text.replace(secret, "(hidden)"))
}

/// The lowercase host and the path of an https URL (http on loopback).
fn host_and_path(url: &str) -> Option<(String, String)> {
    if !evo_plugins::accounts::acceptable_url(url) {
        return None;
    }
    let rest = url.split_once("://")?.1;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if authority.starts_with('[') {
        authority.split(']').next()?.trim_start_matches('[').to_string()
    } else {
        authority.split(':').next()?.to_string()
    };
    let path = rest[end..].split(['?', '#']).next().unwrap_or_default();
    Some((host.to_ascii_lowercase(), if path.is_empty() { "/".into() } else { path.to_string() }))
}

/// `host` is `domain` or one of its subdomains (a cookie's leading dot ignored).
fn domain_matches(host: &str, domain: &str) -> bool {
    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
    !domain.is_empty() && (host == domain || host.ends_with(&format!(".{domain}")))
}

/// Listen on a random loopback port and answer there (once per app run).
pub fn start(backing: Arc<dyn Backing>) -> Result<u16, String> {
    if let Some(port) = host().port() {
        return Ok(port);
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| format!("the plugin host could not listen: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let _ = host().port.set(port);
    tauri::async_runtime::spawn(async move {
        match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => http::serve(listener, host(), backing).await,
            Err(e) => eprintln!("[warn] plugin host: {e}"),
        }
    });
    Ok(port)
}

/// `set_plugin_host` for an engine: the URL and a key per plugin whose
/// hooks run (FR-26), or `None` before the host listens.
pub fn engine_payload(plugins: &[Plugin]) -> Option<Value> {
    let url = host().url()?;
    let mut keys = Map::new();
    for plugin in plugins.iter().filter(|p| !p.hooks.is_empty()) {
        if let Ok(key) = host().key(&plugin.id, Caller::Hooks) {
            keys.insert(plugin.id.clone(), Value::String(key));
        }
    }
    Some(json!({ "url": url, "keys": keys }))
}

/// The app's [`Backing`]: the plugin store, vaults and Settings → Logs.
pub struct AppBacking(pub tauri::AppHandle);

impl AppBacking {
    fn store() -> evo_plugins::Store {
        evo_plugins::Store::in_agent_dir(&crate::mcp::agent_dir())
    }
}

impl Backing for AppBacking {
    fn plugin(&self, id: &str) -> Option<(Installed, Plugin)> {
        let store = Self::store();
        let installed = store.installed().into_iter().find(|i| i.id == id && i.enabled)?;
        let mut plugin = store.load(&installed);
        evo_plugins::store::gate(&mut plugin, &installed);
        Some((installed, plugin))
    }

    fn secrets(&self, id: &str) -> Result<Map<String, Value>, String> {
        crate::plugin_settings::with_vault(&Self::store(), id, |vault| vault.read())
    }

    fn log(&self, plugin: &str, level: &str, message: &str) {
        use tauri::Manager;
        if let Some(db) = self.0.try_state::<crate::db::Db>() {
            db.log(evo_store::logs::NewLog::new(level, &format!("plugin:{plugin}"), message.to_string()));
        }
    }

    fn signed_out(&self, id: &str, account: &Account, record: Option<&str>) {
        let name = self.plugin(id).map(|(_, p)| p.name).unwrap_or_else(|| id.to_string());
        crate::plugin_auth::needs_sign_in(id, &name, account, record, "its requests were refused");
    }

    fn fresh(&self, plugin: &str, account: &Account, record: &str, _kept: Option<Map<String, Value>>) -> Result<Map<String, Value>, crate::plugin_auth::oauth::Failure> {
        // A blocking thread (the host's handler): wait for the renewal here.
        tauri::async_runtime::block_on(crate::plugin_auth::fresh_session(plugin, account, record)).map(|(session, _)| session)
    }
}
