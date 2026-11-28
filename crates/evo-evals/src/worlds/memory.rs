//! The memory world: an agent folder in a temporary directory, the person's
//! memory owner (`personal`), and the engine's real memory and learning
//! code — the agent's memory tool, the panel's save, undo, the tidy-up, the
//! dream and the skill review — on a scripted model. After each step the
//! evidence is what memory and the learned skills hold and the journal.

use std::path::Path;
use std::sync::Arc;

use evo_ai::providers::faux::{FauxProvider, FauxScript, FauxStep};
use evo_engine::learning::{dream, entries, review, run, skills, tidy};
use evo_engine::memory::{self, Scope};
use serde_json::{json, Value};

/// A scenario's agent folder: set as the engine's for the scenario, and
/// removed with it.
pub struct World {
    _env: std::sync::MutexGuard<'static, ()>,
    pub dir: tempfile::TempDir,
    pub cwd: std::path::PathBuf,
    previous: Option<std::ffi::OsString>,
}

impl World {
    pub fn new() -> Result<Self, String> {
        // One world at a time: the agent folder is the process's.
        static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let env = ENV
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let previous = std::env::var_os("EVO_CODING_AGENT_DIR");
        std::env::set_var("EVO_CODING_AGENT_DIR", dir.path());
        std::fs::write(dir.path().join("settings.json"), "{}").map_err(|e| e.to_string())?;
        let cwd = dir.path().join("project");
        std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
        Ok(Self {
            _env: env,
            dir,
            cwd,
            previous,
        })
    }

    fn agent(&self) -> &Path {
        self.dir.path()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var("EVO_CODING_AGENT_DIR", value),
            None => std::env::remove_var("EVO_CODING_AGENT_DIR"),
        }
    }
}

fn text<'a>(step: &'a Value, key: &str) -> &'a str {
    step.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A step on a scripted model answering `answers` in turn.
fn learn_step<'a>(
    world: &World,
    task: run::Task,
    args: &'a evo_engine::args::Args,
    answers: &[Value],
    today: &str,
) -> run::Step<'a> {
    let script = FauxScript::new();
    script.set_responses(
        answers
            .iter()
            .map(|answer| {
                FauxStep::text(
                    answer
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| answer.to_string()),
                )
            })
            .collect(),
    );
    let provider = FauxProvider::new(script);
    let model = provider.default_model().clone();
    let provider: Arc<dyn evo_ai::provider::Provider> = provider;
    run::Step {
        task,
        owner: run::Owner::parse("personal", world.agent()).expect("personal"),
        args,
        today: today.into(),
        model: (!answers.is_empty()).then_some((provider, model)),
    }
}

/// What the world holds now: the visible text of each topic, the files,
/// the learned skills and the journal.
pub fn snapshot(world: &World, today: &str) -> Value {
    let dir = world.agent().join("memory");
    let mut topics = serde_json::Map::new();
    let mut files = serde_json::Map::new();
    for m in memory::list_dir(&dir) {
        topics.insert(m.name.clone(), json!(entries::visible(&m.content, today)));
        files.insert(m.name.clone(), json!(m.content));
    }
    let mut learned = serde_json::Map::new();
    for skill in skills::list(&world.agent().join("learned-skills")) {
        learned.insert(skill.name.clone(), json!(skill.text));
    }
    let journal: Vec<Value> = evo_store::open()
        .ok()
        .and_then(|conn| evo_store::journal::list(&conn, &["personal"], "memory", 100).ok().zip(evo_store::journal::list(&conn, &["personal"], "skill", 100).ok()))
        .map(|(memory_rows, skill_rows)| {
            memory_rows
                .into_iter()
                .chain(skill_rows)
                .map(|row| json!({ "id": row.id, "by": row.by, "action": row.action, "name": row.name, "subject": row.subject, "undone": row.undone_at.is_some() }))
                .collect()
        })
        .unwrap_or_default();
    json!({ "topics": topics, "files": files, "skills": learned, "journal": journal })
}

/// Run one step; its evidence.
pub async fn step(world: &World, step: &Value, today: &str) -> Result<Value, String> {
    let args = evo_engine::args::Args::default();
    let answers = step
        .get("answers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let result: Value = match text(step, "do") {
        "seed" => {
            let dir = world.agent().join("memory");
            let m = memory::save_in(
                &dir,
                text(step, "topic"),
                text(step, "description"),
                step.get("type").and_then(Value::as_str).unwrap_or("user"),
                text(step, "body"),
                None,
                memory::Meta::default(),
            )?;
            json!({ "ok": m.name })
        }
        "agent" => {
            let tool = evo_engine::tools::memory::create_memory_tool(world.cwd.clone());
            let mut params = step.get("params").cloned().unwrap_or(Value::Null);
            params["scope"] = json!("personal");
            match tool
                .execute("eval", params, Default::default(), Arc::new(|_| {}))
                .await
            {
                Ok(done) => json!({ "ok": format!("{:?}", done.content) }),
                Err(error) => json!({ "error": error.to_string() }),
            }
        }
        "person" => {
            let opened = match step.get("opened").and_then(Value::as_str) {
                Some("stale") => Some("0".to_string()),
                Some(hash) => Some(hash.to_string()),
                None => None,
            };
            match memory::save(
                &world.cwd,
                Scope::Personal,
                text(step, "topic"),
                text(step, "description"),
                step.get("type").and_then(Value::as_str).unwrap_or("user"),
                text(step, "body"),
                None,
                opened.as_deref(),
            )? {
                memory::Saved::Done(_) => json!({ "saved": true }),
                memory::Saved::Conflict(current) => json!({ "saved": false, "conflict": current }),
            }
        }
        "undoLast" => {
            let rows = snapshot(world, today)["journal"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let by = step.get("by").and_then(Value::as_str);
            let row = rows
                .iter()
                .find(|row| {
                    row["subject"] == "memory"
                        && row["undone"] == false
                        && row["action"] != "undone"
                        && by.is_none_or(|by| row["by"] == by)
                })
                .ok_or("nothing to undo")?;
            match memory::undo_change(&world.cwd, row["id"].as_i64().unwrap_or(0)) {
                Ok(()) => json!({ "undone": row["id"] }),
                Err(error) => json!({ "error": error }),
            }
        }
        "tidy" => tidy::run(&learn_step(world, run::Task::Tidy, &args, &answers, today)).await,
        "dream" => {
            let raw: Vec<dream::RawChat> = step
                .get("chats")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .iter()
                .enumerate()
                .map(|(index, chat)| dream::RawChat {
                    id: chat
                        .get("id")
                        .and_then(Value::as_str)
                        .map_or_else(|| format!("c{index}"), str::to_string),
                    title: text(chat, "title").into(),
                    lines: chat
                        .get("lines")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .map(|line| dream::RawLine {
                            person: line.get("person").and_then(Value::as_bool).unwrap_or(true),
                            text: text(line, "text").into(),
                            at_ms: 1_000_000 - index as i64,
                        })
                        .collect(),
                    by_day: false,
                    channel: chat
                        .get("channel")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
                .collect();
            let chats = dream::select(raw, 0);
            if chats.len() < dream::MIN_CHATS {
                json!({ "skipped": "too few chats" })
            } else {
                dream::dream_with(
                    &learn_step(world, run::Task::Dream, &args, &answers, today),
                    &chats,
                )
                .await
            }
        }
        "review" => {
            review::review_with(
                &learn_step(world, run::Task::Review, &args, &answers, today),
                "eval-chat",
                Some("e1"),
                text(step, "conversation"),
                run::Task::Review,
            )
            .await
        }
        other => return Err(format!("the memory world has no step `{other}`")),
    };
    let mut evidence = snapshot(world, today);
    evidence["result"] = result;
    Ok(evidence)
}
