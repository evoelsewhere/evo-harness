//! Plugin views: native child webviews that show a plugin's HTML, served by
//! the `evo-plugin://` scheme. They reach no IPC command; their only way
//! into the app is `POST /__bridge` on their own origin, which the scheme
//! handler receives with the caller's label (docs/features/plugin-ui/spec.md,
//! FR-4 – FR-16; ADR 0021).
//!
//! - [`scheme`]: files from the plugin folder, CSP and MIME (FR-9);
//! - [`views`]: the child webviews, their bounds and lifetime (FR-4 – FR-7, FR-15);
//! - [`bridge`]: the calls a view makes, checked against its permissions (FR-10 – FR-13);
//! - [`watch`]: Developer mode reloads (FR-14);
//! - `client.js`: the `window.evo` a view gets (FR-10, FR-12).

pub mod bridge;
pub mod scheme;
pub mod views;
pub mod watch;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex, RwLock};

use evo_plugins::ui::{host_key, PluginUi, View};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

pub use scheme::serve;

/// Every plugin view's label starts with this; the invoke gate keys on it.
pub const VIEW_PREFIX: &str = "plugin-";
/// The scheme views are served from.
pub const SCHEME: &str = "evo-plugin";

/// Whether a call from webview `label` may reach an app command: plugin
/// views reach none. Tauri's ACL already refuses them (no capability covers
/// their origin); this is the second guard (ADR 0021).
pub fn allowed(label: &str) -> bool {
    !label.starts_with(VIEW_PREFIX)
}

/// An enabled plugin with views, as the scheme and the bridge see it.
#[derive(Debug, Clone)]
pub struct Entry {
    /// `name@marketplace`.
    pub id: String,
    pub name: String,
    /// Its host label: `evo-plugin://<key>/` is its origin.
    pub key: String,
    /// The plugin folder, canonical.
    pub root: PathBuf,
    /// Its data folder (`storage.json` lives there; FR-13).
    pub data: PathBuf,
    pub ui: PluginUi,
    /// The permissions it asks for and the person approved (FR-3, FR-16).
    pub permissions: Vec<String>,
    pub developer_mode: bool,
}

impl Entry {
    pub fn may(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }

    /// The https origins it may fetch from (`network:` permissions).
    pub fn network(&self) -> Vec<String> {
        self.permissions.iter().filter_map(|p| p.strip_prefix("network:")).map(str::to_string).collect()
    }

    /// One of its views, by slot (`sidebar`, `panels`, `cards`, `composer`) and id.
    pub fn view(&self, slot: &str, id: &str) -> Option<&View> {
        let list = match slot {
            "sidebar" => &self.ui.sidebar,
            "panels" => &self.ui.panels,
            "cards" => &self.ui.cards,
            "composer" => &self.ui.composer,
            _ => return None,
        };
        list.iter().find(|view| view.id == id)
    }
}

/// What the shell keeps about plugin views.
#[derive(Default)]
pub struct PluginUiState {
    /// Enabled plugins with views, by host key.
    plugins: RwLock<HashMap<String, Arc<Entry>>>,
    /// Open views, by webview label.
    pub(crate) views: Mutex<HashMap<String, views::Live>>,
    next: AtomicU32,
    /// The app's theme as the main window last sent it: `{tokens, dark}` (FR-12).
    theme: Mutex<Value>,
    /// One storage write at a time (bridge.rs).
    pub(crate) storage: tokio::sync::Mutex<()>,
    /// One view created at a time: WebView2 creates on the main thread, and
    /// two creations at once (a slot mounted twice) hung the app.
    pub(crate) creating: tokio::sync::Mutex<()>,
}

impl PluginUiState {
    /// The enabled plugin served under `key`.
    pub fn plugin(&self, key: &str) -> Option<Arc<Entry>> {
        self.plugins.read().expect("plugins lock").get(key).cloned()
    }

    /// Every enabled plugin with views.
    pub fn entries(&self) -> Vec<Arc<Entry>> {
        let mut all: Vec<Arc<Entry>> = self.plugins.read().expect("plugins lock").values().cloned().collect();
        all.sort_by(|a, b| a.id.cmp(&b.id));
        all
    }

    /// The enabled plugin `id`.
    pub fn plugin_by_id(&self, id: &str) -> Option<Arc<Entry>> {
        self.plugins.read().expect("plugins lock").values().find(|entry| entry.id == id).cloned()
    }

    pub fn theme(&self) -> Value {
        let theme = self.theme.lock().expect("theme lock").clone();
        if theme.is_null() {
            json!({ "tokens": {}, "dark": false })
        } else {
            theme
        }
    }

    pub(crate) fn set_theme(&self, theme: Value) {
        *self.theme.lock().expect("theme lock") = theme;
    }
}

/// The enabled plugins with views, read from the store.
fn read_entries() -> Vec<Entry> {
    let store = evo_plugins::Store::in_agent_dir(&crate::mcp::agent_dir());
    store
        .installed()
        .into_iter()
        .filter(|installed| installed.enabled)
        .filter_map(|installed| {
            let plugin = store.load(&installed);
            if plugin.ui.is_empty() {
                return None;
            }
            let root = std::fs::canonicalize(&plugin.root).ok()?;
            let permissions = plugin
                .permissions
                .iter()
                .filter(|p| installed.approved_permissions.contains(p))
                .cloned()
                .collect();
            Some(Entry {
                key: host_key(&installed.id),
                id: installed.id.clone(),
                name: plugin.name.clone(),
                root,
                data: plugin.data.clone(),
                ui: plugin.ui.clone(),
                permissions,
                developer_mode: installed.developer_mode,
            })
        })
        .collect()
}

/// The plugins changed (installed, enabled, approved, updated): read them
/// again. Views of a plugin that is gone or no longer offers that view close
/// at once (FR-15); views of an updated plugin reload.
pub async fn plugins_changed(app: &AppHandle) {
    let entries = tauri::async_runtime::spawn_blocking(read_entries).await.unwrap_or_default();
    let state = app.state::<PluginUiState>();
    let before = {
        let mut plugins = state.plugins.write().expect("plugins lock");
        let next: HashMap<String, Arc<Entry>> = entries.into_iter().map(|e| (e.key.clone(), Arc::new(e))).collect();
        std::mem::replace(&mut *plugins, next)
    };
    views::follow_plugins(app, &before);
    // The agent's plugin tools are offered only while a plugin has views of their kind.
    let kinds = |plugins: &mut dyn Iterator<Item = &Arc<Entry>>| {
        plugins.fold((false, false), |(cards, pages), e| (cards || !e.ui.cards.is_empty(), pages || !e.ui.sidebar.is_empty() || !e.ui.panels.is_empty()))
    };
    let was = kinds(&mut before.values());
    let now = kinds(&mut state.entries().iter());
    if was != now {
        crate::supervisor::tools_changed(app);
    }
}
