//! The memory index worker: which chats are indexed, and when.
//!
//! Automatic indexing reads what is new in an idle chat with the model the
//! person chose, updates the memory topics and writes the chat's summary. The
//! reading and the model call are the engine's (`evo-engine --mode index`, a
//! short-lived process); this worker decides when one starts: every 30
//! seconds it looks for chats with messages no run has read that have been
//! idle long enough, runs one index process at a time, stores each result
//! (`chat_index` in `evo.db`) and tells the windows.
//!
//! Spec: docs/features/memory-topics-and-chat-recall (FR-24 – FR-26, FR-28,
//! FR-30, FR-33, NFR-5), plan 3.5;
//! docs/architecture/decisions/0016-memory-indexing-runs-in-a-short-lived-engine-process.md.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use evo_store::chats::{self, IndexCandidate, IndexFilter};
use evo_store::logs::NewLog;
use serde::Serialize;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::process::Command;
use tokio::sync::Notify;

use crate::db::Db;
use crate::engine::EngineState;

/// **Automatic indexing** (off unless `true`).
pub const AUTO_INDEX_KEY: &str = "memory.autoIndex";
/// **Index model**: `{provider, modelId}`.
pub const INDEX_MODEL_KEY: &str = "memory.indexModel";
/// **Index after idle**, in minutes.
pub const IDLE_MINUTES_KEY: &str = "memory.indexIdleMinutes";
/// When indexing was first switched on: chats last active before it are the
/// "earlier chats", indexed only on request.
pub const INDEX_SINCE_KEY: &str = "memory.indexSince";

const DEFAULT_IDLE_MINUTES: i64 = 5;
const TICK: Duration = Duration::from_secs(30);
/// An index process that takes longer is stopped.
const RUN_LIMIT: Duration = Duration::from_secs(300);
/// A chat whose run failed is tried again after this.
const RETRY_AFTER_MS: i64 = 60 * 60 * 1000;
/// After a failed run the worker waits this long before the next one.
const BACK_OFF_MS: i64 = 10 * 60 * 1000;
/// Most chats one pass looks at.
const PASS_LIMIT: u32 = 500;

/// The memory settings the worker follows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    pub on: bool,
    /// `(provider, model id)`.
    pub model: Option<(String, String)>,
    pub idle_minutes: i64,
    pub since_ms: Option<i64>,
}

/// The worker's settings out of the app settings.
pub fn settings_from(all: &Map<String, Value>) -> Settings {
    let model = all.get(INDEX_MODEL_KEY).and_then(|model| {
        let text = |key: &str| model.get(key).and_then(Value::as_str).filter(|text| !text.is_empty()).map(str::to_string);
        Some((text("provider")?, text("modelId")?))
    });
    Settings {
        // Indexing needs a model: without one it is off whatever the switch says.
        on: all.get(AUTO_INDEX_KEY).and_then(Value::as_bool).unwrap_or(false) && model.is_some(),
        model,
        idle_minutes: all.get(IDLE_MINUTES_KEY).and_then(Value::as_i64).unwrap_or(DEFAULT_IDLE_MINUTES).clamp(1, 120),
        since_ms: all.get(INDEX_SINCE_KEY).and_then(Value::as_i64),
    }
}

/// What a pass is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// The timer: chats idle for the set time.
    Idle,
    /// **Index now**: every chat with new messages, idle or not.
    Now,
    /// **Index earlier chats**: the chats from before indexing was switched on.
    Earlier,
}

/// Which chats a pass may read at `now_ms`; `None` when it may read none.
pub fn filter_for(settings: &Settings, pass: Pass, now_ms: i64) -> Option<IndexFilter> {
    settings.model.as_ref()?;
    let base = IndexFilter { failed_before_ms: now_ms - RETRY_AFTER_MS, limit: PASS_LIMIT, ..Default::default() };
    match pass {
        Pass::Idle if !settings.on => None,
        Pass::Idle => Some(IndexFilter { since_ms: settings.since_ms, idle_since_ms: Some(now_ms - settings.idle_minutes * 60_000), ..base }),
        Pass::Now if !settings.on => None,
        Pass::Now => Some(IndexFilter { since_ms: settings.since_ms, ..base }),
        // Without a start time every chat is an earlier one.
        Pass::Earlier => Some(IndexFilter { before_ms: settings.since_ms, ..base }),
    }
}

/// How an index process ended.
#[derive(Debug, Clone, PartialEq)]
pub enum RunResult {
    Done {
        through: Option<String>,
        summary: Option<String>,
        changes: u64,
        /// `paused` (memory is paused) or `nothing_new`.
        skipped: Option<String>,
    },
    /// The index model cannot be used: the worker stops and says why.
    ModelUnavailable(String),
    Failed(String),
}

/// The result an index process printed: its last line that is a result.
pub fn parse_result(stdout: &str) -> RunResult {
    let line = stdout.lines().rev().filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok()).find(|line| {
        matches!(line.get("type").and_then(Value::as_str), Some("index_result" | "index_error"))
    });
    let Some(line) = line else {
        return RunResult::Failed("the index run ended without a result".into());
    };
    let text = |key: &str| line.get(key).and_then(Value::as_str).filter(|text| !text.is_empty()).map(str::to_string);
    if line["type"] == "index_error" {
        let message = text("message").unwrap_or_else(|| "the index run failed".into());
        return match line.get("kind").and_then(Value::as_str) {
            Some("model_unavailable") => RunResult::ModelUnavailable(message),
            _ => RunResult::Failed(message),
        };
    }
    RunResult::Done { through: text("through"), summary: text("summary"), changes: line.get("changes").and_then(Value::as_u64).unwrap_or(0), skipped: text("skipped") }
}

/// The arguments of an index process for `candidate`.
pub fn index_args(candidate: &IndexCandidate, transcript: &std::path::Path, model: &(String, String)) -> Vec<String> {
    let mut args: Vec<String> = ["--mode", "index", "--session-id", &candidate.chat.id, "--session-db"].iter().map(|arg| arg.to_string()).collect();
    args.push(transcript.display().to_string());
    args.extend(["--provider".to_string(), model.0.clone(), "--model".to_string(), model.1.clone()]);
    if let Some(mark) = &candidate.indexed_through {
        args.extend(["--index-after".to_string(), mark.clone()]);
    }
    // A chat the Scheduler started gets a summary but changes no topic.
    if candidate.chat.scheduled {
        args.push("--index-summary-only".to_string());
    }
    args
}

/// What the windows show of the worker.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// `off` (not switched on, or no model), `idle`, `running`, or `stopped`
    /// (it cannot go on: `message` says why).
    pub state: &'static str,
    /// The chat being read.
    pub chat: Option<String>,
    pub message: Option<String>,
    /// Chats still to read in this pass.
    pub waiting: usize,
}

/// How a pass someone asked for (**Index now**, **Index earlier chats**)
/// ended, sent as `memory://index-pass` so the button can say so even when
/// there was nothing to read and the status never changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassReport {
    /// Chats an index run read.
    pub read: usize,
    /// Why a run failed; the worker waits [`BACK_OFF_MS`] before the next one.
    pub failed: Option<String>,
}

#[derive(Default)]
struct Inner {
    /// Why the worker cannot go on; cleared by a poke.
    stopped: Option<String>,
    back_off_until_ms: i64,
    /// A pass someone asked for.
    asked: Option<Pass>,
    status: Status,
    /// The running index process, to stop it with the app or with its chat.
    running: Option<(String, u32)>,
}

/// The worker, managed as Tauri state.
#[derive(Default)]
pub struct IndexWorker {
    inner: Mutex<Inner>,
    wake: Notify,
}

impl IndexWorker {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn status(&self) -> Status {
        self.lock().status.clone()
    }

    fn set_status(&self, app: &AppHandle, status: Status) {
        let changed = {
            let mut inner = self.lock();
            let changed = inner.status != status;
            inner.status = status.clone();
            changed
        };
        if changed {
            let _ = app.emit("memory://index-status", status);
        }
    }

    /// Look again now: a memory setting or the providers changed.
    pub fn poke(&self) {
        {
            let mut inner = self.lock();
            inner.stopped = None;
            inner.back_off_until_ms = 0;
        }
        self.wake.notify_one();
    }

    fn ask(&self, pass: Pass) {
        {
            let mut inner = self.lock();
            inner.asked = Some(pass);
            inner.stopped = None;
            inner.back_off_until_ms = 0;
        }
        self.wake.notify_one();
    }
}

fn settings(db: &Db) -> Settings {
    db.with(|conn| evo_store::settings::all(conn)).map(|all| settings_from(&all)).unwrap_or_default()
}

/// Run one index process for `candidate` and store what it found.
async fn index_one(app: &AppHandle, candidate: &IndexCandidate, model: &(String, String)) -> RunResult {
    let db = app.state::<Db>();
    let worker = app.state::<IndexWorker>();
    let chat = candidate.chat.id.as_str();
    let transcript = match db.transcript(chat) {
        Ok(path) => path,
        Err(error) => return RunResult::Failed(error),
    };
    let folder: PathBuf = crate::agent_tools::chat_folder(app, chat);
    let mut command = Command::new(crate::engine::engine_binary());
    command.args(index_args(candidate, &transcript, model)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    if folder.is_dir() {
        command.current_dir(&folder);
    }
    #[cfg(windows)]
    {
        // Do not flash a console window for the child process.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return RunResult::Failed(format!("could not start the index run: {error}")),
    };
    if let Some(pid) = child.id() {
        worker.lock().running = Some((chat.to_string(), pid));
    }
    let output = tokio::time::timeout(RUN_LIMIT, child.wait_with_output()).await;
    worker.lock().running = None;
    let result = match output {
        Ok(Ok(output)) => parse_result(&String::from_utf8_lossy(&output.stdout)),
        Ok(Err(error)) => RunResult::Failed(format!("the index run failed: {error}")),
        // Dropping the future dropped the child: `kill_on_drop` stopped it.
        Err(_) => RunResult::Failed("the index run took too long and was stopped".into()),
    };
    let now = chats::now_ms();
    let stored = match &result {
        // A paused memory read nothing: the chat waits as it was.
        RunResult::Done { skipped: Some(skipped), .. } if skipped == "paused" => Ok(true),
        RunResult::Done { through, summary, .. } => db.with(|conn| evo_store::chat_index::set_indexed(conn, chat, summary.as_deref(), through.as_deref(), now)),
        RunResult::Failed(message) => db.with(|conn| evo_store::chat_index::set_failed(conn, chat, now, message)),
        RunResult::ModelUnavailable(_) => Ok(true),
    };
    if let Err(error) = stored {
        db.log(NewLog::new("warn", "memory", format!("an index result was not stored: {error}")).chat(Some(chat)));
    }
    match &result {
        RunResult::Done { changes, skipped, .. } => {
            db.log(NewLog::new("info", "memory", "chat indexed").chat(Some(chat)).data(json!({ "changes": changes, "skipped": skipped })));
            if *changes > 0 {
                // Idle chats read the changed topics from their next message.
                let _ = app.emit("memory://changed", json!({ "chat": chat }));
            }
        }
        // The message is the engine's or the provider's, never the chat's text.
        RunResult::Failed(message) | RunResult::ModelUnavailable(message) => {
            db.log(NewLog::new("warn", "memory", "index run failed").chat(Some(chat)).data(json!({ "error": message })));
        }
    }
    result
}

/// One pass: every chat the pass may read, one after the other.
async fn pass(app: &AppHandle, wanted: Pass) -> PassReport {
    let db = app.state::<Db>();
    let worker = app.state::<IndexWorker>();
    let engines = app.state::<EngineState>();
    let now = chats::now_ms();
    let found = settings(&db);
    let mut report = PassReport::default();
    let Some(filter) = filter_for(&found, wanted, now) else {
        worker.set_status(app, Status { state: "off", ..Default::default() });
        return report;
    };
    let Some(model) = found.model.clone() else { return report };
    {
        let inner = worker.lock();
        if let Some(message) = &inner.stopped {
            let status = Status { state: "stopped", message: Some(message.clone()), ..Default::default() };
            drop(inner);
            worker.set_status(app, status);
            return report;
        }
        if inner.back_off_until_ms > now {
            return report;
        }
    }
    let candidates = db.with(|conn| chats::index_candidates(conn, &filter)).unwrap_or_default();
    // A chat in a run is not read: its turn is not finished.
    let candidates: Vec<IndexCandidate> = candidates.into_iter().filter(|candidate| !engines.is_working(&candidate.chat.id)).collect();
    for (index, candidate) in candidates.iter().enumerate() {
        // The switch or the model may have changed since the pass began.
        if settings(&db).model.as_ref() != Some(&model) || (wanted != Pass::Earlier && !settings(&db).on) {
            break;
        }
        worker.set_status(app, Status { state: "running", chat: Some(candidate.chat.id.clone()), message: None, waiting: candidates.len() - index - 1 });
        match index_one(app, candidate, &model).await {
            RunResult::Done { skipped: Some(skipped), .. } if skipped == "paused" => {
                worker.lock().stopped = Some("Memory is paused.".into());
                break;
            }
            RunResult::Done { .. } => report.read += 1,
            RunResult::ModelUnavailable(message) => {
                worker.lock().stopped = Some(message);
                break;
            }
            RunResult::Failed(message) => {
                worker.lock().back_off_until_ms = chats::now_ms() + BACK_OFF_MS;
                report.failed = Some(message);
                break;
            }
        }
    }
    let stopped = worker.lock().stopped.clone();
    worker.set_status(
        app,
        match stopped {
            Some(message) => Status { state: "stopped", message: Some(message), ..Default::default() },
            None => Status { state: "idle", ..Default::default() },
        },
    );
    report
}

/// Start the worker: a pass every [`TICK`], or at once when one is asked for.
pub fn setup(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let asked = app.state::<IndexWorker>().lock().asked.take();
            let report = pass(&app, asked.unwrap_or(Pass::Idle)).await;
            if asked.is_some() {
                let _ = app.emit("memory://index-pass", report);
            }
            let worker = app.state::<IndexWorker>();
            if worker.lock().asked.is_some() {
                continue;
            }
            let _ = tokio::time::timeout(TICK, worker.wake.notified()).await;
        }
    });
}

/// Stop the running index process, if any: the app is quitting.
pub fn shutdown(app: &AppHandle) {
    stop_running(app, None);
}

/// Stop the running index process when it reads `chat` (any chat for `None`):
/// its transcript is about to be removed.
pub fn stop_running(app: &AppHandle, chat: Option<&str>) {
    let Some(worker) = app.try_state::<IndexWorker>() else { return };
    let running = worker.lock().running.clone();
    if let Some((running_chat, pid)) = running {
        if chat.is_none_or(|chat| chat == running_chat) {
            kill(pid);
        }
    }
}

#[cfg(windows)]
fn kill(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(process, 1);
            let _ = CloseHandle(process);
        }
    }
}

#[cfg(not(windows))]
fn kill(pid: u32) {
    let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
}

/// A memory setting changed: remember when indexing was first switched on,
/// and look again.
pub fn settings_changed(app: &AppHandle, key: &str, value: &Value) {
    if key == AUTO_INDEX_KEY && value.as_bool() == Some(true) {
        let db = app.state::<Db>();
        let _ = db.with(|conn| {
            if evo_store::settings::get(conn, INDEX_SINCE_KEY)?.is_none() {
                evo_store::settings::set(conn, INDEX_SINCE_KEY, &json!(chats::now_ms()))?;
            }
            Ok(())
        });
    }
    app.state::<IndexWorker>().poke();
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn memory_index_status(worker: State<'_, IndexWorker>) -> Status {
    worker.status()
}

/// **Index now**: read every chat with new messages without waiting for it to
/// be idle.
#[tauri::command]
pub fn memory_index_now(worker: State<'_, IndexWorker>) {
    worker.ask(Pass::Now);
}

/// How many chats from before indexing was switched on have not been read.
#[tauri::command]
pub fn memory_index_earlier_count(db: State<'_, Db>) -> Result<usize, String> {
    let found = settings(&db);
    let Some(filter) = filter_for(&found, Pass::Earlier, chats::now_ms()) else {
        return Ok(0);
    };
    db.with(|conn| chats::index_candidates(conn, &filter)).map(|candidates| candidates.len())
}

/// **Index earlier chats**: read the chats from before indexing was switched on.
#[tauri::command]
pub fn memory_index_earlier(worker: State<'_, IndexWorker>) {
    worker.ask(Pass::Earlier);
}

/// A memory setting kept by the engine, or the providers, changed.
#[tauri::command]
pub fn memory_index_poke(worker: State<'_, IndexWorker>) {
    worker.poke();
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_store::chats::Chat;

    fn settings_of(value: Value) -> Settings {
        settings_from(value.as_object().unwrap())
    }

    fn chat(id: &str, scheduled: bool) -> Chat {
        serde_json::from_value(json!({
            "id": id, "workspaceId": null, "title": "T", "titleSource": "auto", "preview": null, "provider": null, "model": null,
            "pinned": false, "unread": false, "archivedAt": null, "turnCount": 1, "costUsd": 0.0, "tokens": 0,
            "createdAt": 0, "updatedAt": 0, "parentChatId": null, "origin": "user", "worktreePath": null,
            "memoryOff": false, "incognito": false, "scheduled": scheduled
        }))
        .unwrap()
    }

    // FR-24, FR-25: indexing is off until a model is chosen.
    #[test]
    fn memory_index_settings_need_a_model() {
        assert_eq!(settings_of(json!({})), Settings { on: false, model: None, idle_minutes: 5, since_ms: None });
        let no_model = settings_of(json!({ "memory.autoIndex": true }));
        assert!(!no_model.on, "the switch alone does not start indexing");
        assert!(!settings_of(json!({ "memory.autoIndex": true, "memory.indexModel": { "provider": "p", "modelId": "" } })).on);
        let on = settings_of(json!({
            "memory.autoIndex": true,
            "memory.indexModel": { "provider": "p", "modelId": "m" },
            "memory.indexIdleMinutes": 15,
            "memory.indexSince": 1_000
        }));
        assert_eq!(on, Settings { on: true, model: Some(("p".into(), "m".into())), idle_minutes: 15, since_ms: Some(1_000) });
        // A model chosen but the switch off: nothing is indexed by the timer.
        let chosen = settings_of(json!({ "memory.indexModel": { "provider": "p", "modelId": "m" } }));
        assert!(!chosen.on && chosen.model.is_some());
        // The idle time stays within sensible bounds.
        assert_eq!(settings_of(json!({ "memory.indexIdleMinutes": 0 })).idle_minutes, 1);
        assert_eq!(settings_of(json!({ "memory.indexIdleMinutes": 9_999 })).idle_minutes, 120);
    }

    // FR-26, FR-33: which chats each kind of pass may read.
    #[test]
    fn memory_index_passes_follow_the_settings() {
        let now = 10_000_000;
        let on = Settings { on: true, model: Some(("p".into(), "m".into())), idle_minutes: 5, since_ms: Some(4_000) };
        let idle = filter_for(&on, Pass::Idle, now).unwrap();
        assert_eq!((idle.since_ms, idle.before_ms, idle.idle_since_ms), (Some(4_000), None, Some(now - 300_000)));
        assert_eq!(idle.failed_before_ms, now - RETRY_AFTER_MS, "a failed chat waits an hour");
        // "Index now" drops the idle rule.
        let at_once = filter_for(&on, Pass::Now, now).unwrap();
        assert_eq!((at_once.since_ms, at_once.idle_since_ms), (Some(4_000), None));
        // "Index earlier chats": only the chats from before indexing was on.
        let earlier = filter_for(&on, Pass::Earlier, now).unwrap();
        assert_eq!((earlier.since_ms, earlier.before_ms, earlier.idle_since_ms), (None, Some(4_000), None));

        let off = Settings { on: false, ..on.clone() };
        assert!(filter_for(&off, Pass::Idle, now).is_none());
        assert!(filter_for(&off, Pass::Now, now).is_none());
        assert!(filter_for(&off, Pass::Earlier, now).is_some(), "earlier chats are read on request, with the model chosen");
        let no_model = Settings { model: None, ..on };
        for pass in [Pass::Idle, Pass::Now, Pass::Earlier] {
            assert!(filter_for(&no_model, pass, now).is_none());
        }
    }

    // FR-25, plan 3.3: what the worker reads from an index process.
    #[test]
    fn memory_index_results_are_read_from_the_last_result_line() {
        let done = "noise\n{\"type\":\"index_result\",\"chat\":\"c\",\"through\":\"e9\",\"summary\":\"S.\",\"changes\":2,\"skipped\":null}\n";
        assert_eq!(parse_result(done), RunResult::Done { through: Some("e9".into()), summary: Some("S.".into()), changes: 2, skipped: None });
        let nothing = r#"{"type":"index_result","chat":"c","through":"e9","summary":null,"changes":0,"skipped":"nothing_new"}"#;
        assert_eq!(parse_result(nothing), RunResult::Done { through: Some("e9".into()), summary: None, changes: 0, skipped: Some("nothing_new".into()) });
        let unavailable = r#"{"type":"index_error","chat":"c","kind":"model_unavailable","message":"no credentials for p"}"#;
        assert_eq!(parse_result(unavailable), RunResult::ModelUnavailable("no credentials for p".into()));
        let failed = r#"{"type":"index_error","chat":"c","kind":"failed","message":"timeout"}"#;
        assert_eq!(parse_result(failed), RunResult::Failed("timeout".into()));
        // A process that printed no result (a crash, a kill) failed.
        assert!(matches!(parse_result(""), RunResult::Failed(_)));
        assert!(matches!(parse_result("{\"type\":\"ready\"}\nnot json"), RunResult::Failed(_)));
    }

    // FR-30, plan 3.3: how an index process is started.
    #[test]
    fn memory_index_arguments_name_the_chat_the_model_and_the_mark() {
        let model = ("p".to_string(), "m".to_string());
        let transcript = std::path::Path::new("/t/c1.db");
        let fresh = IndexCandidate { chat: chat("c1", false), indexed_through: None };
        assert_eq!(index_args(&fresh, transcript, &model), ["--mode", "index", "--session-id", "c1", "--session-db", "/t/c1.db", "--provider", "p", "--model", "m"]);
        let again = IndexCandidate { chat: chat("c1", false), indexed_through: Some("e9".into()) };
        assert!(index_args(&again, transcript, &model).ends_with(&["--index-after".to_string(), "e9".to_string()]));
        // A chat the Scheduler started: a summary, no topic change.
        let task = IndexCandidate { chat: chat("c2", true), indexed_through: None };
        assert_eq!(index_args(&task, transcript, &model).last().map(String::as_str), Some("--index-summary-only"));
    }
}
