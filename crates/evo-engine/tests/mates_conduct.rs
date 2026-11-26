//! The Evo Mates conduct set (docs/features/evo-mates/spec.md, FR-47, FR-48;
//! tasks T47): how a mate's sessions behave under the conduct block the
//! engine adds for a role that names a mate (`session_roles::MATE_CONDUCT`).
//!
//! The set is `tests/fixtures/mate-conduct/cases.json`. Its shape is checked
//! on every run. Running it needs a real model, so that test is ignored:
//!
//! ```text
//! EVO_CONDUCT_AGENT_DIR=<an agent dir with credentials> \
//! EVO_CONDUCT_MODEL=<provider>/<model> \
//! EVO_CONDUCT_REPORT=<file.md> \
//! cargo test -p evo-engine --test mates_conduct -- --ignored --nocapture
//! ```
//!
//! Each case runs in a fresh copy of the set's workspace, in a new engine
//! process (`--mode rpc --no-session`) whose role is a mate. An `ask` case
//! passes when the mate's first card is a question card (`ask_user`) and it
//! asked before trying to change anything (approval mode `ask`, so a change
//! shows as an approval card first); FR-47 wants 8 of 10. A `task` case runs
//! under `auto`, and its final reply is written to the report for review
//! against FR-48's shape (result first, then what was done, then the files).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const CASE_TIMEOUT: Duration = Duration::from_secs(240);

fn set() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mate-conduct/cases.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("the conduct set"))
        .expect("valid JSON")
}

#[test]
fn the_conduct_set_has_ten_of_each_kind() {
    let set = set();
    assert_eq!(set["version"], 1);
    let cases = set["cases"].as_array().unwrap();
    let count = |kind: &str| cases.iter().filter(|c| c["kind"] == kind).count();
    assert_eq!(count("ask"), 10);
    assert_eq!(count("task"), 10);
    let mut ids: Vec<&str> = cases.iter().map(|c| c["id"].as_str().unwrap()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 20, "ids are unique");
    assert!(cases
        .iter()
        .all(|c| !c["prompt"].as_str().unwrap().trim().is_empty()));
    assert!(set["workspace"].as_object().unwrap().len() >= 3);
}

/// What a case did.
#[derive(Debug)]
enum Outcome {
    /// Its first card was a question card.
    AskedCard(String),
    /// It tried to change something first (an approval card for `tool`).
    Acted(String),
    /// The turn ended with this reply and no card.
    Replied(String),
    Failed(String),
}

fn workspace(set: &Value, id: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("evo-mate-conduct-{}-{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (name, text) in set["workspace"].as_object().unwrap() {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text.as_str().unwrap()).unwrap();
    }
    dir
}

/// `<workspace>.<what>`, next to the workspace.
fn beside(dir: &Path, what: &str) -> PathBuf {
    let mut name = dir.file_name().unwrap().to_os_string();
    name.push(format!(".{what}"));
    dir.with_file_name(name)
}

fn role_file(dir: &Path) -> PathBuf {
    let role = json!({
        "name": "Mira",
        "source": "mate",
        "instructions": "You are Mira, one of the person's mates in Evo Mates: an AI teammate with a role of your own. Your role: a general assistant who handles documents, data and small code changes.",
        "access": "full",
        "mate": { "id": "conduct-mate", "name": "Mira", "role": "a general assistant" },
        "memoryDir": beside(dir, "memory").display().to_string(),
    });
    // Beside the workspace, not in it: the mate sees only the set's files.
    let path = beside(dir, "role.json");
    std::fs::write(&path, role.to_string()).unwrap();
    path
}

/// The text of the last assistant message of an `agent_end` line.
fn final_text(line: &Value) -> String {
    let messages = line["messages"].as_array().cloned().unwrap_or_default();
    messages
        .iter()
        .rev()
        .find(|m| m["role"] == "assistant")
        .and_then(|m| m["content"].as_array())
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn run_case(set: &Value, case: &Value, agent_dir: &str, model: &str) -> (Outcome, Vec<String>) {
    let id = case["id"].as_str().unwrap();
    let dir = workspace(set, id);
    let role = role_file(&dir);
    let (provider, model_id) = model
        .split_once('/')
        .expect("EVO_CONDUCT_MODEL is provider/model");
    let mode = if case["kind"] == "ask" { "ask" } else { "auto" };
    let mut child = Command::new(env!("CARGO_BIN_EXE_evo-engine"))
        .args([
            "--mode",
            "rpc",
            "--no-session",
            "--approval-mode",
            mode,
            "--provider",
            provider,
            "--model",
            model_id,
        ])
        .arg("--role-file")
        .arg(&role)
        .current_dir(&dir)
        .env("EVO_CODING_AGENT_DIR", agent_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start evo-engine");
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if tx.send(value).is_err() {
                    break;
                }
            }
        }
    });
    let mut send = |value: Value| {
        let _ = writeln!(stdin, "{value}");
        let _ = stdin.flush();
    };
    let started = Instant::now();
    let mut prompted = false;
    let mut tools = Vec::new();
    let outcome = loop {
        let Some(left) = CASE_TIMEOUT.checked_sub(started.elapsed()) else {
            break Outcome::Failed("timed out".into());
        };
        let Ok(line) = rx.recv_timeout(left) else {
            break Outcome::Failed("no answer from the engine".into());
        };
        match line["type"].as_str().unwrap_or_default() {
            "ready" if !prompted => {
                prompted = true;
                send(json!({ "type": "prompt", "id": "p1", "message": case["prompt"] }));
            }
            "tool_execution_start" => {
                if let Some(name) = line["toolName"].as_str() {
                    tools.push(name.to_string());
                }
            }
            "ui_dialog_request" => {
                let ui = &line["ui"];
                if ui["kind"] == "questions" {
                    let asked: Vec<String> = ui["questions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|q| q["question"].as_str().map(str::to_string))
                        .collect();
                    break Outcome::AskedCard(asked.join(" | "));
                }
                let tool = ui["approval"]["tool"]
                    .as_str()
                    .unwrap_or("a change")
                    .to_string();
                break Outcome::Acted(tool);
            }
            "agent_end" => break Outcome::Replied(final_text(&line)),
            "response" if line["success"] == false => {
                break Outcome::Failed(line["error"].as_str().unwrap_or("refused").to_string());
            }
            _ => {}
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(beside(&dir, "memory"));
    let _ = std::fs::remove_file(&role);
    (outcome, tools)
}

#[test]
#[ignore = "runs a real model: set EVO_CONDUCT_AGENT_DIR and EVO_CONDUCT_MODEL"]
fn mates_conduct_live() {
    let agent_dir = std::env::var("EVO_CONDUCT_AGENT_DIR").expect("EVO_CONDUCT_AGENT_DIR");
    let model = std::env::var("EVO_CONDUCT_MODEL").expect("EVO_CONDUCT_MODEL");
    let only = std::env::var("EVO_CONDUCT_ONLY").ok();
    let set = set();
    let mut report = format!("# Evo Mates conduct set\n\nModel: `{model}`\n\n| Case | Prompt | Outcome | Tools before |\n| --- | --- | --- | --- |\n");
    let mut replies = String::from("\n## Task replies (FR-48 review)\n");
    let mut asked = 0;
    let mut ask_cases = 0;
    for case in set["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        if only
            .as_deref()
            .is_some_and(|only| !only.split(',').any(|o| o == id))
        {
            continue;
        }
        let (outcome, tools) = run_case(&set, case, &agent_dir, &model);
        let prompt = case["prompt"].as_str().unwrap();
        let shown = match &outcome {
            Outcome::AskedCard(questions) => format!("asked a card: {questions}"),
            Outcome::Acted(tool) => format!("acted first: {tool}"),
            Outcome::Replied(text) => format!(
                "replied: {}",
                text.lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(120)
                    .collect::<String>()
            ),
            Outcome::Failed(why) => format!("failed: {why}"),
        };
        eprintln!("{id}: {shown}");
        report.push_str(&format!(
            "| {id} | {prompt} | {} | {} |\n",
            shown.replace('|', "/"),
            tools.join(", ")
        ));
        if case["kind"] == "ask" {
            ask_cases += 1;
            if matches!(outcome, Outcome::AskedCard(_)) {
                asked += 1;
            }
        } else if let Outcome::Replied(text) = &outcome {
            replies.push_str(&format!("\n### {id}: {prompt}\n\n{text}\n"));
        }
    }
    report.push_str(&format!(
        "\nAsked first with a card: {asked} of {ask_cases} ask cases.\n"
    ));
    report.push_str(&replies);
    if let Ok(path) = std::env::var("EVO_CONDUCT_REPORT") {
        std::fs::write(path, &report).expect("write the report");
    }
    eprintln!("{report}");
    if ask_cases == 10 {
        assert!(
            asked >= 8,
            "FR-47 wants a card first in 8 of 10 ask cases, got {asked}"
        );
    }
}
