//! A plugin's settings, secrets and data (docs/features/plugin-settings/spec.md,
//! FR-2 – FR-6, FR-9, FR-17, FR-19; ADR 0022).
//!
//! Plain values live in `installed.json` (personal, or per workspace);
//! secret values live in the plugin's sealed vault (`evo-vault`), and
//! `installed.json` keeps only which are set. Only this shell opens vaults,
//! on blocking threads, one writer at a time.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use evo_plugins::settings::{self, Kind};
use evo_plugins::{Installed, Plugin, Store};
use evo_vault::{setting_name, Keychain, Vault};
use serde_json::{json, Map, Value};
use tauri::AppHandle;

/// One vault writer at a time (read, change, write).
static VAULT: Mutex<()> = Mutex::new(());

fn store() -> Store {
    Store::in_agent_dir(&crate::mcp::agent_dir())
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())?
}

/// Run `work` with plugin `id`'s vault, holding the writer lock.
pub fn with_vault<T>(store: &Store, id: &str, work: impl FnOnce(&Vault) -> Result<T, String>) -> Result<T, String> {
    let _one = VAULT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = store.vault_dir();
    let vault = Vault::new(&Keychain, &dir, id);
    work(&vault)
}

fn installed(store: &Store, id: &str) -> Result<(Installed, Plugin), String> {
    let installed = store.installed().into_iter().find(|i| i.id == id).ok_or_else(|| format!("`{id}` is not installed"))?;
    let plugin = store.load(&installed);
    Ok((installed, plugin))
}

/// The `secretsSet` entry of `key` (personal, or a workspace's own).
fn set_entry(key: &str, workspace: Option<&str>) -> String {
    match workspace {
        Some(workspace) => format!("{key}@{workspace}"),
        None => key.to_string(),
    }
}

/// Save plain settings, personal or for `workspace`. Values that do not fit
/// are not saved; their problems come back as `[{key, message}]` (FR-2).
#[tauri::command]
pub async fn plugin_settings_save(app: AppHandle, id: String, values: Map<String, Value>, workspace: Option<String>) -> Result<Value, String> {
    let problems = blocking(move || {
        let store = store();
        let (_, plugin) = installed(&store, &id)?;
        let problems = settings::validate(&plugin.settings, &values);
        if !problems.is_empty() {
            return Ok(problems);
        }
        // Normalized values (numbers from text, trimmed text); null clears.
        let mut clean = Map::new();
        for (key, value) in values {
            let Some(setting) = plugin.settings.iter().find(|s| s.key == key) else { continue };
            if workspace.is_some() && setting.scope != settings::Scope::Workspace {
                continue;
            }
            let empty = value.is_null() || (value.as_str() == Some("") && setting.kind != Kind::Boolean);
            if !empty {
                clean.insert(key, settings::check(setting, &value).unwrap_or(value));
            }
        }
        store.edit(&id, |entry| match &workspace {
            Some(workspace) => {
                entry.workspace_config.insert(workspace.clone(), clean);
                entry.workspace_config.retain(|_, values| !values.is_empty());
            }
            None => entry.config = clean,
        })?;
        Ok(Vec::new())
    })
    .await?;
    if problems.is_empty() {
        crate::plugins::changed_public(&app).await;
    }
    Ok(json!({ "problems": problems.into_iter().map(|(key, message)| json!({ "key": key, "message": message })).collect::<Vec<_>>() }))
}

/// Set a secret setting (FR-5): checked, sealed in the vault, never stored
/// or shown elsewhere.
#[tauri::command]
pub async fn plugin_secret_set(app: AppHandle, id: String, key: String, value: String, workspace: Option<String>) -> Result<(), String> {
    blocking(move || {
        let store = store();
        let (_, plugin) = installed(&store, &id)?;
        let setting = plugin.settings.iter().find(|s| s.key == key && s.kind == Kind::Secret).ok_or_else(|| format!("`{key}` is not a secret setting of this plugin"))?;
        if workspace.is_some() && setting.scope != settings::Scope::Workspace {
            return Err(format!("`{key}` is not set per workspace"));
        }
        let value = settings::check(setting, &Value::String(value)).map_err(|why| format!("{} {why}", setting.title))?;
        with_vault(&store, &id, |vault| vault.set(&setting_name(&key, workspace.as_deref()), value))?;
        let entry = set_entry(&key, workspace.as_deref());
        store.edit(&id, |installed| {
            if !installed.secrets_set.contains(&entry) {
                installed.secrets_set.push(entry);
            }
        })?;
        Ok(())
    })
    .await?;
    crate::plugins::changed_public(&app).await;
    Ok(())
}

/// Clear a secret setting.
#[tauri::command]
pub async fn plugin_secret_clear(app: AppHandle, id: String, key: String, workspace: Option<String>) -> Result<(), String> {
    blocking(move || {
        let store = store();
        with_vault(&store, &id, |vault| vault.remove(&setting_name(&key, workspace.as_deref())))?;
        let entry = set_entry(&key, workspace.as_deref());
        store.edit(&id, |installed| installed.secrets_set.retain(|e| e != &entry))?;
        Ok(())
    })
    .await?;
    crate::plugins::changed_public(&app).await;
    Ok(())
}

fn size_of(path: &Path) -> u64 {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => meta.len(),
        Ok(meta) if meta.is_dir() => std::fs::read_dir(path).map(|entries| entries.flatten().map(|e| size_of(&e.path())).sum()).unwrap_or(0),
        _ => 0,
    }
}

/// What a plugin keeps on this computer (FR-19).
#[tauri::command]
pub async fn plugin_data(id: String) -> Result<Value, String> {
    blocking(move || {
        let store = store();
        let data = store.data_dir(&id);
        let (installed, _) = installed(&store, &id)?;
        Ok(json!({
            "path": data,
            "bytes": size_of(&data),
            "storageBytes": size_of(&data.join("storage.json")),
            "secrets": installed.secrets_set.len(),
            "accounts": installed.accounts.len(),
        }))
    })
    .await
}

/// Empty the plugin's data folder (its storage, caches, environment). Its
/// settings, secrets and accounts stay.
#[tauri::command]
pub async fn plugin_data_clear(app: AppHandle, id: String) -> Result<(), String> {
    blocking(move || {
        let store = store();
        installed(&store, &id)?;
        let data = store.data_dir(&id);
        if let Ok(entries) = std::fs::read_dir(&data) {
            for entry in entries.flatten() {
                let path = entry.path();
                let result = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
                result.map_err(|e| format!("could not remove {}: {e}", path.display()))?;
            }
        }
        Ok(())
    })
    .await?;
    crate::plugins::changed_public(&app).await;
    Ok(())
}

/// The person answered an optional permission (FR-9): allowed adds it,
/// not allowed is remembered so the plugin is not asked again.
#[tauri::command]
pub async fn plugin_optional_answer(app: AppHandle, id: String, permission: String, allow: bool) -> Result<(), String> {
    blocking(move || {
        let store = store();
        let (_, plugin) = installed(&store, &id)?;
        if !plugin.permission_info.iter().any(|p| p.id == permission && p.optional) {
            return Err(format!("{} does not ask for `{permission}` as optional", plugin.name));
        }
        if allow {
            store.approve(&id, std::slice::from_ref(&permission))?;
        } else {
            store.revoke(&id, &permission)?;
        }
        store.edit(&id, |installed| {
            installed.optional_answers.insert(permission, allow);
        })?;
        Ok(())
    })
    .await?;
    crate::plugins::changed_public(&app).await;
    Ok(())
}

/// Plain `sensitive` values an earlier build kept in `installed.json` move
/// to the vault, once (FR-17). A value that cannot move (no keychain)
/// stays, and is not used: secrets reach processes from the vault only.
pub fn move_old_secrets(app: &AppHandle) {
    let store = store();
    for installed in store.installed() {
        let plugin = store.load(&installed);
        let secrets: Vec<String> = plugin.secret_keys().into_iter().filter(|key| installed.config.contains_key(key)).collect();
        if secrets.is_empty() {
            continue;
        }
        let moved = with_vault(&store, &installed.id, |vault| {
            for key in &secrets {
                if let Some(Value::String(value)) = installed.config.get(key).filter(|v| v.as_str().is_some_and(|s| !s.is_empty())) {
                    vault.set(&setting_name(key, None), Value::String(value.clone()))?;
                }
            }
            Ok(())
        });
        match moved {
            Ok(()) => {
                let _ = store.edit(&installed.id, |entry| {
                    for key in &secrets {
                        let had = entry.config.remove(key).is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()));
                        if had && !entry.secrets_set.contains(key) {
                            entry.secrets_set.push(key.clone());
                        }
                    }
                });
            }
            Err(error) => crate::plugins::log(app, "warn", format!("{}: could not move its secrets to the keychain: {error}", installed.name)),
        }
    }
}

/// Fill `${secret.KEY}` in the `env` and `headers` of `plugin`'s connector
/// servers from its vault (personal values; FR-6). A server whose secret
/// is not set gets no such variable.
pub fn fill_server_secrets(store: &Store, plugin: &mut Plugin) {
    let text = serde_json::to_string(&plugin.mcp_servers).unwrap_or_default();
    if !text.contains("${secret.") {
        return;
    }
    let values = with_vault(store, &plugin.id, |vault| vault.read()).unwrap_or_default();
    let installed = store.installed().into_iter().find(|i| i.id == plugin.id).unwrap_or_default();
    fill_with(plugin, &installed, &values);
}

/// [`fill_server_secrets`] with the vault's values in hand. A connector
/// server declared the old way gets an account's token with
/// `${secret.account.<id>}` (plugin-settings FR-26).
fn fill_with(plugin: &mut Plugin, installed: &Installed, values: &Map<String, Value>) {
    let lookup = |key: &str| match key.strip_prefix("account.") {
        Some(account) => crate::plugin_auth::server_token(installed, values, account),
        None => values.get(&setting_name(key, None)).and_then(Value::as_str).map(str::to_string),
    };
    for server in plugin.mcp_servers.values_mut() {
        for field in ["env", "headers"] {
            let Some(map) = server.get_mut(field).and_then(Value::as_object_mut) else { continue };
            let mut strings: BTreeMap<String, String> = map.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect();
            evo_plugins::runtime::fill_secrets(&mut strings, lookup);
            map.retain(|k, _| strings.contains_key(k));
            for (k, v) in strings {
                map.insert(k, Value::String(v));
            }
        }
    }
}


/// Remove the plugin's vault and its key (uninstall; FR-19, FR-30).
pub fn forget(store: &Store, id: &str) -> Result<(), String> {
    with_vault(store, id, |vault| vault.clear())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_fill_env_and_headers_and_a_missing_one_is_left_out() {
        let mut plugin = Plugin {
            mcp_servers: json!({
            "s": { "command": "run", "args": ["x"], "env": { "TOKEN": "${secret.token}", "MISSING": "${secret.none}", "PLAIN": "p" } },
            "h": { "url": "https://x", "headers": { "Authorization": "Bearer ${secret.token}" } }
        })
        .as_object()
        .unwrap()
        .clone(),
            ..Default::default()
        };
        let values = json!({ "setting:token": "t-1", "setting:token@ws": "other" });
        fill_with(&mut plugin, &Installed::default(), values.as_object().unwrap());
        assert_eq!(plugin.mcp_servers["s"]["env"], json!({ "TOKEN": "t-1", "PLAIN": "p" }));
        assert_eq!(plugin.mcp_servers["h"]["headers"]["Authorization"], "Bearer t-1");
        assert_eq!(plugin.mcp_servers["s"]["args"], json!(["x"]));
    }

    #[test]
    fn a_connector_server_gets_the_default_sign_ins_token() {
        use evo_plugins::store::AccountRecord;
        let mut plugin = Plugin {
            mcp_servers: json!({ "s": { "command": "run", "env": { "GH": "${secret.account.gh}", "OFF": "${secret.account.none}" } } })
                .as_object()
                .unwrap()
                .clone(),
            ..Default::default()
        };
        let record = |id: &str, default: bool, state: &str| AccountRecord { account: "gh".into(), id: id.into(), state: state.into(), default, ..Default::default() };
        let installed = Installed { accounts: vec![record("a", false, "signedIn"), record("b", true, "signedIn"), record("c", false, "needsSignIn")], ..Default::default() };
        let values = json!({ "account:a": { "accessToken": "tok-a" }, "account:b": { "accessToken": "tok-b" } });
        fill_with(&mut plugin, &installed, values.as_object().unwrap());
        assert_eq!(plugin.mcp_servers["s"]["env"], json!({ "GH": "tok-b" }), "the default, and nothing half-filled");
    }

    #[test]
    fn secrets_set_entries_name_the_workspace() {
        assert_eq!(set_entry("token", None), "token");
        assert_eq!(set_entry("token", Some("ws-1")), "token@ws-1");
    }
}
