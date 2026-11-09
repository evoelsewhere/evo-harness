//! The app side of computer app control (docs/features/computer-app-control,
//! plan 3.2): the journal of windows moved off screen kept in `evo.db`, putting
//! them back at start-up, and handing every window back when evo quits.
//!
//! The agent's tool and the native backend live in `agent_tools/computer`. The
//! person's own commands for the card (Stop, Allow again, Show the app, Close)
//! join this file in tasks phase 4.

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::agent_tools::computer::{self, Journal};
use crate::db::Db;

/// The journal in `evo.db`'s `computer_parked` table (spec FR-8).
struct DbJournal {
    app: AppHandle,
}

impl Journal for DbJournal {
    fn record(&self, window: i64, pid: i64, placement: &Value) {
        let result = self.app.state::<Db>().with(|conn| evo_store::computer::record(conn, window, pid, placement, None));
        if let Err(error) = result {
            eprintln!("computer app: could not record a window moved off screen: {error}");
        }
    }

    fn forget(&self, window: i64) {
        let _ = self.app.state::<Db>().with(|conn| evo_store::computer::forget(conn, window));
    }

    fn load(&self) -> Vec<(i64, i64, Value)> {
        self.app
            .state::<Db>()
            .with(|conn| evo_store::computer::all(conn))
            .map(|rows| rows.into_iter().map(|row| (row.window, row.pid, row.placement)).collect())
            .unwrap_or_default()
    }
}

/// At start-up: put back the windows a previous run left off screen, and
/// journal moved windows from now on.
pub fn setup(app: &AppHandle) {
    computer::recover_stranded(Box::new(DbJournal { app: app.clone() }));
}

/// evo is quitting: hand every controlled window back (FR-7). Windows still
/// out of reach stay in the journal for the next start.
pub fn shutdown() {
    computer::release_all();
}

/// Tells the windows the shell's tool list changed, so each ready chat's engine
/// gets it again (`stores/engine-store.ts`).
pub const TOOLS_CHANGED: &str = "agent-tools://changed";

/// A computer app control setting changed (FR-1): the tool is offered or
/// withdrawn on every chat's next call, and turning the feature off hands every
/// app back. The gate and the policy read their settings on every call.
pub fn settings_changed(app: &AppHandle, key: &str, value: &Value) {
    if key == computer::tool::ENABLED_KEY {
        if value != &json!(true) {
            computer::release_all();
        }
        let _ = app.emit(TOOLS_CHANGED, json!({ "reason": key }));
    }
}
