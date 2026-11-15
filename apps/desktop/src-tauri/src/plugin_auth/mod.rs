//! Plugin sign-in and sessions (docs/features/plugin-settings/spec.md,
//! FR-7, FR-20 – FR-31; ADR 0022).
//!
//! A plugin declares accounts (`evo_plugins::accounts`). Signing in keeps
//! one record per sign-in in `installed.json` (id, label, state, default,
//! workspaces, scopes; never a secret) and its session in the plugin's
//! vault as `account:<record>` (tokens, or cookies). Kinds:
//!
//! - **OAuth**: the person's default browser, PKCE and a state check, a
//!   one-time loopback return ([`oauth`]).
//! - **Device code**: evo shows the code and the address and polls.
//! - **Token**: a secret setting; Test runs the plugin's check tool.
//! - **Web login**: an incognito window ([`web`]); the declared domains'
//!   cookies are kept.
//!
//! One sign-in runs per account at a time; others wait for it (edge 8). A
//! sign-in ends as cancelled after 5 minutes or when its tab or window is
//! closed (edge 7). Tokens are renewed when under 2 minutes remain, one
//! renewal per sign-in; a refusal marks it "needs sign-in again" and tells
//! the person, a network failure keeps it (edges 9, 10).

pub mod oauth;
#[cfg(test)]
mod tests;
mod web;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use evo_plugins::accounts::{Account, Method, OAuth};
use evo_plugins::store::AccountRecord;
use evo_plugins::{Installed, Plugin, Store};
use evo_vault::{account_name, setting_name};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// The longest a sign-in waits for the person.
const LIMIT: Duration = Duration::from_secs(5 * 60);

static APP: OnceLock<AppHandle> = OnceLock::new();

type Outcome = Option<Result<String, String>>;
/// A sign-in's work once started.
type Flow = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Signed, String>> + Send>>;

/// A sign-in under way.
#[derive(Clone)]
struct Flight {
    cancel: CancellationToken,
    done: watch::Receiver<Outcome>,
    /// What the window shows: `{flow, device?}`.
    start: Value,
}

#[derive(Default)]
struct State {
    flights: Mutex<HashMap<String, Flight>>,
    renewals: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

static STATE: OnceLock<State> = OnceLock::new();

fn state() -> &'static State {
    STATE.get_or_init(State::default)
}

fn store() -> Store {
    Store::in_agent_dir(&crate::mcp::agent_dir())
}

fn now_ms() -> u64 {
    crate::supervisor::now_ms()
}

/// Keep the app for events and notices; renew connector servers' tokens.
pub fn setup(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            renew_for_servers(&app).await;
        }
    });
}

fn emit(id: &str, account: &str, state: &str, error: Option<&str>) {
    if let Some(app) = APP.get() {
        let _ = app.emit("plugins://accounts", json!({ "id": id, "account": account, "state": state, "error": error }));
        let _ = app.emit("plugins://changed", ());
    }
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())?
}

/// The installed plugin `id` and its manifest.
fn find(store: &Store, id: &str) -> Result<(Installed, Plugin), String> {
    let installed = store.installed().into_iter().find(|i| i.id == id).ok_or_else(|| format!("`{id}` is not installed"))?;
    let plugin = store.load(&installed);
    Ok((installed, plugin))
}

fn account<'p>(plugin: &'p Plugin, account: &str) -> Result<&'p Account, String> {
    plugin.accounts.iter().find(|a| a.id == account).ok_or_else(|| format!("{} declares no account `{account}`", plugin.name))
}

/// The app's client for `oauth`: the plugin's own id, or the person's from
/// settings (with a secret when the service needs one; edge 11).
fn client_of(store: &Store, installed: &Installed, plugin: &Plugin, oauth: &OAuth) -> Result<oauth::Client, String> {
    let vault = crate::plugin_settings::with_vault(store, &plugin.id, |v| v.read()).unwrap_or_default();
    let value = |key: &str| {
        installed
            .config
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| vault.get(&setting_name(key, None)).and_then(Value::as_str).map(str::to_string))
            .filter(|v| !v.trim().is_empty())
    };
    let id = oauth
        .client_id
        .clone()
        .or_else(|| oauth.client_id_setting.as_deref().and_then(value))
        .ok_or_else(|| format!("{} needs your app's client ID: fill it in Settings", plugin.name))?;
    let secret = oauth.client_secret_setting.as_deref().and_then(value);
    Ok(oauth::Client { id, secret })
}

/// Scopes the manifest asks for that `record` was not granted (FR-31).
pub fn missing_scopes(account: &Account, record: &AccountRecord) -> Vec<String> {
    match &account.method {
        Method::Oauth(o) | Method::Device(o) if !record.scopes.is_empty() => o.scopes.iter().filter(|s| !record.scopes.contains(s)).cloned().collect(),
        _ => Vec::new(),
    }
}

/// Whether `account` is ready to use: a signed-in record, or a token set.
pub fn signed_in(installed: &Installed, account: &Account) -> bool {
    match &account.method {
        Method::Token { setting, .. } => installed.secrets_set.iter().any(|s| s == setting || s.starts_with(&format!("{setting}@"))),
        _ => installed.accounts.iter().any(|r| r.account == account.id && r.state == "signedIn"),
    }
}

/// The accounts of a plugin as views and the page see them: no secret.
pub fn status(installed: &Installed, plugin: &Plugin) -> Vec<Value> {
    plugin
        .accounts
        .iter()
        .map(|account| {
            let records: Vec<Value> = installed
                .accounts
                .iter()
                .filter(|r| r.account == account.id)
                .map(|r| {
                    json!({
                        "id": r.id, "label": r.label, "state": r.state, "default": r.default,
                        "workspaces": r.workspaces, "scopes": r.scopes, "missingScopes": missing_scopes(account, r),
                    })
                })
                .collect();
            let kind = match &account.method {
                Method::Oauth(_) => "oauth",
                Method::Device(_) => "device",
                Method::Token { .. } => "token",
                Method::Web(_) => "web",
            };
            let state = if signed_in(installed, account) {
                "signedIn"
            } else if installed.accounts.iter().any(|r| r.account == account.id && r.state == "needsSignIn") {
                "needsSignIn"
            } else {
                "signedOut"
            };
            json!({
                "id": account.id, "service": account.service, "kind": kind, "reason": account.reason,
                "optional": account.optional, "state": state, "signIns": records,
                "signingIn": state_of_flight(&plugin.id, &account.id),
            })
        })
        .collect()
}

fn flight_key(id: &str, account: &str) -> String {
    format!("{id}\n{account}")
}

fn state_of_flight(id: &str, account: &str) -> Option<Value> {
    state().flights.lock().unwrap_or_else(|e| e.into_inner()).get(&flight_key(id, account)).map(|f| f.start.clone())
}

/// What a finished flow hands over: the session and the account's name.
struct Signed {
    session: Map<String, Value>,
    label: String,
}

/// Start signing in to `account_id` of plugin `id`, or join the sign-in
/// already running. `{flow: browser | device | window, device?}`.
pub async fn start(id: &str, account_id: &str) -> Result<Value, String> {
    start_flight(id, account_id).await.map(|f| f.start)
}

/// Sign in and wait for the end: the record's id.
pub async fn sign_in_and_wait(id: &str, account_id: &str) -> Result<String, String> {
    let mut done = start_flight(id, account_id).await?.done;
    loop {
        if let Some(outcome) = done.borrow().clone() {
            return outcome;
        }
        if done.changed().await.is_err() {
            return done.borrow().clone().unwrap_or_else(|| Err("cancelled".into()));
        }
    }
}

/// Stop the sign-in of `account_id`.
pub fn cancel(id: &str, account_id: &str) {
    if let Some(flight) = state().flights.lock().unwrap_or_else(|e| e.into_inner()).get(&flight_key(id, account_id)) {
        flight.cancel.cancel();
    }
}

async fn start_flight(id: &str, account_id: &str) -> Result<Flight, String> {
    let key = flight_key(id, account_id);
    if let Some(flight) = state().flights.lock().unwrap_or_else(|e| e.into_inner()).get(&key).cloned() {
        return Ok(flight);
    }
    let (id, account_id) = (id.to_string(), account_id.to_string());
    let (installed, plugin) = {
        let id = id.clone();
        blocking(move || find(&store(), &id)).await?
    };
    if !installed.approved_permissions.iter().any(|p| p == "secrets") {
        return Err(format!("{} may not keep accounts yet: allow its permissions first", plugin.name));
    }
    let account = account(&plugin, &account_id)?.clone();
    let cancel = CancellationToken::new();
    let http = oauth::client()?;
    // What the flow does once started, and what the window shows now.
    let (start, flow): (Value, Flow) = match account.method.clone() {
        Method::Token { .. } => return Err(format!("{} uses a key: enter it in Settings", account.service)),
        Method::Oauth(o) => {
            let client = client_of(&store(), &installed, &plugin, &o)?;
            let (http, cancel) = (http.clone(), cancel.clone());
            (json!({ "flow": "browser" }), Box::pin(async move { oauth_flow(&http, &o, &client, &cancel).await }))
        }
        Method::Device(o) => {
            let client = client_of(&store(), &installed, &plugin, &o)?;
            let endpoints = oauth::endpoints(&http, &o).await?;
            let device_url = endpoints.device.clone().ok_or("the service declares no device address")?;
            let device = oauth::device_start(&http, &device_url, &client, &o.scopes).await?;
            let shown = json!({
                "flow": "device",
                "device": { "code": device.user_code, "url": device.uri, "complete": device.complete, "expiresAt": now_ms() + device.expires_in * 1000 },
            });
            let (http, cancel) = (http.clone(), cancel.clone());
            (shown, Box::pin(async move { device_flow(&http, &o, &endpoints, &client, &device, &cancel).await }))
        }
        Method::Web(w) => {
            let app = APP.get().cloned().ok_or("the app is not ready")?;
            let (service, cancel) = (account.service.clone(), cancel.clone());
            (
                json!({ "flow": "window" }),
                Box::pin(async move {
                    let cookies = web::login(&app, &service, &w, &cancel).await?;
                    let host = tauri::Url::parse(&w.login_url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
                    let mut session = Map::new();
                    session.insert("cookies".into(), Value::Array(cookies));
                    Ok(Signed { session, label: host })
                }),
            )
        }
    };
    let (tx, rx) = watch::channel::<Outcome>(None);
    let flight = Flight { cancel: cancel.clone(), done: rx, start };
    {
        let mut flights = state().flights.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(running) = flights.get(&key).cloned() {
            return Ok(running);
        }
        flights.insert(key.clone(), flight.clone());
    }
    emit(&id, &account_id, "signingIn", None);
    // A device code shows wherever the sign-in started (page, chat, view).
    if let (Some(app), Some(device)) = (APP.get(), flight.start.get("device")) {
        let _ = app.emit("plugins://device-code", json!({ "id": id, "account": account_id, "plugin": plugin.name, "service": account.service, "device": device }));
    }
    let service = account.service.clone();
    tauri::async_runtime::spawn(async move {
        let result = match tokio::time::timeout(LIMIT, flow).await {
            Ok(result) => result,
            Err(_) => {
                cancel.cancel();
                Err("cancelled".into())
            }
        };
        let outcome = match result {
            Ok(signed) => {
                let (id, account_id) = (id.clone(), account_id.clone());
                blocking(move || keep(&store(), &id, &account_id, signed)).await
            }
            Err(error) => Err(error),
        };
        state().flights.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
        match &outcome {
            Ok(_) => {
                emit(&id, &account_id, "signedIn", None);
                if let Some(app) = APP.get() {
                    crate::plugins::log(app, "info", format!("{id}: signed in to {service}"));
                    // Connector servers that use the account get its token.
                    crate::plugins::changed_public(app).await;
                }
            }
            Err(error) => emit(&id, &account_id, "signedOut", Some(error)),
        }
        let _ = tx.send(Some(outcome));
    });
    Ok(flight)
}

async fn oauth_flow(http: &reqwest::Client, o: &OAuth, client: &oauth::Client, cancel: &CancellationToken) -> Result<Signed, String> {
    let endpoints = oauth::endpoints(http, o).await?;
    let authorize = endpoints.authorize.clone().ok_or("the service declares no sign-in address")?;
    let token = endpoints.token.clone().ok_or("the service declares no token address")?;
    let back = oauth::Return::bind(o.redirect_port).await?;
    let (verifier, state) = (oauth::random(), oauth::random());
    let url = oauth::authorize_url(&authorize, &client.id, &back.redirect, &o.scopes, &state, &oauth::challenge(&verifier))?;
    let redirect = back.redirect.clone();
    open_in_browser(&url)?;
    let code = back.code(&state, cancel).await?;
    let answer = oauth::exchange(http, &token, client, &code, &redirect, &verifier).await.map_err(|f| f.message().to_string())?;
    finish_tokens(http, &endpoints, o, &answer).await
}

async fn device_flow(http: &reqwest::Client, o: &OAuth, endpoints: &oauth::Endpoints, client: &oauth::Client, device: &oauth::Device, cancel: &CancellationToken) -> Result<Signed, String> {
    let token = endpoints.token.clone().ok_or("the service declares no token address")?;
    let answer = oauth::device_poll(http, &token, client, device, cancel).await?;
    finish_tokens(http, endpoints, o, &answer).await
}

/// The session and a name for it: the ID token's, else the service's
/// userinfo, else the service's own name.
async fn finish_tokens(http: &reqwest::Client, endpoints: &oauth::Endpoints, o: &OAuth, answer: &Map<String, Value>) -> Result<Signed, String> {
    let session = oauth::session_of(answer, &o.scopes, None, now_ms()).map_err(|f| f.message().to_string())?;
    let access = session.get("accessToken").and_then(Value::as_str).unwrap_or_default().to_string();
    let mut label = session.get("idToken").and_then(Value::as_str).and_then(oauth::label_of_id_token);
    if label.is_none() {
        if let Some(url) = &endpoints.userinfo {
            label = oauth::label_of_userinfo(http, url, &access).await;
        }
    }
    Ok(Signed { session, label: label.unwrap_or_default() })
}

fn open_in_browser(url: &str) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let app = APP.get().ok_or("the app is not ready")?;
    app.opener().open_url(url, None::<&str>).map_err(|e| format!("could not open the browser: {e}"))
}

/// Keep a finished sign-in: its session in the vault, its record in
/// `installed.json`. Signing in again as the same name replaces that
/// record's session; the first record of an account is its default.
fn keep(store: &Store, id: &str, account_id: &str, signed: Signed) -> Result<String, String> {
    let (installed, plugin) = find(store, id)?;
    let account = account(&plugin, account_id)?;
    let same = installed
        .accounts
        .iter()
        .find(|r| r.account == account_id && !signed.label.is_empty() && r.label == signed.label)
        .map(|r| r.id.clone());
    let record_id = same.clone().unwrap_or_else(|| oauth::random().chars().filter(char::is_ascii_alphanumeric).take(10).collect());
    let count = installed.accounts.iter().filter(|r| r.account == account_id).count();
    let label = if signed.label.is_empty() {
        if count == 0 || same.is_some() {
            account.service.clone()
        } else {
            format!("{} {}", account.service, count + 1)
        }
    } else {
        signed.label.clone()
    };
    let scopes: Vec<String> = signed
        .session
        .get("scopes")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    crate::plugin_settings::with_vault(store, id, |vault| vault.set(&account_name(&record_id), Value::Object(signed.session)))?;
    store.edit(id, |entry| {
        let first = !entry.accounts.iter().any(|r| r.account == account_id);
        match entry.accounts.iter_mut().find(|r| r.id == record_id) {
            Some(record) => {
                record.state = "signedIn".into();
                record.label = label.clone();
                record.scopes = scopes.clone();
            }
            None => entry.accounts.push(AccountRecord {
                account: account_id.to_string(),
                id: record_id.clone(),
                label: label.clone(),
                state: "signedIn".into(),
                default: first,
                workspaces: Vec::new(),
                scopes: scopes.clone(),
            }),
        }
    })?;
    Ok(record_id)
}

/// Sign out of `record`: revoke what the service lets evo revoke, delete
/// the session, keep the other accounts (FR-30).
pub async fn sign_out(id: &str, record: &str) -> Result<(), String> {
    let (id_owned, record_owned) = (id.to_string(), record.to_string());
    let (installed, plugin, session) = blocking(move || {
        let store = store();
        let (installed, plugin) = find(&store, &id_owned)?;
        let session = crate::plugin_settings::with_vault(&store, &id_owned, |v| v.get(&account_name(&record_owned))).unwrap_or_default();
        Ok((installed, plugin, session))
    })
    .await?;
    let Some(entry) = installed.accounts.iter().find(|r| r.id == record).cloned() else {
        return Err("that sign-in is gone".into());
    };
    if let (Some(session), Ok(account)) = (session.as_ref().and_then(Value::as_object), account(&plugin, &entry.account)) {
        revoke(&installed, &plugin, account, session).await;
    }
    let (id_owned, record_owned) = (id.to_string(), record.to_string());
    blocking(move || {
        let store = store();
        crate::plugin_settings::with_vault(&store, &id_owned, |v| v.remove(&account_name(&record_owned)))?;
        store.edit(&id_owned, |installed| {
            let was_default = installed.accounts.iter().any(|r| r.id == record_owned && r.default);
            let account = installed.accounts.iter().find(|r| r.id == record_owned).map(|r| r.account.clone());
            installed.accounts.retain(|r| r.id != record_owned);
            if was_default {
                if let Some(next) = installed.accounts.iter_mut().find(|r| Some(&r.account) == account.as_ref()) {
                    next.default = true;
                }
            }
        })?;
        Ok(())
    })
    .await?;
    emit(id, &entry.account, "signedOut", None);
    if let Some(app) = APP.get() {
        crate::plugins::changed_public(app).await;
    }
    Ok(())
}

/// Ask the service to revoke a session's tokens, when it declares how.
async fn revoke(installed: &Installed, plugin: &Plugin, account: &Account, session: &Map<String, Value>) {
    let (Method::Oauth(o) | Method::Device(o)) = &account.method else { return };
    let Ok(http) = oauth::client() else { return };
    let Ok(endpoints) = oauth::endpoints(&http, o).await else { return };
    let (Some(url), Ok(client)) = (endpoints.revoke, client_of(&store(), installed, plugin, o)) else { return };
    let (token, hint) = match session.get("refreshToken").and_then(Value::as_str) {
        Some(refresh) => (refresh, "refresh_token"),
        None => (session.get("accessToken").and_then(Value::as_str).unwrap_or_default(), "access_token"),
    };
    if !token.is_empty() {
        if let Err(error) = oauth::revoke(&http, &url, &client.id, token, hint).await {
            if let Some(app) = APP.get() {
                crate::plugins::log(app, "warn", format!("{}: {} did not revoke the sign-in: {error}", plugin.id, account.service));
            }
        }
    }
}

/// Revoke every sign-in of plugin `id` (before it is uninstalled; FR-30).
pub async fn revoke_all(id: &str) {
    let id = id.to_string();
    let Ok((installed, plugin, vault)) = blocking(move || {
        let store = store();
        let (installed, plugin) = find(&store, &id)?;
        let vault = crate::plugin_settings::with_vault(&store, &id, |v| v.read()).unwrap_or_default();
        Ok((installed, plugin, vault))
    })
    .await
    else {
        return;
    };
    for record in &installed.accounts {
        let (Ok(account), Some(session)) = (account(&plugin, &record.account), vault.get(&account_name(&record.id)).and_then(Value::as_object)) else { continue };
        revoke(&installed, &plugin, account, session).await;
    }
}

/// Make `record` the default, or (with `workspace`) that workspace's choice.
pub fn set_default(id: &str, record: &str, workspace: Option<&str>) -> Result<(), String> {
    store().edit(id, |installed| {
        let Some(account) = installed.accounts.iter().find(|r| r.id == record).map(|r| r.account.clone()) else { return };
        for entry in installed.accounts.iter_mut().filter(|r| r.account == account) {
            match workspace {
                Some(ws) => {
                    entry.workspaces.retain(|w| w != ws);
                    if entry.id == record {
                        entry.workspaces.push(ws.to_string());
                    }
                }
                None => entry.default = entry.id == record,
            }
        }
    })?;
    Ok(())
}

/// A session ready to use: renewed first when under 2 minutes remain (one
/// renewal per sign-in at a time). `renewed` says whether it changed.
pub async fn fresh_session(plugin_id: &str, account: &Account, record: &str) -> Result<(Map<String, Value>, bool), oauth::Failure> {
    let lock = {
        let mut renewals = state().renewals.lock().unwrap_or_else(|e| e.into_inner());
        renewals.entry(format!("{plugin_id}\n{record}")).or_default().clone()
    };
    let _one = lock.lock().await;
    let read = {
        let (id, record) = (plugin_id.to_string(), record.to_string());
        blocking(move || {
            let store = store();
            let (installed, plugin) = find(&store, &id)?;
            let session = crate::plugin_settings::with_vault(&store, &id, |v| v.get(&account_name(&record)))?;
            Ok((installed, plugin, session))
        })
        .await
    };
    let (installed, plugin, session) = read.map_err(oauth::Failure::Offline)?;
    let session = session.and_then(|s| s.as_object().cloned()).ok_or_else(|| oauth::Failure::Refused("not signed in".into()))?;
    let (Method::Oauth(o) | Method::Device(o)) = &account.method else {
        return Ok((session, false));
    };
    let now = now_ms();
    if !oauth::due(&session, now) {
        return Ok((session, false));
    }
    let renewed = async {
        let http = oauth::client().map_err(oauth::Failure::Offline)?;
        let endpoints = oauth::endpoints(&http, o).await.map_err(oauth::Failure::Offline)?;
        let token = endpoints.token.ok_or_else(|| oauth::Failure::Refused("no token address".into()))?;
        let client = client_of(&store(), &installed, &plugin, o).map_err(oauth::Failure::Refused)?;
        oauth::refresh(&http, &token, &client, &session, now).await
    }
    .await;
    match renewed {
        Ok(next) => {
            let (id, record, kept) = (plugin_id.to_string(), record.to_string(), next.clone());
            blocking(move || crate::plugin_settings::with_vault(&store(), &id, |v| v.set(&account_name(&record), Value::Object(kept))))
                .await
                .map_err(oauth::Failure::Offline)?;
            Ok((next, true))
        }
        Err(oauth::Failure::Offline(message)) if !oauth::expired(&session, now) => {
            let _ = message;
            Ok((session, false))
        }
        Err(oauth::Failure::Offline(message)) => Err(oauth::Failure::Offline(message)),
        Err(oauth::Failure::Refused(message)) => {
            needs_sign_in(plugin_id, &plugin.name, account, Some(record), &message);
            Err(oauth::Failure::Refused(message))
        }
    }
}

/// Mark a sign-in "needs sign-in again" and tell the person (FR-25, FR-26).
pub fn needs_sign_in(plugin_id: &str, plugin_name: &str, account: &Account, record: Option<&str>, why: &str) {
    if let Some(record) = record {
        let _ = store().edit(plugin_id, |installed| {
            for entry in installed.accounts.iter_mut().filter(|r| r.id == record) {
                entry.state = "needsSignIn".into();
            }
        });
    }
    emit(plugin_id, &account.id, "needsSignIn", Some(why));
    if let Some(app) = APP.get() {
        crate::plugins::log(app, "warn", format!("{plugin_id}: the {} sign-in needs signing in again ({why})", account.service));
        let verb = if matches!(account.method, Method::Web(_)) { "Log in" } else { "Sign in" };
        crate::agent_tools::user::send_os(app, &format!("{verb} to {} again", account.service), Some(&format!("{plugin_name} can no longer use your {} account.", account.service)), None);
    }
}

/// The access token of `account`'s default sign-in, for a connector server
/// declared the old way (`${secret.account.<id>}` in its `env` or `headers`).
pub fn server_token(installed: &Installed, vault: &Map<String, Value>, account: &str) -> Option<String> {
    let record = installed.accounts.iter().filter(|r| r.account == account && r.state == "signedIn").find(|r| r.default).or_else(|| installed.accounts.iter().find(|r| r.account == account && r.state == "signedIn"))?;
    vault.get(&account_name(&record.id))?.get("accessToken")?.as_str().map(str::to_string)
}

/// Renew the tokens connector servers use ahead of expiry; a renewal
/// restarts the servers with the new token (FR-26).
async fn renew_for_servers(app: &AppHandle) {
    let Ok(list) = blocking(|| Ok(store().enabled())).await else { return };
    let mut changed = false;
    for plugin in list {
        let text = serde_json::to_string(&plugin.mcp_servers).unwrap_or_default();
        for account in plugin.accounts.iter().filter(|a| text.contains(&format!("${{secret.account.{}}}", a.id))) {
            let id = plugin.id.clone();
            let Ok(installed) = blocking(move || find(&store(), &id).map(|(i, _)| i)).await else { continue };
            for record in installed.accounts.iter().filter(|r| r.account == account.id && r.state == "signedIn") {
                if let Ok((_, true)) = fresh_session(&plugin.id, account, &record.id).await {
                    changed = true;
                }
            }
        }
    }
    if changed {
        crate::plugins::changed_public(app).await;
    }
}

/// Accounts a plugin's tool needs that are not signed in, for the server
/// with pool key `plugin:<name>:<server>`: `(plugin name, plugin id, account)`.
pub fn missing_for_server(server_key: &str) -> Vec<(String, String, Account)> {
    let Some(name) = server_key.strip_prefix("plugin:").and_then(|rest| rest.split(':').next()) else {
        return Vec::new();
    };
    let store = store();
    let Some(installed) = store.installed().into_iter().find(|i| i.enabled && i.name == name) else {
        return Vec::new();
    };
    let plugin = store.load(&installed);
    plugin
        .accounts
        .iter()
        .filter(|a| !a.optional && !signed_in(&installed, a))
        .map(|a| (plugin.name.clone(), plugin.id.clone(), a.clone()))
        .collect()
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn plugin_account_sign_in(id: String, account: String) -> Result<Value, String> {
    start(&id, &account).await
}

#[tauri::command]
pub fn plugin_account_cancel(id: String, account: String) {
    cancel(&id, &account);
}

#[tauri::command]
pub async fn plugin_account_sign_out(id: String, record: String) -> Result<(), String> {
    sign_out(&id, &record).await
}

#[tauri::command]
pub async fn plugin_account_default(id: String, record: String, workspace: Option<String>) -> Result<(), String> {
    blocking(move || set_default(&id, &record, workspace.as_deref())).await?;
    if let Some(app) = APP.get() {
        let _ = app.emit("plugins://changed", ());
    }
    Ok(())
}

/// Run a token account's check: the backend tool it names (FR-23).
#[tauri::command]
pub async fn plugin_account_test(app: AppHandle, id: String, account: String) -> Result<Value, String> {
    use tauri::Manager;
    let (installed, plugin) = {
        let id = id.clone();
        blocking(move || find(&store(), &id)).await?
    };
    let account = self::account(&plugin, &account)?;
    let Method::Token { test: Some(tool), .. } = &account.method else {
        return Err(format!("{} offers no check for this key", plugin.name));
    };
    if !installed.enabled {
        return Err(format!("{} is off", plugin.name));
    }
    let pool = app.state::<crate::mcp::McpState>().0.clone();
    let mut keys = pool.plugin_keys(&plugin.name);
    keys.sort_by_key(|key| !key.ends_with(":backend"));
    let key = keys.first().ok_or_else(|| format!("{}'s backend is not running", plugin.name))?;
    let result = pool.call_server(key, tool, None).await?;
    let text: Vec<String> = result.get("content").and_then(Value::as_array).into_iter().flatten().filter_map(|p| p.get("text").and_then(Value::as_str).map(str::to_string)).collect();
    let ok = result.get("isError") != Some(&Value::Bool(true));
    Ok(json!({ "ok": ok, "message": text.join("\n") }))
}
