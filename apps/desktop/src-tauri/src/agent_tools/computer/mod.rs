//! Computer app control: the agent drives ONE desktop app window per chat, in
//! the background (docs/features/computer-app-control/spec.md, ADR 0015).
//!
//! Every capture and input is addressed to the attached window (and the
//! dialogs and popups it opens), never to the desktop:
//!
//! - Capture reads the window's own pixels (`PrintWindow` on Windows), so the
//!   app can sit off the person's screen.
//! - Input goes through background channels only: the accessibility tree
//!   (UI Automation and MSAA), messages posted to the window, and (to come,
//!   tasks phase 3) a DevTools pipe into web-based apps evo started. The
//!   person's pointer, keyboard focus and foreground window are never used
//!   (spec FR-5); `tests.rs` keeps it that way.
//!
//! Ported from evo v3 (tag `v3.1.1`, `desktop/src-tauri/src/computer_app/`).
//!
//! # Layout
//!
//! - [`backend`]: the `ComputerAppBackend` trait and the factory that returns
//!   the platform's implementation. Everything here talks to it.
//! - `win/`: the Windows product of the factory, one module per concern.
//! - [`action`]: the closed set of agent actions, parsed once.
//! - [`workers`], [`interrupt`]: per-chat worker threads and Stop.
//! - [`apps`]: the app catalog behind `search_apps` / `open_app`.
//! - [`keys`], [`policy`], [`geometry`]: helpers the backends share.

// The tool, the Tauri commands and the card (tasks phases 2 and 4) are the
// callers of this API; until they land only the tests use parts of it.
#![allow(dead_code)]

mod action;
mod backend;
mod interrupt;
mod workers;

mod apps;
mod geometry;
mod marks;
mod readback;
mod zoom;
mod keys;
mod policy;

#[cfg(target_os = "windows")]
mod win;

pub(crate) mod tool;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, OnceLock};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

pub(crate) use action::Action;
use backend::backend;
use interrupt::{generation, interrupt, with_ticket};

// ── Events for the card (docs/features/computer-app-control FR-10, FR-12) ────

/// An app was attached: `{chat, surface, app, title, windowId, web}`.
pub(crate) const ATTACHED_EVENT: &str = "computer://attached";
/// The chat's app was handed back: `{chat, surface, reason}`; `chat` null for all.
pub(crate) const RELEASED_EVENT: &str = "computer://released";
/// Control was stopped or allowed again: `{chat, state: "active" | "stopped"}`.
pub(crate) const STATE_EVENT: &str = "computer://state";
/// The chat's app became, or stopped being, the front window: `{chat, front}`
/// (FR-30). The card warns while it is.
pub(crate) const FRONT_EVENT: &str = "computer://front";

static APP: OnceLock<AppHandle> = OnceLock::new();

/// The app handle the module emits its events through (set at start-up).
pub(crate) fn set_app(app: AppHandle) {
    let _ = APP.set(app);
}

pub(crate) fn notify(event: &str, payload: Value) {
    if let Some(app) = APP.get() {
        let _ = app.emit(event, payload);
    }
}

pub(crate) fn surface(chat: &str) -> String {
    format!("app:{chat}")
}

fn notify_released(chat: Option<&str>, reason: &str) {
    notify(RELEASED_EVENT, json!({ "chat": chat, "surface": chat.map(surface), "reason": reason }));
}

/// Chats whose card the person closed: `attach` is refused until their turn
/// ends (FR-12).
static CLOSED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

pub(crate) fn mark_closed(chat: &str) {
    CLOSED.lock().expect("closed lock").get_or_insert_with(HashSet::new).insert(chat.to_string());
}

pub(crate) fn is_closed(chat: &str) -> bool {
    CLOSED.lock().expect("closed lock").as_ref().is_some_and(|set| set.contains(chat))
}

fn clear_closed(chat: &str) {
    if let Some(set) = CLOSED.lock().expect("closed lock").as_mut() {
        set.remove(chat);
    }
}

/// Whether `chat` has an app attached.
pub(crate) fn is_attached(chat: &str) -> bool {
    backend().is_attached(chat)
}

/// Run one agent action against `chat`'s attached window. `emit` carries the
/// virtual pointer to the card (FR-11).
pub(crate) async fn act(
    chat: &str,
    action: Action,
    params: Value,
    emit: impl Fn(Value) + Send + 'static,
) -> Result<Value, String> {
    if action == Action::Detach {
        // Letting go ends anything still running for the chat rather than
        // queueing behind it.
        interrupt(chat);
    }
    let ticket = generation(chat);
    let session = chat.to_string();
    workers::run(chat, move || with_ticket(&session, ticket, || backend().run_action(&emit, &session, action, &params))).await
}

/// Apps the person can allow or block in Settings: running and installed
/// ones, each with its icon (FR-1).
pub(crate) async fn list_apps() -> Result<Value, String> {
    // A worker of its own, so a long scan never waits behind a chat's actions.
    workers::run(workers::APP_PICKER, || Ok(backend().list_apps())).await
}

/// One JPEG frame of `chat`'s attached window for the card (FR-10).
pub(crate) async fn preview_frame(chat: String, max_width: u32) -> Result<Value, String> {
    let max_width = max_width.clamp(160, 1920);
    tokio::task::spawn_blocking(move || backend().preview_frame(&chat, max_width))
        .await
        .map_err(|error| format!("preview frame task failed: {error}"))?
}

/// The person revoked control: end the action in progress, release the app
/// and refuse attaching until they allow it again (FR-12).
pub(crate) fn stop(chat: &str) -> Result<Value, String> {
    interrupt(chat);
    backend().stop(chat)?;
    notify(STATE_EVENT, json!({ "chat": chat, "state": "stopped" }));
    Ok(json!({ "stopped": true }))
}

/// End the chat's action in progress without revoking control (a timeout).
pub(crate) fn interrupt_action(chat: &str) {
    interrupt(chat);
}

/// The person allowed control again after stopping it (FR-12).
pub(crate) fn resume(chat: &str) -> Result<Value, String> {
    backend().resume(chat)?;
    notify(STATE_EVENT, json!({ "chat": chat, "state": "active" }));
    // Stop handed the app back; allowing again does not attach it, so a card
    // kept for its Allow again goes now.
    if !backend().is_attached(chat) {
        notify_released(Some(chat), "resume");
    }
    Ok(json!({ "stopped": false }))
}

/// Hand the window back to the person's screen for good ("Show the app").
/// Returns the window so the caller, acting on the person's click, can bring
/// it to the front; this module never does (ADR 0015).
pub(crate) fn reveal(chat: &str) -> Result<Value, String> {
    backend().reveal(chat)
}

/// Hand `chat`'s app back where it was (FR-7): its turn ended or it closed.
/// Refusals the person gave in its run are forgotten with it (FR-24).
pub(crate) async fn release_chat(chat: &str) {
    crate::agent_tools::gate::forget_refusals(chat);
    clear_closed(chat);
    if backend().is_attached(chat) {
        let _ = act(chat, Action::Detach, json!({}), |_| {}).await;
        notify_released(Some(chat), "turn");
    }
}

/// The person closed the card (FR-12): hand the app back and refuse
/// attaching until the chat's turn ends.
pub(crate) async fn close(chat: &str) {
    mark_closed(chat);
    interrupt(chat);
    if backend().is_attached(chat) {
        let _ = act(chat, Action::Detach, json!({}), |_| {}).await;
    }
    notify_released(Some(chat), "close");
}

/// The window is gone, or the agent detached: tell the card.
pub(crate) fn released(chat: &str, reason: &str) {
    forget_front(chat);
    notify_released(Some(chat), reason);
}

// ── The front window (FR-30) ────────────────────────────────────────────────

/// Per chat: whether its app held the front window when last looked at
/// (`shown`, what the card was told) and whether the agent was told (`told`).
#[derive(Default)]
struct FrontState {
    shown: bool,
    told: bool,
}

static FRONT: LazyLock<Mutex<HashMap<String, FrontState>>> = LazyLock::new(Default::default);

/// Record whether `chat`'s app holds the front window: whether that changed.
fn observe_front(chat: &str, now: bool) -> bool {
    let mut front = FRONT.lock().expect("front lock");
    let state = front.entry(chat.to_string()).or_default();
    let changed = state.shown != now;
    state.shown = now;
    if !now {
        state.told = false;
    }
    changed
}

/// Whether the agent is still to be told that `chat`'s app holds the front;
/// marks it told. Once each time the app comes to hold it.
fn take_front_news(chat: &str) -> bool {
    let mut front = FRONT.lock().expect("front lock");
    let state = front.entry(chat.to_string()).or_default();
    let news = state.shown && !state.told;
    if news {
        state.told = true;
    }
    news
}

/// Whether `chat`'s app holds the front window now; tells the card when that
/// changed. Called after each action and on each tick of the card's pictures.
pub(crate) fn refresh_front(chat: &str) -> bool {
    let now = backend().is_attached(chat) && backend().holds_the_front(chat);
    if observe_front(chat, now) {
        notify(FRONT_EVENT, json!({ "chat": chat, "surface": surface(chat), "front": now }));
    }
    now
}

/// Tell the card whether `chat`'s app holds the front window, changed or not:
/// a card that starts streaming may have missed the last change.
pub(crate) fn announce_front(chat: &str) {
    let now = backend().is_attached(chat) && backend().holds_the_front(chat);
    observe_front(chat, now);
    notify(FRONT_EVENT, json!({ "chat": chat, "surface": surface(chat), "front": now }));
}

/// The line for the agent when `chat`'s app has come to hold the front window
/// since it was last told: once each time it happens.
pub(crate) fn front_line(chat: &str, app: &str) -> Option<String> {
    (refresh_front(chat) && take_front_news(chat)).then(|| front_text(app))
}

pub(crate) fn front_text(app: &str) -> String {
    let app = if app.trim().is_empty() { "The app" } else { app };
    format!(
        "Warning: {app} is now the person's front window, though it is off their screen: whatever they type goes to it. \
         Tell the person right away to click one of their own windows before typing (or to press Show the app to work in it \
         themselves). evo does not take the front window back."
    )
}

fn forget_front(chat: &str) {
    FRONT.lock().expect("front lock").remove(chat);
}

/// Hand every controlled window back. Called when evo quits (FR-7).
pub(crate) fn release_all() {
    backend().release_all();
    notify_released(None, "all");
}

/// Where windows moved off screen are recorded until they are back, so a run
/// that crashed leaves a trace the next start acts on (FR-8). The app keeps it
/// in `evo.db` (`src/computer.rs`); tests keep it in memory.
pub(crate) trait Journal: Send + Sync {
    /// `window` (a window handle) of process `pid` is about to move off
    /// screen; `placement` is where and how it was.
    fn record(&self, window: i64, pid: i64, placement: &Value);
    /// `window` is back where it was.
    fn forget(&self, window: i64);
    /// Every window still recorded: `(window, pid, placement)`.
    fn load(&self) -> Vec<(i64, i64, Value)>;
}

/// Put back the windows a previous run left off screen, and record moved
/// windows in `journal` from now on. Called once at start-up.
pub(crate) fn recover_stranded(journal: Box<dyn Journal>) {
    backend().recover_stranded(journal);
}

/// A journal kept in memory, for tests.
#[cfg(test)]
#[derive(Default, Clone)]
pub(crate) struct MemoryJournal(pub std::sync::Arc<std::sync::Mutex<Vec<(i64, i64, Value)>>>);

#[cfg(test)]
impl Journal for MemoryJournal {
    fn record(&self, window: i64, pid: i64, placement: &Value) {
        let mut rows = self.0.lock().unwrap();
        rows.retain(|row| row.0 != window);
        rows.push((window, pid, placement.clone()));
    }
    fn forget(&self, window: i64) {
        self.0.lock().unwrap().retain(|row| row.0 != window);
    }
    fn load(&self) -> Vec<(i64, i64, Value)> {
        self.0.lock().unwrap().clone()
    }
}
