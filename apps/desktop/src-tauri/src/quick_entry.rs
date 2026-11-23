//! Quick entry: a small message box that opens over any app from a global
//! shortcut or the tray icon; Enter starts a new chat with the text in the
//! main window (docs/features/quick-entry/spec.md, FR-1–FR-9; plan 3.5).
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

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use evo_store::logs::NewLog;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::db::Db;

/// The box's window label.
pub const LABEL: &str = "quick-entry";
const ENABLED_KEY: &str = "system.quickEntry";
const SHORTCUT_KEY: &str = "system.quickEntryShortcut";
/// The window's logical width; the card inside leaves room for its shadow.
const WIDTH: f64 = 680.0;
/// The window's logical height with one line of text.
const HEIGHT: f64 = 84.0;
/// The window grows with the text up to this, then the field scrolls (FR-8).
const MAX_HEIGHT: f64 = 320.0;
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

/// The window's logical height for a page asking for `height` (FR-8).
pub fn window_height(height: f64) -> f64 {
    if height.is_finite() {
        height.clamp(HEIGHT, MAX_HEIGHT)
    } else {
        HEIGHT
    }
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
        // The page draws the card's shadow; a native one would frame the
        // transparent margin.
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
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
    if !crate::system::saved_flag(&db, ENABLED_KEY, true) {
        return;
    }
    state.enabled.store(true, Ordering::Release);
    if let Err(error) = build(app) {
        db.log(NewLog::new("warn", "shell", format!("no quick entry window: {error}")));
    }
    register_saved(app);
}

/// Quick entry's settings: on, the shortcut, and whether it is registered.
pub fn settings(app: &AppHandle) -> (bool, String, bool) {
    let state = app.state::<QuickEntryState>();
    let enabled = state.enabled.load(Ordering::Acquire);
    let shortcut = locked(&state.shortcut).clone();
    (enabled, shortcut, !enabled || state.registered.load(Ordering::Acquire))
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
    let height = window
        .inner_size()
        .ok()
        .map(|size| size.height as f64 / window.scale_factor().unwrap_or(1.0))
        .unwrap_or(HEIGHT);
    let size = ((WIDTH * scale) as u32, (height * scale) as u32);
    let origin = monitor.position();
    let area = monitor.size();
    let (x, y) = placement((origin.x, origin.y), (area.width, area.height), size);
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show(app: &AppHandle, window: &tauri::WebviewWindow) {
    place(app, window);
    *locked(&app.state::<QuickEntryState>().shown_at) = Some(Instant::now());
    let _ = window.show();
    let _ = window.set_focus();
    // The page puts the caret in the field and reads the theme again.
    let _ = app.emit_to(LABEL, "quick-entry://shown", ());
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
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

/// The box lost focus: it hides, keeping its text (FR-7).
pub fn on_focus_lost(window: &tauri::Window) {
    if window.label() != LABEL {
        return;
    }
    let state = window.state::<QuickEntryState>();
    let just_shown = locked(&state.shown_at).is_some_and(|at| at.elapsed() < BLUR_GRACE);
    if just_shown || !window.is_visible().unwrap_or(false) {
        return;
    }
    *locked(&state.blurred_at) = Some(Instant::now());
    let _ = window.hide();
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

/// The box's field grew or shrank to `height` logical pixels (FR-8).
#[tauri::command]
pub fn quick_entry_resize(window: tauri::WebviewWindow, height: f64) {
    if window.label() == LABEL {
        let _ = window.set_size(LogicalSize::new(WIDTH, window_height(height)));
    }
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
        assert_eq!(window_height(10.0), HEIGHT);
        assert_eq!(window_height(150.0), 150.0);
        assert_eq!(window_height(5000.0), MAX_HEIGHT);
        assert_eq!(window_height(f64::NAN), HEIGHT);
    }

    #[test]
    fn text_sent_without_a_main_window_is_taken_once() {
        let state = QuickEntryState::default();
        *locked(&state.pending) = Some("hello".into());
        assert_eq!(locked(&state.pending).take(), Some("hello".into()));
        assert_eq!(locked(&state.pending).take(), None);
    }
}
