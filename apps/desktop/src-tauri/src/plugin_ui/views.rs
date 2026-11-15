//! Plugin views as native child webviews `plugin-<n>` of an app window
//! (docs/features/plugin-ui/spec.md, FR-4 – FR-7, FR-9, FR-15).
//!
//! The window's slot (Phase 3: `PluginSlot`) opens a view, keeps it at the
//! slot's bounds, hides it under overlays and closes it. A view is created
//! hidden and shown once its page has loaded. At most [`CARD_CAP`] card views
//! live per window; opening another closes the oldest, whose slot then shows
//! its placeholder. Views of a plugin that is turned off or removed close at
//! once; an updated plugin's views reload.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, WebviewUrl, Window};

use super::bridge::Bucket;
use super::{Entry, PluginUiState, SCHEME, VIEW_PREFIX};

/// Live card views per window (plan 3.5).
pub const CARD_CAP: usize = 6;
/// The largest card data (FR-7).
pub const MAX_CARD_DATA: usize = 64 * 1024;

/// An open view.
pub struct Live {
    pub label: String,
    /// The plugin's id and host key.
    pub plugin: String,
    pub key: String,
    /// The window it is a child of.
    pub window: String,
    pub slot: String,
    pub view: String,
    /// A card's data (`view.data()`); `null` elsewhere.
    pub data: Value,
    /// The slot wants it shown; it shows once loaded.
    pub visible: bool,
    pub loaded: bool,
    /// Chats whose events it gets; `""` is the window's active chat (bridge.rs).
    pub subscriptions: HashSet<String>,
    pub bucket: Bucket,
    /// Opening order, for the card cap.
    pub seq: u32,
}

/// Bounds in logical pixels of the window.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    fn rect(self) -> tauri::Rect {
        tauri::Rect {
            position: LogicalPosition::new(self.x, self.y).into(),
            size: LogicalSize::new(self.width.max(1.0), self.height.max(1.0)).into(),
        }
    }
}

/// The `window.evo` client with this view's boot data (client.js).
fn client_script(entry: &Entry, slot: &str, view: &str, title: &str, label: &str, theme: &Value) -> String {
    let boot = json!({
        "plugin": { "id": entry.id, "name": entry.name },
        "view": { "id": view, "slot": slot, "title": title },
        "label": label,
        "theme": theme,
    });
    include_str!("client.js").replace("__EVO_BOOT__", &boot.to_string())
}

/// Whether a view of `key` may navigate to `url`: its own origin only.
pub fn own_origin(key: &str, url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    match url.scheme() {
        s if s == SCHEME => host == key,
        "http" | "https" => host.strip_prefix(SCHEME).and_then(|rest| rest.strip_prefix('.')) == Some(key),
        "about" => url.as_str() == "about:blank",
        _ => false,
    }
}

fn emit_owner(app: &AppHandle, window: &str, event: &str, payload: Value) {
    let _ = app.emit_to(window, event, payload);
}

/// Close `label` and forget it; its window's slot learns why
/// (`plugin-ui://closed`: `off`, `cap` or `closed`).
fn close_view(app: &AppHandle, label: &str, reason: &str) {
    let live = app.state::<PluginUiState>().views.lock().expect("views lock").remove(label);
    if let Some(webview) = app.get_webview(label) {
        let _ = webview.close();
    }
    if let Some(live) = live {
        if reason != "closed" {
            emit_owner(app, &live.window, "plugin-ui://closed", json!({ "label": label, "plugin": live.plugin, "reason": reason }));
        }
    }
}

/// The view `label`, checked to belong to `window`.
fn owned(app: &AppHandle, window: &Window, label: &str) -> Result<tauri::Webview, String> {
    let state = app.state::<PluginUiState>();
    let views = state.views.lock().expect("views lock");
    match views.get(label) {
        Some(live) if live.window == window.label() => {}
        _ => return Err(format!("no plugin view {label} in this window")),
    }
    drop(views);
    app.get_webview(label).ok_or_else(|| format!("plugin view {label} is closed"))
}

/// Open view `view` of `plugin`'s `slot` at `bounds`, hidden until its page
/// loads (`plugin-ui://loaded`). `data` is a card's data. Returns its label.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn plugin_view_open(
    app: AppHandle,
    window: Window,
    plugin: String,
    slot: String,
    view: String,
    bounds: Bounds,
    visible: Option<bool>,
    data: Option<Value>,
) -> Result<String, String> {
    let state = app.state::<PluginUiState>();
    let _one_at_a_time = state.creating.lock().await;
    let entry = state.plugin_by_id(&plugin).ok_or_else(|| format!("the plugin {plugin} is off or not installed"))?;
    let found = entry.view(&slot, &view).ok_or_else(|| format!("{} has no {slot} view `{view}`", entry.name))?;
    let data = data.unwrap_or(Value::Null);
    if slot == "cards" && data.to_string().len() > MAX_CARD_DATA {
        return Err("a card's data is larger than 64 KB".into());
    }
    let url: Url = format!("{SCHEME}://{}/{}", entry.key, found.entry).parse().map_err(|e| format!("{e}"))?;
    let seq = state.next.fetch_add(1, Ordering::Relaxed) + 1;
    let label = format!("{VIEW_PREFIX}{seq}");
    let owner = window.label().to_string();

    // The card cap: the oldest live card of this window makes room.
    if slot == "cards" {
        let oldest = {
            let views = state.views.lock().expect("views lock");
            let cards: Vec<(&String, u32)> = views.values().filter(|v| v.window == owner && v.slot == "cards").map(|v| (&v.label, v.seq)).collect();
            (cards.len() >= CARD_CAP).then(|| cards.iter().min_by_key(|(_, seq)| *seq).map(|(label, _)| (*label).clone())).flatten()
        };
        if let Some(oldest) = oldest {
            close_view(&app, &oldest, "cap");
        }
    }

    let script = client_script(&entry, &slot, &view, &found.title, &label, &state.theme());
    let (nav_key, load_app) = (entry.key.clone(), app.clone());
    let builder = WebviewBuilder::new(&label, WebviewUrl::CustomProtocol(url))
        .initialization_script(script)
        .devtools(entry.developer_mode || cfg!(debug_assertions))
        // HTML drag and drop inside the view (a sticky wall) needs the page's own handler.
        .disable_drag_drop_handler()
        .on_navigation(move |url| own_origin(&nav_key, url))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_page_load(move |webview, payload| {
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            let state = load_app.state::<PluginUiState>();
            let (show, window) = {
                let mut views = state.views.lock().expect("views lock");
                let Some(live) = views.get_mut(webview.label()) else { return };
                live.loaded = true;
                (live.visible, live.window.clone())
            };
            if show {
                let _ = webview.show();
            }
            emit_owner(&load_app, &window, "plugin-ui://loaded", json!({ "label": webview.label() }));
        });

    state.views.lock().expect("views lock").insert(
        label.clone(),
        Live {
            label: label.clone(),
            plugin: entry.id.clone(),
            key: entry.key.clone(),
            window: owner,
            slot,
            view,
            data,
            visible: visible.unwrap_or(true),
            loaded: false,
            subscriptions: HashSet::new(),
            bucket: Bucket::new(),
            seq,
        },
    );
    let created = window.add_child(builder, LogicalPosition::new(bounds.x, bounds.y), LogicalSize::new(bounds.width.max(1.0), bounds.height.max(1.0)));
    // A view opening starts its plugin's code when it sleeps (plugin-settings FR-11).
    if let Some(mcp) = app.try_state::<crate::mcp::McpState>() {
        let pool = mcp.0.clone();
        let name = entry.name.clone();
        tauri::async_runtime::spawn(async move {
            for key in pool.plugin_keys(&name) {
                let _ = pool.wake(&key).await;
            }
        });
    }
    match created {
        Ok(child) => {
            // Shown when its page has loaded, unless the load already finished.
            let loaded = state.views.lock().expect("views lock").get(&label).is_some_and(|live| live.loaded && live.visible);
            if !loaded {
                let _ = child.hide();
            }
            Ok(label)
        }
        Err(error) => {
            state.views.lock().expect("views lock").remove(&label);
            Err(format!("could not open the view: {error}"))
        }
    }
}

// These run off the main thread (async): a webview call made on it while
// another view is being created there re-enters WebView2 and can hang the app.

/// The slot moved or resized.
#[tauri::command]
pub async fn plugin_view_bounds(app: AppHandle, window: Window, label: String, bounds: Bounds) -> Result<(), String> {
    owned(&app, &window, &label)?.set_bounds(bounds.rect()).map_err(|e| e.to_string())
}

/// The slot is shown or covered (an overlay, another page). A view shows
/// only once its page has loaded.
#[tauri::command]
pub async fn plugin_view_visible(app: AppHandle, window: Window, label: String, visible: bool) -> Result<(), String> {
    let webview = owned(&app, &window, &label)?;
    let loaded = {
        let state = app.state::<PluginUiState>();
        let mut views = state.views.lock().expect("views lock");
        let live = views.get_mut(&label).ok_or("closed")?;
        live.visible = visible;
        live.loaded
    };
    if visible && loaded {
        webview.show().map_err(|e| e.to_string())
    } else if !visible {
        webview.hide().map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

/// Load the view's page again (the slot's Reload; Developer mode).
#[tauri::command]
pub async fn plugin_view_reload(app: AppHandle, window: Window, label: String) -> Result<(), String> {
    owned(&app, &window, &label)?.reload().map_err(|e| e.to_string())
}

/// The slot went away.
#[tauri::command]
pub async fn plugin_view_close(app: AppHandle, window: Window, label: String) -> Result<(), String> {
    owned(&app, &window, &label)?;
    close_view(&app, &label, "closed");
    Ok(())
}

/// The window's page (re)started: views an earlier page of it opened are
/// nobody's now (a reload keeps the native views), so they close.
#[tauri::command]
pub async fn plugin_view_reset(app: AppHandle, window: Window) {
    let labels: Vec<String> = {
        let state = app.state::<PluginUiState>();
        let views = state.views.lock().expect("views lock");
        views.values().filter(|live| live.window == window.label()).map(|live| live.label.clone()).collect()
    };
    for label in labels {
        close_view(&app, &label, "closed");
    }
}

/// The app's theme changed: `{tokens: {name: value}, dark}`. Every view
/// follows (FR-12); views opened later start with it.
#[tauri::command]
pub async fn plugin_view_theme(app: AppHandle, window: Window, tokens: HashMap<String, String>, dark: bool) {
    if window.label() != "main" {
        return;
    }
    let theme = json!({ "tokens": tokens, "dark": dark });
    let state = app.state::<PluginUiState>();
    state.set_theme(theme.clone());
    let labels: Vec<String> = state.views.lock().expect("views lock").keys().cloned().collect();
    for label in labels {
        dispatch(&app, &label, &json!({ "kind": "theme", "theme": theme }));
    }
}

/// Send `event` to the view `label` (`window.__evo_dispatch`).
pub fn dispatch(app: &AppHandle, label: &str, event: &Value) {
    if let Some(webview) = app.get_webview(label) {
        let _ = webview.eval(format!("window.__evo_dispatch && window.__evo_dispatch({event})"));
    }
}

/// The plugins were read again (mod.rs): close the views of a plugin that is
/// off, gone or no longer has that view (FR-15); reload those of a plugin
/// whose files or network origins changed (an update).
pub fn follow_plugins(app: &AppHandle, before: &HashMap<String, Arc<Entry>>) {
    let state = app.state::<PluginUiState>();
    let mut close = Vec::new();
    let mut reload = Vec::new();
    {
        let views = state.views.lock().expect("views lock");
        for live in views.values() {
            match state.plugin(&live.key) {
                Some(entry) if entry.view(&live.slot, &live.view).is_some() => {
                    let changed = before.get(&live.key).is_none_or(|old| old.root != entry.root || old.network() != entry.network() || old.ui != entry.ui);
                    if changed {
                        reload.push(live.label.clone());
                    }
                }
                _ => close.push(live.label.clone()),
            }
        }
    }
    for label in close {
        close_view(app, &label, "off");
    }
    for label in reload {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.reload();
        }
    }
}

/// A window closed: forget its views (their webviews went with it).
pub fn window_closed(app: &AppHandle, window: &str) {
    if let Some(state) = app.try_state::<PluginUiState>() {
        state.views.lock().expect("views lock").retain(|_, live| live.window != window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_navigate_only_within_their_own_origin() {
        let key = "sticky-wall-1a2b3c4d";
        let ok = ["evo-plugin://sticky-wall-1a2b3c4d/ui/b.html", "http://evo-plugin.sticky-wall-1a2b3c4d/x.html", "about:blank"];
        for url in ok {
            assert!(own_origin(key, &url.parse().unwrap()), "{url}");
        }
        let refused = [
            "evo-plugin://other-00000000/ui/a.html",
            "http://evo-plugin.other-00000000/x.html",
            "https://example.com/",
            "http://tauri.localhost/",
            "file:///C:/Windows/win.ini",
            "javascript:alert(1)",
        ];
        for url in refused {
            assert!(!own_origin(key, &url.parse().unwrap()), "{url}");
        }
    }
}
