//! The agent's scheduled-task tools, answered by the shell (ADR 0013, ADR 0014):
//! create, list, update and delete a task. Tasks live in the app database as the
//! setting `scheduler.tasks` (a JSON array). The shell is its only writer: these
//! tools, the Scheduler page (`scheduler_save`, `scheduler_delete`) and the
//! runner ([`run`], which starts due tasks with or without a window) all change
//! it through [`edit`], one read-modify-write at a time, so none overwrites
//! another. The shell emits `scheduler://changed` after every change and the
//! page reloads.
//!
//! Creating, changing and deleting a task keep running prompts on their own, so
//! each asks the person first, in every approval mode, exactly as the webview
//! tools did (the former `features/scheduler/agent-tools.ts`): their schemas,
//! descriptions and result texts are unchanged.
//!
//! Spec: docs/features/agent-tools-layers (FR-5, T11); docs/features/memory-and-scheduler (FR-10, FR-11, FR-17);
//! docs/features/shell-hosted-engines (FR-7, NFR-1).

pub mod run;
pub mod schedule;

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use evo_store::chats;
use evo_store::rusqlite::Connection;
use evo_store::settings;
use evo_store::DbError;
use serde_json::{json, Map, Value};
use tauri::Manager;

use self::schedule::{describe_schedule, format_when, iso, next_run, schedule_from, schedule_problem, Schedule};
use super::{Class, Tool, ToolCtx, ToolOutput};
use crate::db::Db;

/// The setting that holds the tasks (the page's store uses the same key).
pub const KEY: &str = "scheduler.tasks";
/// The tasks' last runs shown by `list_scheduled_tasks`.
const LAST_RUNS: usize = 3;
/// A prompt longer than this is cut in the approval question.
const PROMPT_PREVIEW: usize = 500;

const NOT_TOP_LEVEL: &str = "Scheduled tasks are managed from top-level chats.";

fn schedule_schema() -> Value {
    json!({
        "type": "object",
        "description": "When it runs. kind: \"manual\" (only on demand), \"once\" (at: ISO date-time), \"hourly\" (minute), \"daily\" / \"weekdays\" (time \"HH:MM\"), \"weekly\" (time, days: 0=Sun…6=Sat), or \"cron\" (expression, 5 fields). Local time.",
        "properties": {
            "kind": { "type": "string", "enum": ["manual", "once", "hourly", "daily", "weekdays", "weekly", "cron"] },
            "at": { "type": "string" },
            "minute": { "type": "number" },
            "time": { "type": "string" },
            "days": { "type": "array", "items": { "type": "number" } },
            "expression": { "type": "string" }
        },
        "required": ["kind"]
    })
}

/// The four scheduler tools.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "create_scheduled_task",
            description: "Schedule a prompt to run later or repeatedly, each time as a new chat in this folder (or another). Write the prompt so it stands on its own: the new chat has no memory of this one. The person approves it first.",
            schema: || {
                json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Short name, e.g. Morning PR summary." },
                        "prompt": { "type": "string", "description": "What the scheduled chat should do, complete on its own." },
                        "schedule": schedule_schema(),
                        "folder": { "type": "string", "description": "Folder the chat works in. Default: this chat's folder." }
                    },
                    "required": ["name", "prompt", "schedule"],
                    "additionalProperties": false
                })
            },
            class: Class::Command,
            run: |ctx, args| Box::pin(create_tool(ctx, args)),
        },
        Tool {
            name: "list_scheduled_tasks",
            description: "List the scheduled tasks: id, name, schedule, next run, whether on, last runs.",
            schema: || json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            class: Class::Read,
            run: |ctx, args| Box::pin(list_tool(ctx, args)),
        },
        Tool {
            name: "update_scheduled_task",
            description: "Change a scheduled task (name, prompt, schedule, folder) or switch it on or off. The person approves it first.",
            schema: || {
                json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "name": { "type": "string" },
                        "prompt": { "type": "string" },
                        "schedule": schedule_schema(),
                        "folder": { "type": "string" },
                        "enabled": { "type": "boolean" }
                    },
                    "required": ["id"],
                    "additionalProperties": false
                })
            },
            class: Class::Command,
            run: |ctx, args| Box::pin(update_tool(ctx, args)),
        },
        Tool {
            name: "delete_scheduled_task",
            description: "Delete a scheduled task. The person approves it first.",
            schema: || json!({ "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"], "additionalProperties": false }),
            class: Class::Command,
            run: |ctx, args| Box::pin(delete_tool(ctx, args)),
        },
    ]
}

// ── Arguments (pure) ────────────────────────────────────────────────────────

/// `String(args[key] ?? "")`: a string as is, nothing as an empty string, anything else as its text.
fn loose_str(args: &Value, key: &str) -> String {
    match args.get(key) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// The argument if it was given (JSON `null` counts as not given).
fn given<'a>(args: &'a Value, key: &str) -> Option<&'a Value> {
    args.get(key).filter(|value| !value.is_null())
}

/// JavaScript's `Boolean(value)`.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(text) => !text.is_empty(),
        _ => true,
    }
}

/// A new task, checked, with the question to ask the person.
#[derive(Debug, Clone, PartialEq)]
struct NewTask {
    name: String,
    prompt: String,
    folder: Option<String>,
    schedule: Schedule,
}

impl NewTask {
    fn from_args(args: &Value, default_folder: &str) -> Result<Self, String> {
        let schedule = schedule_from(args.get("schedule").filter(|v| !v.is_null()).unwrap_or(&json!({})))
            .map_err(|problem| format!("Schedule: {problem}"))?;
        if let Some(problem) = schedule_problem(&schedule) {
            return Err(format!("Schedule: {problem}"));
        }
        let name = loose_str(args, "name").trim().to_string();
        let prompt = loose_str(args, "prompt").trim().to_string();
        if name.is_empty() || prompt.is_empty() {
            return Err("create_scheduled_task needs a name and a prompt.".into());
        }
        let folder = match loose_str(args, "folder").trim() {
            "" => Some(default_folder.to_string()).filter(|folder| !folder.is_empty()),
            folder => Some(folder.to_string()),
        };
        Ok(Self { name, prompt, folder, schedule })
    }

    fn question(&self) -> (String, String) {
        let prompt = if self.prompt.chars().count() > PROMPT_PREVIEW {
            format!("{}…", self.prompt.chars().take(PROMPT_PREVIEW).collect::<String>())
        } else {
            self.prompt.clone()
        };
        let folder = self.folder.as_deref().map(|folder| format!(" · in {folder}")).unwrap_or_default();
        (format!("Schedule “{}”?", self.name), format!("{}{folder}\n\n{prompt}", describe_schedule(&self.schedule)))
    }
}

/// What `update_scheduled_task` changes.
#[derive(Debug, Clone, Default, PartialEq)]
struct Patch {
    name: Option<String>,
    prompt: Option<String>,
    schedule: Option<Schedule>,
    /// `Some(None)`: no folder (this computer).
    folder: Option<Option<String>>,
    enabled: Option<bool>,
}

impl Patch {
    fn from_args(args: &Value) -> Result<Self, String> {
        let schedule = match given(args, "schedule") {
            Some(input) => {
                let schedule = schedule_from(input).map_err(|problem| format!("Schedule: {problem}"))?;
                if let Some(problem) = schedule_problem(&schedule) {
                    return Err(format!("Schedule: {problem}"));
                }
                Some(schedule)
            }
            None => None,
        };
        let text = |key: &str| given(args, key).map(|_| loose_str(args, key));
        Ok(Self {
            name: text("name"),
            prompt: text("prompt"),
            schedule,
            folder: text("folder").map(|folder| Some(folder).filter(|folder| !folder.is_empty())),
            enabled: given(args, "enabled").map(truthy),
        })
    }

    /// One line per change, for the question and to tell whether there is any.
    fn changes(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(name) = &self.name {
            lines.push(format!("name → {name}"));
        }
        if self.prompt.is_some() {
            lines.push("new prompt".to_string());
        }
        if let Some(schedule) = &self.schedule {
            lines.push(format!("schedule → {}", describe_schedule(schedule)));
        }
        if let Some(folder) = &self.folder {
            lines.push(format!("folder → {}", folder.as_deref().unwrap_or("")));
        }
        if let Some(enabled) = self.enabled {
            lines.push(if enabled { "switch on" } else { "pause" }.to_string());
        }
        lines
    }
}

// ── Tasks as the page stores them (pure over the JSON array) ────────────────

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `task-<time in base 36>-<5 random characters>`, as the page names its tasks.
fn new_id(now: i64) -> String {
    new_id_with("task", now)
}

/// `<prefix>-<time in base 36>-<5 random characters>` (tasks, their runs).
fn new_id_with(prefix: &str, now: i64) -> String {
    let random = base36(RandomState::new().build_hasher().finish());
    let random = &random[random.len().saturating_sub(5)..];
    format!("{prefix}-{}-{random:0>5}", base36(now.max(0) as u64))
}

fn schedule_of(task: &Value) -> Option<Schedule> {
    task.get("schedule").and_then(|schedule| serde_json::from_value(schedule.clone()).ok())
}

fn text_of<'a>(task: &'a Value, key: &str) -> &'a str {
    task.get(key).and_then(Value::as_str).unwrap_or("")
}

fn describe_task_schedule(task: &Value) -> String {
    schedule_of(task).map(|schedule| describe_schedule(&schedule)).unwrap_or_else(|| "(unknown schedule)".into())
}

/// Set the task's next run: from its schedule when it is on, none when off.
fn with_next(task: &mut Value, now: i64) {
    let enabled = task.get("enabled").and_then(Value::as_bool).unwrap_or(false);
    let next = if enabled { schedule_of(task).and_then(|schedule| next_run(&schedule, now)) } else { None };
    task["nextRunAt"] = next.map_or(Value::Null, Value::from);
}

fn create_task(tasks: &mut Vec<Value>, new: &NewTask, now: i64) -> Value {
    let mut task = json!({
        "id": new_id(now),
        "name": if new.name.is_empty() { "Untitled task" } else { new.name.as_str() },
        "prompt": new.prompt,
        "folder": new.folder,
        "schedule": new.schedule,
        "enabled": true,
        "createdAt": now,
        "updatedAt": now,
        "createdBy": "agent",
        "nextRunAt": null,
        "runs": [],
    });
    with_next(&mut task, now);
    tasks.push(task.clone());
    task
}

/// Apply `patch` to task `id`; the changed task, or `None` when there is none.
fn update_task(tasks: &mut [Value], id: &str, patch: &Patch, now: i64) -> Option<Value> {
    let task = tasks.iter_mut().find(|task| text_of(task, "id") == id)?;
    if let Some(name) = &patch.name {
        let name = name.trim();
        if !name.is_empty() {
            task["name"] = name.into();
        }
    }
    if let Some(prompt) = &patch.prompt {
        task["prompt"] = prompt.trim().into();
    }
    if let Some(schedule) = &patch.schedule {
        task["schedule"] = json!(schedule);
    }
    if let Some(folder) = &patch.folder {
        task["folder"] = json!(folder);
    }
    if let Some(enabled) = patch.enabled {
        task["enabled"] = enabled.into();
    }
    task["updatedAt"] = now.into();
    with_next(task, now);
    Some(task.clone())
}

/// The page saves a task: with the `id` of one, the fields given replace its
/// own (a blank name keeps the old one); without, a new task of the person's.
fn save_task(tasks: &mut Vec<Value>, input: &Value, now: i64) -> Result<Value, String> {
    let text = |key: &str| input.get(key).and_then(Value::as_str).map(str::trim);
    if let Some(id) = text("id").filter(|id| !id.is_empty()) {
        let task = tasks.iter_mut().find(|task| text_of(task, "id") == id).ok_or_else(|| not_found(id))?;
        if let Some(name) = text("name").filter(|name| !name.is_empty()) {
            task["name"] = name.into();
        }
        if let Some(prompt) = text("prompt") {
            task["prompt"] = prompt.into();
        }
        if let Some(folder) = input.get("folder") {
            task["folder"] = folder.as_str().filter(|folder| !folder.is_empty()).map_or(Value::Null, Value::from);
        }
        if let Some(schedule) = input.get("schedule").filter(|schedule| schedule.is_object()) {
            task["schedule"] = page_schedule(schedule)?;
        }
        if let Some(enabled) = input.get("enabled").and_then(Value::as_bool) {
            task["enabled"] = enabled.into();
        }
        task["updatedAt"] = now.into();
        with_next(task, now);
        return Ok(task.clone());
    }
    let mut task = json!({
        "id": new_id(now),
        "name": text("name").filter(|name| !name.is_empty()).unwrap_or("Untitled task"),
        "prompt": text("prompt").unwrap_or(""),
        "folder": text("folder").filter(|folder| !folder.is_empty()),
        "schedule": match input.get("schedule").filter(|schedule| schedule.is_object()) {
            Some(schedule) => page_schedule(schedule)?,
            None => json!({ "kind": "manual" }),
        },
        "enabled": input.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        "createdAt": now,
        "updatedAt": now,
        "createdBy": "you",
        "nextRunAt": null,
        "runs": [],
    });
    with_next(&mut task, now);
    tasks.push(task.clone());
    Ok(task)
}

/// A schedule from the page, in the stored form (a `once` time as
/// milliseconds even when given as a date and time), like the agent's tools
/// store it.
fn page_schedule(input: &Value) -> Result<Value, String> {
    let schedule = schedule_from(input).map_err(|problem| format!("Schedule: {problem}"))?;
    serde_json::to_value(schedule).map_err(|error| error.to_string())
}

fn remove_task(tasks: &mut Vec<Value>, id: &str) -> Option<Value> {
    let at = tasks.iter().position(|task| text_of(task, "id") == id)?;
    Some(tasks.remove(at))
}

/// What `list_scheduled_tasks` shows of a task (the same fields, in the same order).
fn summary(task: &Value) -> Value {
    let when = |key: &str| task.get(key).and_then(Value::as_i64).and_then(iso).map_or(Value::Null, Value::from);
    let runs: Vec<Value> = task
        .get("runs")
        .and_then(Value::as_array)
        .map(|runs| {
            runs.iter()
                .take(LAST_RUNS)
                .map(|run| {
                    let mut out = Map::new();
                    out.insert("at".into(), run.get("at").and_then(Value::as_i64).and_then(iso).map_or(Value::Null, Value::from));
                    out.insert("chatId".into(), run.get("chatId").cloned().unwrap_or(Value::Null));
                    if let Some(error) = run.get("error").filter(|error| !error.is_null()) {
                        out.insert("error".into(), error.clone());
                    }
                    Value::Object(out)
                })
                .collect()
        })
        .unwrap_or_default();
    let mut out = Map::new();
    out.insert("id".into(), task.get("id").cloned().unwrap_or(Value::Null));
    out.insert("name".into(), task.get("name").cloned().unwrap_or(Value::Null));
    out.insert("schedule".into(), describe_task_schedule(task).into());
    out.insert("enabled".into(), task.get("enabled").cloned().unwrap_or(Value::Bool(false)));
    out.insert("nextRun".into(), when("nextRunAt"));
    out.insert("folder".into(), task.get("folder").cloned().unwrap_or(Value::Null));
    out.insert("prompt".into(), task.get("prompt").cloned().unwrap_or(Value::Null));
    out.insert("lastRuns".into(), Value::Array(runs));
    Value::Object(out)
}

fn list_text(tasks: &[Value]) -> String {
    if tasks.is_empty() {
        return "No scheduled tasks.".into();
    }
    let rows: Vec<Value> = tasks.iter().map(summary).collect();
    serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".into())
}

fn created_text(task: &Value, now: i64) -> String {
    let next = task
        .get("nextRunAt")
        .and_then(Value::as_i64)
        .map(|at| format!(", next {}", format_when(at, now)))
        .unwrap_or_default();
    format!(
        "Scheduled “{}” ({}): {}{next}. It appears on the Scheduler page.",
        text_of(task, "name"),
        text_of(task, "id"),
        describe_task_schedule(task)
    )
}

fn not_found(id: &str) -> String {
    format!("No scheduled task {id}; list_scheduled_tasks shows them.")
}

// ── The database (one read-modify-write under the shell's connection lock) ──

fn load(conn: &Connection) -> Result<Vec<Value>, DbError> {
    Ok(match settings::get(conn, KEY)? {
        Some(Value::Array(mut tasks)) => {
            tasks.iter_mut().for_each(in_milliseconds);
            tasks
        }
        _ => Vec::new(),
    })
}

/// Earlier builds of the agent's tools wrote task times in seconds. Read them
/// as milliseconds, so such a task is not taken as missed since 1970
/// (shell-hosted-engines FR-7).
fn in_milliseconds(task: &mut Value) {
    fn fix(value: &mut Value) {
        if let Some(at) = value.as_i64().filter(|at| *at > 0 && *at < 100_000_000_000) {
            *value = Value::from(at * 1000);
        }
    }
    for key in ["createdAt", "updatedAt", "nextRunAt"] {
        if let Some(value) = task.get_mut(key) {
            fix(value);
        }
    }
    if let Some(Value::Array(runs)) = task.get_mut("runs") {
        for run in runs {
            if let Some(at) = run.get_mut("at") {
                fix(at);
            }
        }
    }
}

/// One writer of `scheduler.tasks` at a time (shell-hosted-engines FR-7): every
/// read-modify-write holds it, whichever connection it uses.
static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Read the tasks, let `f` change them, and save when it says it did, as the
/// only writer: the page, the agent's tools and the runner all edit through
/// this, so none overwrites another's change.
pub(crate) fn edit<T>(conn: &Connection, f: impl FnOnce(&mut Vec<Value>) -> (T, bool)) -> Result<T, DbError> {
    let _one = WRITER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    mutate(conn, f)
}

/// Read the tasks, let `f` change them, and save when it says it did ([`edit`] holds the writer lock).
fn mutate<T>(conn: &Connection, f: impl FnOnce(&mut Vec<Value>) -> (T, bool)) -> Result<T, DbError> {
    let mut tasks = load(conn)?;
    let (out, changed) = f(&mut tasks);
    if changed {
        settings::set(conn, KEY, &Value::Array(tasks))?;
    }
    Ok(out)
}

/// Milliseconds since the epoch: the unit of every task time (`createdAt`,
/// `updatedAt`, `nextRunAt`, a run's `at`), as the page has always written
/// them. `evo_store::now()` counts seconds and must not be used here
/// (docs/features/shell-hosted-engines/spec.md, FR-7).
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ── The tools ───────────────────────────────────────────────────────────────

/// Run `f` on a blocking thread with the app database.
async fn with_db<T: Send + 'static>(
    ctx: &ToolCtx,
    f: impl FnOnce(&Db) -> Result<T, String> + Send + 'static,
) -> Result<T, ToolOutput> {
    let app = ctx.app.clone();
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<Db>()))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result)
        .map_err(|error| ToolOutput::error(format!("Could not use the scheduled tasks: {error}")))
}

/// Scheduled tasks belong to top-level chats (the webview gave nested sessions no scheduler tools).
async fn top_level(ctx: &ToolCtx) -> Result<(), ToolOutput> {
    let chat = ctx.chat.clone();
    let nested = with_db(ctx, move |db| db.with(|conn| chats::get_chat(conn, &chat)).map(|row| row.is_some_and(|row| row.parent_chat_id.is_some()))).await?;
    if nested {
        return Err(ToolOutput::error(NOT_TOP_LEVEL));
    }
    Ok(())
}

async fn load_tasks(ctx: &ToolCtx) -> Result<Vec<Value>, ToolOutput> {
    with_db(ctx, |db| db.with(|conn| load(conn))).await
}

fn changed(ctx: &ToolCtx) {
    ctx.emit("scheduler://changed", json!({ "chat": ctx.chat }));
}

async fn list_tool(ctx: ToolCtx, _args: Value) -> ToolOutput {
    if let Err(output) = top_level(&ctx).await {
        return output;
    }
    match load_tasks(&ctx).await {
        Ok(tasks) => {
            let rows: Vec<Value> = tasks.iter().map(summary).collect();
            ToolOutput::text(list_text(&tasks)).with_structured(json!({ "tasks": rows }))
        }
        Err(output) => output,
    }
}

async fn create_tool(ctx: ToolCtx, args: Value) -> ToolOutput {
    if let Err(output) = top_level(&ctx).await {
        return output;
    }
    let default_folder = ctx.folder().to_string_lossy().into_owned();
    let new = match NewTask::from_args(&args, &default_folder) {
        Ok(new) => new,
        Err(message) => return ToolOutput::error(message),
    };
    let (title, description) = new.question();
    match ctx.ask(&title, &description, &["Schedule it", "Don't schedule"]).await {
        -1 => return ToolOutput::error("Cancelled."),
        0 => {}
        _ => return ToolOutput::error("The person declined to schedule this task."),
    }
    let now = now_ms();
    let created = with_db(&ctx, move |db| db.with(|conn| edit(conn, |tasks| (create_task(tasks, &new, now), true)))).await;
    match created {
        Ok(task) => {
            changed(&ctx);
            ToolOutput::text(created_text(&task, now)).with_structured(summary(&task))
        }
        Err(output) => output,
    }
}

async fn update_tool(ctx: ToolCtx, args: Value) -> ToolOutput {
    if let Err(output) = top_level(&ctx).await {
        return output;
    }
    let id = loose_str(&args, "id");
    let tasks = match load_tasks(&ctx).await {
        Ok(tasks) => tasks,
        Err(output) => return output,
    };
    let Some(task) = tasks.iter().find(|task| text_of(task, "id") == id) else {
        return ToolOutput::error(not_found(&id));
    };
    let patch = match Patch::from_args(&args) {
        Ok(patch) => patch,
        Err(message) => return ToolOutput::error(message),
    };
    let changes = patch.changes();
    if changes.is_empty() {
        return ToolOutput::error("Nothing to change.");
    }
    let name = text_of(task, "name").to_string();
    if ctx.ask(&format!("Change “{name}”?"), &changes.join("\n"), &["Change it", "Keep it as it is"]).await != 0 {
        return ToolOutput::error("The person kept the task as it was.");
    }
    let (now, target) = (now_ms(), id.clone());
    let updated = with_db(&ctx, move |db| db.with(|conn| edit(conn, |tasks| {
        let task = update_task(tasks, &target, &patch, now);
        let changed = task.is_some();
        (task, changed)
    })))
    .await;
    match updated {
        Ok(Some(task)) => {
            changed(&ctx);
            ToolOutput::text(format!("Updated “{}”.", text_of(&task, "name"))).with_structured(summary(&task))
        }
        // Deleted while the question was open.
        Ok(None) => ToolOutput::error(not_found(&id)),
        Err(output) => output,
    }
}

async fn delete_tool(ctx: ToolCtx, args: Value) -> ToolOutput {
    if let Err(output) = top_level(&ctx).await {
        return output;
    }
    let id = loose_str(&args, "id");
    let tasks = match load_tasks(&ctx).await {
        Ok(tasks) => tasks,
        Err(output) => return output,
    };
    let Some(task) = tasks.iter().find(|task| text_of(task, "id") == id) else {
        return ToolOutput::error(not_found(&id));
    };
    let name = text_of(task, "name").to_string();
    if ctx.ask(&format!("Delete “{name}”?"), &describe_task_schedule(task), &["Delete it", "Keep it"]).await != 0 {
        return ToolOutput::error("The person kept the task.");
    }
    let target = id.clone();
    let removed = with_db(&ctx, move |db| db.with(|conn| edit(conn, |tasks| {
        let task = remove_task(tasks, &target);
        let changed = task.is_some();
        (task, changed)
    })))
    .await;
    match removed {
        Ok(_) => {
            changed(&ctx);
            ToolOutput::text(format!("Deleted “{name}”."))
        }
        Err(output) => output,
    }
}

#[cfg(test)]
mod clock_tests {
    // docs/features/shell-hosted-engines/spec.md, FR-7: task times are milliseconds.
    #[test]
    fn task_times_are_milliseconds() {
        let now = super::now_ms();
        assert!(now > 1_600_000_000_000, "{now} is not a time in milliseconds");
        assert!((now / 1000 - evo_store::now()).abs() <= 2);
    }

    #[test]
    fn a_page_schedule_is_stored_in_its_normal_form() {
        let mut tasks = Vec::new();
        let now = 1_794_465_600_000_i64;
        let task = super::save_task(
            &mut tasks,
            &serde_json::json!({"name": "T", "prompt": "P", "schedule": {"kind": "once", "at": "2099-01-01T09:00:00"}}),
            now,
        )
        .unwrap();
        assert!(task["schedule"]["at"].is_i64(), "{task}");
        assert_eq!(task["nextRunAt"], task["schedule"]["at"]);
        let bad = super::save_task(&mut tasks, &serde_json::json!({"name": "T", "prompt": "P", "schedule": {"kind": "once"}}), now);
        assert!(bad.is_err());
    }

    #[test]
    fn task_times_written_in_seconds_are_read_as_milliseconds() {
        let mut task = serde_json::json!({
            "createdAt": 1_794_465_313_i64, "updatedAt": 1_794_465_313_000_i64,
            "nextRunAt": 1_794_465_400_i64, "runs": [{"at": 1_794_465_313_i64}, {"at": 1_794_465_313_500_i64}]
        });
        super::in_milliseconds(&mut task);
        assert_eq!(task["createdAt"], 1_794_465_313_000_i64);
        assert_eq!(task["updatedAt"], 1_794_465_313_000_i64, "milliseconds stay");
        assert_eq!(task["nextRunAt"], 1_794_465_400_000_i64);
        assert_eq!(task["runs"][0]["at"], 1_794_465_313_000_i64);
        assert_eq!(task["runs"][1]["at"], 1_794_465_313_500_i64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A database in a folder of its own, removed afterwards.
    struct TempDb {
        dir: PathBuf,
        conn: Connection,
    }

    impl TempDb {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!("evo-scheduler-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            let conn = evo_store::open_at(&dir.join("evo.db")).expect("open the temporary database");
            Self { dir, conn }
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    const NOW: i64 = 1_794_000_000_000;

    fn new_task(name: &str, schedule: Schedule) -> NewTask {
        NewTask { name: name.into(), prompt: "Summarise the open PRs.".into(), folder: Some("C:\\work".into()), schedule }
    }

    fn daily() -> Schedule {
        Schedule::Daily { time: "09:00".into() }
    }

    // Definitions

    #[test]
    fn the_four_tools_keep_their_names_classes_and_schemas() {
        let tools = tools();
        let names: Vec<_> = tools.iter().map(|t| (t.name, t.class)).collect();
        assert_eq!(
            names,
            vec![
                ("create_scheduled_task", Class::Command),
                ("list_scheduled_tasks", Class::Read),
                ("update_scheduled_task", Class::Command),
                ("delete_scheduled_task", Class::Command),
            ]
        );
        for tool in &tools {
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(schema["additionalProperties"], false, "{}", tool.name);
            assert!(tool.description.len() > 20);
        }
        let create = (tools[0].schema)();
        assert_eq!(create["required"], json!(["name", "prompt", "schedule"]));
        assert_eq!(create["properties"]["schedule"]["required"], json!(["kind"]));
        assert_eq!(
            create["properties"]["schedule"]["properties"]["kind"]["enum"],
            json!(["manual", "once", "hourly", "daily", "weekdays", "weekly", "cron"])
        );
        assert_eq!((tools[2].schema)()["required"], json!(["id"]));
        assert_eq!((tools[2].schema)()["properties"]["enabled"]["type"], "boolean");
        assert_eq!((tools[3].schema)()["required"], json!(["id"]));
        assert_eq!((tools[1].schema)()["properties"], json!({}));
        assert!(create["properties"]["schedule"]["description"].as_str().unwrap().ends_with("Local time."));
    }

    // Arguments

    #[test]
    fn a_new_task_is_trimmed_and_defaults_to_the_chats_folder() {
        let args = json!({ "name": "  Morning PR summary ", "prompt": " Summarise. ", "schedule": { "kind": "daily", "time": "08:30" } });
        let new = NewTask::from_args(&args, "C:\\repo").unwrap();
        assert_eq!(new.name, "Morning PR summary");
        assert_eq!(new.prompt, "Summarise.");
        assert_eq!(new.folder.as_deref(), Some("C:\\repo"));
        assert_eq!(new.schedule, Schedule::Daily { time: "08:30".into() });
        let other = NewTask::from_args(&json!({ "name": "n", "prompt": "p", "schedule": { "kind": "manual" }, "folder": " D:\\other " }), "C:\\repo").unwrap();
        assert_eq!(other.folder.as_deref(), Some("D:\\other"));
    }

    #[test]
    fn a_new_task_without_a_name_or_prompt_or_with_a_bad_schedule_is_refused() {
        let bad = |args: Value| NewTask::from_args(&args, "C:\\repo").unwrap_err();
        assert_eq!(bad(json!({ "name": " ", "prompt": "p", "schedule": { "kind": "manual" } })), "create_scheduled_task needs a name and a prompt.");
        assert_eq!(bad(json!({ "name": "n", "schedule": { "kind": "manual" } })), "create_scheduled_task needs a name and a prompt.");
        assert_eq!(bad(json!({ "name": "n", "prompt": "p", "schedule": { "kind": "cron", "expression": "1 2" } })), "Schedule: use five fields: minute hour day month weekday");
        assert_eq!(bad(json!({ "name": "n", "prompt": "p", "schedule": { "kind": "once" } })), "Schedule: a \"once\" schedule needs `at` (ISO date and time)");
        assert_eq!(bad(json!({ "name": "n", "prompt": "p", "schedule": { "kind": "yearly" } })), "Schedule: unknown schedule kind \"yearly\"");
    }

    #[test]
    fn a_missing_schedule_is_a_manual_task() {
        let new = NewTask::from_args(&json!({ "name": "n", "prompt": "p" }), "").unwrap();
        assert_eq!(new.schedule, Schedule::Manual);
        assert_eq!(new.folder, None);
    }

    #[test]
    fn the_question_names_the_schedule_the_folder_and_the_prompt() {
        let (title, description) = new_task("Morning PR summary", daily()).question();
        assert_eq!(title, "Schedule “Morning PR summary”?");
        assert_eq!(description, "Every day at 09:00 · in C:\\work\n\nSummarise the open PRs.");
        let mut long = new_task("n", Schedule::Manual);
        long.folder = None;
        long.prompt = "é".repeat(600);
        let (_, description) = long.question();
        assert_eq!(description, format!("Only when you run it\n\n{}…", "é".repeat(500)));
    }

    #[test]
    fn a_patch_lists_what_it_changes() {
        let args = json!({ "id": "t", "name": "New", "prompt": "x", "schedule": { "kind": "hourly", "minute": 5 }, "folder": "D:\\d", "enabled": false });
        let patch = Patch::from_args(&args).unwrap();
        assert_eq!(patch.changes(), vec!["name → New", "new prompt", "schedule → Every hour at :05", "folder → D:\\d", "pause"]);
        assert_eq!(Patch::from_args(&json!({ "id": "t", "enabled": true })).unwrap().changes(), vec!["switch on"]);
        assert_eq!(Patch::from_args(&json!({ "id": "t", "folder": "" })).unwrap().folder, Some(None));
        assert!(Patch::from_args(&json!({ "id": "t" })).unwrap().changes().is_empty());
        // JSON null is "not given".
        assert!(Patch::from_args(&json!({ "id": "t", "name": null, "schedule": null })).unwrap().changes().is_empty());
        assert_eq!(Patch::from_args(&json!({ "id": "t", "schedule": { "kind": "weekly", "days": [] } })).unwrap().schedule, Some(Schedule::Weekly { time: "09:00".into(), days: vec![1] }));
        assert_eq!(Patch::from_args(&json!({ "id": "t", "schedule": { "kind": "cron", "expression": "bad" } })).unwrap_err(), "Schedule: use five fields: minute hour day month weekday");
    }

    #[test]
    fn loose_values_read_like_javascript() {
        assert_eq!(loose_str(&json!({ "a": 5 }), "a"), "5");
        assert_eq!(loose_str(&json!({ "a": null }), "a"), "");
        assert_eq!(loose_str(&json!({}), "a"), "");
        assert!(truthy(&json!("x")) && !truthy(&json!("")) && !truthy(&json!(0)) && truthy(&json!(true)) && !truthy(&Value::Null));
    }

    // Tasks

    #[test]
    fn ids_look_like_the_pages() {
        let id = new_id(NOW);
        let parts: Vec<_> = id.split('-').collect();
        assert_eq!(parts[0], "task");
        assert_eq!(parts[1], base36(NOW as u64));
        assert_eq!(parts[2].len(), 5);
        assert!(parts[2].chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(new_id(NOW), new_id(NOW));
        assert_eq!(base36(35), "z");
        assert_eq!(base36(36), "10");
        assert_eq!(base36(0), "0");
    }

    #[test]
    fn a_created_task_has_the_page_shape_and_a_next_run() {
        let mut tasks = Vec::new();
        let task = create_task(&mut tasks, &new_task("Morning", daily()), NOW);
        assert_eq!(tasks, vec![task.clone()]);
        assert_eq!(task["name"], "Morning");
        assert_eq!(task["folder"], "C:\\work");
        assert_eq!(task["schedule"], json!({ "kind": "daily", "time": "09:00" }));
        assert_eq!(task["enabled"], true);
        assert_eq!(task["createdBy"], "agent");
        assert_eq!(task["createdAt"], NOW);
        assert_eq!(task["updatedAt"], NOW);
        assert_eq!(task["runs"], json!([]));
        let next = task["nextRunAt"].as_i64().unwrap();
        assert!(next > NOW && next - NOW <= 24 * 3_600_000 + 3_600_000);
        let manual = create_task(&mut tasks, &new_task("Manual", Schedule::Manual), NOW);
        assert_eq!(manual["nextRunAt"], Value::Null);
        assert_eq!(tasks.len(), 2);
        let once = create_task(&mut tasks, &new_task("Once", Schedule::Once { at: NOW + 5_000 }), NOW);
        assert_eq!(once["nextRunAt"], NOW + 5_000);
        let past = create_task(&mut tasks, &new_task("Past", Schedule::Once { at: NOW - 5_000 }), NOW);
        assert_eq!(past["nextRunAt"], Value::Null);
    }

    #[test]
    fn an_update_changes_only_what_it_names_and_recomputes_the_next_run() {
        let mut tasks = Vec::new();
        let created = create_task(&mut tasks, &new_task("Morning", daily()), NOW);
        let id = text_of(&created, "id").to_string();
        // A run the page recorded survives.
        tasks[0]["runs"] = json!([{ "id": "run-1", "at": NOW, "chatId": "c1", "trigger": "schedule" }]);
        tasks[0]["extra"] = json!("kept");
        let later = NOW + 1_000;
        let patch = Patch { name: Some("  Evening  ".into()), schedule: Some(Schedule::Hourly { minute: 0 }), ..Default::default() };
        let task = update_task(&mut tasks, &id, &patch, later).unwrap();
        assert_eq!(task["name"], "Evening");
        assert_eq!(task["prompt"], "Summarise the open PRs.");
        assert_eq!(task["schedule"], json!({ "kind": "hourly", "minute": 0 }));
        assert_eq!(task["updatedAt"], later);
        assert_eq!(task["createdAt"], NOW);
        assert_eq!(task["runs"][0]["chatId"], "c1");
        assert_eq!(task["extra"], "kept");
        assert!(task["nextRunAt"].as_i64().unwrap() > later);
        assert_eq!(tasks[0], task);
    }

    #[test]
    fn pausing_clears_the_next_run_and_a_blank_name_keeps_the_old_one() {
        let mut tasks = Vec::new();
        let created = create_task(&mut tasks, &new_task("Morning", daily()), NOW);
        let id = text_of(&created, "id").to_string();
        let patch = Patch { name: Some("   ".into()), enabled: Some(false), prompt: Some("  New prompt ".into()), folder: Some(None), ..Default::default() };
        let task = update_task(&mut tasks, &id, &patch, NOW).unwrap();
        assert_eq!(task["name"], "Morning");
        assert_eq!(task["enabled"], false);
        assert_eq!(task["nextRunAt"], Value::Null);
        assert_eq!(task["prompt"], "New prompt");
        assert_eq!(task["folder"], Value::Null);
        let on = update_task(&mut tasks, &id, &Patch { enabled: Some(true), ..Default::default() }, NOW).unwrap();
        assert!(on["nextRunAt"].as_i64().unwrap() > NOW);
        assert!(update_task(&mut tasks, "task-missing", &patch, NOW).is_none());
    }

    #[test]
    fn deleting_removes_one_task_by_id() {
        let mut tasks = Vec::new();
        let a = create_task(&mut tasks, &new_task("A", Schedule::Manual), NOW);
        let b = create_task(&mut tasks, &new_task("B", Schedule::Manual), NOW);
        let removed = remove_task(&mut tasks, text_of(&a, "id")).unwrap();
        assert_eq!(removed["name"], "A");
        assert_eq!(tasks, vec![b]);
        assert!(remove_task(&mut tasks, "nope").is_none());
    }

    // Results

    #[test]
    fn an_empty_list_says_so() {
        assert_eq!(list_text(&[]), "No scheduled tasks.");
    }

    #[test]
    fn the_list_shows_each_task_with_its_last_three_runs() {
        let mut tasks = Vec::new();
        create_task(&mut tasks, &new_task("Morning", daily()), NOW);
        tasks[0]["id"] = "task-1".into();
        tasks[0]["nextRunAt"] = json!(1_794_182_400_000_i64);
        tasks[0]["runs"] = json!([
            { "id": "r4", "at": 1_794_100_000_000_i64, "chatId": "c4", "trigger": "schedule" },
            { "id": "r3", "at": 1_794_000_000_000_i64, "chatId": null, "trigger": "schedule", "error": "The chat could not be started." },
            { "id": "r2", "at": 1_793_900_000_000_i64, "chatId": "c2", "trigger": "manual" },
            { "id": "r1", "at": 1_793_800_000_000_i64, "chatId": "c1", "trigger": "manual" },
        ]);
        let text = list_text(&tasks);
        let rows: Value = serde_json::from_str(&text).unwrap();
        let task = &rows[0];
        assert_eq!(task["id"], "task-1");
        assert_eq!(task["schedule"], "Every day at 09:00");
        assert_eq!(task["enabled"], true);
        assert_eq!(task["nextRun"], "2026-11-09T00:00:00.000Z");
        assert_eq!(task["folder"], "C:\\work");
        assert_eq!(task["lastRuns"].as_array().unwrap().len(), 3);
        assert_eq!(task["lastRuns"][0], json!({ "at": "2026-11-08T01:06:40.000Z", "chatId": "c4" }));
        assert_eq!(task["lastRuns"][1]["error"], "The chat could not be started.");
        assert_eq!(task["lastRuns"][1]["chatId"], Value::Null);
        // Two-space indent and the page's field order.
        let keys: Vec<_> = task.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["id", "name", "schedule", "enabled", "nextRun", "folder", "prompt", "lastRuns"]);
        assert!(text.starts_with("[\n  {\n    \"id\": \"task-1\","));
    }

    #[test]
    fn a_task_with_no_next_run_lists_null() {
        let mut tasks = Vec::new();
        create_task(&mut tasks, &new_task("Manual", Schedule::Manual), NOW);
        assert_eq!(summary(&tasks[0])["nextRun"], Value::Null);
        assert_eq!(summary(&tasks[0])["lastRuns"], json!([]));
    }

    #[test]
    fn creating_reports_the_id_the_schedule_and_the_next_run() {
        let mut tasks = Vec::new();
        let task = create_task(&mut tasks, &new_task("Morning", daily()), NOW);
        let text = created_text(&task, NOW);
        assert!(text.starts_with(&format!("Scheduled “Morning” ({}): Every day at 09:00, next ", text_of(&task, "id"))), "{text}");
        assert!(text.ends_with(". It appears on the Scheduler page."), "{text}");
        let manual = create_task(&mut tasks, &new_task("Manual", Schedule::Manual), NOW);
        assert_eq!(
            created_text(&manual, NOW),
            format!("Scheduled “Manual” ({}): Only when you run it. It appears on the Scheduler page.", text_of(&manual, "id"))
        );
    }

    #[test]
    fn a_missing_task_points_at_the_list() {
        assert_eq!(not_found("task-x"), "No scheduled task task-x; list_scheduled_tasks shows them.");
    }

    // The database

    #[test]
    fn tasks_round_trip_through_the_settings_the_page_uses() {
        let db = TempDb::new();
        assert!(load(&db.conn).unwrap().is_empty());
        let created = mutate(&db.conn, |tasks| (create_task(tasks, &new_task("Morning", daily()), NOW), true)).unwrap();
        let stored = settings::get(&db.conn, KEY).unwrap().unwrap();
        assert_eq!(stored, json!([created.clone()]));
        // The page's other settings are untouched.
        settings::set(&db.conn, "sidebar.width", &json!(280)).unwrap();
        let id = text_of(&created, "id").to_string();
        let patch = Patch { enabled: Some(false), ..Default::default() };
        let updated = mutate(&db.conn, |tasks| (update_task(tasks, &id, &patch, NOW + 1), true)).unwrap().unwrap();
        assert_eq!(updated["enabled"], false);
        assert_eq!(load(&db.conn).unwrap()[0]["enabled"], false);
        let removed = mutate(&db.conn, |tasks| (remove_task(tasks, &id), true)).unwrap();
        assert!(removed.is_some());
        assert!(load(&db.conn).unwrap().is_empty());
        assert_eq!(settings::get(&db.conn, "sidebar.width").unwrap(), Some(json!(280)));
    }

    #[test]
    fn nothing_is_written_when_nothing_changed() {
        let db = TempDb::new();
        let found = mutate(&db.conn, |tasks| (remove_task(tasks, "none"), false)).unwrap();
        assert!(found.is_none());
        assert_eq!(settings::get(&db.conn, KEY).unwrap(), None);
    }

    #[test]
    fn a_setting_that_is_not_a_list_reads_as_no_tasks() {
        let db = TempDb::new();
        settings::set(&db.conn, KEY, &json!("garbage")).unwrap();
        assert!(load(&db.conn).unwrap().is_empty());
    }

    #[test]
    fn a_task_the_page_wrote_is_read_and_changed_without_losing_fields() {
        let db = TempDb::new();
        // As `scheduler-store.ts` saves it, with a field this module does not know.
        let page_task = json!({
            "id": "task-lx2k9-ab12c", "name": "Standup", "prompt": "p", "folder": null,
            "schedule": { "kind": "weekdays", "time": "09:00" }, "enabled": true,
            "createdAt": 1, "updatedAt": 1, "createdBy": "you", "nextRunAt": 2,
            "runs": [{ "id": "run-1", "at": 3, "chatId": "c", "trigger": "manual" }], "color": "blue"
        });
        settings::set(&db.conn, KEY, &json!([page_task])).unwrap();
        let patch = Patch { name: Some("Daily standup".into()), ..Default::default() };
        mutate(&db.conn, |tasks| (update_task(tasks, "task-lx2k9-ab12c", &patch, NOW), true)).unwrap();
        let stored = &load(&db.conn).unwrap()[0];
        assert_eq!(stored["name"], "Daily standup");
        assert_eq!(stored["color"], "blue");
        assert_eq!(stored["createdBy"], "you");
        assert_eq!(stored["runs"][0]["chatId"], "c");
        assert!(stored["nextRunAt"].as_i64().unwrap() > NOW);
        assert_eq!(describe_task_schedule(stored), "Every weekday at 09:00");
    }

    #[test]
    fn a_task_with_an_unreadable_schedule_still_lists() {
        let task = json!({ "id": "t", "name": "n", "schedule": { "kind": "fortnightly" }, "enabled": true });
        assert_eq!(summary(&task)["schedule"], "(unknown schedule)");
        let mut task = task;
        with_next(&mut task, NOW);
        assert_eq!(task["nextRunAt"], Value::Null);
    }
}
