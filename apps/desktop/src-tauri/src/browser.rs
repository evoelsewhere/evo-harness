//! In-app browser: each tab is a child webview laid over the side panel's
//! browser area (the UI keeps its bounds in step with the layout). Pages get
//! `browser_page.js` before their own scripts — console capture and the page
//! outline the agent reads — and no IPC to the app.
//!
//! Page events (load start/finish, title, a new window asked for) go to the
//! owning window as `browser://event`. On Windows, `browser_cdp` calls the
//! WebView2 DevTools Protocol for screenshots and real (trusted) input;
//! elsewhere the UI falls back to script-driven clicks and typing.
//!
//! Agent browser (docs/features/agent-browser, FR-4, FR-13, FR-14, FR-16,
//! NFR-1), also Windows only: subscribed DevTools events are kept per tab in a
//! ring and emitted as `browser://cdp`; page dialogs are held open until
//! `browser_dialog_answer` (`browser://dialog`); downloads go to the folder of
//! `browser_download_config` (`browser://download`). The pure parts live in
//! `browser/` (ring, dialog state machine, download names), the WebView2
//! wiring in `browser/hooks.rs`.
//!
//! Tab ownership (T6, FR-24): every page belongs to a chat and a window; the
//! shell remembers the visible page of each chat and the last bounds of the
//! panel (`browser/tabs.rs`), so the agent tools in `agent_tools/browser/`
//! can resolve, create, select and close pages without the webview. The
//! plain functions below (`eval`, `cdp`, `create_for_chat`, …) are what those
//! tools call; the `browser_*` commands wrap them for the webview.

pub mod dialog;
pub mod downloads;
#[cfg(windows)]
mod hooks;
pub mod ring;
pub mod stage;
pub mod tabs;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, WebviewUrl, Window};

const PAGE_SCRIPT: &str = include_str!("browser_page.js");
pub const TAB_PREFIX: &str = "browser-";

/// The script every page gets. Where the shell cannot hold a dialog open (not
/// Windows), the page script answers dialogs itself: it checks this flag.
fn page_script() -> String {
    if cfg!(windows) {
        PAGE_SCRIPT.to_string()
    } else {
        format!("window.__evo_noShellDialogs = true;\n{PAGE_SCRIPT}")
    }
}

/// Completes a page dialog held open by the shell (the WebView2 deferral).
type Completer = Box<dyn FnOnce(dialog::Answer) + Send>;

#[derive(Default)]
pub struct BrowserState {
    /// Pages: owner window, chat, load state, visible page per chat, bounds, agent-driven marks.
    tabs: Mutex<tabs::Tabs>,
    next: AtomicU32,
    /// Tab → the DevTools events it received since subscribing.
    rings: Mutex<HashMap<String, ring::EventRing>>,
    /// (tab, event name) pairs with a DevTools event receiver registered.
    subscribed: Mutex<HashSet<(String, String)>>,
    dialogs: Mutex<dialog::Dialogs<Completer>>,
    downloads: Mutex<downloads::DownloadLog>,
    /// Where downloads go; `None` cancels them.
    download_dir: Mutex<Option<PathBuf>>,
    /// Sites the person approved saving downloads for (FR-16): origins; pages on this machine need none.
    download_origins: Mutex<std::collections::HashSet<String>>,
    /// Which staged pages stream their frames, and at what rate (stage.rs).
    stage: Mutex<stage::Streams>,
}

/// Milliseconds on a monotonic clock, for dialog expiry.
fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

fn dismissal() -> dialog::Answer {
    dialog::Answer { accept: false, text: None }
}

impl BrowserState {
    /// Close every tab of `window` (it closed).
    pub fn close_window(&self, app: &AppHandle, window: &str) {
        let labels: Vec<String> = {
            let mut tabs = self.tabs.lock().expect("browser tabs lock");
            let labels = tabs.labels_of_window(window);
            for label in &labels {
                tabs.remove(label);
            }
            labels
        };
        for label in labels {
            self.forget(&label);
            if let Some(webview) = app.get_webview(&label) {
                let _ = webview.close();
            }
        }
    }

    /// Drop what is kept for a tab that closed: its event ring and
    /// subscriptions, and a dialog it still had open (dismissed, so the page
    /// is not left waiting). Download records stay.
    fn forget(&self, tab: &str) {
        self.rings.lock().expect("browser rings lock").remove(tab);
        self.subscribed.lock().expect("browser subscriptions lock").retain(|(t, _)| t != tab);
        self.stage.lock().expect("browser stage lock").remove(tab);
        let pending = self.dialogs.lock().expect("browser dialogs lock").clear(tab);
        if let Some(complete) = pending {
            complete(dismissal());
        }
    }

    /// Send `payload` to the window that owns `tab`.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn emit_owner<S: Serialize + Clone>(&self, app: &AppHandle, tab: &str, event: &str, payload: S) {
        let owner = self.tabs.lock().expect("browser tabs lock").window_of(tab).map(str::to_string);
        if let Some(owner) = owner {
            let _ = app.emit_to(owner, event, payload);
        }
    }

    /// Remember that `event` is subscribed on `tab`; false when it already was.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn mark_subscribed(&self, tab: &str, event: &str) -> bool {
        self.subscribed.lock().expect("browser subscriptions lock").insert((tab.to_string(), event.to_string()))
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn unmark_subscribed(&self, tab: &str, event: &str) {
        self.subscribed.lock().expect("browser subscriptions lock").remove(&(tab.to_string(), event.to_string()));
    }

    /// Store a DevTools event of `tab`; its sequence number, or `None` when the
    /// tab is gone.
    fn record_event(&self, tab: &str, method: &str, params: Value) -> Option<u64> {
        if !self.tabs.lock().expect("browser tabs lock").contains(tab) {
            return None;
        }
        let mut rings = self.rings.lock().expect("browser rings lock");
        let ring = rings.entry(tab.to_string()).or_insert_with(|| ring::EventRing::new(ring::RING_CAP));
        Some(ring.push(method.to_string(), params))
    }

    /// A DevTools event arrived: keep it and tell the window.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn cdp_event(&self, app: &AppHandle, tab: &str, method: &str, params: Value) {
        // Frames of a staged page are pictures, not events to keep: acknowledge and forward.
        if method == "Page.screencastFrame" {
            self.stage_frame(app, tab, &params);
            return;
        }
        if let Some(seq) = self.record_event(tab, method, params.clone()) {
            self.emit_owner(app, tab, "browser://cdp", json!({ "tab": tab, "seq": seq, "method": method, "params": params }));
        }
    }

    /// A frame of a staged page: acknowledged at once (the page sends the next), and sent to the
    /// window that owns the page while something shows it.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn stage_frame(&self, app: &AppHandle, tab: &str, frame: &Value) {
        if let Some(ack) = stage::ack_params(frame) {
            let (app, tab) = (app.clone(), tab.to_string());
            tauri::async_runtime::spawn(async move {
                let _ = cdp(&app, &tab, "Page.screencastFrameAck", ack).await;
            });
        }
        let seq = {
            let mut streams = self.stage.lock().expect("browser stage lock");
            if !streams.streaming(tab) {
                return;
            }
            streams.next_seq(tab)
        };
        if let Some(payload) = stage::frame_payload(tab, seq, frame) {
            self.emit_owner(app, tab, "browser://frame", payload);
        }
    }

    /// A page dialog opened and is held by `complete`. Dismisses one that was
    /// still open on the tab, tells the window, and dismisses this one after
    /// [`dialog::DIALOG_TTL_MS`] if nobody answers.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn dialog_opened(&self, app: &AppHandle, tab: &str, info: dialog::DialogInfo, complete: Completer) {
        // While an agent drives the page it answers with browser_dialog; otherwise the person does.
        let agent_driven = self.tabs.lock().expect("browser tabs lock").is_driven(tab, now_ms());
        let payload = json!({ "tab": tab, "kind": info.kind, "message": info.message, "url": info.url, "agentDriven": agent_driven });
        let (id, displaced) = self.dialogs.lock().expect("browser dialogs lock").open(tab, info, now_ms(), complete);
        if let Some(old) = displaced {
            old(dismissal());
        }
        self.emit_owner(app, tab, "browser://dialog", payload);
        let (app, tab) = (app.clone(), tab.to_string());
        tauri::async_runtime::spawn(async move {
            // A little past the limit, so the clock check inside agrees.
            tokio::time::sleep(Duration::from_millis(dialog::DIALOG_TTL_MS + 50)).await;
            let expired = app.state::<BrowserState>().dialogs.lock().expect("browser dialogs lock").expire_one(&tab, id, now_ms());
            if let Some(complete) = expired {
                complete(dismissal());
            }
        });
    }

    /// A page started a download: where it goes (or `blocked`), recorded.
    #[cfg_attr(not(windows), allow(dead_code))]
    /// A page on this machine, or on a site the person approved with `browser_download_folder`
    /// (the page's own origin or the file's), may save downloads.
    fn download_allowed(&self, page_url: &str, file_url: &str) -> bool {
        use crate::agent_tools::gate::{is_local_origin, origin_of};
        if is_local_origin(page_url) {
            return true;
        }
        let origins = self.download_origins.lock().expect("browser download origins lock");
        [page_url, file_url].iter().any(|url| !url.is_empty() && origins.contains(&origin_of(url)))
    }

    /// `page_url` is the page the tab shows when the download starts.
    fn begin_download(&self, tab: &str, url: &str, suggested: &str, page_url: &str) -> downloads::Download {
        let folder = self.download_dir.lock().expect("browser download dir lock").clone();
        let folder = folder.filter(|_| self.download_allowed(page_url, url));
        // The folder may have been removed since it was configured.
        let folder = folder.filter(|f| std::fs::create_dir_all(f).is_ok());
        self.downloads.lock().expect("browser downloads lock").begin(tab, url, suggested, folder.as_deref(), &|p| p.exists())
    }

    /// Progress of a download; emits the record when something changed.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn download_changed(&self, app: &AppHandle, id: u64, bytes: Option<u64>, state: Option<downloads::State>) {
        let changed = self.downloads.lock().expect("browser downloads lock").update(id, bytes, state);
        if let Some(record) = changed {
            self.emit_owner(app, &record.tab, "browser://download", record.clone());
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserEvent {
    tab: String,
    kind: &'static str,
    url: Option<String>,
    title: Option<String>,
}

fn emit(app: &AppHandle, window: &str, event: BrowserEvent) {
    let _ = app.emit_to(window, "browser://event", event);
}

/// A URL from what the user typed: a URL, a host, a local path, or a search.
pub fn normalize_url(input: &str) -> Result<Url, String> {
    let text = input.trim();
    if text.is_empty() {
        return Url::parse("about:blank").map_err(|e| e.to_string());
    }
    if let Ok(url) = Url::parse(text) {
        if matches!(url.scheme(), "http" | "https" | "file" | "about" | "data") {
            return Ok(url);
        }
        // `localhost:3000` parses with scheme "localhost".
        if !text.contains("://") && text.split(':').next().is_some_and(|host| host == "localhost" || host.contains('.')) {
            return Url::parse(&format!("http://{text}")).map_err(|e| e.to_string());
        }
        if url.scheme().len() == 1 {
            // A Windows path: C:\…
            return Url::from_file_path(text).map_err(|_| format!("not a file path: {text}"));
        }
        return Err(format!("{} links are not opened here", url.scheme()));
    }
    if text.starts_with('/') || text.starts_with("\\\\") {
        return Url::from_file_path(text).map_err(|_| format!("not a file path: {text}"));
    }
    let host_like = !text.contains(' ') && (text.contains('.') || text.starts_with("localhost") || text.starts_with("127.0.0.1"));
    if host_like {
        let local = text.starts_with("localhost") || text.starts_with("127.") || text.starts_with("[::1]");
        let scheme = if local { "http" } else { "https" };
        return Url::parse(&format!("{scheme}://{text}")).map_err(|e| e.to_string());
    }
    let mut search = Url::parse("https://duckduckgo.com/").expect("search url");
    search.query_pairs_mut().append_pair("q", text);
    Ok(search)
}

pub(crate) fn webview(app: &AppHandle, tab: &str) -> Result<tauri::Webview, String> {
    app.get_webview(tab).ok_or_else(|| format!("browser tab {tab} is closed"))
}

/// Open a page owned by `host`, showing `url` at `bounds` (logical pixels in
/// the window) for `chat`, and register it. Shared by the webview's
/// `browser_create` and the agent tools' [`create_for_chat`].
fn open_page(app: &AppHandle, host: Window, url: Url, chat: Option<String>, bounds: tabs::Bounds, visible: bool) -> Result<String, String> {
    let state = app.state::<BrowserState>();
    let owner = host.label().to_string();
    let label = format!("{TAB_PREFIX}{}", state.next.fetch_add(1, Ordering::Relaxed) + 1);
    let shown_url = url.to_string();

    let (load_app, load_owner) = (app.clone(), owner.clone());
    let (title_app, title_owner) = (app.clone(), owner.clone());
    let (popup_app, popup_owner, popup_label) = (app.clone(), owner.clone(), label.clone());
    let builder = WebviewBuilder::new(&label, WebviewUrl::External(url))
        .initialization_script(page_script())
        .zoom_hotkeys_enabled(true)
        .devtools(true)
        .on_page_load(move |webview, payload| {
            let kind = match payload.event() {
                PageLoadEvent::Started => "loadStart",
                PageLoadEvent::Finished => "loadEnd",
            };
            {
                // The agent tools wait on these (a navigation is done when loading ends).
                let state = load_app.state::<BrowserState>();
                let mut tabs = state.tabs.lock().expect("browser tabs lock");
                tabs.set_loading(webview.label(), kind == "loadStart");
                tabs.set_url(webview.label(), payload.url().as_str());
            }
            emit(
                &load_app,
                &load_owner,
                BrowserEvent { tab: webview.label().to_string(), kind, url: Some(payload.url().to_string()), title: None },
            );
        })
        .on_document_title_changed(move |webview, title| {
            title_app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").set_title(webview.label(), &title);
            emit(
                &title_app,
                &title_owner,
                BrowserEvent { tab: webview.label().to_string(), kind: "title", url: None, title: Some(title) },
            );
        })
        // Links that open a new window open a new tab instead.
        .on_new_window(move |url, _features| {
            emit(
                &popup_app,
                &popup_owner,
                BrowserEvent { tab: popup_label.clone(), kind: "newWindow", url: Some(url.to_string()), title: None },
            );
            NewWindowResponse::Deny
        });

    let child = host
        .add_child(builder, LogicalPosition::new(bounds.x, bounds.y), LogicalSize::new(bounds.width.max(1.0), bounds.height.max(1.0)))
        .map_err(|e| format!("could not open a browser tab: {e}"))?;
    if !visible {
        let _ = child.hide();
    }
    state.tabs.lock().expect("browser tabs lock").insert(&label, &owner, chat, &shown_url);
    #[cfg(windows)]
    hooks::install(app, &label, &child);
    // Record the page's requests from its start, so `browser_network` has headers and bodies.
    crate::agent_tools::browser::network::prime(app, &label);
    Ok(label)
}

/// Open a tab showing `url` at the given bounds (logical pixels in the window)
/// for `chat` (the chat the webview shows; `null` when none is known).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn browser_create(
    app: AppHandle,
    window: Window,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
    chat: Option<String>,
) -> Result<String, String> {
    let url = normalize_url(&url)?;
    open_page(&app, window, url, chat.filter(|c| !c.is_empty()), tabs::Bounds { x, y, width, height }, visible)
}

/// Open a page for an agent tool: in `window` (the one showing the chat; `main`
/// when it is gone), hidden, at the panel's last bounds, owned by `chat`. Unless
/// `background` it becomes the chat's visible page. Tells every window
/// (`browser://created`) so the one that owns it adopts it into the chat's tabs.
pub fn create_for_chat(app: &AppHandle, window: &str, chat: &str, url: &str, background: bool) -> Result<String, String> {
    let url = normalize_url(url)?;
    let host = app.get_window(window).or_else(|| app.get_window("main")).ok_or_else(|| "there is no window to open the page in".to_string())?;
    let state = app.state::<BrowserState>();
    let bounds = state.tabs.lock().expect("browser tabs lock").bounds().unwrap_or(tabs::Bounds::FALLBACK);
    let owner = host.label().to_string();
    let id = open_page(app, host, url.clone(), Some(chat.to_string()), bounds, false)?;
    if !background {
        state.tabs.lock().expect("browser tabs lock").select(chat, &id);
    }
    let _ = app.emit(
        "browser://created",
        json!({ "tab": id, "url": url.to_string(), "chat": chat, "window": owner, "background": background, "loading": true, "title": "" }),
    );
    Ok(id)
}

/// The panel reported where pages show. Remembered for pages a tool creates.
#[tauri::command]
pub fn browser_bounds(app: AppHandle, state: tauri::State<'_, BrowserState>, tab: String, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
    // A staged page stays where the stage put it; the window shows its picture.
    if state.tabs.lock().expect("browser tabs lock").is_staged(&tab) {
        return Ok(());
    }
    state.tabs.lock().expect("browser tabs lock").set_bounds(tabs::Bounds { x, y, width, height });
    let webview = webview(&app, &tab)?;
    webview
        .set_bounds(tauri::Rect {
            position: LogicalPosition::new(x, y).into(),
            size: LogicalSize::new(width.max(1.0), height.max(1.0)).into(),
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn browser_visible(app: AppHandle, state: tauri::State<'_, BrowserState>, tab: String, visible: bool, focus: Option<bool>) -> Result<(), String> {
    if state.tabs.lock().expect("browser tabs lock").is_staged(&tab) {
        return Ok(());
    }
    let webview = webview(&app, &tab)?;
    if visible {
        webview.show().map_err(|e| e.to_string())?;
        if focus == Some(true) {
            let _ = webview.set_focus();
        }
    } else {
        webview.hide().map_err(|e| e.to_string())?;
    }
    state.tabs.lock().expect("browser tabs lock").set_shown(&tab, visible);
    Ok(())
}

/// Put a page on the stage (`on`) or take it off, and start, change or stop its frames: `fps`
/// frames per second, none for no stream. A staged page is a visible webview of the desktop size
/// parked outside the window (see `stage.rs`); taking it off hides it, and the panel places it
/// again. Windows only (it needs DevTools).
#[tauri::command]
pub async fn browser_stage(app: AppHandle, tab: String, on: bool, fps: Option<u32>) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("the stage needs the DevTools protocol (Windows)".into());
    }
    let view = webview(&app, &tab)?;
    let state = app.state::<BrowserState>();
    if on {
        view.set_bounds(tauri::Rect {
            position: LogicalPosition::new(stage::PARK_X, stage::PARK_Y).into(),
            size: LogicalSize::new(stage::WIDTH, stage::HEIGHT).into(),
        })
        .map_err(|e| e.to_string())?;
        view.show().map_err(|e| e.to_string())?;
        state.tabs.lock().expect("browser tabs lock").set_staged(&tab, true);
        state.tabs.lock().expect("browser tabs lock").set_shown(&tab, false);
    }
    let change = state.stage.lock().expect("browser stage lock").want(&tab, if on { fps } else { None });
    match change {
        stage::Change::Start(rate) => {
            cdp_subscribe(&app, &tab, vec!["Page.screencastFrame".to_string()]).await?;
            let _ = cdp(&app, &tab, "Page.stopScreencast", json!({})).await;
            cdp(&app, &tab, "Page.startScreencast", stage::screencast_params(rate)).await?;
        }
        stage::Change::Stop => {
            let _ = cdp(&app, &tab, "Page.stopScreencast", json!({})).await;
        }
        stage::Change::Keep => {}
    }
    if !on {
        state.tabs.lock().expect("browser tabs lock").set_staged(&tab, false);
        let _ = view.hide();
    }
    Ok(())
}

/// The webview shows `tab` as `chat`'s visible page (it changed, or the chat's
/// tab list did). A page of no known chat joins `chat`. False when refused.
#[tauri::command]
pub fn browser_select_tab(state: tauri::State<'_, BrowserState>, chat: String, tab: String) -> bool {
    state.tabs.lock().expect("browser tabs lock").select(&chat, &tab)
}

/// A page the webview adopted after a reload that the shell has no chat for: it belongs to `chat`.
#[tauri::command]
pub fn browser_set_chat(state: tauri::State<'_, BrowserState>, tab: String, chat: String) -> bool {
    state.tabs.lock().expect("browser tabs lock").assign_chat(&tab, &chat)
}

/// An agent tool made `tab` the visible page of `chat`: the webview follows (`browser://selected`).
pub fn select_tab_for_tool(app: &AppHandle, chat: &str, tab: &str) -> bool {
    let selected = app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").select(chat, tab);
    if selected {
        let _ = app.emit("browser://selected", json!({ "chat": chat, "tab": tab }));
    }
    selected
}

/// Navigate to what the user typed (URL, host or search); returns the URL used.
pub fn navigate_tab(app: &AppHandle, tab: &str, url: &str) -> Result<String, String> {
    let target = normalize_url(url)?;
    webview(app, tab)?.navigate(target.clone()).map_err(|e| e.to_string())?;
    let state = app.state::<BrowserState>();
    let mut tabs = state.tabs.lock().expect("browser tabs lock");
    tabs.set_url(tab, target.as_str());
    tabs.set_loading(tab, true);
    Ok(target.to_string())
}

#[tauri::command]
pub fn browser_navigate(app: AppHandle, tab: String, url: String) -> Result<String, String> {
    navigate_tab(&app, &tab, &url)
}

/// back | forward | reload | stop
pub fn history_tab(app: &AppHandle, tab: &str, action: &str) -> Result<(), String> {
    let webview = webview(app, tab)?;
    match action {
        "reload" => {
            app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").set_loading(tab, true);
            webview.reload().map_err(|e| e.to_string())
        }
        "back" => webview.eval("history.back()").map_err(|e| e.to_string()),
        "forward" => webview.eval("history.forward()").map_err(|e| e.to_string()),
        "stop" => webview.eval("window.stop()").map_err(|e| e.to_string()),
        other => Err(format!("unknown browser action {other}")),
    }
}

#[tauri::command]
pub fn browser_history(app: AppHandle, tab: String, action: String) -> Result<(), String> {
    history_tab(&app, &tab, &action)
}

/// The address the tab's webview shows now.
pub fn native_url(app: &AppHandle, tab: &str) -> Result<String, String> {
    webview(app, tab)?.url().map(|u| u.to_string()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn browser_url(app: AppHandle, tab: String) -> Result<String, String> {
    native_url(&app, &tab)
}

/// Close the page and forget what is kept for it.
pub fn close_tab(app: &AppHandle, tab: &str) {
    let state = app.state::<BrowserState>();
    state.tabs.lock().expect("browser tabs lock").remove(tab);
    state.forget(tab);
    if let Some(webview) = app.get_webview(tab) {
        let _ = webview.close();
    }
}

/// An agent tool closed `tab`: the webview drops it too (`browser://closed`).
pub fn close_tab_for_tool(app: &AppHandle, tab: &str) {
    let chat = app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").get(tab).and_then(|t| t.chat.clone());
    close_tab(app, tab);
    let _ = app.emit("browser://closed", json!({ "tab": tab, "chat": chat }));
}

#[tauri::command]
pub fn browser_close(app: AppHandle, tab: String) {
    close_tab(&app, &tab);
}

#[tauri::command]
pub fn browser_devtools(app: AppHandle, tab: String) -> Result<(), String> {
    webview(&app, &tab)?.open_devtools();
    Ok(())
}

#[tauri::command]
pub fn browser_zoom(app: AppHandle, tab: String, scale: f64) -> Result<(), String> {
    webview(&app, &tab)?.set_zoom(scale.clamp(0.25, 5.0)).map_err(|e| e.to_string())
}

// ── What the agent tools ask of the page registry ───────────────────────────

/// What the shell knows of `tab`.
pub fn tab_record(app: &AppHandle, tab: &str) -> Option<tabs::Tab> {
    app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").get(tab).cloned()
}

/// A chat's pages, oldest first.
pub fn chat_tabs(app: &AppHandle, chat: &str) -> Vec<(String, tabs::Tab)> {
    app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").of_chat(chat)
}

/// The page a chat shows (selected, else its newest).
pub fn visible_tab(app: &AppHandle, chat: &str) -> Option<String> {
    app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").visible_of(chat)
}

/// The tab a tool call of `chat` works on (`tab_id`, else the visible one), marked as driven by an agent.
pub fn resolve_tab(app: &AppHandle, chat: &str, tab_id: Option<&str>) -> Result<String, String> {
    let state = app.state::<BrowserState>();
    let mut tabs = state.tabs.lock().expect("browser tabs lock");
    let tab = tabs.resolve(chat, tab_id)?;
    let first = tabs.mark_driven(&tab, now_ms());
    drop(tabs);
    if first {
        announce_driven(app, &tab, chat);
    }
    Ok(tab)
}

/// Tell the windows an agent is involved with `tab` (once per page): `browser://driven`.
fn announce_driven(app: &AppHandle, tab: &str, chat: &str) {
    let _ = app.emit("browser://driven", json!({ "tab": tab, "chat": chat }));
}

/// Like [`resolve_tab`] for a `tab_id` that may be wrong: `None` instead of an error (no marking).
pub fn find_tab(app: &AppHandle, chat: &str, tab_id: Option<&str>) -> Option<String> {
    app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").resolve(chat, tab_id).ok()
}

/// A tool is using `tab`: dialogs it opens are the agent's to answer for two minutes.
pub fn mark_driven(app: &AppHandle, tab: &str) {
    let state = app.state::<BrowserState>();
    let (first, chat) = {
        let mut tabs = state.tabs.lock().expect("browser tabs lock");
        let first = tabs.mark_driven(tab, now_ms());
        (first, tabs.chat_of(tab).map(str::to_string))
    };
    if first {
        announce_driven(app, tab, chat.as_deref().unwrap_or(""));
    }
}

/// The panel shows `tab`.
pub fn is_shown(app: &AppHandle, tab: &str) -> bool {
    app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").is_shown(tab)
}

/// The panel shows some page of `chat`.
pub fn any_shown(app: &AppHandle, chat: &str) -> bool {
    app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").any_shown_of(chat)
}

/// Counts the bounds reports of the panel; a change means the panel took its place.
pub fn bounds_seq(app: &AppHandle) -> u64 {
    app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").bounds_seq()
}

/// Whether the page is loading, by the load events of the webview.
pub fn is_loading(app: &AppHandle, tab: &str) -> bool {
    app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").get(tab).is_some_and(|t| t.loading)
}

pub fn set_loading(app: &AppHandle, tab: &str, loading: bool) {
    app.state::<BrowserState>().tabs.lock().expect("browser tabs lock").set_loading(tab, loading);
}

/// How long `browser_eval` waits for the page, including a returned promise.
const EVAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Turns the result into something JSON can carry: elements as a short tag,
/// errors as their message, Map/Set as arrays, functions as their source.
const EVAL_SERIALIZE: &str = r#"(v) => {
  const seen = new WeakSet();
  const walk = (x, depth) => {
    if (x === undefined) return null;
    if (typeof x === "function") return `[function ${x.name || "anonymous"}]`;
    if (typeof x === "bigint") return `${x}n`;
    if (typeof x === "symbol") return x.toString();
    if (x === null || typeof x !== "object") return x;
    if (seen.has(x)) return "[circular]";
    if (depth > 8) return "[…]";
    seen.add(x);
    if (x instanceof Error) return `${x.name}: ${x.message}`;
    if (typeof Element !== "undefined" && x instanceof Element) {
      const attrs = ["id", "class", "type", "name", "href", "value"]
        .filter((a) => x.hasAttribute(a))
        .map((a) => `${a}="${String(x.getAttribute(a)).slice(0, 60)}"`);
      return `<${[x.tagName.toLowerCase(), ...attrs].join(" ")}>`;
    }
    if (typeof Node !== "undefined" && x instanceof Node) return `[${x.nodeName}]`;
    if (x instanceof Map) return [...x.entries()].map(([k, val]) => [walk(k, depth + 1), walk(val, depth + 1)]);
    if (x instanceof Set || (typeof NodeList !== "undefined" && x instanceof NodeList) || (typeof HTMLCollection !== "undefined" && x instanceof HTMLCollection)) return [...x].map((i) => walk(i, depth + 1));
    if (Array.isArray(x)) return x.map((i) => walk(i, depth + 1));
    if (typeof x.toJSON === "function") { try { return walk(x.toJSON(), depth + 1); } catch {} }
    const out = {};
    for (const key of Object.keys(x)) { try { out[key] = walk(x[key], depth + 1); } catch {} }
    return out;
  };
  return walk(v, 0);
}"#;

/// The page-side wrapper: runs `body` (which yields the value), keeps a
/// returned promise under `key` for polling, and reports exceptions.
fn eval_wrapper(body: &str, key: &str) -> String {
    format!(
        r#"(() => {{
  const __ser = {EVAL_SERIALIZE};
  const __err = (e) => ({{ __error: e instanceof Error ? `${{e.name}}: ${{e.message}}` : String(e) }});
  try {{
    const __r = {body};
    if (__r && typeof __r.then === "function") {{
      const pending = (window.__evoPending = window.__evoPending || {{}});
      pending["{key}"] = {{ done: false }};
      __r.then(
        (v) => {{ pending["{key}"] = {{ done: true, value: __ser(v) }}; }},
        (e) => {{ pending["{key}"] = {{ done: true, error: __err(e).__error }}; }},
      );
      return {{ __pending: "{key}" }};
    }}
    return __ser(__r);
  }} catch (e) {{
    return __err(e);
  }}
}})()"#
    )
}

async fn eval_raw(webview: &tauri::Webview, script: String) -> Result<String, String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = Mutex::new(Some(tx));
    webview
        .eval_with_callback(script, move |json| {
            if let Some(tx) = tx.lock().ok().and_then(|mut slot| slot.take()) {
                let _ = tx.send(json);
            }
        })
        .map_err(|e| e.to_string())?;
    tokio::time::timeout(EVAL_TIMEOUT, rx)
        .await
        .map_err(|_| format!("the page did not answer within {}s", EVAL_TIMEOUT.as_secs()))?
        .map_err(|_| "the page went away".to_string())
}

/// Run `script` in the page; resolves to its JSON result (`null` for undefined).
/// A returned promise is awaited. With `statements`, a script that is not a
/// single expression (`a(); b`) runs as a program and yields its last value.
pub async fn eval(app: &AppHandle, tab: &str, script: &str, statements: bool) -> Result<Value, String> {
    let webview = webview(app, tab)?;
    let key = format!(
        "e{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    // A single expression runs as written: no eval(), so page security policies don't apply.
    let mut json = eval_raw(
        &webview,
        eval_wrapper(&format!("(() => {{ return ({script}\n); }})()"), &key),
    )
    .await?;
    // An empty answer means the script did not compile as an expression.
    if json.trim().is_empty() {
        if !statements {
            return Err("the script is not a valid JavaScript expression".into());
        }
        let source = serde_json::to_string(script).map_err(|e| e.to_string())?;
        json = eval_raw(
            &webview,
            eval_wrapper(&format!("(0, eval)({source})"), &key),
        )
        .await?;
        if json.trim().is_empty() {
            return Err("the script could not run".into());
        }
    }
    let value: Value = serde_json::from_str(&json).unwrap_or(Value::String(json));
    if let Some(error) = value.get("__error").and_then(Value::as_str) {
        if error.starts_with("EvalError") {
            return Err(format!(
                "{error}\nThis page's security policy blocks running statements; pass one expression instead, e.g. (() => {{ …; return value; }})()"
            ));
        }
        return Err(error.to_string());
    }
    let Some(pending) = value
        .get("__pending")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(value);
    };
    // A promise: poll until it settles.
    let started = std::time::Instant::now();
    let probe = format!(
        r#"(() => {{ const p = window.__evoPending && window.__evoPending["{pending}"]; if (!p) return {{ gone: true }}; if (p.done) delete window.__evoPending["{pending}"]; return p; }})()"#
    );
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let json = eval_raw(&webview, probe.clone()).await?;
        let state: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
        if state.get("gone").is_some() || state.is_null() {
            return Err("the page navigated away before the promise settled".into());
        }
        if state.get("done").and_then(Value::as_bool) == Some(true) {
            if let Some(error) = state.get("error").and_then(Value::as_str) {
                return Err(format!("the promise rejected: {error}"));
            }
            return Ok(state.get("value").cloned().unwrap_or(Value::Null));
        }
        if started.elapsed() > EVAL_TIMEOUT {
            let _ = eval_raw(
                &webview,
                format!(r#"window.__evoPending && delete window.__evoPending["{pending}"]"#),
            )
            .await;
            return Err(format!(
                "the promise did not settle within {}s",
                EVAL_TIMEOUT.as_secs()
            ));
        }
    }
}

#[tauri::command]
pub async fn browser_eval(app: AppHandle, tab: String, script: String, statements: Option<bool>) -> Result<Value, String> {
    eval(&app, &tab, &script, statements == Some(true)).await
}

/// One page a window owns, as `browser_list` reports it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedTab {
    pub id: String,
    pub url: String,
    pub title: String,
    pub loading: bool,
    /// The chat the page belongs to; `null` when unknown.
    pub chat: Option<String>,
}

/// This window's open tabs, with their chat (a reloaded window adopts them).
#[tauri::command]
pub fn browser_list(app: AppHandle, window: Window, state: tauri::State<'_, BrowserState>) -> Vec<ListedTab> {
    let tabs = state.tabs.lock().expect("browser tabs lock");
    tabs.of_window(window.label())
        .into_iter()
        .filter_map(|(id, tab)| {
            let url = app.get_webview(&id)?.url().map(|u| u.to_string()).unwrap_or(tab.url);
            Some(ListedTab { id, url, title: tab.title, loading: tab.loading, chat: tab.chat })
        })
        .collect()
}

/// Call a DevTools Protocol method on the tab (Windows / WebView2 only).
pub async fn cdp(app: &AppHandle, tab: &str, method: &str, params: Value) -> Result<Value, String> {
    let webview = webview(app, tab)?;
    cdp_on(&webview, method.to_string(), params).await
}

#[tauri::command]
pub async fn browser_cdp(app: AppHandle, tab: String, method: String, params: Option<Value>) -> Result<Value, String> {
    cdp(&app, &tab, &method, params.unwrap_or(Value::Object(Default::default()))).await
}

#[cfg(windows)]
async fn cdp_on(webview: &tauri::Webview, method: String, params: Value) -> Result<Value, String> {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows_core::HSTRING;

    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    let params = params.to_string();
    webview
        .with_webview(move |platform| {
            let result = (|| -> Result<(), String> {
                let core = unsafe { platform.controller().CoreWebView2() }.map_err(|e| e.to_string())?;
                let tx = std::sync::Mutex::new(Some(tx));
                let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
                    if let Some(tx) = tx.lock().ok().and_then(|mut slot| slot.take()) {
                        let _ = tx.send(result.map(|()| json).map_err(|e| e.message().to_string()));
                    }
                    Ok(())
                }));
                unsafe { core.CallDevToolsProtocolMethod(&HSTRING::from(method), &HSTRING::from(params), &handler) }
                    .map_err(|e| e.to_string())
            })();
            if let Err(error) = result {
                log_cdp_error(&error);
            }
        })
        .map_err(|e| e.to_string())?;
    let json = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| "the browser did not answer within 30s".to_string())?
        .map_err(|_| "the browser call was dropped".to_string())??;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn log_cdp_error(error: &str) {
    eprintln!("browser devtools call failed: {error}");
}

#[cfg(not(windows))]
async fn cdp_on(_webview: &tauri::Webview, _method: String, _params: Value) -> Result<Value, String> {
    Err("unsupported: the DevTools protocol is only available on Windows".into())
}

/// What the built-in browser can do on this platform (the `browser_status`
/// tool lists it; the tools fall back to page scripts where a flag is false).
#[derive(Serialize)]
pub struct BrowserStatus {
    /// `browser_cdp` works.
    pub devtools: bool,
    /// `browser_cdp_subscribe` / `browser_cdp_events` work.
    pub events: bool,
    /// The shell holds page dialogs open for `browser_dialog_answer`.
    pub dialogs: bool,
    /// Downloads are handled by the shell.
    pub downloads: bool,
    pub platform: &'static str,
}

pub fn status(app: &AppHandle, tab: &str) -> Result<BrowserStatus, String> {
    webview(app, tab)?;
    let windows = cfg!(windows);
    Ok(BrowserStatus {
        devtools: windows,
        events: windows,
        dialogs: windows,
        downloads: windows,
        platform: if windows {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        },
    })
}

#[tauri::command]
pub fn browser_status(app: AppHandle, tab: String) -> Result<BrowserStatus, String> {
    status(&app, &tab)
}

/// Start keeping the named DevTools events of the tab (`Network.requestWillBeSent`,
/// …). Idempotent per tab and event. The caller enables the protocol domains
/// itself with `cdp`.
pub async fn cdp_subscribe(app: &AppHandle, tab: &str, events: Vec<String>) -> Result<(), String> {
    let webview = webview(app, tab)?;
    subscribe(app, &webview, tab, events).await
}

#[tauri::command]
pub async fn browser_cdp_subscribe(app: AppHandle, tab: String, events: Vec<String>) -> Result<(), String> {
    cdp_subscribe(&app, &tab, events).await
}

#[cfg(windows)]
async fn subscribe(app: &AppHandle, webview: &tauri::Webview, tab: &str, events: Vec<String>) -> Result<(), String> {
    hooks::subscribe(app, webview, tab, events).await
}

#[cfg(not(windows))]
async fn subscribe(_app: &AppHandle, _webview: &tauri::Webview, _tab: &str, _events: Vec<String>) -> Result<(), String> {
    Err("not available on this platform".into())
}

/// DevTools events of the tab with `seq >= since`, oldest first: at most
/// `limit` (default 200), only those named in `names` when given. Pass `next`
/// as `since` next time; `dropped` counts events since `since` the ring lost.
pub fn cdp_events(app: &AppHandle, tab: &str, since: u64, names: Option<&[String]>, limit: Option<usize>) -> Result<ring::Page, String> {
    let state = app.state::<BrowserState>();
    if !state.tabs.lock().expect("browser tabs lock").contains(tab) {
        return Err(format!("browser tab {tab} is closed"));
    }
    let rings = state.rings.lock().expect("browser rings lock");
    Ok(match rings.get(tab) {
        Some(ring) => ring.read(since, names, limit.unwrap_or(ring::DEFAULT_LIMIT)),
        None => ring::Page { events: Vec::new(), next: since.max(1), dropped: 0 },
    })
}

#[tauri::command]
pub fn browser_cdp_events(app: AppHandle, tab: String, since: u64, names: Option<Vec<String>>, limit: Option<usize>) -> Result<ring::Page, String> {
    cdp_events(&app, &tab, since, names.as_deref(), limit)
}

/// The dialog the tab is waiting on, if any.
pub fn dialog_pending(app: &AppHandle, tab: &str) -> Option<dialog::DialogInfo> {
    app.state::<BrowserState>().dialogs.lock().expect("browser dialogs lock").pending(tab).cloned()
}

#[tauri::command]
pub fn browser_dialog_pending(app: AppHandle, tab: String) -> Option<dialog::DialogInfo> {
    dialog_pending(&app, &tab)
}

/// Accept or dismiss the tab's open dialog (`text` answers a prompt). False
/// when no dialog is open.
pub fn dialog_answer(app: &AppHandle, tab: &str, accept: bool, text: Option<String>) -> bool {
    let answered = app.state::<BrowserState>().dialogs.lock().expect("browser dialogs lock").answer(tab, accept, text);
    match answered {
        Some((complete, answer)) => {
            complete(answer);
            true
        }
        None => false,
    }
}

#[tauri::command]
pub fn browser_dialog_answer(app: AppHandle, tab: String, accept: bool, text: Option<String>) -> bool {
    dialog_answer(&app, &tab, accept, text)
}

/// Set (for every tab) the folder downloads are saved into; `None` cancels
/// downloads and forgets the approved sites, which is the default. `site` is
/// the page whose downloads the person approved (the approval asked about it).
pub fn download_config(app: &AppHandle, folder: Option<&str>, site: &str) -> Result<(), String> {
    let folder = match folder.map(str::trim).filter(|f| !f.is_empty()) {
        Some(folder) => Some(downloads::prepare_folder(folder)?),
        None => None,
    };
    let state = app.state::<BrowserState>();
    let mut origins = state.download_origins.lock().expect("browser download origins lock");
    if folder.is_none() {
        origins.clear();
    } else if !site.is_empty() {
        origins.insert(crate::agent_tools::gate::origin_of(site));
    }
    drop(origins);
    *state.download_dir.lock().expect("browser download dir lock") = folder;
    Ok(())
}

#[tauri::command]
pub fn browser_download_config(app: AppHandle, folder: Option<String>, site: Option<String>) -> Result<(), String> {
    download_config(&app, folder.as_deref(), site.as_deref().unwrap_or(""))
}

/// Downloads pages started, oldest first, of one tab or of all.
pub fn downloads_of(app: &AppHandle, tab: Option<&str>) -> Vec<downloads::Download> {
    app.state::<BrowserState>().downloads.lock().expect("browser downloads lock").list(tab)
}

#[tauri::command]
pub fn browser_downloads(app: AppHandle, tab: Option<String>) -> Vec<downloads::Download> {
    downloads_of(&app, tab.as_deref())
}

#[cfg(test)]
mod tests {
    use super::{dialog, downloads, normalize_url, BrowserState};
    use serde_json::json;

    fn state_with_tab(tab: &str) -> BrowserState {
        let state = BrowserState::default();
        state.tabs.lock().unwrap().insert(tab, "main", None, "about:blank");
        state
    }

    #[test]
    fn events_are_kept_per_tab_and_dropped_with_it() {
        let state = state_with_tab("browser-1");
        assert_eq!(state.record_event("browser-1", "Network.requestWillBeSent", json!({ "requestId": "1" })), Some(1));
        assert_eq!(state.record_event("browser-1", "Network.loadingFinished", json!({})), Some(2));
        assert_eq!(state.record_event("browser-9", "Network.loadingFinished", json!({})), None, "unknown tab");
        {
            let rings = state.rings.lock().unwrap();
            let page = rings["browser-1"].read(0, None, 200);
            assert_eq!(page.events.len(), 2);
            assert_eq!(page.next, 3);
        }
        state.forget("browser-1");
        assert!(state.rings.lock().unwrap().get("browser-1").is_none());
    }

    #[test]
    fn subscribing_twice_registers_once() {
        let state = state_with_tab("browser-1");
        assert!(state.mark_subscribed("browser-1", "Network.requestWillBeSent"));
        assert!(!state.mark_subscribed("browser-1", "Network.requestWillBeSent"));
        assert!(state.mark_subscribed("browser-1", "Page.frameNavigated"));
        assert!(state.mark_subscribed("browser-2", "Network.requestWillBeSent"));
        state.unmark_subscribed("browser-1", "Network.requestWillBeSent");
        assert!(state.mark_subscribed("browser-1", "Network.requestWillBeSent"));
        state.forget("browser-1");
        assert!(state.mark_subscribed("browser-1", "Page.frameNavigated"), "closing the tab forgets its subscriptions");
        assert!(!state.mark_subscribed("browser-2", "Network.requestWillBeSent"));
    }

    #[test]
    fn a_site_downloads_only_after_the_person_approved_it() {
        let state = BrowserState::default();
        let dir = std::env::temp_dir().join(format!("evo-browser-origins-{}", std::process::id()));
        *state.download_dir.lock().unwrap() = Some(dir.clone());
        // A folder is set but nobody approved this site: blocked; a page on this machine is fine.
        let blocked = state.begin_download("browser-1", "https://files.example/a.zip", "a.zip", "https://example.com/page");
        assert_eq!(blocked.state, downloads::State::Blocked);
        let local = state.begin_download("browser-1", "https://files.example/a.zip", "b.zip", "http://127.0.0.1:8765/");
        assert_eq!(local.state, downloads::State::Started);
        // Approving the page's site lets it (and a file it links from elsewhere) save.
        state.download_origins.lock().unwrap().insert("https://example.com".to_string());
        let approved = state.begin_download("browser-1", "https://files.example/a.zip", "c.zip", "https://example.com/page");
        assert_eq!(approved.state, downloads::State::Started);
        let other = state.begin_download("browser-1", "https://files.example/a.zip", "d.zip", "https://other.example/");
        assert_eq!(other.state, downloads::State::Blocked);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn closing_a_tab_dismisses_its_dialog() {
        let state = state_with_tab("browser-1");
        let (tx, rx) = std::sync::mpsc::channel();
        let info = dialog::DialogInfo { kind: "confirm".into(), message: "sure?".into(), default_text: None, url: "https://x.test/".into() };
        state.dialogs.lock().unwrap().open("browser-1", info, 0, Box::new(move |answer: dialog::Answer| tx.send(answer).unwrap()) as super::Completer);
        state.forget("browser-1");
        assert_eq!(rx.try_recv().unwrap(), dialog::Answer { accept: false, text: None });
        assert!(state.dialogs.lock().unwrap().pending("browser-1").is_none());
    }

    #[test]
    fn downloads_are_blocked_until_a_folder_is_set_and_never_overwrite() {
        let state = BrowserState::default();
        let blocked = state.begin_download("browser-1", "https://x.test/a.txt", r"C:\Users\me\Downloads\a.txt", "http://localhost:3000/");
        assert_eq!(blocked.state, downloads::State::Blocked);
        assert_eq!(blocked.name, "a.txt");

        let dir = std::env::temp_dir().join(format!("evo-browser-downloads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        *state.download_dir.lock().unwrap() = Some(dir.clone());
        let first = state.begin_download("browser-1", "https://x.test/a.txt", "a.txt", "http://localhost:3000/");
        assert_eq!(first.state, downloads::State::Started);
        assert!(dir.is_dir(), "the folder is created");
        std::fs::write(&first.path, b"x").unwrap();
        let second = state.begin_download("browser-1", "https://x.test/a.txt", "a.txt", "http://localhost:3000/");
        assert_eq!(second.name, "a (1).txt");
        assert_eq!(state.downloads.lock().unwrap().list(Some("browser-1")).len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn urls_hosts_paths_and_searches() {
        assert_eq!(normalize_url("https://example.com/a").unwrap().as_str(), "https://example.com/a");
        assert_eq!(normalize_url("example.com").unwrap().as_str(), "https://example.com/");
        assert_eq!(normalize_url("localhost:3000").unwrap().as_str(), "http://localhost:3000/");
        assert_eq!(normalize_url("127.0.0.1:8080/x").unwrap().as_str(), "http://127.0.0.1:8080/x");
        assert!(normalize_url("rust borrow checker").unwrap().as_str().starts_with("https://duckduckgo.com/?q=rust+borrow+checker"));
        assert!(normalize_url("javascript:alert(1)").is_err());
        #[cfg(windows)]
        assert!(normalize_url(r"C:\Users\x\index.html").unwrap().as_str().starts_with("file:///C:/Users/x/index.html"));
    }
}
