//! Plugins and marketplaces (`evo-plugins`) for Customize → Plugins.
//!
//! Installs live in `<agent dir>/plugins`. After every change the shell
//! gives the connector pool the enabled plugins' MCP servers and emits
//! `plugins://changed`; the supervisor has every chat's engine re-read its
//! skills, commands, agent types and hooks, viewed or not
//! (docs/features/shell-hosted-engines/spec.md, FR-8).

use std::path::{Path, PathBuf};

use evo_mcp::{Lifecycle, PluginServer, ServerConfig};
use evo_plugins::{Installed, Plugin, Store};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::mcp::McpState;

fn store() -> Store {
    Store::in_agent_dir(&crate::mcp::agent_dir())
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

/// A line in Settings → Logs, source `plugins`.
pub(crate) fn log(app: &AppHandle, level: &str, message: String) {
    if let Some(db) = app.try_state::<crate::db::Db>() {
        db.log(evo_store::logs::NewLog::new(level, "plugins", message));
    }
}

/// [`changed`], for the plugin settings commands.
pub(crate) async fn changed_public(app: &AppHandle) {
    changed(app).await;
}

/// Give the pool the enabled plugins' servers, then tell the UI. Secrets
/// are filled into the servers' `env` and `headers` from the vault here,
/// in memory only (plugin-settings FR-6).
async fn changed(app: &AppHandle) {
    let (servers, to_prepare) = tauri::async_runtime::spawn_blocking(plugin_servers).await.unwrap_or_default();
    // Backends whose runtime or dependencies are not ready yet: get them
    // ready; this runs again when they are (plugin-settings FR-18).
    for plugin in to_prepare {
        crate::plugin_runtime::prepare(app, plugin);
    }
    if let Some(state) = app.try_state::<McpState>() {
        state.0.set_plugin_servers(servers).await;
    }
    // Views follow: a plugin turned off or removed closes them (plugin-ui FR-15).
    crate::plugin_ui::plugins_changed(app).await;
    let _ = app.emit("plugins://changed", ());
    crate::supervisor::settings::resources_changed(app);
    // Plugin AI runs stop when their plugin is off, gone or lost `ai` (plugin-ai FR-13).
    let allowed: Vec<String> = tauri::async_runtime::spawn_blocking(|| {
        store().installed().into_iter().filter(|i| i.enabled && i.approved_permissions.iter().any(|p| p == "ai")).map(|i| i.id).collect()
    })
    .await
    .unwrap_or_default();
    crate::plugin_ai::retain(|id| allowed.iter().any(|a| a == id));
}

/// When a plugin's code runs: the person's choice, else its manifest's,
/// else on demand; "with evo" only with `startup` (plugin-settings FR-11).
fn lifecycle(installed: &Installed, plugin: &Plugin) -> Lifecycle {
    use evo_plugins::runtime::{StartMode, DEFAULT_IDLE_MINUTES};
    let mode = installed.start_mode.or(plugin.runtime.as_ref().map(|r| r.startup)).unwrap_or_default();
    let minutes = installed.idle_stop_minutes.or(plugin.runtime.as_ref().map(|r| r.idle_stop_minutes)).unwrap_or(DEFAULT_IDLE_MINUTES);
    let on_demand = Lifecycle::OnDemand { idle: std::time::Duration::from_secs(u64::from(minutes) * 60) };
    match mode {
        StartMode::Off => Lifecycle::Off,
        StartMode::WithApp if installed.approved_permissions.iter().any(|p| p == "startup") => Lifecycle::Always,
        _ => on_demand,
    }
}

/// The servers of the enabled plugins for the pool — their connector
/// servers, and their backends when ready — and the backends to prepare.
fn plugin_servers() -> (Vec<PluginServer>, Vec<Plugin>) {
    let store = store();
    let mut servers = Vec::new();
    let mut to_prepare = Vec::new();
    // Host API keys stay only for parts that still run (plugin-settings FR-26).
    let mut keep: Vec<(String, crate::plugin_host::Caller)> = Vec::new();
    for installed in store.installed().into_iter().filter(|i| i.enabled) {
        let mut plugin = store.load(&installed);
        evo_plugins::store::gate(&mut plugin, &installed);
        if !plugin.hooks.is_empty() {
            keep.push((plugin.id.clone(), crate::plugin_host::Caller::Hooks));
        }
        if plugin.runtime.is_some() {
            keep.push((plugin.id.clone(), crate::plugin_host::Caller::Backend));
        }
        crate::plugin_settings::fill_server_secrets(&store, &mut plugin);
        let lifecycle = lifecycle(&installed, &plugin);
        for (name, config) in &plugin.mcp_servers {
            if let Ok(config) = serde_json::from_value::<ServerConfig>(config.clone()) {
                servers.push(PluginServer { plugin: plugin.name.clone(), server: name.clone(), config, lifecycle });
            }
        }
        let Some(runtime) = plugin.runtime.clone() else { continue };
        if !crate::plugin_runtime::ready(&plugin) {
            to_prepare.push(plugin);
            continue;
        }
        if let Some(config) = backend_config(&store, &installed, &plugin, &runtime) {
            servers.push(PluginServer { plugin: plugin.name.clone(), server: "backend".into(), config, lifecycle });
        }
    }
    crate::plugin_host::host().retain(|plugin, caller| keep.iter().any(|(p, c)| p == plugin && *c == caller));
    (servers, to_prepare)
}

/// The pool's config for a plugin's backend: the runtime's command with the
/// person's approvals, its settings and secrets, a small environment.
fn backend_config(store: &Store, installed: &Installed, plugin: &Plugin, runtime: &evo_plugins::runtime::Runtime) -> Option<ServerConfig> {
    use crate::plugin_runtime::{install, tool_for, Tool};
    let tool = tool_for(runtime.language);
    let deno = install::installed(Tool::Deno).unwrap_or_default();
    let values = evo_plugins::settings::effective(&plugin.settings, &installed.config, None);
    let mut launch = evo_plugins::runtime::launch(&evo_plugins::runtime::LaunchInput {
        runtime,
        root: &plugin.root,
        data: &plugin.data,
        approved: &installed.approved_permissions,
        deno: &deno,
        host_port: crate::plugin_host::host().port(),
        workspace: None,
        values: &values,
    });
    let secrets = crate::plugin_settings::with_vault(store, &plugin.id, |vault| vault.read()).unwrap_or_default();
    evo_plugins::runtime::fill_secrets(&mut launch.env, |key| secrets.get(&evo_vault::setting_name(key, None)).and_then(Value::as_str).map(str::to_string));
    // Its key for the host API: in its environment only, hidden like secrets.
    if launch.env.contains_key("EVO_PLUGIN_HOST") {
        if let Ok(key) = crate::plugin_host::host().key(&plugin.id, crate::plugin_host::Caller::Backend) {
            launch.env.insert("EVO_PLUGIN_KEY".into(), key);
        }
    }
    let extra: Vec<(String, String)> = match tool {
        Tool::Deno => crate::plugin_runtime::deno_env(),
        Tool::Uv => Vec::new(),
    };
    launch.env.extend(extra);
    let mut config = ServerConfig {
        command: Some(launch.program.display().to_string()),
        args: launch.args,
        env: launch.env,
        cwd: Some(launch.cwd.display().to_string()),
        ..Default::default()
    };
    config.extra.insert("minimalEnv".into(), Value::Bool(true));
    let _ = std::fs::create_dir_all(&plugin.data);
    Some(config)
}

/// Connect the enabled plugins' servers at launch, after moving secrets an
/// earlier build kept in plain text into the vault (plugin-settings FR-17).
pub fn setup(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let moving = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || crate::plugin_settings::move_old_secrets(&moving)).await;
        changed(&app).await
    });
}

fn count_files(dir: &Path, depth: usize, wanted: &dyn Fn(&Path) -> bool) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| e.path())
        .map(|p| {
            if p.is_dir() {
                if depth > 0
                    && p.file_name()
                        .is_some_and(|n| n != ".git" && n != "node_modules")
                {
                    count_files(&p, depth - 1, wanted)
                } else {
                    0
                }
            } else {
                usize::from(wanted(&p))
            }
        })
        .sum()
}

fn names_in(dir: &Path, wanted: &dyn Fn(&Path) -> Option<String>) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        if let Some(name) = wanted(&entry.path()) {
            out.push(name);
        }
    }
    out.sort();
    out
}

/// What a plugin holds, for its page.
fn describe(installed: &Installed, plugin: &Plugin) -> Value {
    let skills: Vec<String> = plugin
        .skills
        .iter()
        .flat_map(|dir| {
            if dir.join("SKILL.md").is_file() {
                vec![dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()]
            } else {
                names_in(dir, &|p| {
                    p.join("SKILL.md")
                        .is_file()
                        .then(|| p.file_name().unwrap().to_string_lossy().into_owned())
                })
            }
        })
        .collect();
    let commands: usize = plugin
        .commands
        .iter()
        .map(|d| count_files(d, 4, &|p| p.extension().is_some_and(|e| e == "md")))
        .sum();
    let agents: Vec<String> = plugin
        .agents
        .iter()
        .flat_map(|p| {
            if p.is_file() {
                vec![p
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()]
            } else {
                names_in(p, &|f| {
                    (f.extension().is_some_and(|e| e == "md"))
                        .then(|| f.file_stem().unwrap().to_string_lossy().into_owned())
                })
            }
        })
        .collect();
    let hooks: Vec<Value> = plugin
        .hooks
        .iter()
        .map(|(event, groups)| json!({ "event": event, "count": groups.as_array().map_or(0, |g| g.iter().map(|x| x.get("hooks").and_then(Value::as_array).map_or(0, Vec::len)).sum::<usize>()) }))
        .collect();
    let mut out = json!({
        "id": installed.id,
        "name": plugin.name,
        "marketplace": installed.marketplace,
        "version": installed.version,
        "enabled": installed.enabled,
        "path": installed.path,
        "installedAt": installed.installed_at,
        "config": installed.config,
        "description": plugin.description,
        "author": plugin.author,
        "homepage": plugin.homepage,
        "repository": plugin.repository,
        "license": plugin.license,
        "keywords": plugin.keywords,
        "userConfig": plugin.user_config,
        "skills": skills,
        "commands": commands,
        "agents": agents,
        "hooks": hooks,
        "connectors": plugin.mcp_servers.keys().collect::<Vec<_>>(),
        "bin": plugin.bin.is_some(),
        "problems": plugin.problems,
        // Views and consent (docs/features/plugin-ui/spec.md, FR-1 – FR-3, FR-14, FR-16).
        "ui": plugin.ui,
        "icons": view_icons(plugin),
        "permissions": permission_rows(plugin, &plugin.permissions),
        "approvedPermissions": installed.approved_permissions,
        "unapproved": permission_rows(plugin, &installed.unapproved()),
        "pending": installed.pending.as_ref().map(|p| json!({ "version": p.version, "permissions": permission_rows(plugin, &p.permissions) })),
        "developerMode": installed.developer_mode,
    });
    // Settings, secrets and what waits (docs/features/plugin-settings/spec.md).
    let mut gated = plugin.clone();
    evo_plugins::store::gate(&mut gated, installed);
    out["settings"] = json!(plugin.settings);
    out["workspaceConfig"] = json!(installed.workspace_config);
    out["secretsSet"] = json!(installed.secrets_set);
    out["accounts"] = json!(plugin.accounts);
    out["accountStatus"] = json!(crate::plugin_auth::status(installed, plugin));
    out["runtime"] = json!(plugin.runtime);
    out["optionalAnswers"] = json!(installed.optional_answers);
    out["startMode"] = json!(installed.start_mode);
    out["idleStopMinutes"] = json!(installed.idle_stop_minutes);
    out["waiting"] = json!(gated.waiting);
    out
}

/// The views' icons as data URLs, by path: the app's windows draw them
/// without reaching the plugin's scheme (an `<img>` runs no script in an SVG).
fn view_icons(plugin: &Plugin) -> serde_json::Map<String, Value> {
    use base64::Engine as _;
    const MAX_ICON: u64 = 128 * 1024;
    let ui = &plugin.ui;
    let mut icons = serde_json::Map::new();
    for view in ui.sidebar.iter().chain(&ui.panels).chain(&ui.cards).chain(&ui.composer) {
        let Some(path) = view.icon.as_deref() else { continue };
        if icons.contains_key(path) {
            continue;
        }
        let file = plugin.root.join(path);
        let small = std::fs::metadata(&file).is_ok_and(|m| m.len() <= MAX_ICON);
        let Some(bytes) = small.then(|| std::fs::read(&file).ok()).flatten() else { continue };
        let mime = if path.to_ascii_lowercase().ends_with(".svg") { "image/svg+xml" } else { "image/png" };
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        icons.insert(path.to_string(), Value::String(format!("data:{mime};base64,{data}")));
    }
    icons
}

/// Permissions with the words the person reads when approving them, the
/// plugin's reason, whether each is optional or implied by its parts, and
/// which of its parts a network or file permission does not bind
/// (plugin-settings FR-8, FR-9, FR-15).
fn permission_rows(plugin: &Plugin, permissions: &[String]) -> Vec<Value> {
    let unenforced = plugin.unenforced_parts();
    permissions
        .iter()
        .map(|p| {
            let info = plugin.permission_info.iter().find(|i| &i.id == p);
            let binds = p.starts_with("network:") || p.starts_with("files:");
            json!({
                "id": p,
                "label": evo_plugins::ui::describe_permission(p),
                "reason": info.and_then(|i| i.reason.clone()),
                "optional": info.is_some_and(|i| i.optional),
                "implied": info.is_some_and(|i| i.implied),
                "notEnforcedFor": if binds { unenforced.clone() } else { Vec::new() },
            })
        })
        .collect()
}

/// What an install or update leaves for the person to approve: none, or
/// the plugin's permissions in plain words (FR-3).
fn consent(installed: &Installed) -> Value {
    let plugin = store().load(installed);
    json!({
        "id": installed.id,
        "version": installed.version,
        "enabled": installed.enabled,
        "needsConsent": permission_rows(&plugin, &installed.unapproved()),
        "pendingVersion": installed.pending.as_ref().map(|p| p.version.clone()),
    })
}

#[tauri::command]
pub async fn plugins_state() -> Result<Value, String> {
    blocking(|| {
        let store = store();
        let installed: Vec<Value> = store.installed().iter().map(|i| describe(i, &store.load(i))).collect();
        let marketplaces: Vec<Value> = store
            .marketplaces()
            .iter()
            .map(|m| {
                json!({
                    "name": m.name,
                    "source": m.source.display(),
                    "kind": match m.source { evo_plugins::MarketplaceSource::Git { .. } => "git", _ => "folder" },
                    "location": m.location,
                    "description": m.description,
                    "owner": m.owner,
                    "updatedAt": m.updated_at,
                    "plugins": m.entries().map(|e| e.len()).unwrap_or(0),
                    "error": m.entries().err(),
                })
            })
            .collect();
        Ok(json!({
            "installed": installed,
            "marketplaces": marketplaces,
            "catalog": store.catalog(),
            "root": store.root(),
        }))
    })
    .await
}

#[tauri::command]
pub async fn plugin_marketplace_add(app: AppHandle, source: String) -> Result<Value, String> {
    let added = blocking(move || store().add_marketplace(&source)).await?;
    changed(&app).await;
    Ok(json!({ "name": added.name }))
}

#[tauri::command]
pub async fn plugin_marketplace_update(app: AppHandle, name: String) -> Result<(), String> {
    blocking(move || store().update_marketplace(&name).map(|_| ())).await?;
    changed(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn plugin_marketplace_remove(app: AppHandle, name: String) -> Result<(), String> {
    blocking(move || store().remove_marketplace(&name)).await?;
    changed(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn plugin_install(
    app: AppHandle,
    marketplace: String,
    name: String,
) -> Result<Value, String> {
    let installed = blocking(move || store().install(&marketplace, &name)).await?;
    changed(&app).await;
    Ok(consent(&installed))
}

#[tauri::command]
pub async fn plugin_install_folder(app: AppHandle, path: String) -> Result<Value, String> {
    let installed = blocking(move || store().install_folder(&PathBuf::from(path))).await?;
    changed(&app).await;
    Ok(consent(&installed))
}

#[tauri::command]
pub async fn plugin_uninstall(app: AppHandle, id: String) -> Result<(), String> {
    // The services are asked to revoke its sign-ins first (FR-30).
    crate::plugin_auth::revoke_all(&id).await;
    blocking(move || {
        let store = store();
        store.uninstall(&id)?;
        // Its secrets and accounts go with it (plugin-settings FR-19, FR-30).
        // Its AI runs and threads too (plugin-ai FR-19).
        if let Ok(conn) = evo_store::open() {
            let _ = evo_store::plugin_ai::delete_plugin(&conn, &id);
        }
        crate::plugin_settings::forget(&store, &id)
    })
    .await?;
    changed(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn plugin_update(app: AppHandle, id: String) -> Result<Value, String> {
    let installed = blocking(move || store().update(&id)).await?;
    changed(&app).await;
    Ok(consent(&installed))
}

#[tauri::command]
pub async fn plugin_set_enabled(app: AppHandle, id: String, on: bool) -> Result<(), String> {
    blocking(move || {
        let store = store();
        // A plugin whose views ask for permissions is turned on by approving
        // them (`plugin_approve`), never around them (FR-3).
        if on {
            let unapproved = store
                .installed()
                .into_iter()
                .find(|i| i.id == id)
                .map(|i| i.unapproved())
                .unwrap_or_default();
            if !unapproved.is_empty() {
                return Err(format!("approve its permissions first: {}", unapproved.join(", ")));
            }
        }
        store.set_enabled(&id, on)
    })
    .await?;
    changed(&app).await;
    Ok(())
}

/// Approve permissions for a plugin's views; once all are approved it is on,
/// and a pending update replaces the installed version (FR-3).
#[tauri::command]
pub async fn plugin_approve(app: AppHandle, id: String, permissions: Vec<String>) -> Result<Value, String> {
    let installed = blocking(move || store().approve(&id, &permissions)).await?;
    changed(&app).await;
    Ok(consent(&installed))
}

/// Take back an approved permission (FR-16).
#[tauri::command]
pub async fn plugin_revoke(app: AppHandle, id: String, permission: String) -> Result<(), String> {
    blocking(move || store().revoke(&id, &permission)).await?;
    changed(&app).await;
    Ok(())
}

/// Reload the plugin's views when its files change (FR-14).
#[tauri::command]
pub async fn plugin_set_developer_mode(app: AppHandle, id: String, on: bool) -> Result<(), String> {
    blocking(move || store().set_developer_mode(&id, on)).await?;
    changed(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn plugin_set_config(
    app: AppHandle,
    id: String,
    config: Map<String, Value>,
) -> Result<(), String> {
    blocking(move || store().set_config(&id, config)).await?;
    changed(&app).await;
    Ok(())
}
