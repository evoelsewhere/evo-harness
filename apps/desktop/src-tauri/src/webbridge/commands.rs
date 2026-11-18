//! What Settings → Webbridge asks of the shell (docs/features/browser-bridge/spec.md
//! FR-2, FR-4, FR-23; plan 3.3 Tauri commands), and the labels people give
//! their browser profiles (`profiles.json`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Manager};

use super::setup::{self, Browser};
use super::{changed, log, server, Profile, Webbridge, ENABLED_KEY};
use crate::db::Db;

/// One browser in Settings.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub id: &'static str,
    pub label: &'static str,
    /// Chrome or Edge would start this app's host.
    pub registered: bool,
    /// Where the person manages extensions.
    pub extensions_page: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub browsers: Vec<BrowserStatus>,
    pub profiles: Vec<Profile>,
    /// The pipe is served.
    pub running: bool,
    /// Why the last connection was refused.
    pub refused: Option<String>,
    /// The unpacked extension's folder, while the extension is not in the stores.
    pub extension_folder: Option<String>,
    /// The host is missing beside the app (a build without it).
    pub host_missing: Option<String>,
}

pub fn status(app: &AppHandle) -> Status {
    let state = app.state::<Webbridge>();
    Status {
        browsers: Browser::ALL
            .iter()
            .map(|b| BrowserStatus {
                id: b.id(),
                label: b.label(),
                registered: setup::is_registered(*b),
                extensions_page: b.extensions_page(),
            })
            .collect(),
        profiles: state.profiles(),
        running: state.is_running(),
        refused: state.refused(),
        extension_folder: extension_folder().map(|p| p.to_string_lossy().into_owned()),
        host_missing: setup::host_path().err(),
    }
}

/// Where the unpacked extension is: shipped beside the app's resources, or in the repository during development.
fn extension_folder() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent();
    while let Some(d) = dir {
        for candidate in [
            d.join("extensions").join("evo-webbridge"),
            d.join("evo-webbridge"),
            d.join("resources").join("evo-webbridge"),
        ] {
            if candidate.join("manifest.json").is_file() {
                return Some(candidate);
            }
        }
        dir = d.parent();
    }
    None
}

#[tauri::command]
pub fn webbridge_status(app: AppHandle) -> Status {
    status(&app)
}

/// Register the host with a browser and start serving the pipe.
#[tauri::command]
pub fn webbridge_setup(app: AppHandle, browser: String) -> Result<Status, String> {
    let browser = Browser::parse(&browser).ok_or("unknown browser")?;
    setup::register(browser)?;
    app.state::<Db>()
        .with(|conn| evo_store::settings::set(conn, ENABLED_KEY, &json!(true)))?;
    server::start(&app);
    log(&app, "info", format!("set up {}", browser.label()));
    changed(&app);
    Ok(status(&app))
}

/// Remove the host's registration with a browser.
#[tauri::command]
pub fn webbridge_remove(app: AppHandle, browser: String) -> Result<Status, String> {
    let browser = Browser::parse(&browser).ok_or("unknown browser")?;
    setup::unregister(browser)?;
    if Browser::ALL.iter().all(|b| !setup::is_registered(*b)) {
        app.state::<Db>()
            .with(|conn| evo_store::settings::set(conn, ENABLED_KEY, &json!(false)))?;
    }
    log(&app, "info", format!("removed {}", browser.label()));
    changed(&app);
    Ok(status(&app))
}

/// Give a browser profile a name.
#[tauri::command]
pub fn webbridge_rename(app: AppHandle, profile: String, label: String) -> Result<Status, String> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 40 {
        return Err("a profile name has 1 to 40 characters".into());
    }
    let mut profiles = load();
    profiles.entry(profile.clone()).or_default().label = label.to_string();
    save(&profiles)?;
    let state = app.state::<Webbridge>();
    {
        let mut connections = state.connections.lock().expect("webbridge lock");
        for connection in connections.values_mut() {
            if let Some(p) = connection.profile.as_mut().filter(|p| p.key == profile) {
                p.label = label.to_string();
            }
        }
    }
    let _ = state.notify(&profile, json!({ "t": "label", "label": label }));
    changed(&app);
    Ok(status(&app))
}

// ── profiles.json ───────────────────────────────────────────────────────────

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Known {
    #[serde(default)]
    label: String,
    #[serde(default)]
    last_seen_ms: u64,
}

fn file() -> std::path::PathBuf {
    super::dir().join("profiles.json")
}

fn load() -> BTreeMap<String, Known> {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| serde_json::from_value(v["profiles"].clone()).ok())
        .unwrap_or_default()
}

fn save(profiles: &BTreeMap<String, Known>) -> Result<(), String> {
    let path = file();
    std::fs::create_dir_all(path.parent().expect("profiles.json has a folder"))
        .map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&json!({ "version": 1, "profiles": profiles }))
        .map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// The name the person gave a profile.
pub fn label_of(key: &str) -> Option<String> {
    load()
        .get(key)
        .map(|k| k.label.clone())
        .filter(|l| !l.is_empty())
}

/// A profile connected: remember it and when.
pub fn remember(profile: &Profile) {
    let mut profiles = load();
    let entry = profiles.entry(profile.key.clone()).or_default();
    if entry.label.is_empty() {
        entry.label = profile.label.clone();
    }
    entry.last_seen_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let _ = save(&profiles);
}
