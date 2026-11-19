//! Turn checkpoints over RPC (docs/features/source-control/spec.md, FR-9 –
//! FR-13, plan 3.3, 3.5, 3.6): a top-level run in a repository records a
//! checkpoint on `agent_settled` and as a session entry, `list_checkpoints`
//! returns it after a restart, `revert_turn` puts files back and tells the
//! agent, and `draft_commit_message` leaves the Side chat thread alone (FR-7).
//!
//! The repositories are temporary; the tests skip when git is missing.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use evo_ai::providers::faux::{faux_assistant_message, FauxProvider, FauxScript, FauxStep};
use evo_ai::types::{Context, Message, StopReason, UserContent};
use evo_harness::session::memory::{InMemorySessionStorage, SystemClock};
use evo_harness::session::session::DefaultIdGenerator;
use evo_harness::session::types::{Entry, EntryQuery, SessionMetadata};
use evo_harness::session::Session;
use evo_harness::types::{AgentHarnessOptions, HarnessTool};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;

use super::tests::RpcOutput;
use super::*;
use crate::checkpoints::{Checkpoints, Gate};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn git(dir: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// A repository with `a.txt` and `b.txt` committed; `None` without git.
fn repo() -> Option<tempfile::TempDir> {
    let dir = tempfile::tempdir().unwrap();
    if git(dir.path(), &["init", "-q"]).is_err() {
        eprintln!("skipping: git is not available");
        return None;
    }
    for args in [
        &["config", "user.email", "evo@example.invalid"][..],
        &["config", "user.name", "evo"],
        &["config", "core.autocrlf", "false"],
    ] {
        git(dir.path(), args).unwrap();
    }
    std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "two\n").unwrap();
    git(dir.path(), &["add", "."]).unwrap();
    git(dir.path(), &["commit", "-q", "-m", "init"]).unwrap();
    Some(dir)
}

fn read(dir: &std::path::Path, path: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(path)).ok()
}

/// A chat in `cwd` with the built-in tools (and `agent` when asked), its
/// tool chain ending in the checkpoint link of `gate`.
async fn chat(
    cwd: &std::path::Path,
    script: FauxScript,
    gate: &Arc<Gate>,
    with_agent: bool,
) -> AgentHarness {
    let provider = FauxProvider::new(script);
    let cell: crate::tools::agent::HarnessCell = Arc::new(std::sync::OnceLock::new());
    let mut tools = crate::tools::builtin_tools_in(cwd);
    if with_agent {
        tools.push(HarnessTool::new(crate::tools::agent::create_agent_tool(
            cell.clone(),
            cwd.to_path_buf(),
        )));
    }
    let active_tool_names = tools
        .iter()
        .map(|tool| tool.tool.schema().name.clone())
        .collect();
    let session = Session::new(
        Arc::new(InMemorySessionStorage::new(
            SessionMetadata {
                id: format!("chk-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
                created_at: 0,
                parent_session_id: None,
            },
            Arc::new(SystemClock),
            Arc::new(DefaultIdGenerator::new()),
        )),
        None,
    );
    let harness = AgentHarness::create(AgentHarnessOptions {
        model: provider.default_model().clone(),
        models: vec![provider],
        tools,
        active_tool_names,
        session,
        before_tool_call: crate::checkpoints::before_tool_call(None, Arc::clone(gate)),
        ..Default::default()
    })
    .await
    .unwrap();
    let _ = cell.set(harness.downgrade());
    harness
}

/// Run `harness` as an RPC engine with `checkpoints`.
fn serve(
    harness: &AgentHarness,
    checkpoints: Arc<Checkpoints>,
) -> (
    tokio::io::DuplexStream,
    RpcOutput,
    tokio::task::JoinHandle<i32>,
) {
    let (client, input) = tokio::io::duplex(1 << 16);
    let output = RpcOutput::default();
    let captured = output.clone();
    let harness = harness.clone();
    let task = tokio::spawn(async move {
        rpc_with_io(
            &harness,
            None,
            RpcProviders {
                catalog: Arc::new(std::sync::RwLock::new(Vec::new())),
                reload: None,
                checkpoints: Some(checkpoints),
            },
            evo_extensions::UiDialogMailbox::new(),
            None,
            crate::args::RpcMessageUpdates::default(),
            tokio::io::BufReader::new(input),
            captured,
        )
        .await
    });
    (client, output, task)
}

async fn send(client: &mut tokio::io::DuplexStream, line: Value) {
    client
        .write_all(format!("{line}\n").as_bytes())
        .await
        .unwrap();
}

async fn wait_for(output: &RpcOutput, what: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let Some(line) = output.messages().into_iter().find(&what) {
                return line;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the line arrived")
}

/// The response to request `id`.
async fn reply(output: &RpcOutput, id: u64) -> Value {
    wait_for(output, |m| m["type"] == "response" && m["id"] == id).await
}

/// The `n`th `agent_settled` line (from 0).
async fn settled(output: &RpcOutput, n: usize) -> Value {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let all: Vec<Value> = output
                .messages()
                .into_iter()
                .filter(|m| m["type"] == "agent_settled")
                .collect();
            if let Some(line) = all.get(n) {
                return line.clone();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the run settled")
}

async fn stop(mut client: tokio::io::DuplexStream, task: tokio::task::JoinHandle<i32>) {
    send(&mut client, json!({"type": "stop", "id": 999})).await;
    assert_eq!(task.await.unwrap(), 0);
}

async fn checkpoint_entries(harness: &AgentHarness) -> Vec<Value> {
    harness
        .find_entries(&EntryQuery {
            entry_type: Some("custom"),
            custom_type: Some("checkpoint".into()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Custom(entry) => entry.data,
            _ => None,
        })
        .collect()
}

/// The ids and texts of the branch's user messages, oldest first.
async fn user_messages(harness: &AgentHarness) -> Vec<(String, String)> {
    let mut entries = harness
        .find_entries(&EntryQuery {
            entry_type: Some("message"),
            ..Default::default()
        })
        .await
        .unwrap();
    entries.sort_by_key(|entry| entry.base().seq);
    entries
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Message(entry) => match &entry.message {
                evo_agent::AgentMessage::User(user) => Some((
                    entry.base.id.clone(),
                    user.content.as_text().unwrap_or_default().to_string(),
                )),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn refs(dir: &std::path::Path, chat: &str) -> Vec<String> {
    git(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname)",
            &format!("refs/evo/checkpoints/{chat}/"),
        ],
    )
    .unwrap()
    .lines()
    .map(str::to_string)
    .collect()
}

fn chat_id(harness: &AgentHarness) -> String {
    harness.session().storage().metadata().id.clone()
}

fn texts(ctx: &Context) -> Vec<String> {
    ctx.messages
        .iter()
        .filter_map(|m| match m {
            Message::User(u) => Some(match &u.content {
                UserContent::Text(text) => text.clone(),
                UserContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| match b {
                        evo_ai::types::Content::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect(),
            }),
            _ => None,
        })
        .collect()
}

/// A step that records the user texts it was sent and answers.
fn recording(seen: Arc<Mutex<Vec<Vec<String>>>>) -> FauxStep {
    FauxStep::factory(move |ctx: &Context| {
        seen.lock().unwrap().push(texts(ctx));
        faux_assistant_message("ok", StopReason::Stop)
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn checkpoints_a_top_level_run_records_its_turn_and_it_survives_a_restart() {
    let Some(dir) = repo() else { return };
    let root = dir.path();
    let gate = Arc::new(Gate::default());
    let script = FauxScript::new()
        .with_tool_call("write", json!({"path": "a.txt", "content": "changed\n"}))
        .with_tool_call("write", json!({"path": "new.txt", "content": "new\n"}))
        .with_text("done");
    let harness = chat(root, script.clone(), &gate, false).await;
    let chat = chat_id(&harness);
    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::new(Arc::clone(&gate), root.to_path_buf()),
    );
    send(
        &mut client,
        json!({"type": "prompt", "id": 1, "message": "Change a.txt"}),
    )
    .await;
    let done = settled(&output, 0).await;
    assert_eq!(done["outcome"], "completed", "{done}");
    let checkpoint = done["checkpoint"].clone();
    assert_eq!(checkpoint["id"], "c-1", "{done}");
    assert_eq!(checkpoint["seq"], 1);
    assert_eq!(checkpoint["complete"], true);
    assert_eq!(checkpoint["runId"], done["runId"]);
    assert!(checkpoint["startedAt"].as_i64().unwrap() > 0);
    assert_eq!(
        checkpoint["files"],
        json!([
            {"path": "a.txt", "status": "modified", "added": 1, "removed": 1},
            {"path": "new.txt", "status": "added", "added": 1, "removed": 0}
        ])
    );
    let prompts = user_messages(&harness).await;
    assert_eq!(prompts[0].1, "Change a.txt");
    assert_eq!(checkpoint["promptEntryId"], prompts[0].0);
    // The start snapshot holds the old text, the end snapshot the new.
    let start = checkpoint["start"].as_str().unwrap();
    let end = checkpoint["end"].as_str().unwrap();
    assert_eq!(
        git(root, &["show", &format!("{start}:a.txt")]).unwrap(),
        "one"
    );
    assert_eq!(
        git(root, &["show", &format!("{end}:a.txt")]).unwrap(),
        "changed"
    );
    assert_eq!(
        refs(root, &chat),
        vec![
            format!("refs/evo/checkpoints/{chat}/1/end"),
            format!("refs/evo/checkpoints/{chat}/1/start"),
        ]
    );
    // Kept as a session entry, and listed.
    assert_eq!(checkpoint_entries(&harness).await, vec![checkpoint.clone()]);
    send(&mut client, json!({"type": "list_checkpoints", "id": 2})).await;
    let listed = reply(&output, 2).await;
    assert_eq!(listed["data"], json!({"checkpoints": [checkpoint.clone()]}));
    stop(client, task).await;

    // A new engine for the chat: the list is the same and turns go on at 2.
    script.append_responses(vec![
        FauxStep::tool_call("write", json!({"path": "a.txt", "content": "again\n"})),
        FauxStep::text("done again"),
    ]);
    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::new(Arc::clone(&gate), root.to_path_buf()),
    );
    send(&mut client, json!({"type": "list_checkpoints", "id": 3})).await;
    assert_eq!(
        reply(&output, 3).await["data"],
        json!({"checkpoints": [checkpoint]})
    );
    send(
        &mut client,
        json!({"type": "prompt", "id": 4, "message": "Once more"}),
    )
    .await;
    let again = settled(&output, 0).await;
    assert_eq!(again["checkpoint"]["id"], "c-2", "{again}");
    assert_eq!(
        again["checkpoint"]["files"],
        json!([{"path": "a.txt", "status": "modified", "added": 1, "removed": 1}])
    );
    stop(client, task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn checkpoints_outside_a_repository_record_none() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Arc::new(Gate::default());
    let script = FauxScript::new()
        .with_tool_call("write", json!({"path": "a.txt", "content": "x\n"}))
        .with_text("done");
    let harness = chat(dir.path(), script, &gate, false).await;
    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::new(Arc::clone(&gate), dir.path().to_path_buf()),
    );
    send(
        &mut client,
        json!({"type": "prompt", "id": 1, "message": "Go"}),
    )
    .await;
    let done = settled(&output, 0).await;
    assert_eq!(done["outcome"], "completed");
    assert!(done.get("checkpoint").is_none(), "{done}");
    assert_eq!(read(dir.path(), "a.txt").as_deref(), Some("x\n"));
    send(&mut client, json!({"type": "list_checkpoints", "id": 2})).await;
    assert_eq!(reply(&output, 2).await["data"], json!({"checkpoints": []}));
    assert!(checkpoint_entries(&harness).await.is_empty());
    stop(client, task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn checkpoints_a_subagent_takes_none_of_its_own() {
    let Some(dir) = repo() else { return };
    let root = dir.path();
    let gate = Arc::new(Gate::default());
    let script = FauxScript::new()
        .with_tool_call(
            "agent",
            json!({"description": "Look", "prompt": "Read a.txt and report.", "agentType": "explore"}),
        )
        .with_tool_call("read", json!({"path": "a.txt"}))
        .with_text("a.txt says one")
        .with_tool_call("write", json!({"path": "a.txt", "content": "changed\n"}))
        .with_text("done");
    let harness = chat(root, script, &gate, true).await;
    let chat = chat_id(&harness);
    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::new(Arc::clone(&gate), root.to_path_buf()),
    );
    send(
        &mut client,
        json!({"type": "prompt", "id": 1, "message": "Go"}),
    )
    .await;
    let done = settled(&output, 0).await;
    assert_eq!(done["outcome"], "completed", "{done}");
    assert_eq!(done["checkpoint"]["id"], "c-1");
    assert_eq!(
        refs(root, &chat),
        vec![
            format!("refs/evo/checkpoints/{chat}/1/end"),
            format!("refs/evo/checkpoints/{chat}/1/start"),
        ]
    );
    assert_eq!(checkpoint_entries(&harness).await.len(), 1);
    let all = git(root, &["for-each-ref", "--format=%(refname)", "refs/evo/"]).unwrap();
    assert_eq!(all.lines().count(), 2, "{all}");
    stop(client, task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn checkpoints_a_late_start_snapshot_lets_the_edit_go_and_marks_the_turn_incomplete() {
    let Some(dir) = repo() else { return };
    let root = dir.path();
    let gate = Arc::new(Gate::with_wait(Duration::from_millis(100)));
    let script = FauxScript::new()
        .with_tool_call("write", json!({"path": "a.txt", "content": "changed\n"}))
        .with_text("done");
    let harness = chat(root, script, &gate, false).await;
    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::with_start_delay(
            Arc::clone(&gate),
            root.to_path_buf(),
            Duration::from_secs(2),
        ),
    );
    send(
        &mut client,
        json!({"type": "prompt", "id": 1, "message": "Go"}),
    )
    .await;
    let done = settled(&output, 0).await;
    assert_eq!(done["outcome"], "completed", "{done}");
    assert_eq!(done["checkpoint"]["complete"], false, "{done}");
    // The edit ran before the snapshot: the turn cannot show it.
    assert_eq!(done["checkpoint"]["files"], json!([]));
    stop(client, task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn checkpoints_a_turn_left_open_is_closed_when_the_engine_starts() {
    let Some(dir) = repo() else { return };
    let root = dir.path();
    let gate = Arc::new(Gate::default());
    let script = FauxScript::new().with_text("hello");
    let harness = chat(root, script, &gate, false).await;
    let chat = chat_id(&harness);
    // An earlier engine took turn 3's start, then stopped in the middle of it.
    let start =
        evo_worktrees::checkpoint::snapshot(root, &chat, 3, evo_worktrees::checkpoint::Kind::Start)
            .unwrap();
    std::fs::write(root.join("a.txt"), "half done\n").unwrap();

    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::new(Arc::clone(&gate), root.to_path_buf()),
    );
    send(&mut client, json!({"type": "list_checkpoints", "id": 1})).await;
    let listed = reply(&output, 1).await;
    let checkpoints = listed["data"]["checkpoints"].as_array().unwrap().clone();
    assert_eq!(checkpoints.len(), 1, "{listed}");
    let closed = &checkpoints[0];
    assert_eq!(closed["id"], "c-3");
    assert_eq!(closed["complete"], false);
    assert_eq!(closed["start"], start);
    assert!(closed["end"].is_string());
    assert_eq!(
        closed["files"],
        json!([{"path": "a.txt", "status": "modified", "added": 1, "removed": 1}])
    );
    send(
        &mut client,
        json!({"type": "prompt", "id": 2, "message": "Hi"}),
    )
    .await;
    assert_eq!(settled(&output, 0).await["checkpoint"]["id"], "c-4");
    stop(client, task).await;
}

/// A chat in a repository that has finished turn 1, which wrote `a.txt`,
/// `b.txt` and `new.txt`.
async fn after_one_turn(
    root: &std::path::Path,
) -> (
    AgentHarness,
    tokio::io::DuplexStream,
    RpcOutput,
    tokio::task::JoinHandle<i32>,
) {
    let gate = Arc::new(Gate::default());
    let steps = FauxScript::new()
        .with_tool_call("write", json!({"path": "a.txt", "content": "changed a\n"}))
        .with_tool_call("write", json!({"path": "b.txt", "content": "changed b\n"}))
        .with_tool_call("write", json!({"path": "new.txt", "content": "new\n"}))
        .with_text("done");
    let harness = chat(root, steps.clone(), &gate, false).await;
    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::new(Arc::clone(&gate), root.to_path_buf()),
    );
    send(
        &mut client,
        json!({"type": "prompt", "id": 1, "message": "Change the files"}),
    )
    .await;
    let done = settled(&output, 0).await;
    assert_eq!(done["checkpoint"]["files"].as_array().unwrap().len(), 3);
    (harness, client, output, task)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revert_turn_is_busy_while_a_run_is_active() {
    let Some(dir) = repo() else { return };
    let gate = Arc::new(Gate::default());
    let script = FauxScript::new()
        .with_text("a long answer ".repeat(200))
        .with_tokens_per_second(20.0);
    let harness = chat(dir.path(), script, &gate, false).await;
    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::new(Arc::clone(&gate), dir.path().to_path_buf()),
    );
    send(
        &mut client,
        json!({"type": "prompt", "id": 1, "message": "Talk"}),
    )
    .await;
    reply(&output, 1).await;
    send(
        &mut client,
        json!({"type": "revert_turn", "id": 2, "checkpoint": "c-1", "force": false, "paths": null}),
    )
    .await;
    let answer = reply(&output, 2).await;
    assert_eq!(answer["success"], false);
    assert_eq!(answer["error"], "busy");
    send(&mut client, json!({"type": "abort", "id": 3})).await;
    settled(&output, 0).await;
    stop(client, task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revert_turn_skips_files_changed_since_and_force_reverts_them() {
    let Some(dir) = repo() else { return };
    let root = dir.path();
    let (harness, mut client, output, task) = after_one_turn(root).await;
    // The person edits b.txt after the turn.
    std::fs::write(root.join("b.txt"), "mine\n").unwrap();

    send(
        &mut client,
        json!({"type": "revert_turn", "id": 2, "checkpoint": "c-1", "force": false, "paths": null}),
    )
    .await;
    let answer = reply(&output, 2).await;
    assert_eq!(
        answer["data"],
        json!({
            "reverted": ["new.txt", "a.txt"],
            "skipped": [{"path": "b.txt", "reason": "changed-since"}],
            "undo": "c-1-undo"
        }),
        "{answer}"
    );
    assert_eq!(read(root, "a.txt").as_deref(), Some("one\n"));
    assert_eq!(read(root, "new.txt"), None);
    assert_eq!(read(root, "b.txt").as_deref(), Some("mine\n"));
    send(&mut client, json!({"type": "list_checkpoints", "id": 3})).await;
    let listed = reply(&output, 3).await;
    let reverted = &listed["data"]["checkpoints"][0]["reverted"];
    assert_eq!(reverted["undo"], "c-1-undo");
    assert_eq!(reverted["paths"], json!(["new.txt", "a.txt"]));
    assert!(reverted["at"].as_i64().unwrap() > 0);

    // Revert anyway: b.txt goes back too.
    send(
        &mut client,
        json!({"type": "revert_turn", "id": 4, "checkpoint": "c-1", "force": true, "paths": ["b.txt"]}),
    )
    .await;
    let forced = reply(&output, 4).await;
    assert_eq!(forced["data"]["reverted"], json!(["b.txt"]), "{forced}");
    assert_eq!(read(root, "b.txt").as_deref(), Some("two\n"));
    send(&mut client, json!({"type": "list_checkpoints", "id": 5})).await;
    assert_eq!(
        reply(&output, 5).await["data"]["checkpoints"][0]["reverted"]["paths"],
        json!(["new.txt", "a.txt", "b.txt"])
    );
    // The entry kept the turn's own files.
    assert_eq!(checkpoint_entries(&harness).await.len(), 3);
    stop(client, task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revert_turn_undo_restores_the_files_and_takes_back_the_unsent_note() {
    let Some(dir) = repo() else { return };
    let root = dir.path();
    let gate = Arc::new(Gate::default());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let script = FauxScript::new()
        .with_tool_call("write", json!({"path": "a.txt", "content": "changed a\n"}))
        .with_tool_call("write", json!({"path": "new.txt", "content": "new\n"}))
        .with_text("done");
    script.append_responses(vec![recording(seen.clone())]);
    let harness = chat(root, script, &gate, false).await;
    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::new(Arc::clone(&gate), root.to_path_buf()),
    );
    send(
        &mut client,
        json!({"type": "prompt", "id": 1, "message": "Change the files"}),
    )
    .await;
    settled(&output, 0).await;
    send(
        &mut client,
        json!({"type": "revert_turn", "id": 2, "checkpoint": "c-1"}),
    )
    .await;
    assert_eq!(reply(&output, 2).await["data"]["undo"], "c-1-undo");
    assert_eq!(read(root, "a.txt").as_deref(), Some("one\n"));
    assert_eq!(read(root, "new.txt"), None);

    send(
        &mut client,
        json!({"type": "revert_turn", "id": 3, "checkpoint": "c-1-undo"}),
    )
    .await;
    let undone = reply(&output, 3).await;
    assert_eq!(
        undone["data"],
        json!({"reverted": ["new.txt", "a.txt"], "skipped": [], "undo": null}),
        "{undone}"
    );
    assert_eq!(read(root, "a.txt").as_deref(), Some("changed a\n"));
    assert_eq!(read(root, "new.txt").as_deref(), Some("new\n"));
    send(&mut client, json!({"type": "list_checkpoints", "id": 4})).await;
    let listed = reply(&output, 4).await;
    assert!(
        listed["data"]["checkpoints"][0].get("reverted").is_none(),
        "{listed}"
    );

    // The agent never saw the revert, so its next run has no note at all.
    send(
        &mut client,
        json!({"type": "prompt", "id": 5, "message": "Next"}),
    )
    .await;
    settled(&output, 1).await;
    let seen = seen.lock().unwrap().clone();
    assert!(
        !seen[0].iter().any(|text| text.contains("<turn-reverted")),
        "{seen:?}"
    );
    stop(client, task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revert_turn_queues_a_note_for_the_next_run() {
    let Some(dir) = repo() else { return };
    let root = dir.path();
    let gate = Arc::new(Gate::default());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let script = FauxScript::new()
        .with_tool_call("write", json!({"path": "a.txt", "content": "changed a\n"}))
        .with_text("done");
    script.append_responses(vec![recording(seen.clone())]);
    let harness = chat(root, script, &gate, false).await;
    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::new(Arc::clone(&gate), root.to_path_buf()),
    );
    send(
        &mut client,
        json!({"type": "prompt", "id": 1, "message": "Change a.txt"}),
    )
    .await;
    settled(&output, 0).await;
    send(
        &mut client,
        json!({"type": "revert_turn", "id": 2, "checkpoint": "c-1"}),
    )
    .await;
    assert_eq!(
        reply(&output, 2).await["data"]["reverted"],
        json!(["a.txt"])
    );

    send(
        &mut client,
        json!({"type": "prompt", "id": 3, "message": "Go on"}),
    )
    .await;
    let next = settled(&output, 1).await;
    let seen = seen.lock().unwrap().clone();
    let texts = &seen[0];
    let expected = "<turn-reverted checkpoint=\"c-1\">The person reverted your changes from an earlier turn (\"Change a.txt\"): a.txt is back as it was before it.</turn-reverted>";
    let at = texts
        .iter()
        .position(|text| text == expected)
        .unwrap_or_else(|| panic!("no revert note in {texts:?}"));
    assert_eq!(texts.last().map(String::as_str), Some("Go on"));
    assert!(at < texts.len() - 1, "the note comes before the prompt");
    // The next turn's checkpoint points at its prompt, not at the note.
    let prompts = user_messages(&harness).await;
    let go_on = prompts.iter().find(|(_, text)| text == "Go on").unwrap();
    assert_eq!(next["checkpoint"]["promptEntryId"], go_on.0, "{next}");
    stop(client, task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn commit_message_draft_over_rpc_leaves_the_side_chat_thread_alone() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Arc::new(Gate::default());
    let script = FauxScript::new()
        .with_text("side answer")
        .with_text("feat(scm): stage hunks");
    let harness = chat(dir.path(), script, &gate, false).await;
    let tip = harness.get_tip_id().await.unwrap();
    let (mut client, output, task) = serve(
        &harness,
        Checkpoints::new(Arc::clone(&gate), dir.path().to_path_buf()),
    );
    send(
        &mut client,
        json!({"type": "side_question", "id": 1, "op": "start", "question": "What is this?"}),
    )
    .await;
    reply(&output, 1).await;
    let mut id = 100;
    let status = loop {
        id += 1;
        send(
            &mut client,
            json!({"type": "side_question", "id": id, "op": "status"}),
        )
        .await;
        let status = reply(&output, id).await;
        if status["data"]["status"] == "done" {
            break status;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(status["data"]["text"], "side answer");

    send(
        &mut client,
        json!({"type": "draft_commit_message", "id": 2, "diff": "diff --git a/x b/x\n+x\n", "subjects": ["feat(chat): x"]}),
    )
    .await;
    let drafted = reply(&output, 2).await;
    assert_eq!(drafted["success"], true, "{drafted}");
    assert_eq!(
        drafted["data"],
        json!({"message": "feat(scm): stage hunks"})
    );

    send(
        &mut client,
        json!({"type": "side_question", "id": 3, "op": "status"}),
    )
    .await;
    let after = reply(&output, 3).await;
    assert_eq!(
        after["data"]["turns"].as_array().unwrap().len(),
        1,
        "{after}"
    );
    assert_eq!(after["data"]["text"], "side answer");
    assert_eq!(harness.get_tip_id().await.unwrap(), tip);
    stop(client, task).await;
}
