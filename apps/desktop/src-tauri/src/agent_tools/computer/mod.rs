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
mod keys;
mod policy;

#[cfg(target_os = "windows")]
mod win;

#[cfg(test)]
mod tests;

use serde_json::{json, Value};

pub(crate) use action::Action;
use backend::backend;
use interrupt::{generation, interrupt, with_ticket};

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
    Ok(json!({ "stopped": true }))
}

/// End the chat's action in progress without revoking control (a timeout).
pub(crate) fn interrupt_action(chat: &str) {
    interrupt(chat);
}

/// The person allowed control again after stopping it (FR-12).
pub(crate) fn resume(chat: &str) -> Result<Value, String> {
    backend().resume(chat)?;
    Ok(json!({ "stopped": false }))
}

/// Hand the window back to the person's screen for good ("Show the app").
/// Returns the window so the caller, acting on the person's click, can bring
/// it to the front; this module never does (ADR 0015).
pub(crate) fn reveal(chat: &str) -> Result<Value, String> {
    backend().reveal(chat)
}

/// Hand every controlled window back. Called when evo quits (FR-7).
pub(crate) fn release_all() {
    backend().release_all();
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
