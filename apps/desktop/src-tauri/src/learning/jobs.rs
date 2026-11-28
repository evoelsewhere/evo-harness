//! Running a learning step: one `evo-engine --mode learn` process per
//! memory owner at a time, stopped when it runs too long, its result read
//! from its last line, one log line when it fails
//! (docs/features/learning-layers/spec.md, NFR-2, NFR-5; plan 3.5).
//!
//! A chat never waits on a step: the step is its own process, reads files
//! and writes each one with an atomic rename.

use std::collections::HashSet;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use evo_store::logs::NewLog;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::process::Command;
use tokio::sync::Notify;

use crate::db::Db;

/// A step's process is stopped after this: each model call has three
/// minutes (NFR-5), and a tidy-up may make one per topic.
pub const PROCESS_LIMIT: Duration = Duration::from_secs(600);

/// The learning worker's state, managed by Tauri.
#[derive(Default)]
pub struct Learning {
    /// Owners with a step running now: one at a time each.
    running: Mutex<HashSet<String>>,
    /// Wakes the clock loop early (a setting changed).
    pub wake: Notify,
    /// Steps that found nothing to do today, `(owner, task, day)`: not tried
    /// again before tomorrow.
    pub checked: Mutex<HashSet<(String, String, String)>>,
}

impl Learning {
    fn take(&self, owner: &str) -> bool {
        self.running.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(owner.to_string())
    }

    fn release(&self, owner: &str) {
        self.running.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(owner);
    }

    pub fn is_running(&self, owner: &str) -> bool {
        self.running.lock().unwrap_or_else(std::sync::PoisonError::into_inner).contains(owner)
    }
}

/// The arguments of a step's process.
pub fn step_args(task: &str, owner: &str, today: &str, extra: &[String]) -> Vec<String> {
    let mut args: Vec<String> = ["--mode", "learn", "--task", task, "--owner", owner, "--today", today].iter().map(|arg| arg.to_string()).collect();
    args.extend(extra.iter().cloned());
    args
}

/// The result a step's process printed: its last result line.
pub fn parse_result(stdout: &str) -> Result<Value, String> {
    let line = stdout
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find(|line| matches!(line.get("type").and_then(Value::as_str), Some("learn_result" | "learn_error")));
    let Some(line) = line else {
        return Err("the learning step ended without a result".into());
    };
    if line["type"] == "learn_error" {
        return Err(line.get("message").and_then(Value::as_str).unwrap_or("the learning step failed").to_string());
    }
    match line.get("error").and_then(Value::as_str) {
        Some(error) if !error.is_empty() => Err(error.to_string()),
        _ => Ok(line),
    }
}

/// Run one step for `owner` and wait for its result. A second step for an
/// owner whose step runs is refused (it is tried again at the next look).
pub async fn run_step(app: &AppHandle, task: &str, owner: &str, extra: Vec<String>) -> Result<Value, String> {
    run_step_in(app, task, owner, extra, None).await
}

/// [`run_step`] in folder `cwd` (a chat's folder: its project's skills).
pub async fn run_step_in(app: &AppHandle, task: &str, owner: &str, extra: Vec<String>, cwd: Option<std::path::PathBuf>) -> Result<Value, String> {
    let learning = app.state::<Learning>();
    if !learning.take(owner) {
        return Err(format!("a learning step for {owner} is already running"));
    }
    let (today, _) = super::clock::local_now();
    let mut command = Command::new(crate::engine::engine_binary());
    command.args(step_args(task, owner, &today, &extra)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    if let Some(folder) = cwd.filter(|folder| folder.is_dir()) {
        command.current_dir(folder);
    }
    #[cfg(windows)]
    {
        // Do not flash a console window for the child process.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let outcome = match command.spawn() {
        Ok(child) => match tokio::time::timeout(PROCESS_LIMIT, child.wait_with_output()).await {
            Ok(Ok(output)) => parse_result(&String::from_utf8_lossy(&output.stdout)),
            Ok(Err(error)) => Err(format!("the learning step failed: {error}")),
            // Dropping the future dropped the child: `kill_on_drop` stopped it.
            Err(_) => Err("the learning step took too long and was stopped".into()),
        },
        Err(error) => Err(format!("could not start the learning step: {error}")),
    };
    learning.release(owner);
    let db = app.state::<Db>();
    match &outcome {
        Ok(result) => {
            let changed = result.get("changed").and_then(Value::as_array).map_or(0, Vec::len);
            db.log(NewLog::new("info", "learning", format!("{task} ran")).data(json!({ "owner": owner, "changed": changed, "skipped": result.get("skipped") })));
            if changed > 0 || result.get("suggestions").and_then(Value::as_array).is_some_and(|list| !list.is_empty()) {
                let _ = app.emit("learning://changed", json!({ "owner": owner, "task": task }));
                // Engines read the changed memory and skills from their next message.
                crate::supervisor::settings::resources_changed(app);
            }
        }
        // One line with the reason; the step is tried again at the next due time (NFR-5).
        Err(error) => db.log(NewLog::new("warn", "learning", format!("{task} failed")).data(json!({ "owner": owner, "error": error }))),
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    // NFR-5, plan 3.3: what the worker reads from a step.
    #[test]
    fn learning_results_are_read_from_the_last_result_line() {
        let done = "noise\n{\"type\":\"learn_result\",\"task\":\"tidy\",\"owner\":\"personal\",\"changed\":[{\"name\":\"a\"}],\"error\":null}\n";
        assert_eq!(parse_result(done).unwrap()["changed"][0]["name"], "a");
        let failed = r#"{"type":"learn_result","task":"dream","owner":"mate:m1","error":"429: slow down"}"#;
        assert_eq!(parse_result(failed).unwrap_err(), "429: slow down");
        assert!(parse_result("").is_err(), "a crash printed nothing");
        assert_eq!(parse_result(r#"{"type":"learn_error","message":"unknown task"}"#).unwrap_err(), "unknown task");
        assert_eq!(step_args("tidy", "mate:m1", "2026-11-28", &["--chat".into(), "c1".into()]), ["--mode", "learn", "--task", "tidy", "--owner", "mate:m1", "--today", "2026-11-28", "--chat", "c1"]);
        let learning = Learning::default();
        assert!(learning.take("mate:m1"));
        assert!(!learning.take("mate:m1"), "one step per owner at a time");
        assert!(learning.take("mate:m2"));
        learning.release("mate:m1");
        assert!(!learning.is_running("mate:m1"));
    }
}
