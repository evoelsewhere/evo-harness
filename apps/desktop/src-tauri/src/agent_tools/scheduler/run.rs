//! The Scheduler's runner, in the shell, with or without a window
//! (docs/features/shell-hosted-engines/spec.md, FR-7, NFR-1, NFR-3; plan 3.5
//! "Scheduler (FR-7)"; docs/features/memory-and-scheduler/spec.md, FR-10,
//! FR-12 – FR-14, FR-16, NFR-2).
//!
//! - **Start.** At app start (also when started hidden into the tray), a task
//!   missed while evo was closed runs once (`missed`), then goes on from now.
//! - **Tick.** Every [`TICK`] the tasks that are due start. A task whose last
//!   run's chat still works (a run, its agents, mail or a question) is
//!   skipped: its next run moves on.
//! - **A run** creates a top-level chat in the task's folder (which becomes a
//!   workspace), titled with the task's name, asks the supervisor for its
//!   engine and posts the task's prompt to it as hub mail delivered as a
//!   prompt. The mail is stored: it is sent once the engine is set up, and
//!   no window holds it, so a window that reloads loses nothing (NFR-3).
//!   Then the run is recorded (the last [`MAX_RUNS`]), a one-time task
//!   switches itself off, the next run is set, the windows hear
//!   `scheduler://changed`, and a notice says so.
//!
//! The Scheduler page edits tasks through the shell (`scheduler_save`,
//! `scheduler_delete`, `scheduler_run_now`), as the agent's tools do: every
//! read-modify-write of `scheduler.tasks` goes through [`super::edit`], one at
//! a time, so neither overwrites the other.

use std::future::Future;
use std::time::Duration;

use evo_store::chats::{self, NewChat};
use evo_store::rusqlite::Connection;
use evo_store::DbError;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::{edit, load, new_id_with, text_of, with_next};
use crate::db::Db;
use crate::supervisor::{notices, AppHost, EngineSupervisor, EnsureRequest, Reason};

/// How often due tasks are looked for: a due task starts within this time (NFR-2).
pub const TICK: Duration = Duration::from_secs(20);
/// Runs kept per task, newest first (memory-and-scheduler FR-10).
pub const MAX_RUNS: usize = 30;
/// A chat's title, at most (as the page cut it).
const TITLE_CHARS: usize = 80;
/// A task chat with no name (as the page names it).
const UNTITLED: &str = "New chat";

/// What started a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Schedule,
    /// Run now, from the page.
    Manual,
    /// Missed while evo was closed.
    Missed,
}

impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Trigger::Schedule => "schedule",
            Trigger::Manual => "manual",
            Trigger::Missed => "missed",
        }
    }
}

/// What the runner tells the person (a bell entry, and an OS notification
/// when no window is focused).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunNotice {
    pub task: String,
    pub title: String,
    pub body: String,
    pub error: bool,
    /// The run's chat, which the notice opens.
    pub chat: Option<String>,
}

/// What the runner needs of the app; the tests use a fake.
pub trait RunHost: Send + Sync {
    /// Use the app database.
    fn db<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T, DbError>) -> Result<T, String>;
    /// Whether `chat` still works: a run, its agents, mail or an open question.
    fn working(&self, chat: &str) -> bool;
    /// Start `chat`'s engine (a new chat: it starts on the default model).
    fn start(&self, chat: &str) -> impl Future<Output = Result<(), String>> + Send;
    /// Post `text` to `chat` as hub mail delivered as its prompt.
    fn prompt(&self, chat: &str, text: &str) -> impl Future<Output = ()> + Send;
    /// A chat was created: the windows' sidebars add it.
    fn chat_created(&self, chat: &chats::Chat);
    /// The tasks changed: the windows read them again.
    fn changed(&self);
    fn notice(&self, notice: RunNotice);
}

/// The ids of the tasks due at `now`: on, with a next run not after it.
pub fn due(tasks: &[Value], now: i64) -> Vec<String> {
    on_with_next(tasks, |next| next <= now)
}

/// The ids of the tasks missed before `now` (at app start).
pub fn missed(tasks: &[Value], now: i64) -> Vec<String> {
    on_with_next(tasks, |next| next < now)
}

fn on_with_next(tasks: &[Value], wanted: impl Fn(i64) -> bool) -> Vec<String> {
    tasks
        .iter()
        .filter(|task| task.get("enabled").and_then(Value::as_bool).unwrap_or(false))
        .filter(|task| task.get("nextRunAt").and_then(Value::as_i64).is_some_and(&wanted))
        .map(|task| text_of(task, "id").to_string())
        .collect()
}

/// The chat of a task's last run, if it has one.
fn last_chat(task: &Value) -> Option<&str> {
    task.pointer("/runs/0/chatId").and_then(Value::as_str).filter(|chat| !chat.is_empty())
}

/// Add `run` to task `id` (newest first, at most [`MAX_RUNS`]), switch off a
/// one-time task it ran on schedule, and set its next run from `now`.
pub fn record_run(tasks: &mut [Value], id: &str, run: Value, trigger: Trigger, now: i64) -> Option<Value> {
    let task = tasks.iter_mut().find(|task| text_of(task, "id") == id)?;
    let mut runs = vec![run];
    runs.extend(task.get("runs").and_then(Value::as_array).cloned().unwrap_or_default());
    runs.truncate(MAX_RUNS);
    task["runs"] = Value::Array(runs);
    let once = task.pointer("/schedule/kind").and_then(Value::as_str) == Some("once");
    if once && trigger != Trigger::Manual {
        task["enabled"] = Value::Bool(false);
    }
    with_next(task, now);
    Some(task.clone())
}

/// A chat title from a task's name, as the page made it.
fn title_from(name: &str) -> String {
    let title: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(TITLE_CHARS).collect();
    if title.is_empty() {
        UNTITLED.to_string()
    } else {
        title
    }
}

/// Create task chat `id` titled `name` in `folder` (a workspace).
fn create_chat<H: RunHost>(host: &H, id: &str, name: &str, folder: Option<&str>) -> Result<(), String> {
    if let Some(folder) = folder {
        crate::db::check_workspace_folder(folder)?;
    }
    let row = host.db(|conn| {
        let workspace = folder.map(|folder| chats::ensure_workspace(conn, folder)).transpose()?;
        chats::create_chat(
            conn,
            &NewChat { id: id.to_string(), workspace_id: workspace.map(|w| w.id), title: title_from(name), scheduled: true, ..Default::default() },
        )
    })?;
    host.chat_created(&row);
    Ok(())
}

/// Run task `id` now: its chat's id, or `None` when the run failed (its error
/// is recorded). An error when there is no such task.
pub async fn run<H: RunHost>(host: &H, id: &str, trigger: Trigger, now: i64) -> Result<Option<String>, String> {
    let task = host
        .db(|conn| load(conn))?
        .into_iter()
        .find(|task| text_of(task, "id") == id)
        .ok_or_else(|| format!("No scheduled task {id}."))?;
    let name = text_of(&task, "name").to_string();
    let folder = task.get("folder").and_then(Value::as_str).filter(|folder| !folder.is_empty()).map(str::to_string);
    let chat = crate::mesh::new_chat_id();
    let created = create_chat(host, &chat, &name, folder.as_deref());
    let started = match &created {
        Ok(()) => host.start(&chat).await,
        Err(error) => Err(error.clone()),
    };
    // Its first prompt, once its engine is on its way: the hub keeps it until
    // the engine is set up (NFR-3).
    if started.is_ok() {
        host.prompt(&chat, text_of(&task, "prompt")).await;
    }
    let created = created.is_ok().then(|| chat.clone());
    let mut record = json!({ "id": new_id_with("run", now), "at": now, "chatId": created, "trigger": trigger.as_str() });
    if let Err(error) = &started {
        record["error"] = json!(error);
    }
    let saved = host.db(|conn| edit(conn, |tasks| {
        let task = record_run(tasks, id, record, trigger, now);
        let changed = task.is_some();
        (task, changed)
    }));
    if let Err(error) = &saved {
        host.notice(RunNotice {
            task: id.to_string(),
            title: format!("Could not save “{name}”'s run"),
            body: error.clone(),
            error: true,
            chat: None,
        });
    }
    host.changed();
    let chat = started.as_ref().ok().map(|()| chat);
    host.notice(match &started {
        Ok(()) => RunNotice {
            task: id.to_string(),
            title: format!("Running “{name}”"),
            body: if trigger == Trigger::Missed {
                "Missed while evo was closed; running now.".to_string()
            } else {
                super::describe_task_schedule(&task)
            },
            error: false,
            chat: chat.clone(),
        },
        Err(error) => RunNotice { task: id.to_string(), title: format!("“{name}” could not run"), body: error.clone(), error: true, chat: created },
    });
    Ok(chat)
}

/// At app start: every task missed while evo was closed runs once.
pub async fn catch_up<H: RunHost>(host: &H, now: i64) {
    let Ok(tasks) = host.db(|conn| load(conn)) else {
        return;
    };
    for id in missed(&tasks, now) {
        let _ = run(host, &id, Trigger::Missed, now).await;
    }
}

/// Start the tasks due at `now`; one whose last run still works is skipped
/// and its next run moves on (memory-and-scheduler FR-13).
pub async fn tick<H: RunHost>(host: &H, now: i64) {
    let Ok(tasks) = host.db(|conn| load(conn)) else {
        return;
    };
    for id in due(&tasks, now) {
        let busy = tasks.iter().find(|task| text_of(task, "id") == id).and_then(last_chat).is_some_and(|chat| host.working(chat));
        if !busy {
            let _ = run(host, &id, Trigger::Schedule, now).await;
            continue;
        }
        let skipped = host.db(|conn| edit(conn, |tasks| {
            let task = tasks.iter_mut().find(|task| text_of(task, "id") == id);
            let changed = task.is_some();
            if let Some(task) = task {
                with_next(task, now);
            }
            ((), changed)
        }));
        if skipped.is_ok() {
            host.changed();
        }
    }
}

// ── The app ─────────────────────────────────────────────────────────────────

/// [`RunHost`] over the running app.
struct AppRun(AppHandle);

impl RunHost for AppRun {
    fn db<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T, DbError>) -> Result<T, String> {
        self.0.state::<Db>().with(f)
    }

    fn working(&self, chat: &str) -> bool {
        self.0.state::<EngineSupervisor>().working(&AppHost(self.0.clone()), chat)
    }

    fn start(&self, chat: &str) -> impl Future<Output = Result<(), String>> + Send {
        let (app, chat) = (self.0.clone(), chat.to_string());
        async move {
            let request = EnsureRequest { chat, cwd: None, options: None, restore: false, restart: false, reason: Reason::Schedule };
            app.state::<EngineSupervisor>().ensure(&AppHost(app.clone()), request).await.map(|_| ())
        }
    }

    fn prompt(&self, chat: &str, text: &str) -> impl Future<Output = ()> + Send {
        let (app, chat, text) = (self.0.clone(), chat.to_string(), text.to_string());
        async move {
            // Mail to itself, kept until its engine is set up and takes it.
            let (hub_app, to) = (app.clone(), chat.clone());
            let effects = tauri::async_runtime::spawn_blocking(move || {
                hub_app.state::<crate::mesh::MeshHub>().0.post(&to, &to, evo_mesh::wire::MailBody::new(text, evo_mesh::wire::Deliver::Prompt))
            })
            .await
            .unwrap_or_default();
            crate::mesh::apply(&app, effects).await;
        }
    }

    fn chat_created(&self, chat: &chats::Chat) {
        let _ = self.0.emit("chat://updated", json!({ "from": "shell", "chat": chat }));
    }

    fn changed(&self) {
        let _ = self.0.emit("scheduler://changed", json!({}));
    }

    fn notice(&self, notice: RunNotice) {
        let entry = notices::BellEntry {
            id: format!("scheduler-{}", new_id_with("notice", super::now_ms())),
            title: notice.title,
            body: notice.body,
            level: if notice.error { "error" } else { "info" }.to_string(),
            source: "scheduler".to_string(),
            chat: notice.chat.clone().unwrap_or_default(),
            key: format!("scheduler:{}", notice.task),
            at: crate::supervisor::now_ms(),
        };
        notices::deliver(&self.0, entry, notice.chat);
    }
}

/// At app start, with or without a window: missed runs, then a tick every
/// [`TICK`] for as long as the app runs.
pub fn setup(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let host = AppRun(app);
        catch_up(&host, super::now_ms()).await;
        loop {
            tokio::time::sleep(TICK).await;
            tick(&host, super::now_ms()).await;
        }
    });
}

/// The page saves a task (`id` given: changes it; none: creates it).
#[tauri::command]
pub async fn scheduler_save(app: AppHandle, task: Value) -> Result<Value, String> {
    let db_app = app.clone();
    let saved = tauri::async_runtime::spawn_blocking(move || {
        db_app.state::<Db>().with(|conn| edit(conn, |tasks| match super::save_task(tasks, &task, super::now_ms()) {
            Ok(saved) => (Ok(saved), true),
            Err(error) => (Err(error), false),
        }))
    })
    .await
    .map_err(|error| error.to_string())???;
    let _ = app.emit("scheduler://changed", json!({}));
    Ok(saved)
}

/// The page deletes a task.
#[tauri::command]
pub async fn scheduler_delete(app: AppHandle, id: String) -> Result<(), String> {
    let db_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        db_app.state::<Db>().with(|conn| edit(conn, |tasks| {
            let removed = super::remove_task(tasks, &id).is_some();
            ((), removed)
        }))
    })
    .await
    .map_err(|error| error.to_string())??;
    let _ = app.emit("scheduler://changed", json!({}));
    Ok(())
}

/// The page's Run now: the new chat's id, or `None` when it could not start.
#[tauri::command]
pub async fn scheduler_run_now(app: AppHandle, id: String) -> Result<Option<String>, String> {
    run(&AppRun(app), &id, Trigger::Manual, super::now_ms()).await
}

// docs/features/shell-hosted-engines/spec.md, FR-7 (T28).
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    const NOW: i64 = 1_794_000_000_000;

    /// The app database and a project folder of a test, removed on drop.
    struct Fake {
        root: PathBuf,
        conn: Mutex<Connection>,
        working: Mutex<HashSet<String>>,
        start_fails: bool,
        log: Mutex<Vec<String>>,
    }

    impl Fake {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!("evo-scheduler-run-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(root.join("project")).unwrap();
            let conn = evo_store::open_at(&root.join("evo.db")).unwrap();
            Self { root, conn: Mutex::new(conn), working: Mutex::default(), start_fails: false, log: Mutex::default() }
        }

        fn folder(&self) -> String {
            self.root.join("project").display().to_string()
        }

        fn note(&self, line: String) {
            self.log.lock().unwrap().push(line);
        }

        fn lines(&self, prefix: &str) -> Vec<String> {
            self.log.lock().unwrap().iter().filter(|line| line.starts_with(prefix)).cloned().collect()
        }

        fn tasks(&self) -> Vec<Value> {
            load(&self.conn.lock().unwrap()).unwrap()
        }

        fn task(&self, id: &str) -> Value {
            self.tasks().into_iter().find(|task| text_of(task, "id") == id).unwrap()
        }

        fn put(&self, tasks: Value) {
            evo_store::settings::set(&self.conn.lock().unwrap(), super::super::KEY, &tasks).unwrap();
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    impl RunHost for Fake {
        fn db<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T, DbError>) -> Result<T, String> {
            f(&mut self.conn.lock().unwrap()).map_err(|error| error.to_string())
        }

        fn working(&self, chat: &str) -> bool {
            self.working.lock().unwrap().contains(chat)
        }

        fn start(&self, chat: &str) -> impl Future<Output = Result<(), String>> + Send {
            self.note(format!("start {chat}"));
            let fails = self.start_fails;
            async move {
                if fails {
                    Err("engine binary not found".to_string())
                } else {
                    Ok(())
                }
            }
        }

        fn prompt(&self, chat: &str, text: &str) -> impl Future<Output = ()> + Send {
            self.note(format!("prompt {chat} {text}"));
            async {}
        }

        fn chat_created(&self, chat: &chats::Chat) {
            self.note(format!("created {} {}", chat.id, chat.title));
        }

        fn changed(&self) {
            self.note("changed".to_string());
        }

        fn notice(&self, notice: RunNotice) {
            self.note(format!("notice {} | {} | {}", notice.title, notice.body, notice.chat.as_deref().unwrap_or("-")));
        }
    }

    fn task(id: &str, schedule: Value, next: Option<i64>, enabled: bool, folder: Option<&str>) -> Value {
        json!({
            "id": id, "name": format!("Task {id}"), "prompt": format!("Do {id}."), "folder": folder,
            "schedule": schedule, "enabled": enabled, "createdAt": 1, "updatedAt": 1, "createdBy": "you",
            "nextRunAt": next, "runs": [],
        })
    }

    fn daily() -> Value {
        json!({ "kind": "daily", "time": "09:00" })
    }

    #[tokio::test]
    async fn a_task_missed_while_evo_was_closed_runs_once_at_start() {
        let fake = Fake::new();
        fake.put(json!([
            task("missed", daily(), Some(NOW - 60_000), true, None),
            task("later", daily(), Some(NOW + 60_000), true, None),
            task("off", daily(), Some(NOW - 60_000), false, None),
        ]));
        catch_up(&fake, NOW).await;
        assert_eq!(fake.lines("start").len(), 1, "{:?}", fake.log.lock().unwrap());
        let missed = fake.task("missed");
        assert_eq!(missed["runs"][0]["trigger"], "missed");
        assert!(missed["nextRunAt"].as_i64().unwrap() > NOW, "it goes on from now");
        assert_eq!(fake.lines("notice"), [format!("notice Running “Task missed” | Missed while evo was closed; running now. | {}", missed["runs"][0]["chatId"].as_str().unwrap())]);
        // Started again (the next start, a hidden one): nothing more to catch up.
        catch_up(&fake, NOW).await;
        assert_eq!(fake.lines("start").len(), 1);
        assert!(fake.task("later")["runs"].as_array().unwrap().is_empty());
        assert!(fake.task("off")["runs"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_due_task_starts_on_the_tick_as_a_scheduled_chat_in_its_folder() {
        let fake = Fake::new();
        let folder = fake.folder();
        fake.put(json!([task("t", daily(), Some(NOW - 5), true, Some(&folder)), task("soon", daily(), Some(NOW + 5), true, None)]));
        tick(&fake, NOW).await;
        let t = fake.task("t");
        let chat = t["runs"][0]["chatId"].as_str().expect("the run's chat").to_string();
        assert_eq!(t["runs"][0]["trigger"], "schedule");
        assert_eq!(t["runs"][0]["at"], NOW);
        assert!(t["runs"][0].get("error").is_none());
        assert!(t["nextRunAt"].as_i64().unwrap() > NOW);
        let conn = fake.conn.lock().unwrap();
        let row = chats::get_chat(&conn, &chat).unwrap().expect("the chat row");
        assert!(row.scheduled);
        assert_eq!(row.title, "Task t");
        assert!(row.parent_chat_id.is_none());
        let workspace = chats::get_workspace(&conn, row.workspace_id.as_deref().unwrap()).unwrap().unwrap();
        assert_eq!(PathBuf::from(workspace.path), PathBuf::from(&folder));
        drop(conn);
        assert!(fake.task("soon")["runs"].as_array().unwrap().is_empty());
        assert_eq!(fake.lines("changed").len(), 1);
    }

    #[tokio::test]
    async fn the_first_prompt_is_hub_mail_posted_once_the_engine_is_asked_for() {
        let fake = Fake::new();
        fake.put(json!([task("t", json!({ "kind": "manual" }), None, true, None)]));
        let chat = run(&fake, "t", Trigger::Manual, NOW).await.unwrap().expect("a chat");
        // In order: the chat, its engine, then the prompt as mail. Nothing
        // else keeps the prompt: a window that reloads meanwhile loses nothing.
        let log: Vec<String> = fake.log.lock().unwrap().iter().filter(|line| line.contains(&chat)).cloned().collect();
        assert_eq!(log[..3], [format!("created {chat} Task t"), format!("start {chat}"), format!("prompt {chat} Do t.")]);
        assert_eq!(fake.lines("prompt").len(), 1, "sent once");
    }

    #[tokio::test]
    async fn a_run_that_cannot_start_records_its_error_and_sends_no_prompt() {
        let mut fake = Fake::new();
        fake.start_fails = true;
        fake.put(json!([task("t", daily(), Some(NOW - 5), true, None), task("gone", daily(), Some(NOW - 5), true, Some("Z:\\no\\such\\folder"))]));
        tick(&fake, NOW).await;
        let t = fake.task("t");
        // Its chat exists (the person can open it); its engine did not start.
        assert!(t["runs"][0]["chatId"].is_string());
        assert_eq!(t["runs"][0]["error"], "engine binary not found");
        assert!(fake.lines("prompt").is_empty());
        let gone = fake.task("gone");
        assert_eq!(gone["runs"][0]["chatId"], Value::Null);
        assert!(gone["runs"][0]["error"].as_str().unwrap().contains("folder"));
        assert_eq!(fake.lines("notice “Task t” could not run").len(), 1);
    }

    #[tokio::test]
    async fn a_run_whose_last_chat_still_works_is_skipped_and_its_next_run_moves_on() {
        let fake = Fake::new();
        let mut busy = task("t", daily(), Some(NOW - 5), true, None);
        busy["runs"] = json!([{ "id": "run-1", "at": 1, "chatId": "c-busy", "trigger": "schedule" }]);
        fake.put(json!([busy]));
        fake.working.lock().unwrap().insert("c-busy".into());
        tick(&fake, NOW).await;
        assert!(fake.lines("start").is_empty());
        let t = fake.task("t");
        assert_eq!(t["runs"].as_array().unwrap().len(), 1);
        assert!(t["nextRunAt"].as_i64().unwrap() > NOW);
        // Done working: the next due run starts.
        fake.working.lock().unwrap().clear();
        let next = t["nextRunAt"].as_i64().unwrap();
        tick(&fake, next).await;
        assert_eq!(fake.lines("start").len(), 1);
    }

    #[tokio::test]
    async fn a_one_time_task_switches_off_after_its_run_but_not_after_run_now() {
        let fake = Fake::new();
        let once = json!({ "kind": "once", "at": NOW - 5 });
        fake.put(json!([task("once", once.clone(), Some(NOW - 5), true, None), task("now", json!({ "kind": "once", "at": NOW + 3_600_000 }), Some(NOW + 3_600_000), true, None)]));
        tick(&fake, NOW).await;
        let ran = fake.task("once");
        assert_eq!(ran["enabled"], false);
        assert_eq!(ran["nextRunAt"], Value::Null);
        run(&fake, "now", Trigger::Manual, NOW).await.unwrap();
        let manual = fake.task("now");
        assert_eq!(manual["enabled"], true);
        assert_eq!(manual["nextRunAt"], NOW + 3_600_000);
        assert_eq!(manual["runs"][0]["trigger"], "manual");
    }

    #[test]
    fn a_task_keeps_its_last_thirty_runs_newest_first() {
        let mut t = task("t", daily(), Some(NOW), true, None);
        t["runs"] = Value::Array((0..30).map(|n| json!({ "id": format!("run-{n}") })).collect());
        let mut tasks = vec![t];
        let saved = record_run(&mut tasks, "t", json!({ "id": "run-new" }), Trigger::Schedule, NOW).unwrap();
        let runs = saved["runs"].as_array().unwrap();
        assert_eq!(runs.len(), MAX_RUNS);
        assert_eq!(runs[0]["id"], "run-new");
        assert_eq!(runs[29]["id"], "run-28");
        assert!(record_run(&mut tasks, "none", json!({}), Trigger::Manual, NOW).is_none());
    }

    #[test]
    fn edits_of_the_page_and_of_the_agents_tools_at_the_same_time_are_all_kept() {
        let fake = Fake::new();
        let path = fake.root.join("evo.db");
        let writer = |prefix: &'static str| {
            let path = path.clone();
            std::thread::spawn(move || {
                // Each its own connection: only the writer lock keeps them apart.
                let conn = evo_store::open_at(&path).unwrap();
                for n in 0..25 {
                    let input = json!({ "name": format!("{prefix}-{n}"), "prompt": "p", "folder": null, "schedule": { "kind": "manual" } });
                    edit(&conn, |tasks| (super::super::save_task(tasks, &input, NOW).unwrap(), true)).unwrap();
                }
            })
        };
        let (page, tool) = (writer("page"), writer("tool"));
        page.join().unwrap();
        tool.join().unwrap();
        let names: HashSet<String> = fake.tasks().iter().map(|task| text_of(task, "name").to_string()).collect();
        assert_eq!(names.len(), 50, "an edit was lost");
    }

    #[test]
    fn the_page_creates_and_changes_tasks_through_the_shell() {
        let mut tasks = Vec::new();
        let input = json!({ "name": "  Morning  ", "prompt": " Sum up. ", "folder": null, "schedule": daily() });
        let created = super::super::save_task(&mut tasks, &input, NOW).unwrap();
        assert!(text_of(&created, "id").starts_with("task-"));
        assert_eq!(created["name"], "Morning");
        assert_eq!(created["prompt"], "Sum up.");
        assert_eq!(created["createdBy"], "you");
        assert_eq!(created["enabled"], true);
        assert!(created["nextRunAt"].as_i64().unwrap() > NOW);
        let id = text_of(&created, "id").to_string();
        let changed = super::super::save_task(&mut tasks, &json!({ "id": id, "enabled": false, "name": " " }), NOW + 1).unwrap();
        assert_eq!(changed["enabled"], false);
        assert_eq!(changed["nextRunAt"], Value::Null);
        assert_eq!(changed["name"], "Morning", "a blank name keeps the old one");
        assert_eq!(changed["prompt"], "Sum up.");
        assert_eq!(changed["updatedAt"], NOW + 1);
        assert_eq!(tasks.len(), 1);
        assert!(super::super::save_task(&mut tasks, &json!({ "id": "task-none", "name": "x" }), NOW).is_err());
    }
}
