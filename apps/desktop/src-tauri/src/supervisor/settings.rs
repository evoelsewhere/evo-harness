//! Settings reach every engine, viewed or not
//! (docs/features/shell-hosted-engines/spec.md, FR-8, NFR-1, NFR-4; plan
//! 3.3 "Settings keys", 3.5 "Settings fan-out (FR-8)").
//!
//! The shell sees each change where it happens and passes it on, once, to
//! every engine it runs, whichever windows hold them (none, one or several):
//!
//! - `approvalMode` (`db::settings_set`): `set_approval_mode` to every set-up
//!   engine; one still starting gets it when it is set up.
//! - providers: a window's provider command that succeeded (`set_api_key`, …)
//!   or an engine's `providers_changed` line: `reload_providers` to every
//!   other engine, idle ones at once, ones in a run when it settles.
//! - resources (plugins changed, memory indexing changed topics, a window
//!   changed skills, instructions or memory): `reload_resources`, the same way.
//! - `sandbox`: the hub's and the engines' limits at once
//!   (session-roles-and-mail FR-5); engines with no work start again with the
//!   new configuration, the others once their work is over.
//!
//! After each, the windows hear `engine://reloaded { chat, what }` and read
//! that engine's state again.
//!
//! Two settings a window kept in its local storage are app settings now
//! (NFR-4), so the shell follows them with no window open:
//! `mesh.autoStartSessions` (the hub starts sessions without asking) and
//! `connectors.chatOff` (connectors switched off per chat). A window hands
//! over what it had once (`settings_import_window`); a stored value is never
//! overwritten.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use evo_store::rusqlite::Connection;
use evo_store::settings;
use evo_store::DbError;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};

use super::{AppHost, EngineSupervisor, EnsureRequest, Host, Reason, Status};
use crate::db::Db;

/// Agents start nested sessions without asking (Settings → Sandbox); was the
/// window's `autoStartSessions`.
pub const AUTO_START_KEY: &str = "mesh.autoStartSessions";
/// Connectors (pool keys) switched off per chat, `{ chat: [server] }`; was
/// the window's `chatConnectorsOff`.
pub const CHAT_OFF_KEY: &str = "connectors.chatOff";
/// Sandbox changes come in bursts (a person flipping switches): engines start
/// again once the burst is over.
pub const SANDBOX_SETTLE: Duration = Duration::from_millis(600);
/// Commands that change providers for every engine, not just the one asked.
pub const PROVIDER_COMMANDS: &[&str] = &["set_api_key", "set_provider", "remove_provider", "refresh_provider_models", "refresh_models_catalog"];

/// What waits for an engine that could not take a change yet: one still
/// starting, or in a run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pending {
    pub approval: Option<String>,
    pub providers: bool,
    pub resources: bool,
    /// Start it again with the new sandbox once it has no work.
    pub sandbox: bool,
}

/// A change every engine must follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The approval mode (`ask`, `auto-edits`, `auto`).
    Approval(String),
    /// Providers or keys changed; `from` already follows them.
    Providers { from: Option<String> },
    /// Skills, plugins, memory or instructions changed; `from` already reloaded.
    Resources { from: Option<String> },
    /// Settings → Sandbox changed; the subagent limit when it is saved.
    Sandbox { max_running_subagents: Option<u64> },
}

/// Whether `line` from an engine means providers changed for every engine:
/// its own `providers_changed` (a sign-in, new models.dev data), or a
/// window's provider command that succeeded. The shell's own requests do not
/// count.
pub fn provider_change(line: &Value) -> bool {
    match line.get("type").and_then(Value::as_str) {
        Some("providers_changed") => true,
        Some("response") => {
            let success = line.get("success").and_then(Value::as_bool).unwrap_or(false);
            let command = line.get("command").and_then(Value::as_str).unwrap_or("");
            let own = line.get("id").and_then(Value::as_str).is_some_and(|id| id.starts_with(super::rpc::ID_PREFIX));
            success && !own && PROVIDER_COMMANDS.contains(&command)
        }
        _ => false,
    }
}

impl EngineSupervisor {
    /// Every engine the supervisor runs: chat, generation, status.
    fn engines(&self) -> Vec<(String, u64, Status)> {
        let mut engines: Vec<(String, u64, Status)> =
            self.lock().iter().map(|(chat, hosted)| (chat.clone(), hosted.generation, hosted.status)).collect();
        engines.sort_by(|a, b| a.0.cmp(&b.0));
        engines
    }

    /// Whether a run of `chat` is active.
    fn in_run(&self, chat: &str) -> bool {
        self.follow.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(chat).is_some_and(|follow| follow.run)
    }

    /// Keep `f`'s change for `chat`'s engine of `generation` until it can take it.
    fn mark(&self, chat: &str, generation: u64, f: impl FnOnce(&mut Pending)) -> Option<()> {
        self.with_record(chat, generation, |hosted| f(&mut hosted.pending))
    }

    /// Pass `change` to every engine (FR-8): at once where it can take it,
    /// later where it cannot (still starting, in a run, working).
    pub async fn setting_changed<H: Host>(&self, host: &H, change: Change) {
        for (chat, generation, status) in self.engines() {
            let ready = status == Status::Ready;
            match &change {
                Change::Approval(mode) => {
                    // An engine applies it from its next tool call, also in a run.
                    if ready {
                        self.send_approval(host, &chat, mode).await;
                    } else {
                        self.mark(&chat, generation, |pending| pending.approval = Some(mode.clone()));
                    }
                }
                Change::Providers { from } | Change::Resources { from } => {
                    if from.as_deref() == Some(chat.as_str()) {
                        continue;
                    }
                    let providers = matches!(change, Change::Providers { .. });
                    if ready && !self.in_run(&chat) {
                        self.reload(host, &chat, providers).await;
                    } else {
                        self.mark(&chat, generation, |pending| {
                            if providers {
                                pending.providers = true;
                            } else {
                                pending.resources = true;
                            }
                        });
                    }
                }
                Change::Sandbox { max_running_subagents } => {
                    // One still starting may have read the old configuration;
                    // a window's next start reads the new one.
                    if !ready {
                        continue;
                    }
                    if let Some(limit) = max_running_subagents {
                        // An engine without the command takes the limit at its next start.
                        if let Err(error) = host.request(&chat, "set_limits", json!({ "maxRunningSubagents": limit })).await {
                            host.log(&chat, "info", format!("set_limits: {error}"));
                        }
                    }
                    if self.working(host, &chat) {
                        self.mark(&chat, generation, |pending| pending.sandbox = true);
                    } else {
                        self.restart_for_settings(host, &chat, generation).await;
                    }
                }
            }
        }
    }

    /// What waited for `chat`'s engine of `generation` (it is set up, or its
    /// run settled): send it now. A sandbox restart waits while it still works.
    pub(super) async fn flush_pending<H: Host>(&self, host: &H, chat: &str, generation: u64) {
        let Some(pending) = self.with_record(chat, generation, |hosted| std::mem::take(&mut hosted.pending)) else {
            return;
        };
        if let Some(mode) = &pending.approval {
            self.send_approval(host, chat, mode).await;
        }
        if pending.providers {
            self.reload(host, chat, true).await;
        }
        if pending.resources {
            self.reload(host, chat, false).await;
        }
        if pending.sandbox {
            if self.working(host, chat) {
                self.mark(chat, generation, |pending| pending.sandbox = true);
            } else {
                self.restart_for_settings(host, chat, generation).await;
            }
        }
    }

    async fn send_approval<H: Host>(&self, host: &H, chat: &str, mode: &str) {
        match host.request(chat, "set_approval_mode", json!({ "mode": mode })).await {
            Ok(_) => host.reloaded(chat, "approval"),
            Err(error) => host.log(chat, "warn", format!("set_approval_mode failed: {error}")),
        }
    }

    async fn reload<H: Host>(&self, host: &H, chat: &str, providers: bool) {
        let (command, what) = if providers { ("reload_providers", "providers") } else { ("reload_resources", "resources") };
        match host.request(chat, command, json!({})).await {
            Ok(_) => host.reloaded(chat, what),
            Err(error) => host.log(chat, "warn", format!("{command} failed: {error}")),
        }
    }

    /// Start `chat`'s engine again in its folder, with the saved sandbox. A
    /// saved chat reopens its session; the windows follow (`engine://state`
    /// reason `settings`).
    async fn restart_for_settings<H: Host>(&self, host: &H, chat: &str, generation: u64) {
        let Some(folder) = self.with_record(chat, generation, |hosted| hosted.folder.clone()) else {
            return;
        };
        if self.is_held(chat) {
            return;
        }
        host.log(chat, "info", "restarting the engine with the new sandbox settings".to_string());
        let request = EnsureRequest {
            chat: chat.to_string(),
            cwd: Some(folder.display().to_string()),
            options: None,
            restore: host.row(chat).is_some(),
            restart: true,
            reason: Reason::Settings,
        };
        if let Err(error) = self.ensure(host, request).await {
            host.log(chat, "error", format!("could not restart the engine: {error}"));
            host.notice(chat, super::notices::Event::CouldNotStart);
        }
    }
}

// ── Settings moved from a window's storage (NFR-4) ──────────────────────────

/// What a window kept in its local storage before these became app settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSettings {
    pub auto_start_sessions: Option<bool>,
    pub chat_connectors_off: Option<Map<String, Value>>,
}

/// `{ chat: [server] }` with only non-empty lists of strings.
pub fn chat_off_value(value: &Map<String, Value>) -> Map<String, Value> {
    value
        .iter()
        .filter_map(|(chat, off)| {
            let off: Vec<Value> = off.as_array()?.iter().filter(|key| key.is_string()).cloned().collect();
            (!off.is_empty()).then(|| (chat.clone(), Value::Array(off)))
        })
        .collect()
}

/// Store what a window kept where nothing is stored yet; the keys written,
/// with their values. A stored value is never overwritten, so a second
/// import (another window, a later start) writes nothing.
pub fn import_window(conn: &Connection, values: &WindowSettings) -> Result<Vec<(&'static str, Value)>, DbError> {
    let mut written = Vec::new();
    if let Some(on) = values.auto_start_sessions {
        if settings::get(conn, AUTO_START_KEY)?.is_none() {
            settings::set(conn, AUTO_START_KEY, &Value::Bool(on))?;
            written.push((AUTO_START_KEY, Value::Bool(on)));
        }
    }
    if let Some(off) = &values.chat_connectors_off {
        let off = Value::Object(chat_off_value(off));
        if off.as_object().is_some_and(|off| !off.is_empty()) && settings::get(conn, CHAT_OFF_KEY)?.is_none() {
            settings::set(conn, CHAT_OFF_KEY, &off)?;
            written.push((CHAT_OFF_KEY, off));
        }
    }
    Ok(written)
}

/// The saved `mesh.autoStartSessions` (off when unset).
pub fn auto_start_sessions(db: &Db) -> bool {
    db.with(|conn| settings::get(conn, AUTO_START_KEY)).ok().flatten().and_then(|value| value.as_bool()).unwrap_or(false)
}

// ── The app ─────────────────────────────────────────────────────────────────

fn spawn_change(app: &AppHandle, change: Change) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        app.state::<EngineSupervisor>().setting_changed(&AppHost(app.clone()), change).await;
    });
}

/// An app setting was written (`settings_set`, an import): the engines and
/// the hub follow it (FR-8).
pub fn on_setting(app: &AppHandle, key: &str, value: &Value) {
    match key {
        crate::engine::APPROVAL_MODE_KEY => {
            if let Some(mode) = value.as_str().filter(|mode| matches!(*mode, "ask" | "auto-edits" | "auto")) {
                spawn_change(app, Change::Approval(mode.to_string()));
            }
        }
        crate::engine::SANDBOX_KEY => sandbox_changed(app),
        AUTO_START_KEY => {
            if let Some(hub) = app.try_state::<crate::mesh::MeshHub>() {
                hub.0.set_auto_start(value.as_bool().unwrap_or(false));
            }
        }
        CHAT_OFF_KEY => crate::agent_tools::mcp::chat_off_changed(app, value),
        _ => {}
    }
}

/// Settings → Sandbox changed: once the burst is over, the hub's session
/// limit, then every engine (limits at once, restarts when idle).
fn sandbox_changed(app: &AppHandle) {
    static TICK: AtomicU64 = AtomicU64::new(0);
    let tick = TICK.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SANDBOX_SETTLE).await;
        if TICK.load(Ordering::Relaxed) != tick {
            return;
        }
        let (limit, subagents) = {
            let db = app.state::<Db>();
            let config = crate::engine::sandbox_config(&db);
            (crate::engine::nested_session_limit(&db), config.get("maxRunningSubagents").and_then(Value::as_u64))
        };
        app.state::<crate::mesh::MeshHub>().0.set_max_children(limit);
        app.state::<EngineSupervisor>()
            .setting_changed(&AppHost(app.clone()), Change::Sandbox { max_running_subagents: subagents })
            .await;
    });
}

/// Providers changed in `from`'s engine: every other engine reloads them.
pub fn providers_changed(app: &AppHandle, from: &str) {
    spawn_change(app, Change::Providers { from: Some(from.to_string()) });
}

/// Plugins, memory topics, skills or instructions changed: every engine
/// re-reads its resources.
pub fn resources_changed(app: &AppHandle) {
    spawn_change(app, Change::Resources { from: None });
}

/// A window changed skills, instructions or memory through `except`'s engine
/// (which reloaded itself): every other engine re-reads its resources.
#[tauri::command]
pub fn engine_reload_resources(app: AppHandle, except: Option<String>) {
    spawn_change(&app, Change::Resources { from: except });
}

/// A window hands over the settings it kept in its local storage (NFR-4):
/// stored where nothing is stored yet, then followed like any change.
#[tauri::command]
pub async fn settings_import_window(app: AppHandle, values: WindowSettings) -> Result<(), String> {
    let db_app = app.clone();
    let written = tauri::async_runtime::spawn_blocking(move || db_app.state::<Db>().with(|conn| import_window(conn, &values)))
        .await
        .map_err(|error| error.to_string())??;
    for (key, value) in written {
        app.state::<Db>().log(evo_store::logs::NewLog::new("info", "shell", format!("moved the window's setting {key} to the app settings")));
        on_setting(&app, key, &value);
    }
    Ok(())
}

// docs/features/shell-hosted-engines/spec.md, FR-8, NFR-4 (T25).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::tests::{set_up, top, viewer, Fake};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU32;

    fn start_run(supervisor: &EngineSupervisor, fake: &Fake, chat: &str, generation: u64) {
        supervisor.follow_line(fake, chat, generation, &json!({ "type": "agent_start" }), 0);
    }

    async fn settle(supervisor: &EngineSupervisor, fake: &Fake, chat: &str, generation: u64) {
        supervisor.follow_line(fake, chat, generation, &json!({ "type": "agent_settled", "outcome": "aborted" }), 0);
        supervisor.on_settled(fake, chat, generation).await;
    }

    fn sorted(mut lines: Vec<String>) -> Vec<String> {
        lines.sort();
        lines
    }

    #[tokio::test]
    async fn the_approval_mode_reaches_every_set_up_engine_and_a_starting_one_once_set_up() {
        let fake = Fake::default().with_row("a", top(None)).with_row("b", top(None)).with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        set_up(&fake, &supervisor, "a").await;
        let b = set_up(&fake, &supervisor, "b").await;
        // A run does not hold it back: it applies from the next tool call.
        start_run(&supervisor, &fake, "b", b);
        let c = supervisor.ensure(&fake, viewer("c", Some("/w"), true)).await.unwrap().generation;
        supervisor.setting_changed(&fake, Change::Approval("auto".into())).await;
        assert_eq!(fake.lines("request a set_approval_mode"), [r#"request a set_approval_mode {"mode":"auto"}"#]);
        assert_eq!(fake.lines("request b set_approval_mode"), [r#"request b set_approval_mode {"mode":"auto"}"#]);
        assert!(fake.lines("request c set_approval_mode").is_empty());
        assert_eq!(sorted(fake.lines("reloaded")), ["reloaded a approval", "reloaded b approval"]);
        // Set up: it gets the mode before its mail goes out.
        supervisor.on_ready(&fake, "c", c).await;
        let log = fake.log();
        let mode = log.iter().position(|line| line == r#"request c set_approval_mode {"mode":"auto"}"#).expect("c got the mode");
        let hosted = log.iter().position(|line| line == "hosted c").unwrap();
        assert!(mode < hosted, "{log:?}");
    }

    #[tokio::test]
    async fn providers_changed_in_one_engine_reload_the_others_busy_ones_when_they_settle() {
        let fake = Fake::default().with_row("a", top(None)).with_row("b", top(None)).with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        set_up(&fake, &supervisor, "a").await;
        set_up(&fake, &supervisor, "b").await;
        let c = set_up(&fake, &supervisor, "c").await;
        start_run(&supervisor, &fake, "c", c);
        supervisor.setting_changed(&fake, Change::Providers { from: Some("a".into()) }).await;
        assert!(fake.lines("request a reload_providers").is_empty(), "the engine it came from already follows them");
        assert_eq!(fake.lines("request b reload_providers").len(), 1);
        assert!(fake.lines("request c reload_providers").is_empty(), "not in the middle of a run");
        settle(&supervisor, &fake, "c", c).await;
        assert_eq!(fake.lines("request c reload_providers").len(), 1);
        assert_eq!(sorted(fake.lines("reloaded")), ["reloaded b providers", "reloaded c providers"]);
        // Once: the next settle sends nothing more.
        start_run(&supervisor, &fake, "c", c);
        settle(&supervisor, &fake, "c", c).await;
        assert_eq!(fake.lines("request c reload_providers").len(), 1);
    }

    #[test]
    fn a_windows_provider_command_or_an_engines_own_change_means_providers_changed() {
        assert!(provider_change(&json!({ "type": "providers_changed", "provider": "acme" })));
        let response = |id: &str, command: &str, success: bool| json!({ "type": "response", "id": id, "command": command, "success": success });
        assert!(provider_change(&response("ab12cd-4", "set_api_key", true)));
        assert!(provider_change(&response("ab12cd-5", "remove_provider", true)));
        assert!(!provider_change(&response("ab12cd-6", "set_api_key", false)), "a failed command changed nothing");
        assert!(!provider_change(&response("ab12cd-7", "get_state", true)));
        assert!(!provider_change(&response("shell-3", "refresh_models_catalog", true)), "the shell's own requests");
        assert!(!provider_change(&json!({ "type": "agent_start" })));
    }

    #[tokio::test]
    async fn resources_reload_in_every_other_engine_busy_ones_when_they_settle() {
        let fake = Fake::default().with_row("a", top(None)).with_row("b", top(None));
        let supervisor = EngineSupervisor::default();
        set_up(&fake, &supervisor, "a").await;
        let b = set_up(&fake, &supervisor, "b").await;
        start_run(&supervisor, &fake, "b", b);
        supervisor.setting_changed(&fake, Change::Resources { from: None }).await;
        assert_eq!(fake.lines("request a reload_resources").len(), 1);
        assert!(fake.lines("request b reload_resources").is_empty());
        settle(&supervisor, &fake, "b", b).await;
        assert_eq!(fake.lines("request b reload_resources").len(), 1);
        // A window's change through `a` (which reloaded itself) skips it.
        supervisor.setting_changed(&fake, Change::Resources { from: Some("a".into()) }).await;
        assert_eq!(fake.lines("request a reload_resources").len(), 1);
        assert_eq!(fake.lines("request b reload_resources").len(), 2);
    }

    #[tokio::test]
    async fn a_sandbox_change_restarts_engines_with_no_work_and_the_others_once_they_settle() {
        let fake = Fake::default().with_row("a", top(None)).with_row("b", top(None));
        let supervisor = EngineSupervisor::default();
        let a = set_up(&fake, &supervisor, "a").await;
        let b = set_up(&fake, &supervisor, "b").await;
        start_run(&supervisor, &fake, "b", b);
        supervisor.setting_changed(&fake, Change::Sandbox { max_running_subagents: Some(2) }).await;
        // The limits reach both at once.
        assert_eq!(fake.lines("request a set_limits"), [r#"request a set_limits {"maxRunningSubagents":2}"#]);
        assert_eq!(fake.lines("request b set_limits"), [r#"request b set_limits {"maxRunningSubagents":2}"#]);
        // The idle one starts again in its folder, reopening its session.
        let starts = fake.lines("start a");
        assert_eq!(starts.len(), 2, "{:?}", fake.log());
        assert!(starts[1].starts_with("start a /w None"), "{starts:?}");
        let restarted = supervisor.attach_state(&fake, "a").generation.unwrap();
        assert!(restarted > a);
        assert!(fake.lines("state a").contains(&format!("state a {restarted} starting settings")), "{:?}", fake.log());
        // The busy one is left alone until its run is over.
        assert_eq!(fake.lines("start b").len(), 1);
        settle(&supervisor, &fake, "b", b).await;
        assert_eq!(fake.lines("start b").len(), 2, "{:?}", fake.log());
    }

    #[tokio::test]
    async fn a_sandbox_change_waits_while_mail_or_agents_keep_a_chat_working() {
        let fake = Fake::default().with_row("a", top(None));
        let supervisor = EngineSupervisor::default();
        let a = set_up(&fake, &supervisor, "a").await;
        fake.mail_for("a", true);
        supervisor.setting_changed(&fake, Change::Sandbox { max_running_subagents: None }).await;
        assert!(fake.lines("request a set_limits").is_empty(), "no limit saved");
        assert_eq!(fake.lines("start a").len(), 1);
        fake.mail_for("a", false);
        settle(&supervisor, &fake, "a", a).await;
        assert_eq!(fake.lines("start a").len(), 2);
    }

    /// A scratch app database, removed on drop.
    struct Store {
        root: PathBuf,
        conn: Connection,
    }

    impl Store {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!("evo-settings-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&root).unwrap();
            let conn = evo_store::open_at(&root.join("evo.db")).unwrap();
            Self { root, conn }
        }
    }

    impl Drop for Store {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn window(auto: Option<bool>, off: Value) -> WindowSettings {
        WindowSettings { auto_start_sessions: auto, chat_connectors_off: off.as_object().cloned() }
    }

    #[test]
    fn a_windows_settings_are_imported_once_and_never_over_a_stored_value() {
        let store = Store::new();
        let written = import_window(&store.conn, &window(Some(true), json!({ "c1": ["gh"], "c2": [], "c3": "bad" }))).unwrap();
        assert_eq!(written, [(AUTO_START_KEY, json!(true)), (CHAT_OFF_KEY, json!({ "c1": ["gh"] }))]);
        assert_eq!(settings::get(&store.conn, AUTO_START_KEY).unwrap(), Some(json!(true)));
        assert_eq!(settings::get(&store.conn, CHAT_OFF_KEY).unwrap(), Some(json!({ "c1": ["gh"] })));
        // Another window (or a later start) with other values writes nothing.
        let again = import_window(&store.conn, &window(Some(false), json!({ "c9": ["x"] }))).unwrap();
        assert!(again.is_empty());
        assert_eq!(settings::get(&store.conn, AUTO_START_KEY).unwrap(), Some(json!(true)));
        assert_eq!(settings::get(&store.conn, CHAT_OFF_KEY).unwrap(), Some(json!({ "c1": ["gh"] })));
    }

    #[test]
    fn a_value_the_person_already_set_in_the_app_is_kept() {
        let store = Store::new();
        settings::set(&store.conn, AUTO_START_KEY, &json!(false)).unwrap();
        let written = import_window(&store.conn, &window(Some(true), json!({}))).unwrap();
        assert!(written.is_empty(), "nothing to move: {written:?}");
        assert_eq!(settings::get(&store.conn, AUTO_START_KEY).unwrap(), Some(json!(false)));
        assert_eq!(settings::get(&store.conn, CHAT_OFF_KEY).unwrap(), None, "no switches, nothing stored");
    }
}
