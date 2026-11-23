//! Quick entry: a small message box that opens over any app from a global
//! shortcut or the tray icon. Enter answers in the box; Ctrl+Enter starts a
//! new chat with the text in the main window
//! (docs/features/quick-entry/spec.md, FR-1–FR-22; plan 3.5).
//!
//! * The box is its own window (`quick-entry`, page `quick-entry.html`):
//!   frameless, transparent, always on top, out of the taskbar. It is built
//!   hidden while quick entry is on, so it shows at once (FR-1, NFR-1).
//! * The shortcut (`system.quickEntryShortcut`) and a left click on the tray
//!   icon toggle it (FR-2, FR-3). It opens on the monitor under the mouse
//!   (FR-4) and hides on Esc or when it loses focus, keeping its text (FR-7).
//! * Sending hands the text to the main window (`app://quick-entry`), which
//!   starts the chat as **New chat** does; a main window built again takes it
//!   once loaded (`quick_entry_pending`, FR-5, FR-6). Only the main window's
//!   engine store talks to engines: the box never does.
//! * Settings › General turns it on and off and records the shortcut; both
//!   apply at once (FR-9).
//! * Answers in the box: the box is a viewer window of one chat at a time
//!   (ADR 0018). The shell keeps that chat, its last use, the pin and the
//!   pinned position, decides on every show whether the box keeps its chat
//!   ([`keep_chat`], FR-18), hands a chat to the main window at a target
//!   (`quick_entry_open`, FR-15) and sends notice clicks for the box's chat
//!   to the box (`quick-entry://notice`, FR-19).
//! * While it shows a chat the box can be resized by its edges; a size the
//!   person gives it is kept and the page fills it (FR-23).

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use evo_store::logs::NewLog;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::db::Db;

/// The box's window label.
pub const LABEL: &str = "quick-entry";
const ENABLED_KEY: &str = "system.quickEntry";
const SHORTCUT_KEY: &str = "system.quickEntryShortcut";
/// Minutes after which the box opens empty; null is Never (FR-18).
const NEW_CHAT_KEY: &str = "system.quickEntryNewChatAfter";
/// The default of [`NEW_CHAT_KEY`].
pub const NEW_CHAT_AFTER: u64 = 15;
/// The window's logical width: the card fills the window, which has no
/// margin to catch clicks; the system draws its shadow and corners.
const WIDTH: f64 = 656.0;
/// The window's logical height with one line of text.
const HEIGHT: f64 = 48.0;
/// The window grows with the text up to this, then the field scrolls (FR-8),
/// and never stops below it once it shows a chat.
const MAX_HEIGHT: f64 = 320.0;
/// A box with a chat grows up to this share of its monitor's height (FR-11).
const MONITOR_SHARE: f64 = 0.6;
/// The smallest box the person can resize to (logical px, FR-23).
const MIN_WIDTH: f64 = 420.0;
const MIN_HEIGHT: f64 = 160.0;
/// How far down the monitor the box's top sits.
const TOP_SHARE: f64 = 0.22;
/// Focus changes right after showing are the show's own (a tray click).
const BLUR_GRACE: Duration = Duration::from_millis(300);
/// A tray click right after the box hid on blur was meant to close it.
const REOPEN_GRACE: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct QuickEntryState {
    enabled: AtomicBool,
    /// The saved shortcut, as the person recorded it.
    shortcut: Mutex<String>,
    /// The saved shortcut is registered with the OS.
    registered: AtomicBool,
    shown_at: Mutex<Option<Instant>>,
    /// When the box last hid because it lost focus.
    blurred_at: Mutex<Option<Instant>>,
    /// Text sent while there was no main window, for the one being built.
    pending: Mutex<Option<String>>,
    /// The chat the box shows; `None`: empty (FR-11, FR-18).
    chat: Mutex<Option<String>>,
    /// When the box's chat was last used (ms since the epoch): a message
    /// sent or a reply finished (FR-18).
    last_used: Mutex<Option<u64>>,
    /// Minutes unused after which the box opens empty; `None`: Never.
    new_chat_after: Mutex<Option<u64>>,
    /// Pinned: blur does not hide the box (FR-17).
    pinned: AtomicBool,
    /// Where a pinned box was left (physical px), until it is unpinned.
    position: Mutex<Option<(i32, i32)>>,
    /// A size the person gave the box (logical px), kept until the app quits (FR-23).
    user_size: Mutex<Option<(f64, f64)>>,
    /// The size the shell last gave the window (physical px): a different
    /// size is the person's resize.
    expected: Mutex<Option<(u32, u32)>>,
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ── Decisions, free of the app ─────────────────────────────────────────────

/// The shortcut quick entry starts with, on every platform: Alt+Space is
/// Windows' system menu and other assistants' box, and Ctrl+Alt+Space is
/// taken by other desktop assistants too (spec 6.3).
pub fn default_shortcut() -> &'static str {
    "Alt+Shift+Space"
}

/// `text` as a shortcut the box can use: it parses and has a modifier, so it
/// never takes a plain key from every app.
pub fn check_shortcut(text: &str) -> Result<Shortcut, String> {
    let shortcut = Shortcut::from_str(text.trim()).map_err(|_| format!("\"{}\" is not a shortcut", text.trim()))?;
    if shortcut.mods.is_empty() {
        return Err("A shortcut needs Ctrl, Alt, Shift or Command with its key".into());
    }
    Ok(shortcut)
}

/// What a left click on the tray icon does (FR-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayClick {
    QuickEntry,
    Main,
}

pub fn tray_click(enabled: bool, window: bool) -> TrayClick {
    if enabled && window {
        TrayClick::QuickEntry
    } else {
        TrayClick::Main
    }
}

/// The box's top-left corner on a monitor at `origin` of `area` (physical
/// pixels), for a box of `size`: centred, [`TOP_SHARE`] down (FR-4).
pub fn placement(origin: (i32, i32), area: (u32, u32), size: (u32, u32)) -> (i32, i32) {
    let x = origin.0 + (area.0 as i32 - size.0 as i32) / 2;
    let room = area.1.saturating_sub(size.1) as f64;
    let y = origin.1 + (area.1 as f64 * TOP_SHARE).min(room) as i32;
    (x.max(origin.0), y)
}

/// The text a send starts a chat with: trimmed, nothing when blank (FR-5).
pub fn prompt_text(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// The window's logical height for a page asking for `height`, up to
/// `limit` (FR-8, FR-11).
pub fn window_height(height: f64, limit: f64) -> f64 {
    let limit = limit.max(HEIGHT);
    if height.is_finite() {
        height.clamp(HEIGHT, limit)
    } else {
        HEIGHT
    }
}

/// How tall the box may grow on a monitor `monitor_height` logical pixels
/// tall: [`MONITOR_SHARE`] of it, never less than the field's limit (FR-11).
pub fn height_limit(monitor_height: Option<f64>) -> f64 {
    match monitor_height {
        Some(height) if height.is_finite() && height > 0.0 => (height * MONITOR_SHARE).max(MAX_HEIGHT),
        _ => MAX_HEIGHT,
    }
}

/// Whether the box opens on its chat rather than empty (FR-18): the chat
/// still `exists` (not deleted or archived); one that is `busy` (running or
/// waiting for the person) is always kept; otherwise it is kept while it was
/// used no longer than `after` minutes ago (`None`: Never starts anew).
pub fn keep_chat(exists: bool, busy: bool, last_used: Option<u64>, now: u64, after: Option<u64>) -> bool {
    if !exists {
        return false;
    }
    if busy {
        return true;
    }
    match (after, last_used) {
        (None, _) | (_, None) => true,
        (Some(minutes), Some(used)) => now.saturating_sub(used) <= minutes.saturating_mul(60_000),
    }
}

/// Where a pinned box left at `position` opens: there while a monitor
/// (`origin`, `size`, physical px) still holds that point, else nowhere and
/// the usual placement is used (FR-4, FR-17).
pub fn pinned_placement(position: Option<(i32, i32)>, monitors: &[((i32, i32), (u32, u32))]) -> Option<(i32, i32)> {
    let (x, y) = position?;
    monitors
        .iter()
        .any(|&((left, top), (width, height))| {
            x >= left && y >= top && i64::from(x) < i64::from(left) + i64::from(width) && i64::from(y) < i64::from(top) + i64::from(height)
        })
        .then_some((x, y))
}

/// The saved **Start a new chat after**: missing is the default, null is
/// Never, a number is minutes (at least one) (FR-9, FR-18).
pub fn new_chat_after(saved: Option<&Value>) -> Option<u64> {
    match saved {
        None => Some(NEW_CHAT_AFTER),
        Some(Value::Null) => None,
        Some(value) => Some(value.as_u64().unwrap_or(NEW_CHAT_AFTER).max(1)),
    }
}

/// Whether the window's new `size` (physical px) is the person's resize
/// rather than the size the shell last set, `expected` (FR-23). Rounding
/// may move a size the shell set by a pixel.
pub fn is_users_resize(expected: Option<(u32, u32)>, size: (u32, u32)) -> bool {
    expected.is_some_and(|(width, height)| width.abs_diff(size.0) > 2 || height.abs_diff(size.1) > 2)
}

/// The window's logical size for a card asking for `height`: the person's
/// size while the box shows a chat; otherwise the width they chose (or the
/// default) and a height that follows the card up to `limit` (FR-8, FR-11, FR-23).
pub fn box_size(user: Option<(f64, f64)>, chat: bool, height: f64, limit: f64) -> (f64, f64) {
    match user {
        Some(size) if chat => size,
        Some((width, _)) => (width, window_height(height, limit)),
        None => (WIDTH, window_height(height, limit)),
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

// ── The app ─────────────────────────────────────────────────────────────────

fn saved_text(db: &Db, key: &str) -> Option<String> {
    db.with(|conn| evo_store::settings::all(conn))
        .ok()
        .and_then(|settings| settings.get(key).and_then(Value::as_str).map(str::to_string))
        .filter(|text| !text.trim().is_empty())
}

/// Build the hidden box. Call it at setup or off the event loop's thread:
/// on Windows a window built there (a tray click, a sync command) deadlocks.
fn build(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("quick-entry.html".into()))
        .title(format!("{} quick entry", crate::window::app_name()))
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        // The card fills the window, so the system's shadow (and, on
        // Windows 11, its rounded corners) frame the box itself; a margin
        // for a drawn shadow would be an invisible border that takes clicks.
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        // Files dropped on the box reach its page as an HTML drop (FR-24):
        // the shell's own drop handler finds no WebView2 window to attach to
        // in a window built hidden, and turns WebView2's own drops off.
        .disable_drag_drop_handler()
        .visible(false)
        .focused(false)
        .build()?;
    Ok(())
}

fn register(app: &AppHandle, text: &str) -> Result<(), String> {
    let shortcut = check_shortcut(text)?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|error| format!("Evo could not use {}: another app may use it ({error})", text.trim()))
}

fn unregister(app: &AppHandle, text: &str) {
    if let Ok(shortcut) = check_shortcut(text) {
        let _ = app.global_shortcut().unregister(shortcut);
    }
}

/// Register the saved shortcut; a failure is logged and shown in Settings.
fn register_saved(app: &AppHandle) {
    let state = app.state::<QuickEntryState>();
    let shortcut = locked(&state.shortcut).clone();
    match register(app, &shortcut) {
        Ok(()) => state.registered.store(true, Ordering::Release),
        Err(error) => {
            state.registered.store(false, Ordering::Release);
            app.state::<Db>().log(NewLog::new("warn", "shell", format!("quick entry shortcut: {error}")));
        }
    }
}

/// At launch: read the settings; with quick entry on, build the box and
/// register the shortcut (FR-1, FR-2).
pub fn setup(app: &AppHandle) {
    let db = app.state::<Db>();
    let state = app.state::<QuickEntryState>();
    *locked(&state.shortcut) = saved_text(&db, SHORTCUT_KEY).unwrap_or_else(|| default_shortcut().to_string());
    let saved = db.with(|conn| evo_store::settings::all(conn)).ok();
    *locked(&state.new_chat_after) = new_chat_after(saved.as_ref().and_then(|all| all.get(NEW_CHAT_KEY)));
    if !crate::system::saved_flag(&db, ENABLED_KEY, true) {
        return;
    }
    state.enabled.store(true, Ordering::Release);
    if let Err(error) = build(app) {
        db.log(NewLog::new("warn", "shell", format!("no quick entry window: {error}")));
    }
    register_saved(app);
}

/// Quick entry's settings: on, the shortcut, whether it is registered, and
/// the minutes after which the box starts a new chat (`None`: Never).
pub fn settings(app: &AppHandle) -> (bool, String, bool, Option<u64>) {
    let state = app.state::<QuickEntryState>();
    let enabled = state.enabled.load(Ordering::Acquire);
    let shortcut = locked(&state.shortcut).clone();
    let after = *locked(&state.new_chat_after);
    (enabled, shortcut, !enabled || state.registered.load(Ordering::Acquire), after)
}

/// Set **Start a new chat after** (`None`: Never); applies on the next show
/// (FR-9, FR-18).
pub fn set_new_chat_after(app: &AppHandle, minutes: Option<u64>) -> Result<(), String> {
    let minutes = minutes.map(|m| m.max(1));
    let value = minutes.map_or(Value::Null, |m| json!(m));
    app.state::<Db>().with(|conn| evo_store::settings::set(conn, NEW_CHAT_KEY, &value))?;
    *locked(&app.state::<QuickEntryState>().new_chat_after) = minutes;
    Ok(())
}

/// Turn quick entry on or off, at once (FR-9).
pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<QuickEntryState>();
    let db = app.state::<Db>();
    if enabled && !state.enabled.swap(true, Ordering::AcqRel) {
        let builder = app.clone();
        // Off the command's thread (see `build`).
        tauri::async_runtime::spawn(async move {
            if let Err(error) = build(&builder) {
                builder.state::<Db>().log(NewLog::new("warn", "shell", format!("no quick entry window: {error}")));
            }
        });
        register_saved(app);
    } else if !enabled && state.enabled.swap(false, Ordering::AcqRel) {
        if state.registered.swap(false, Ordering::AcqRel) {
            unregister(app, &locked(&state.shortcut));
        }
        if let Some(window) = app.get_webview_window(LABEL) {
            let _ = window.destroy();
        }
    }
    db.with(|conn| evo_store::settings::set(conn, ENABLED_KEY, &Value::Bool(enabled)))
}

/// Use `text` as the shortcut: the new one is registered before the old one
/// lets go, so a shortcut another app holds leaves the old one working (FR-9).
pub fn set_shortcut(app: &AppHandle, text: &str) -> Result<(), String> {
    let text = text.trim().to_string();
    let wanted = check_shortcut(&text)?;
    let state = app.state::<QuickEntryState>();
    let old = locked(&state.shortcut).clone();
    if state.enabled.load(Ordering::Acquire) {
        let registered = state.registered.load(Ordering::Acquire);
        let same = check_shortcut(&old).is_ok_and(|old| old == wanted);
        if !(same && registered) {
            register(app, &text)?;
            if registered && !same {
                unregister(app, &old);
            }
            state.registered.store(true, Ordering::Release);
        }
    }
    app.state::<Db>().with(|conn| evo_store::settings::set(conn, SHORTCUT_KEY, &Value::String(text.clone())))?;
    *locked(&state.shortcut) = text;
    Ok(())
}

/// Put the box on the monitor under the mouse (FR-4).
fn place(app: &AppHandle, window: &tauri::WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let own = window.scale_factor().unwrap_or(1.0);
    let (width, height) = window
        .inner_size()
        .ok()
        .map(|size| (size.width as f64 / own, size.height as f64 / own))
        .unwrap_or((WIDTH, HEIGHT));
    let size = ((width * scale) as u32, (height * scale) as u32);
    let origin = monitor.position();
    let area = monitor.size();
    let (x, y) = placement((origin.x, origin.y), (area.width, area.height), size);
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Put a pinned box where it was left, when a monitor still holds that
/// point; otherwise the usual placement (FR-4, FR-17).
fn place_pinned(app: &AppHandle, window: &tauri::WebviewWindow) -> bool {
    let state = app.state::<QuickEntryState>();
    if !state.pinned.load(Ordering::Acquire) {
        return false;
    }
    let monitors: Vec<((i32, i32), (u32, u32))> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| ((m.position().x, m.position().y), (m.size().width, m.size().height)))
        .collect();
    let Some((x, y)) = pinned_placement(*locked(&state.position), &monitors) else {
        return false;
    };
    let _ = window.set_position(PhysicalPosition::new(x, y));
    true
}

/// The chat the box opens on: its own, unless it is gone, archived, or
/// unused for longer than the set time while not busy (FR-18).
fn chat_to_show(app: &AppHandle) -> Option<String> {
    let state = app.state::<QuickEntryState>();
    let chat = locked(&state.chat).clone()?;
    let row = app.state::<Db>().with(|conn| evo_store::chats::get_chat(conn, &chat)).ok().flatten();
    let exists = row.is_some_and(|row| row.archived_at.is_none());
    let busy = exists
        && app
            .state::<crate::supervisor::EngineSupervisor>()
            .working(&crate::supervisor::AppHost(app.clone()), &chat);
    let kept = keep_chat(exists, busy, *locked(&state.last_used), now_ms(), *locked(&state.new_chat_after));
    if !kept {
        *locked(&state.chat) = None;
        *locked(&state.last_used) = None;
    }
    kept.then_some(chat)
}

fn show(app: &AppHandle, window: &tauri::WebviewWindow) {
    if !place_pinned(app, window) {
        place(app, window);
    }
    let state = app.state::<QuickEntryState>();
    *locked(&state.shown_at) = Some(Instant::now());
    let chat = chat_to_show(app);
    let pinned = state.pinned.load(Ordering::Acquire);
    // A box resized by the person fills that size while it shows a chat (FR-23).
    let fixed = chat.is_some() && locked(&state.user_size).is_some();
    let _ = window.show();
    let _ = window.set_focus();
    // The page shows the chat (or empties), puts the caret in the field and
    // reads the theme again (FR-18, FR-22).
    let _ = app.emit_to(LABEL, "quick-entry://shown", json!({ "chat": chat, "pinned": pinned, "fixed": fixed }));
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
        // The page stops drawing its chat while hidden (NFR-3).
        let _ = app.emit_to(LABEL, "quick-entry://hidden", ());
    }
}

/// Open the box, or hide it when it is open (FR-2).
pub fn toggle(app: &AppHandle) {
    if crate::system::quitting(app) || !app.state::<QuickEntryState>().enabled.load(Ordering::Acquire) {
        return;
    }
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
        hide(app);
    } else {
        show(app, &window);
    }
}

/// A left click on the tray icon (FR-3).
pub fn tray_clicked(app: &AppHandle) {
    let state = app.state::<QuickEntryState>();
    let window = app.get_webview_window(LABEL).is_some();
    match tray_click(state.enabled.load(Ordering::Acquire), window) {
        TrayClick::Main => {
            crate::system::show_main(app);
        }
        TrayClick::QuickEntry => {
            // Pressing the icon took focus from the box, which hid: this
            // click closes it rather than opening it again.
            let blurred = locked(&state.blurred_at).take();
            if blurred.is_some_and(|at| at.elapsed() < REOPEN_GRACE) {
                return;
            }
            toggle(app);
        }
    }
}

/// The box lost focus: it hides, keeping its text and its chat, unless it
/// is pinned (FR-7, FR-17).
pub fn on_focus_lost(window: &tauri::Window) {
    if window.label() != LABEL {
        return;
    }
    let state = window.state::<QuickEntryState>();
    if state.pinned.load(Ordering::Acquire) {
        return;
    }
    let just_shown = locked(&state.shown_at).is_some_and(|at| at.elapsed() < BLUR_GRACE);
    if just_shown || !window.is_visible().unwrap_or(false) {
        return;
    }
    *locked(&state.blurred_at) = Some(Instant::now());
    let _ = window.hide();
    let _ = window.app_handle().emit_to(LABEL, "quick-entry://hidden", ());
}

/// The box's window changed size: when the person resized it, that size is
/// kept and the page fills it (FR-23).
pub fn on_resized(window: &tauri::Window, size: PhysicalSize<u32>) {
    if window.label() != LABEL || !window.is_visible().unwrap_or(false) || !window.is_resizable().unwrap_or(false) {
        return;
    }
    let state = window.state::<QuickEntryState>();
    if !is_users_resize(*locked(&state.expected), (size.width, size.height)) {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let first = locked(&state.user_size).replace((size.width as f64 / scale, size.height as f64 / scale)).is_none();
    *locked(&state.expected) = Some((size.width, size.height));
    if first || locked(&state.chat).is_some() {
        let _ = window.app_handle().emit_to(LABEL, "quick-entry://sized", json!({ "fixed": true }));
    }
}

/// A pinned box was dragged: it opens there next time (FR-17).
pub fn on_moved(window: &tauri::Window, position: PhysicalPosition<i32>) {
    if window.label() != LABEL || !window.is_visible().unwrap_or(false) {
        return;
    }
    let state = window.state::<QuickEntryState>();
    if state.pinned.load(Ordering::Acquire) {
        *locked(&state.position) = Some((position.x, position.y));
    }
}

/// A notification of `chat` was clicked: when the box shows that chat, it
/// decides between itself and the main window (`quick-entry://notice`);
/// false when the box does not hold it (FR-19).
pub fn notice_clicked(app: &AppHandle, chat: &str) -> bool {
    if app.get_webview_window(LABEL).is_none() {
        return false;
    }
    if locked(&app.state::<QuickEntryState>().chat).as_deref() != Some(chat) {
        return false;
    }
    app.emit_to(LABEL, "quick-entry://notice", json!({ "chat": chat })).is_ok()
}

/// The box sends its text: it hides, and the main window starts a new chat
/// with it, built again first when it is gone (FR-5, FR-6).
#[tauri::command]
pub fn quick_entry_submit(app: AppHandle, text: String) {
    hide(&app);
    let Some(text) = prompt_text(&text) else {
        return;
    };
    if app.get_window(crate::system::MAIN).is_some() {
        crate::system::show_main(&app);
        let _ = app.emit_to(crate::system::MAIN, "app://quick-entry", json!({ "text": text }));
    } else {
        *locked(&app.state::<QuickEntryState>().pending) = Some(text);
        crate::system::show_main(&app);
    }
}

/// Esc in the box (FR-7).
#[tauri::command]
pub fn quick_entry_hide(app: AppHandle) {
    hide(&app);
}

/// The box's card grew or shrank to `height` logical pixels (FR-8, FR-11).
#[tauri::command]
pub fn quick_entry_resize(window: tauri::WebviewWindow, height: f64) {
    if window.label() != LABEL {
        return;
    }
    let state = window.state::<QuickEntryState>();
    let chat = locked(&state.chat).is_some();
    let user = *locked(&state.user_size);
    // A size the person gave the box stays while it shows a chat (FR-23).
    if chat && user.is_some() {
        return;
    }
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| monitor.size().height as f64 / monitor.scale_factor());
    let (width, height) = box_size(user, chat, height, height_limit(monitor));
    let scale = window.scale_factor().unwrap_or(1.0);
    *locked(&state.expected) = Some(((width * scale).round() as u32, (height * scale).round() as u32));
    let _ = window.set_size(LogicalSize::new(width, height));
}

/// The box now holds `chat` (`None`: empty); `used`: a message was sent or a
/// reply finished just now (FR-18).
#[tauri::command]
pub fn quick_entry_chat(state: State<'_, QuickEntryState>, window: tauri::Window, chat: Option<String>, used: bool) {
    if window.label() != LABEL {
        return;
    }
    let mut held = locked(&state.chat);
    let changed = *held != chat;
    // Resizable by its edges while it shows a chat (FR-23).
    let showing = chat.is_some();
    if window.is_resizable().unwrap_or(false) != showing {
        let _ = window.set_resizable(showing);
        let _ = window.set_min_size(showing.then(|| LogicalSize::new(MIN_WIDTH, MIN_HEIGHT)));
        // A chat again in a box the person resized: back to their size.
        let user = *locked(&state.user_size);
        if let (true, Some((width, height))) = (showing, user) {
            let scale = window.scale_factor().unwrap_or(1.0);
            *locked(&state.expected) = Some(((width * scale).round() as u32, (height * scale).round() as u32));
            let _ = window.set_size(LogicalSize::new(width, height));
            let _ = window.app_handle().emit_to(LABEL, "quick-entry://sized", json!({ "fixed": true }));
        }
    }
    *held = chat;
    let mut last = locked(&state.last_used);
    if used {
        *last = Some(now_ms());
    } else if changed {
        *last = None;
    }
}

/// Pin or unpin the box; unpinning forgets where it was left (FR-17).
#[tauri::command]
pub fn quick_entry_pin(state: State<'_, QuickEntryState>, window: tauri::WebviewWindow, pinned: bool) {
    if window.label() != LABEL {
        return;
    }
    state.pinned.store(pinned, Ordering::Release);
    let mut position = locked(&state.position);
    *position = if pinned {
        window.outer_position().ok().map(|p| (p.x, p.y))
    } else {
        None
    };
}

/// Show the box (after a notice click the box answers itself, FR-19).
#[tauri::command]
pub fn quick_entry_show(app: AppHandle) {
    if crate::system::quitting(&app) {
        return;
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        show(&app, &window);
    }
}

/// Hide the box (pinned too) and show `chat` in the main window at `target`
/// (a question, a turn, a side panel tab); the chat keeps running and the
/// box keeps it (FR-6, FR-12, FR-14, FR-15).
#[tauri::command]
pub fn quick_entry_open(app: AppHandle, chat: String, target: Option<Value>) {
    hide(&app);
    crate::system::open_in_main_at(&app, &chat, target);
}

/// Text sent while there was no main window: the one built again starts a
/// chat with it once loaded; taken once (FR-6).
#[tauri::command]
pub fn quick_entry_pending(state: State<'_, QuickEntryState>, window: tauri::Window) -> Option<String> {
    if window.label() != crate::system::MAIN {
        return None;
    }
    locked(&state.pending).take()
}

#[cfg(test)]
mod tests {
    use super::*;

    // docs/features/quick-entry/spec.md (T1).

    #[test]
    fn the_default_shortcut_is_usable_and_avoids_the_windows_system_menu() {
        assert!(check_shortcut(default_shortcut()).is_ok());
        assert_eq!(default_shortcut(), "Alt+Shift+Space");
        // Spaces around it are fine; a plain key or nonsense is not.
        assert!(check_shortcut(" Ctrl+Shift+K ").is_ok());
        assert!(check_shortcut("CommandOrControl+Shift+Space").is_ok());
        assert!(check_shortcut("Space").is_err());
        assert!(check_shortcut("K").is_err());
        assert!(check_shortcut("Ctrl+Nope").is_err());
        assert!(check_shortcut("").is_err());
        // The same keys written another way are the same shortcut.
        assert_eq!(check_shortcut("ctrl+alt+space").unwrap(), check_shortcut("Ctrl+Alt+Space").unwrap());
    }

    #[test]
    fn a_tray_click_opens_the_box_only_while_quick_entry_is_on() {
        assert_eq!(tray_click(true, true), TrayClick::QuickEntry);
        assert_eq!(tray_click(false, true), TrayClick::Main);
        assert_eq!(tray_click(false, false), TrayClick::Main);
        // On, but its window could not be built: the main window, as before.
        assert_eq!(tray_click(true, false), TrayClick::Main);
    }

    #[test]
    fn the_box_opens_centred_in_the_upper_part_of_the_monitor() {
        // A 1920×1080 monitor at the origin, a 680×84 box.
        assert_eq!(placement((0, 0), (1920, 1080), (680, 84)), (620, 237));
        // A second monitor to the left, scaled: its own origin counts.
        assert_eq!(placement((-2560, 0), (2560, 1440), (1020, 126)), (-2560 + 770, 316));
        // A box wider or taller than the monitor stays on it.
        assert_eq!(placement((100, 50), (600, 200), (680, 190)), (100, 60));
    }

    #[test]
    fn a_send_starts_a_chat_only_with_text() {
        assert_eq!(prompt_text("  hello\n"), Some("hello".into()));
        assert_eq!(prompt_text("line one\nline two"), Some("line one\nline two".into()));
        assert_eq!(prompt_text(""), None);
        assert_eq!(prompt_text(" \n\t "), None);
    }

    #[test]
    fn the_box_grows_with_its_text_up_to_a_limit() {
        assert_eq!(window_height(10.0, MAX_HEIGHT), HEIGHT);
        assert_eq!(window_height(150.0, MAX_HEIGHT), 150.0);
        assert_eq!(window_height(5000.0, MAX_HEIGHT), MAX_HEIGHT);
        assert_eq!(window_height(f64::NAN, MAX_HEIGHT), HEIGHT);
        // A limit below one line still leaves one line.
        assert_eq!(window_height(150.0, 10.0), HEIGHT);
    }

    // Answers in the box (T8).

    #[test]
    fn a_box_with_a_chat_grows_up_to_sixty_percent_of_its_monitor() {
        // A 1080 px tall monitor: 648 px.
        assert_eq!(height_limit(Some(1080.0)), 648.0);
        assert_eq!(window_height(5000.0, height_limit(Some(1080.0))), 648.0);
        // A short monitor never drops below the field's own limit.
        assert_eq!(height_limit(Some(400.0)), MAX_HEIGHT);
        // No monitor known: the field's limit.
        assert_eq!(height_limit(None), MAX_HEIGHT);
        assert_eq!(height_limit(Some(f64::NAN)), MAX_HEIGHT);
    }

    #[test]
    fn the_box_keeps_its_chat_until_the_set_time_passes() {
        let minute = 60_000;
        let now = 100 * minute;
        // Used 10 minutes ago, 15 minutes set: kept; 16 minutes ago: anew.
        assert!(keep_chat(true, false, Some(now - 10 * minute), now, Some(15)));
        assert!(keep_chat(true, false, Some(now - 15 * minute), now, Some(15)));
        assert!(!keep_chat(true, false, Some(now - 16 * minute), now, Some(15)));
        // Never: always kept.
        assert!(keep_chat(true, false, Some(0), now, None));
        // Running or waiting for the person: kept however old.
        assert!(keep_chat(true, true, Some(0), now, Some(5)));
        // Gone or archived: never kept, busy or not.
        assert!(!keep_chat(false, true, Some(now), now, None));
        // Held but never used (just attached): kept.
        assert!(keep_chat(true, false, None, now, Some(5)));
    }

    #[test]
    fn a_pinned_box_opens_where_it_was_left_while_a_monitor_holds_it() {
        let monitors = [((0, 0), (1920, 1080)), ((-2560, 0), (2560, 1440))];
        assert_eq!(pinned_placement(Some((100, 200)), &monitors), Some((100, 200)));
        assert_eq!(pinned_placement(Some((-1000, 900)), &monitors), Some((-1000, 900)));
        // The monitor it was on is gone: the usual placement.
        assert_eq!(pinned_placement(Some((4000, 100)), &monitors), None);
        assert_eq!(pinned_placement(Some((1920, 0)), &monitors), None);
        // Never moved.
        assert_eq!(pinned_placement(None, &monitors), None);
    }

    #[test]
    fn only_a_size_the_shell_did_not_set_is_the_persons_resize() {
        // Rounding moves the shell's own size by a pixel or two.
        assert!(!is_users_resize(Some((1020, 900)), (1021, 899)));
        assert!(is_users_resize(Some((1020, 900)), (1200, 900)));
        assert!(is_users_resize(Some((1020, 900)), (1020, 600)));
        // Before the shell set any size (the window being built): not the person.
        assert!(!is_users_resize(None, (500, 500)));
    }

    #[test]
    fn a_resized_box_keeps_its_size_while_it_shows_a_chat() {
        // Never resized: the default width, the height follows the card.
        assert_eq!(box_size(None, true, 400.0, 648.0), (WIDTH, 400.0));
        assert_eq!(box_size(None, false, 10.0, 648.0), (WIDTH, HEIGHT));
        // Resized: the person's size with a chat, whatever the card asks.
        assert_eq!(box_size(Some((900.0, 700.0)), true, 300.0, 648.0), (900.0, 700.0));
        // The empty box keeps the width only.
        assert_eq!(box_size(Some((900.0, 700.0)), false, 10.0, 648.0), (900.0, HEIGHT));
    }

    #[test]
    fn start_a_new_chat_after_defaults_to_fifteen_minutes() {
        assert_eq!(new_chat_after(None), Some(15));
        assert_eq!(new_chat_after(Some(&Value::Null)), None);
        assert_eq!(new_chat_after(Some(&json!(30))), Some(30));
        assert_eq!(new_chat_after(Some(&json!(0))), Some(1));
        assert_eq!(new_chat_after(Some(&json!("x"))), Some(15));
    }

    #[test]
    fn text_sent_without_a_main_window_is_taken_once() {
        let state = QuickEntryState::default();
        *locked(&state.pending) = Some("hello".into());
        assert_eq!(locked(&state.pending).take(), Some("hello".into()));
        assert_eq!(locked(&state.pending).take(), None);
    }
}
