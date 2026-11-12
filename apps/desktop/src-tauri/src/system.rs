//! Startup, background and the app's lifecycle (Settings → General;
//! docs/features/shell-hosted-engines/spec.md, FR-11, FR-12, NFR-5; plan 3.5
//! "Lifecycle and tray", 3.8).
//!
//! * Run on startup: the OS starts evo at sign-in with [`HIDDEN_ARG`]
//!   (tauri-plugin-autostart). With the tray icon on it then stays in the
//!   tray without opening a window.
//! * Show in system tray: an icon in the notification area / menu bar.
//!   Closing the main window then only hides it, and closing every window
//!   keeps evo running, so chats' engines, which the shell hosts, carry on
//!   with no window (FR-11). The tray's menu says how many chats work and
//!   wait for the person, and lists the ones that wait (FR-12, FR-9).
//!   Without the icon, closing the last window quits evo.
//! * Quitting (the tray's Quit, or closing the last window with the tray
//!   off) asks once, "N chats are working. Quit anyway?", while chats work
//!   or wait (FR-11). A visible window asks (`app://quit-requested`); with
//!   none, the main window is shown, or built again, and asks. Quitting
//!   stops every engine.
//! * The main window comes back from the tray, the dock or a second launch,
//!   also after it was destroyed: it is built again from `tauri.conf.json`.
//! * Keep computer awake: holds off system sleep (not the display) while evo
//!   runs.
//!
//! Tray and keep-awake are stored in the app settings (`system.tray`,
//! `system.keepAwake`); run-on-startup is the OS's own record.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use evo_store::logs::NewLog;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::db::Db;
use crate::supervisor::{notices, AppHost, EngineSupervisor, WorkState};

/// Passed by the OS at sign-in: start in the tray, no window.
pub const HIDDEN_ARG: &str = "--hidden";

const TRAY_KEY: &str = "system.tray";
const AWAKE_KEY: &str = "system.keepAwake";
const TRAY_ID: &str = "main";
/// The main window's label (`tauri.conf.json`).
const MAIN: &str = "main";
/// Waiting chats the tray's menu lists, at most.
pub const TRAY_WAITING_SHOWN: usize = 5;
/// A chat's title in the tray's menu, at most this many characters.
pub const TRAY_TITLE_MAX: usize = 48;
/// Menu ids of the tray's lines for waiting chats: this, then the chat id.
const TRAY_CHAT: &str = "tray-chat:";
/// Work changes come in bursts (a run's agents): the tray and the windows
/// hear the state at most this often.
const WORK_SETTLE: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct SystemState {
    tray: AtomicBool,
    awake: Mutex<Option<awake::Hold>>,
    /// Quitting was decided: nothing opens a window or holds the exit any more.
    quitting: AtomicBool,
    /// The main window is being built again.
    building: AtomicBool,
    /// The window asked "Quit anyway?", until it is answered. A main window
    /// built to ask reads it when it loads (`quit_pending`).
    quit_asked: Mutex<Option<String>>,
    /// A chat to open in a main window being built again (`app_open_pending`).
    open_pending: Mutex<Option<String>>,
    /// An update of the tray and the windows is on its way.
    work_scheduled: AtomicBool,
    /// What the tray and the windows last heard (`app://working`).
    work_shown: Mutex<Option<Vec<WorkingChat>>>,
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettings {
    run_on_startup: bool,
    tray: bool,
    keep_awake: bool,
}

fn saved_flag(db: &Db, key: &str, default: bool) -> bool {
    db.with(|conn| evo_store::settings::all(conn))
        .ok()
        .and_then(|settings| settings.get(key).and_then(Value::as_bool))
        .unwrap_or(default)
}

// ── Lifecycle and tray decisions (FR-11, FR-12), free of the app ───────────

/// A chat that works or waits for the person (`engine_working`, plan 3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkingChat {
    pub chat: String,
    /// Its title; "A chat" for an incognito one, as in notices.
    pub title: String,
    pub state: WorkState,
}

/// The supervisor's work (`EngineSupervisor::work`) with titles (`None`:
/// incognito, or none): the waiting chats first, then by title.
pub fn working_list(work: Vec<(String, WorkState)>, title: impl Fn(&str) -> Option<String>) -> Vec<WorkingChat> {
    let mut list: Vec<WorkingChat> = work
        .into_iter()
        .map(|(chat, state)| {
            let title = title(&chat).filter(|title| !title.trim().is_empty()).unwrap_or_else(|| notices::INCOGNITO_NAME.to_string());
            WorkingChat { chat, title, state }
        })
        .collect();
    list.sort_by(|a, b| {
        (a.state != WorkState::Waiting)
            .cmp(&(b.state != WorkState::Waiting))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.chat.cmp(&b.chat))
    });
    list
}

/// "1 chat", "2 chats".
fn chats(n: usize) -> String {
    format!("{n} chat{}", if n == 1 { "" } else { "s" })
}

fn counts(list: &[WorkingChat]) -> (usize, usize) {
    let waiting = list.iter().filter(|chat| chat.state == WorkState::Waiting).count();
    (list.len() - waiting, waiting)
}

/// One line of the tray's menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayLine {
    pub id: String,
    pub text: String,
    pub enabled: bool,
}

fn cut(title: &str) -> String {
    if title.chars().count() <= TRAY_TITLE_MAX {
        return title.to_string();
    }
    let kept: String = title.chars().take(TRAY_TITLE_MAX - 1).collect();
    format!("{}…", kept.trim_end())
}

/// The tray's lines about work (FR-12): "2 chats working" and "1 chat
/// waiting for you" (disabled while there are none), then the waiting chats
/// (FR-9), at most [`TRAY_WAITING_SHOWN`].
pub fn tray_lines(list: &[WorkingChat]) -> Vec<TrayLine> {
    let (working, waiting) = counts(list);
    let mut lines = vec![
        TrayLine {
            id: "tray-working".into(),
            text: if working == 0 { "No chats working".into() } else { format!("{} working", chats(working)) },
            enabled: working > 0,
        },
        TrayLine {
            id: "tray-waiting".into(),
            text: if waiting == 0 { "No chats waiting for you".into() } else { format!("{} waiting for you", chats(waiting)) },
            enabled: waiting > 0,
        },
    ];
    lines.extend(
        list.iter()
            .filter(|chat| chat.state == WorkState::Waiting)
            .take(TRAY_WAITING_SHOWN)
            .map(|chat| TrayLine { id: format!("{TRAY_CHAT}{}", chat.chat), text: cut(&chat.title), enabled: true }),
    );
    lines
}

/// The tray icon's tooltip: the app's name, then what waits and works.
pub fn tray_tooltip(name: &str, list: &[WorkingChat]) -> String {
    let (working, waiting) = counts(list);
    let mut parts = vec![name.to_string()];
    if waiting > 0 {
        parts.push(format!("{} waiting for you", chats(waiting)));
    }
    if working > 0 {
        parts.push(format!("{} working", chats(working)));
    }
    parts.join(" · ")
}

/// What a click on a tray menu line does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayTarget {
    /// Show the main window.
    Main,
    /// Show this chat.
    Chat(String),
    Quit,
}

/// The tray menu line `id`'s target, with `list` the chats it was drawn from:
/// the waiting line opens the first waiting chat.
pub fn tray_target(id: &str, list: &[WorkingChat]) -> Option<TrayTarget> {
    match id {
        "tray-open" | "tray-working" => Some(TrayTarget::Main),
        "tray-quit" => Some(TrayTarget::Quit),
        "tray-waiting" => Some(
            list.iter()
                .find(|chat| chat.state == WorkState::Waiting)
                .map_or(TrayTarget::Main, |chat| TrayTarget::Chat(chat.chat.clone())),
        ),
        _ => id.strip_prefix(TRAY_CHAT).filter(|chat| !chat.is_empty()).map(|chat| TrayTarget::Chat(chat.to_string())),
    }
}

/// An open window, as the quit question picks where to ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowView {
    pub label: String,
    pub visible: bool,
    pub focused: bool,
}

/// What a request to quit does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuitStep {
    /// Nothing works: quit now.
    Quit,
    /// Ask in this window.
    Ask(String),
    /// No window is visible: show the main window (built again if it is
    /// gone) and ask there.
    AskInMain,
}

/// Quit with `working` chats working or waiting, `windows` open (FR-11):
/// with work, ask once in the focused window, else the visible main window,
/// else another visible one, else the main window, shown.
pub fn quit_step(working: usize, windows: &[WindowView]) -> QuitStep {
    if working == 0 {
        return QuitStep::Quit;
    }
    let visible = || windows.iter().filter(|window| window.visible);
    visible()
        .find(|window| window.focused)
        .or_else(|| visible().find(|window| window.label == MAIN))
        .or_else(|| visible().min_by(|a, b| a.label.cmp(&b.label)))
        .map_or(QuitStep::AskInMain, |window| QuitStep::Ask(window.label.clone()))
}

/// What a window's close button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseStep {
    /// The main window hides (the tray icon brings it back).
    Hide,
    Close,
    /// The last window with the tray off while chats work: ask before quitting.
    Ask,
}

/// Closing window `label`, with the tray on or off, `open` windows open
/// (this one included) and `working` chats working or waiting (FR-11).
pub fn close_step(label: &str, tray: bool, open: usize, working: usize) -> CloseStep {
    if label == MAIN && tray {
        CloseStep::Hide
    } else if open > 1 || tray || working == 0 {
        CloseStep::Close
    } else {
        CloseStep::Ask
    }
}

/// What a request to exit does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStep {
    Exit,
    /// Keep running in the tray.
    Stay,
    /// Keep running and ask in the main window, built again.
    Ask,
}

/// The app is asked to exit: `code` is set when evo asked (Quit, an update's
/// restart), unset when the last window went. With the tray on evo stays;
/// with it off it never quits unasked while chats work (FR-11).
pub fn exit_step(code: Option<i32>, tray: bool, working: usize, quitting: bool) -> ExitStep {
    if quitting || code.is_some() {
        ExitStep::Exit
    } else if tray {
        ExitStep::Stay
    } else if working > 0 {
        ExitStep::Ask
    } else {
        ExitStep::Exit
    }
}

/// What bringing the main window back does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainStep {
    Show,
    /// It was destroyed: build it again from the app's config.
    Build,
    /// The app quits.
    Nothing,
}

pub fn main_step(exists: bool, quitting: bool) -> MainStep {
    match (exists, quitting) {
        (_, true) => MainStep::Nothing,
        (true, false) => MainStep::Show,
        (false, false) => MainStep::Build,
    }
}

// ── The app ─────────────────────────────────────────────────────────────────

/// Every chat that works or waits, with its title (FR-11, FR-12, NFR-5).
pub fn working_chats(app: &AppHandle) -> Vec<WorkingChat> {
    let work = app.state::<EngineSupervisor>().work(&AppHost(app.clone()));
    let db = app.state::<Db>();
    working_list(work, |chat| {
        db.with(|conn| evo_store::chats::get_chat(conn, chat))
            .ok()
            .flatten()
            // Nothing an incognito chat says is shown, its title included.
            .filter(|row| !row.incognito)
            .map(|row| row.title)
    })
}

/// How many chats work or wait.
fn work_count(app: &AppHandle) -> usize {
    app.state::<EngineSupervisor>().work(&AppHost(app.clone())).len()
}

/// The chats with work changed (a run started or settled, a question opened
/// or closed, an engine started or stopped): the tray and the windows'
/// sidebars hear it (`app://working`), at most every [`WORK_SETTLE`].
pub fn work_changed(app: &AppHandle) {
    let Some(state) = app.try_state::<SystemState>() else {
        return;
    };
    if state.work_scheduled.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(WORK_SETTLE).await;
        let state = app.state::<SystemState>();
        // Changes from now on schedule another update.
        state.work_scheduled.store(false, Ordering::Release);
        let list = working_chats(&app);
        {
            let mut shown = locked(&state.work_shown);
            if shown.as_ref() == Some(&list) {
                return;
            }
            *shown = Some(list.clone());
        }
        let _ = app.emit("app://working", json!({ "chats": list }));
        update_tray(&app, &list);
    });
}

fn tray_menu(app: &AppHandle, list: &[WorkingChat]) -> tauri::Result<Menu<tauri::Wry>> {
    let name = crate::window::app_name();
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "tray-open", format!("Open {name}"), true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    for line in tray_lines(list) {
        menu.append(&MenuItem::with_id(app, line.id, line.text, line.enabled, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "tray-quit", format!("Quit {name}"), true, None::<&str>)?)?;
    Ok(menu)
}

fn update_tray(app: &AppHandle, list: &[WorkingChat]) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match tray_menu(app, list) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(error) => app.state::<Db>().log(NewLog::new("warn", "shell", format!("could not update the tray menu: {error}"))),
    }
    let _ = tray.set_tooltip(Some(tray_tooltip(crate::window::app_name(), list)));
}

/// The chats the tray's menu was last drawn from.
fn shown_work(app: &AppHandle) -> Vec<WorkingChat> {
    locked(&app.state::<SystemState>().work_shown).clone().unwrap_or_default()
}

/// The main window's title and material (at start, and when built again).
pub fn dress_main(app: &AppHandle) {
    if let Some(main) = app.get_window(MAIN) {
        let _ = main.set_title(crate::window::app_name());
        let material = crate::window::apply(&main, None);
        *app.state::<crate::window::MaterialState>().0.lock().expect("material lock") = material;
    }
}

fn front(window: &tauri::Window) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Build the main window again from `tauri.conf.json` (its size, flags and
/// browser arguments), dress it and show it.
fn build_main(app: &AppHandle) -> Result<(), String> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN)
        .cloned()
        .ok_or_else(|| "the app's config has no main window".to_string())?;
    tauri::WebviewWindowBuilder::from_config(app, &config)
        .and_then(|builder| builder.build())
        .map_err(|error| error.to_string())?;
    dress_main(app);
    if let Some(main) = app.get_window(MAIN) {
        front(&main);
    }
    Ok(())
}

/// Bring the main window back (tray, second launch, dock icon, a
/// notification); built again when it was destroyed (FR-11). True when a new
/// one is being built: its page has not loaded yet.
pub fn show_main(app: &AppHandle) -> bool {
    let state = app.state::<SystemState>();
    // `get_window`: with browser tabs open the main window holds several
    // webviews and is no longer a `WebviewWindow`.
    let main = app.get_window(MAIN);
    match main_step(main.is_some(), state.quitting.load(Ordering::Acquire)) {
        MainStep::Show => {
            if let Some(main) = main {
                front(&main);
            }
            false
        }
        MainStep::Build => {
            if !state.building.swap(true, Ordering::AcqRel) {
                let app = app.clone();
                // Off the caller: on Windows a window built on the event
                // loop's thread (a tray click) deadlocks.
                tauri::async_runtime::spawn(async move {
                    let built = build_main(&app);
                    app.state::<SystemState>().building.store(false, Ordering::Release);
                    if let Err(error) = built {
                        app.state::<Db>().log(NewLog::new("error", "shell", format!("could not open the main window again: {error}")));
                    }
                });
            }
            true
        }
        MainStep::Nothing => false,
    }
}

/// Show `chat` in the main window (`app://open-chat`). A main window built
/// again opens it once it has loaded (`app_open_pending`).
pub fn open_in_main(app: &AppHandle, chat: &str) {
    if app.get_window(MAIN).is_none() {
        *locked(&app.state::<SystemState>().open_pending) = Some(chat.to_string());
        show_main(app);
        return;
    }
    show_main(app);
    let _ = app.emit("app://open-chat", json!({ "chat": chat }));
}

/// Ask "Quit anyway?" in window `label` (FR-11).
fn ask_quit(app: &AppHandle, label: &str, working: usize) {
    *locked(&app.state::<SystemState>().quit_asked) = Some(label.to_string());
    if label == MAIN {
        show_main(app);
    } else if let Some(window) = app.get_window(label) {
        front(&window);
    }
    // A main window still being built reads it when it loads (`quit_pending`).
    let _ = app.emit("app://quit-requested", json!({ "working": working, "window": label }));
}

/// The person asked to quit (the tray's Quit): at once with nothing
/// working, else after one question (FR-11).
pub fn request_quit(app: &AppHandle) {
    let working = work_count(app);
    let windows: Vec<WindowView> = app
        .windows()
        .into_iter()
        .map(|(label, window)| WindowView {
            label,
            visible: window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false),
            focused: window.is_focused().unwrap_or(false),
        })
        .collect();
    match quit_step(working, &windows) {
        QuitStep::Quit => quit_app(app),
        QuitStep::Ask(label) => ask_quit(app, &label, working),
        QuitStep::AskInMain => ask_quit(app, MAIN, working),
    }
}

/// Apply the saved settings at launch, then show the main window unless the
/// OS started evo at sign-in and the tray can hold it.
pub fn setup(app: &AppHandle) {
    let db = app.state::<Db>();
    let state = app.state::<SystemState>();
    let tray = saved_flag(&db, TRAY_KEY, true);
    if tray {
        match build_tray(app) {
            Ok(()) => state.tray.store(true, Ordering::Relaxed),
            Err(error) => db.log(NewLog::new("warn", "shell", format!("no tray icon: {error}"))),
        }
    }
    if saved_flag(&db, AWAKE_KEY, false) {
        match awake::Hold::start() {
            Ok(hold) => *state.awake.lock().expect("awake lock") = Some(hold),
            Err(error) => db.log(NewLog::new("warn", "shell", format!("could not keep the computer awake: {error}"))),
        }
    }
    let hidden = std::env::args().any(|arg| arg == HIDDEN_ARG);
    if !(hidden && state.tray.load(Ordering::Relaxed)) {
        show_main(app);
    }
}

/// A window's close button: the main window hides while the tray icon is
/// there; the last window with the tray off asks first while chats work (FR-11).
pub fn on_close_requested(window: &tauri::Window, api: &tauri::CloseRequestApi) {
    let app = window.app_handle();
    let tray = window.state::<SystemState>().tray.load(Ordering::Relaxed);
    let open = app.windows().len();
    // Only the last window with the tray off needs the count.
    let working = if !tray && open <= 1 { work_count(app) } else { 0 };
    match close_step(window.label(), tray, open, working) {
        CloseStep::Hide => {
            api.prevent_close();
            let _ = window.hide();
        }
        CloseStep::Close => {}
        CloseStep::Ask => {
            api.prevent_close();
            ask_quit(app, window.label(), working);
        }
    }
}

/// The app is asked to exit (`RunEvent::ExitRequested`): with the tray on
/// evo stays when the last window goes; with it off and chats working, the
/// main window is built again to ask (FR-11).
pub fn on_exit_requested(app: &AppHandle, code: Option<i32>, api: &tauri::ExitRequestApi) {
    let state = app.state::<SystemState>();
    let tray = state.tray.load(Ordering::Relaxed);
    let quitting = state.quitting.load(Ordering::Acquire);
    let working = if code.is_none() && !tray && !quitting { work_count(app) } else { 0 };
    match exit_step(code, tray, working, quitting) {
        ExitStep::Exit => {}
        ExitStep::Stay => api.prevent_exit(),
        ExitStep::Ask => {
            api.prevent_exit();
            ask_quit(app, MAIN, working);
        }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(());
    }
    let name = crate::window::app_name();
    let list = working_chats(app);
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tray_tooltip(name, &list))
        .menu(&tray_menu(app, &list)?)
        // Left click opens the window; the menu is on right click.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match tray_target(event.id().as_ref(), &shown_work(app)) {
            Some(TrayTarget::Main) => {
                show_main(app);
            }
            Some(TrayTarget::Chat(chat)) => notices::open_chat(app, &chat),
            Some(TrayTarget::Quit) => request_quit(app),
            None => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    *locked(&app.state::<SystemState>().work_shown) = Some(list);
    Ok(())
}

/// What ends with the app: connectors' servers stop, apps the agent controls
/// go back where they were, every chat's engine stops (on Windows a job object
/// also covers a crash or a forced kill: engine.rs). Runs on exit, and before
/// an update installs, where the exit handlers do not run
/// (docs/features/app-updates/plan.md, 3.5). Blocks: never call it on an
/// async runtime thread.
pub fn shutdown(app: &AppHandle) {
    crate::mcp::shutdown(app);
    crate::computer::shutdown();
    crate::memory_index::shutdown(app);
    tauri::async_runtime::block_on(app.state::<crate::engine::EngineState>().stop_all());
    // After the engines: Windows keeps a transcript locked while one has it open.
    crate::chat_flags::forget_incognito(app);
}

/// Quit for real: stop every chat's engine, then exit (FR-11).
pub fn quit_app(app: &AppHandle) {
    let state = app.state::<SystemState>();
    state.quitting.store(true, Ordering::Release);
    locked(&state.quit_asked).take();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let engines = app.state::<crate::engine::EngineState>();
        for chat in engines.running().await {
            engines.stop(&chat).await;
        }
        app.exit(0);
    });
}

/// Every chat that works or waits for the person, with its title
/// (FR-11, FR-12, NFR-5).
#[tauri::command]
pub fn engine_working(app: AppHandle) -> Vec<WorkingChat> {
    working_chats(&app)
}

/// The quit question's "Quit anyway": stop every engine and exit (FR-11).
#[tauri::command]
pub fn quit_confirmed(app: AppHandle) {
    quit_app(&app);
}

/// The quit question was answered "Cancel".
#[tauri::command]
pub fn quit_dismissed(state: State<'_, SystemState>) {
    locked(&state.quit_asked).take();
}

/// The number of chats working, when this window was asked to quit and has
/// not answered: a main window built to ask hears it here once it loads.
#[tauri::command]
pub fn quit_pending(app: AppHandle, window: tauri::Window) -> Option<usize> {
    let asked = locked(&app.state::<SystemState>().quit_asked).clone();
    (asked.as_deref() == Some(window.label())).then(|| work_count(&app))
}

/// The chat a main window built again opens first (a notification or the
/// tray asked for it while there was none); taken once.
#[tauri::command]
pub fn app_open_pending(state: State<'_, SystemState>, window: tauri::Window) -> Option<String> {
    if window.label() != MAIN {
        return None;
    }
    locked(&state.open_pending).take()
}

fn current(app: &AppHandle, state: &SystemState) -> SystemSettings {
    SystemSettings {
        run_on_startup: app.autolaunch().is_enabled().unwrap_or(false),
        tray: state.tray.load(Ordering::Relaxed),
        keep_awake: state.awake.lock().expect("awake lock").is_some(),
    }
}

#[tauri::command]
pub fn system_settings(app: AppHandle, state: State<'_, SystemState>) -> SystemSettings {
    current(&app, &state)
}

/// Turn one setting on or off (`runOnStartup`, `tray`, `keepAwake`); returns
/// all three as they now are.
#[tauri::command]
pub fn system_set(
    app: AppHandle,
    state: State<'_, SystemState>,
    db: State<'_, Db>,
    key: String,
    enabled: bool,
) -> Result<SystemSettings, String> {
    match key.as_str() {
        "runOnStartup" => {
            let autolaunch = app.autolaunch();
            if enabled { autolaunch.enable() } else { autolaunch.disable() }.map_err(|e| e.to_string())?;
        }
        "tray" => {
            if enabled {
                build_tray(&app).map_err(|e| format!("could not add the tray icon: {e}"))?;
            } else {
                app.remove_tray_by_id(TRAY_ID);
                // Nothing could bring a hidden main window back now.
                show_main(&app);
            }
            state.tray.store(enabled, Ordering::Relaxed);
            db.with(|conn| evo_store::settings::set(conn, TRAY_KEY, &Value::Bool(enabled)))?;
        }
        "keepAwake" => {
            let mut hold = state.awake.lock().expect("awake lock");
            if enabled && hold.is_none() {
                *hold = Some(awake::Hold::start()?);
            } else if !enabled {
                // Dropping the hold lets the computer sleep again.
                hold.take();
            }
            drop(hold);
            db.with(|conn| evo_store::settings::set(conn, AWAKE_KEY, &Value::Bool(enabled)))?;
        }
        other => return Err(format!("unknown setting {other}")),
    }
    db.log(NewLog::new("info", "shell", format!("{key} turned {}", if enabled { "on" } else { "off" })));
    Ok(current(&app, &state))
}

/// Holding off system sleep. The display may still turn off.
mod awake {
    /// Sleep is held off until this is dropped.
    pub struct Hold {
        #[cfg(target_os = "windows")]
        release: Option<std::sync::mpsc::Sender<()>>,
        #[cfg(not(target_os = "windows"))]
        child: std::process::Child,
    }

    #[cfg(target_os = "windows")]
    impl Hold {
        /// The execution state belongs to a thread: one thread holds it
        /// until told to let go.
        pub fn start() -> Result<Self, String> {
            use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
            let (release, wait) = std::sync::mpsc::channel::<()>();
            let (started, ok) = std::sync::mpsc::channel::<bool>();
            std::thread::Builder::new()
                .name("keep-awake".into())
                .spawn(move || {
                    let held = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
                    let _ = started.send(held.0 != 0);
                    let _ = wait.recv();
                    unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
                })
                .map_err(|e| e.to_string())?;
            match ok.recv() {
                Ok(true) => Ok(Self { release: Some(release) }),
                _ => Err("Windows refused to keep the computer awake".into()),
            }
        }
    }

    #[cfg(target_os = "windows")]
    impl Drop for Hold {
        fn drop(&mut self) {
            if let Some(release) = self.release.take() {
                let _ = release.send(());
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    impl Hold {
        pub fn start() -> Result<Self, String> {
            #[cfg(target_os = "macos")]
            let mut command = {
                // -i: no idle sleep; -w: stop with this process.
                let mut command = std::process::Command::new("/usr/bin/caffeinate");
                command.args(["-i", "-w", &std::process::id().to_string()]);
                command
            };
            #[cfg(not(target_os = "macos"))]
            let mut command = {
                let mut command = std::process::Command::new("systemd-inhibit");
                // The inhibitor lasts while `tail` waits for this process, so a
                // crash does not leave the computer unable to sleep.
                command.args([
                    "--what=sleep",
                    "--who=evo",
                    "--why=Keep computer awake is on",
                    "--mode=block",
                    "tail",
                    &format!("--pid={}", std::process::id()),
                    "-f",
                    "/dev/null",
                ]);
                command
            };
            let child = command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| format!("could not keep the computer awake: {e}"))?;
            Ok(Self { child })
        }
    }

    #[cfg(not(target_os = "windows"))]
    impl Drop for Hold {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::tests::{set_up, top, Fake};
    use crate::supervisor::{EngineSupervisor, WorkState};
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn keep_awake_holds_and_lets_go() {
        let hold = super::awake::Hold::start().expect("hold off sleep");
        drop(hold);
        // A second hold after the first let go works too.
        drop(super::awake::Hold::start().expect("hold again"));
    }

    // ── Lifecycle and tray: docs/features/shell-hosted-engines/spec.md, FR-11, FR-12, NFR-5 (T31) ──

    fn chat(chat: &str, title: &str, state: WorkState) -> WorkingChat {
        WorkingChat { chat: chat.into(), title: title.into(), state }
    }

    #[tokio::test]
    async fn engine_working_lists_working_and_waiting_chats() {
        let mut fake = Fake::default();
        for id in ["run", "ask", "idle", "mail", "both", "secret"] {
            fake = fake.with_row(id, top(None));
        }
        let supervisor = EngineSupervisor::default();
        let mut generations = HashMap::new();
        for id in ["run", "ask", "idle", "mail", "both", "secret"] {
            generations.insert(id, set_up(&fake, &supervisor, id).await);
        }
        let line = |id: &str, line: serde_json::Value| supervisor.follow_line(&fake, id, generations[id], &line, 1);
        let question = |n: u32| json!({ "type": "ui_dialog_request", "requestId": format!("approve:{n}"), "ui": { "kind": "confirm", "title": "Run it?" } });
        line("run", json!({ "type": "agent_start" }));
        line("ask", question(1));
        fake.mail_for("mail", true);
        // A run that waits for an approval waits for the person.
        line("both", json!({ "type": "agent_start" }));
        line("both", question(2));
        line("secret", json!({ "type": "agent_start" }));
        // Mail for a chat with no engine (the person stopped it): not working.
        fake.mail_for("stopped", true);

        let titles = |id: &str| match id {
            "secret" => None,
            other => Some(format!("Chat {other}")),
        };
        let listed = working_list(supervisor.work(&fake), titles);
        assert_eq!(
            listed,
            vec![
                chat("ask", "Chat ask", WorkState::Waiting),
                chat("both", "Chat both", WorkState::Waiting),
                // An incognito chat (no title given) is "A chat", as in notices.
                chat("secret", "A chat", WorkState::Working),
                chat("mail", "Chat mail", WorkState::Working),
                chat("run", "Chat run", WorkState::Working),
            ]
        );
        // The wire shape (`engine_working`, plan 3.3).
        assert_eq!(serde_json::to_value(&listed[0]).unwrap(), json!({ "chat": "ask", "title": "Chat ask", "state": "waiting" }));

        // Everything settles: nothing is listed.
        line("run", json!({ "type": "agent_settled", "outcome": "completed" }));
        line("both", json!({ "type": "ui_dialog_closed", "requestId": "approve:2" }));
        line("both", json!({ "type": "agent_settled", "outcome": "completed" }));
        line("ask", json!({ "type": "ui_dialog_closed", "requestId": "approve:1" }));
        line("secret", json!({ "type": "agent_settled", "outcome": "completed" }));
        fake.mail_for("mail", false);
        assert_eq!(working_list(supervisor.work(&fake), titles), Vec::new());
    }

    fn view(label: &str, visible: bool, focused: bool) -> WindowView {
        WindowView { label: label.into(), visible, focused }
    }

    #[test]
    fn quit_with_work_asks_once_in_a_window_without_work_quits() {
        let windows = [view("main", true, false), view("chat-b", true, true)];
        // No work: quit at once.
        assert_eq!(quit_step(0, &windows), QuitStep::Quit);
        assert_eq!(quit_step(0, &[]), QuitStep::Quit);
        // Work: ask in the focused window,
        assert_eq!(quit_step(2, &windows), QuitStep::Ask("chat-b".into()));
        // else the visible main window, else another visible one,
        assert_eq!(quit_step(1, &[view("chat-b", true, false), view("main", true, false)]), QuitStep::Ask("main".into()));
        assert_eq!(quit_step(1, &[view("main", false, false), view("chat-c", true, false), view("chat-b", true, false)]), QuitStep::Ask("chat-b".into()));
        // else the main window, shown (or built again) to ask there.
        assert_eq!(quit_step(1, &[view("main", false, false)]), QuitStep::AskInMain);
        assert_eq!(quit_step(1, &[]), QuitStep::AskInMain);
    }

    #[test]
    fn closing_the_last_window_with_the_tray_off_asks_when_chats_work() {
        // The tray on: the main window hides; other windows close; the app stays.
        assert_eq!(close_step("main", true, 1, 3), CloseStep::Hide);
        assert_eq!(close_step("main", true, 2, 0), CloseStep::Hide);
        assert_eq!(close_step("chat-b", true, 1, 3), CloseStep::Close);
        // The tray off: a window that is not the last closes.
        assert_eq!(close_step("main", false, 2, 3), CloseStep::Close);
        assert_eq!(close_step("chat-b", false, 2, 3), CloseStep::Close);
        // The last one asks while chats work, and closes (quits) when none does.
        assert_eq!(close_step("main", false, 1, 3), CloseStep::Ask);
        assert_eq!(close_step("chat-b", false, 1, 1), CloseStep::Ask);
        assert_eq!(close_step("main", false, 1, 0), CloseStep::Close);
    }

    #[test]
    fn an_exit_request_keeps_the_app_in_the_tray_and_never_quits_unasked_with_work() {
        // Asked for (Quit, an update's restart): exit.
        assert_eq!(exit_step(Some(0), true, 3, false), ExitStep::Exit);
        assert_eq!(exit_step(Some(0), false, 3, false), ExitStep::Exit);
        // The last window went, the tray on: stay in the tray.
        assert_eq!(exit_step(None, true, 0, false), ExitStep::Stay);
        assert_eq!(exit_step(None, true, 2, false), ExitStep::Stay);
        // The tray off: quit, unless chats work and nobody was asked.
        assert_eq!(exit_step(None, false, 0, false), ExitStep::Exit);
        assert_eq!(exit_step(None, false, 2, false), ExitStep::Ask);
        // Quitting was confirmed: nothing holds it.
        assert_eq!(exit_step(None, true, 2, true), ExitStep::Exit);
        assert_eq!(exit_step(None, false, 2, true), ExitStep::Exit);
    }

    #[test]
    fn the_main_window_is_shown_else_built_again_unless_the_app_quits() {
        assert_eq!(main_step(true, false), MainStep::Show);
        assert_eq!(main_step(false, false), MainStep::Build);
        // Quitting: a second launch or a tray click opens nothing.
        assert_eq!(main_step(true, true), MainStep::Nothing);
        assert_eq!(main_step(false, true), MainStep::Nothing);
    }

    #[test]
    fn the_tray_says_how_many_chats_work_and_lists_the_ones_that_wait() {
        let lines = tray_lines(&[]);
        assert_eq!(
            lines,
            vec![
                TrayLine { id: "tray-working".into(), text: "No chats working".into(), enabled: false },
                TrayLine { id: "tray-waiting".into(), text: "No chats waiting for you".into(), enabled: false },
            ]
        );
        let list = [
            chat("w", "Review the plan", WorkState::Waiting),
            chat("a", "Fix the build", WorkState::Working),
            chat("b", "Write docs", WorkState::Working),
        ];
        let lines = tray_lines(&list);
        let texts: Vec<(&str, &str, bool)> = lines.iter().map(|l| (l.id.as_str(), l.text.as_str(), l.enabled)).collect();
        assert_eq!(
            texts,
            vec![
                ("tray-working", "2 chats working", true),
                ("tray-waiting", "1 chat waiting for you", true),
                ("tray-chat:w", "Review the plan", true),
            ]
        );
        // One of each (FR-12's check).
        let lines = tray_lines(&[chat("w", "W", WorkState::Waiting), chat("a", "A", WorkState::Working)]);
        assert_eq!((lines[0].text.as_str(), lines[1].text.as_str()), ("1 chat working", "1 chat waiting for you"));
        // Many waiting: the first few are listed; a long title is cut.
        let many: Vec<WorkingChat> = (0..8).map(|n| chat(&format!("c{n}"), &"x".repeat(80), WorkState::Waiting)).collect();
        let lines = tray_lines(&many);
        assert_eq!(lines[1].text, "8 chats waiting for you");
        assert_eq!(lines.len(), 2 + TRAY_WAITING_SHOWN);
        assert!(lines[2].text.chars().count() <= TRAY_TITLE_MAX && lines[2].text.ends_with('…'));
        // The tooltip says the same, briefly.
        assert_eq!(tray_tooltip("evo", &[]), "evo");
        assert_eq!(tray_tooltip("evo", &list), "evo · 1 chat waiting for you · 2 chats working");
        // A click on the waiting line opens the first waiting chat.
        assert_eq!(tray_target("tray-waiting", &list), Some(TrayTarget::Chat("w".into())));
        assert_eq!(tray_target("tray-chat:w", &list), Some(TrayTarget::Chat("w".into())));
        assert_eq!(tray_target("tray-working", &list), Some(TrayTarget::Main));
        assert_eq!(tray_target("tray-waiting", &[]), Some(TrayTarget::Main));
        assert_eq!(tray_target("tray-quit", &list), Some(TrayTarget::Quit));
        assert_eq!(tray_target("tray-open", &list), Some(TrayTarget::Main));
        assert_eq!(tray_target("other", &list), None);
    }
}
