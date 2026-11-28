//! Connected apps in the running app: the service as Tauri state, its
//! start (and retries while Composio cannot be reached), the commands the
//! windows call, and `apps://changed` whenever the key, the status or the
//! accounts change (docs/features/connected-apps/plan.md, 3.3, 3.5, 3.6).

use std::sync::Arc;
use std::time::Duration;

use evo_vault::KeyStore;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use super::catalog::{self, Toolkit};
use super::{composio, key, KeyState, Reach, Service};

/// The release build's identifier: its key is `connected-apps`; any other
/// build (debug, e2e) keeps its own, so testing never touches the
/// person's key.
const RELEASE_ID: &str = "com.evoelsewhere.evo";

/// Waits before trying Composio again after it could not be reached
/// (plan 3.6), then every 30 minutes.
const RETRIES: [u64; 5] = [30, 60, 120, 300, 600];
const RETRY_LATER: u64 = 30 * 60;

/// The keychain, with this build's account names.
struct BuildKeys {
    suffix: Option<String>,
}

impl BuildKeys {
    fn name(&self, account: &str) -> String {
        match &self.suffix {
            Some(id) => format!("{account}@{id}"),
            None => account.to_string(),
        }
    }
}

impl KeyStore for BuildKeys {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        evo_vault::Keychain.get(&self.name(account))
    }
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        evo_vault::Keychain.set(&self.name(account), value)
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        evo_vault::Keychain.delete(&self.name(account))
    }
}

/// The app's connected apps.
pub struct AppsState {
    pub service: Arc<Service>,
    catalog: tokio::sync::Mutex<Option<Vec<Toolkit>>>,
}

pub fn state(app: &AppHandle) -> Option<tauri::State<'_, AppsState>> {
    app.try_state::<AppsState>()
}

fn changed(app: &AppHandle) {
    let _ = app.emit("apps://changed", ());
}

fn log(app: &AppHandle, level: &str, message: String) {
    if let Some(db) = app.try_state::<crate::db::Db>() {
        db.log(evo_store::logs::NewLog::new(
            level,
            "connected-apps",
            message,
        ));
    }
}

/// Set up the state and start: no key, no request (NFR-4).
pub fn setup(app: &AppHandle) {
    let identifier = app.config().identifier.clone();
    let keys = BuildKeys {
        suffix: (identifier != RELEASE_ID).then_some(identifier),
    };
    let service = Arc::new(Service::new(
        Arc::new(keys),
        Arc::new(key::DbSettings(app.clone())),
        composio::BASE,
    ));
    app.manage(AppsState {
        service,
        catalog: tokio::sync::Mutex::new(None),
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move { start(&app).await });
}

/// Start the service and hand the pool its server. `true`: Composio could
/// not be reached with a saved key, so it is worth trying again.
async fn start_once(app: &AppHandle) -> bool {
    let Some(service) = state(app).map(|s| s.service.clone()) else {
        return false;
    };
    let retry = match service.start().await {
        Ok(Some(server)) => {
            set_pool(app, Some(server)).await;
            let _ = service.refresh_accounts().await;
            false
        }
        Ok(None) => {
            set_pool(app, None).await;
            false
        }
        Err(error) => {
            log(app, "warn", format!("Composio unavailable: {error}"));
            service.key_state().0 == KeyState::Saved
        }
    };
    changed(app);
    retry
}

/// [`start_once`], then again after 30 s, 1, 2, 5 and 10 minutes and
/// every 30 minutes while Composio cannot be reached (the other tools of
/// every chat go on meanwhile).
async fn start(app: &AppHandle) {
    let mut attempt = 0;
    while start_once(app).await {
        let wait = RETRIES.get(attempt).copied().unwrap_or(RETRY_LATER);
        attempt += 1;
        tokio::time::sleep(Duration::from_secs(wait)).await;
        // Something else (a new key, Retry) may have set it up meanwhile.
        let ready = state(app).is_none_or(|s| {
            s.service.session_now().is_some() || s.service.key_state().0 != KeyState::Saved
        });
        if ready {
            break;
        }
    }
}

async fn set_pool(app: &AppHandle, server: Option<evo_mcp::AppServer>) {
    if let Some(mcp) = app.try_state::<crate::mcp::McpState>() {
        let pool = mcp.0.clone();
        // Connecting may take a while: the commands do not wait for it.
        tauri::async_runtime::spawn(async move { pool.set_app_server(server).await });
    }
}

/// The catalogue (cached a day); empty with no key.
async fn toolkits(app: &AppHandle) -> Result<Vec<Toolkit>, String> {
    let state = state(app).ok_or("Connected apps is not ready.")?;
    let mut cached = state.catalog.lock().await;
    if let Some(list) = cached.as_ref() {
        return Ok(list.clone());
    }
    let client = state.service.client().ok_or("No Composio key is saved.")?;
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let list = catalog::load(&client, &catalog::cache_file(&dir)).await?;
    *cached = Some(list.clone());
    Ok(list)
}

/// A service's name and whether Composio provides its sign-in.
async fn service_of(app: &AppHandle, toolkit: &str) -> (String, bool) {
    let found = toolkits(app)
        .await
        .ok()
        .and_then(|list| list.into_iter().find(|t| t.slug == toolkit));
    match found {
        Some(t) => (t.name, t.managed_auth || t.no_auth),
        None => (toolkit.to_string(), true),
    }
}

/// Service names by toolkit, for the gate's messages.
pub async fn names(app: &AppHandle) -> std::collections::HashMap<String, String> {
    toolkits(app)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|t| (t.slug, t.name))
        .collect()
}

/// Open the sign-in in the person's browser and watch it until the account
/// is active or the link has lived its life.
fn open_and_watch(app: &AppHandle, pending: &super::connect::Pending) -> Result<(), String> {
    app.opener()
        .open_url(&pending.url, None::<&str>)
        .map_err(|e| format!("could not open the browser: {e}"))?;
    let app = app.clone();
    let id = pending.id.clone();
    tauri::async_runtime::spawn(async move {
        let started = std::time::Instant::now();
        while started.elapsed() < super::connect::LINK_LIFE + Duration::from_secs(5) {
            tokio::time::sleep(super::connect::POLL_EVERY).await;
            let Some(state) = state(&app) else { return };
            if !state.service.pending_now().iter().any(|p| p.id == id) {
                break;
            }
            match state.service.poll(&id).await {
                Ok(Some(account)) => {
                    log(
                        &app,
                        "info",
                        format!("connected {} \"{}\"", account.toolkit, account.label),
                    );
                    let _ = app.emit(
                        "apps://connected",
                        json!({ "toolkit": account.toolkit, "label": account.label }),
                    );
                    changed(&app);
                    // Running chats learn of the new account with their next call;
                    // mates waiting on a Connect card go on (FR-11).
                    crate::supervisor::connectors_changed(&app);
                    return;
                }
                Ok(None) => {}
                Err(error) => log(&app, "warn", format!("checking a sign-in: {error}")),
            }
        }
        changed(&app);
    });
    Ok(())
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn apps_state(app: AppHandle) -> Value {
    let Some(state) = state(&app) else {
        return json!({ "key": "none", "status": "off", "accounts": [], "pending": [] });
    };
    let (key, reach, reason) = state.service.key_state();
    json!({
        "key": key,
        "status": reach,
        "reason": reason,
        "accounts": state.service.accounts_now(),
        "pending": state.service.pending_now(),
    })
}

#[tauri::command]
pub async fn apps_set_key(app: AppHandle, key: String) -> Result<(), String> {
    let state = state(&app).ok_or("Connected apps is not ready.")?;
    let service = state.service.clone();
    *state.catalog.lock().await = None;
    let result = service.set_key(&key).await;
    match result {
        Ok(server) => {
            set_pool(&app, Some(server)).await;
            let _ = service.refresh_accounts().await;
            changed(&app);
            Ok(())
        }
        Err(error) => {
            changed(&app);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn apps_remove_key(app: AppHandle) -> Result<(), String> {
    let state = state(&app).ok_or("Connected apps is not ready.")?;
    state.service.remove_key().await?;
    *state.catalog.lock().await = None;
    if let Ok(dir) = app.path().app_cache_dir() {
        let _ = std::fs::remove_file(catalog::cache_file(&dir));
    }
    set_pool(&app, None).await;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn apps_catalog(
    app: AppHandle,
    query: Option<String>,
    category: Option<String>,
) -> Result<Value, String> {
    let all = toolkits(&app).await?;
    Ok(json!({
        "toolkits": catalog::search(&all, query.as_deref(), category.as_deref()),
        "categories": catalog::categories(&all),
    }))
}

#[tauri::command]
pub async fn apps_connect(
    app: AppHandle,
    toolkit: String,
    label: Option<String>,
) -> Result<Value, String> {
    let state = state(&app).ok_or("Connected apps is not ready.")?;
    let service = state.service.clone();
    let (name, managed) = service_of(&app, &toolkit).await;
    let pending = service
        .begin_connect(&toolkit, label.as_deref(), &name, managed, None)
        .await;
    changed(&app);
    let pending = pending?;
    open_and_watch(&app, &pending)?;
    Ok(json!({ "pendingId": pending.id, "label": pending.label }))
}

#[tauri::command]
pub async fn apps_reopen(app: AppHandle, pending_id: String) -> Result<Value, String> {
    let state = state(&app).ok_or("Connected apps is not ready.")?;
    let service = state.service.clone();
    let toolkit = service
        .pending_now()
        .into_iter()
        .find(|p| p.id == pending_id)
        .map(|p| p.toolkit)
        .unwrap_or_default();
    let (name, managed) = service_of(&app, &toolkit).await;
    let pending = service.reopen(&pending_id, &name, managed).await?;
    open_and_watch(&app, &pending)?;
    changed(&app);
    Ok(json!({ "pendingId": pending.id, "label": pending.label }))
}

#[tauri::command]
pub async fn apps_disconnect(app: AppHandle, account_id: String) -> Result<(), String> {
    let state = state(&app).ok_or("Connected apps is not ready.")?;
    let result = state.service.disconnect(&account_id).await;
    changed(&app);
    result
}

#[tauri::command]
pub async fn apps_reconnect(app: AppHandle, account_id: String) -> Result<Value, String> {
    let state = state(&app).ok_or("Connected apps is not ready.")?;
    let service = state.service.clone();
    let toolkit = service
        .accounts_now()
        .into_iter()
        .find(|a| a.id == account_id)
        .map(|a| a.toolkit)
        .ok_or("That account is no longer connected.")?;
    let (name, managed) = service_of(&app, &toolkit).await;
    let pending = service
        .begin_connect(&toolkit, None, &name, managed, Some(&account_id))
        .await;
    changed(&app);
    let pending = pending?;
    open_and_watch(&app, &pending)?;
    Ok(json!({ "pendingId": pending.id, "label": pending.label }))
}

/// Try Composio again now (the page's Retry).
#[tauri::command]
pub async fn apps_retry(app: AppHandle) -> Result<(), String> {
    start_once(&app).await;
    match state(&app).map(|s| s.service.key_state()) {
        Some((_, Reach::Unavailable, reason)) => {
            Err(reason.unwrap_or_else(|| "Composio could not be reached.".into()))
        }
        _ => Ok(()),
    }
}
