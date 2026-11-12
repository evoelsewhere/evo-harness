//! `agent_settled.turn`: what a run used, for the host's turn record
//! (docs/features/shell-hosted-engines/spec.md, FR-6; T19, T24), with the
//! subagent rules of docs/features/subagents-and-nested-sessions/spec.md,
//! FR-6: a foreground child counts with its call, a background child that
//! ends after its turn counts in the next one, nothing twice.
//!
//! One scripted model serves the parent and its subagents. It is priced, so
//! costs are not zero.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use evo_ai::providers::faux::{
    faux_assistant_message, FauxBlock, FauxProvider, FauxScript, FauxStep,
};
use evo_ai::types::{Context, Message, StopReason, UserContent};
use evo_harness::session::memory::{InMemorySessionStorage, SystemClock};
use evo_harness::session::session::DefaultIdGenerator;
use evo_harness::session::types::SessionMetadata;
use evo_harness::session::Session;
use evo_harness::types::{AgentHarnessOptions, HarnessTool};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;

use super::tests::RpcOutput;
use super::*;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A parent with the built-in tools and `agent` in `cwd`, on a priced faux
/// model, its events streamed like the app's engine.
async fn parent(
    cwd: &std::path::Path,
    script: FauxScript,
) -> (AgentHarness, tokio::sync::broadcast::Receiver<AgentEvent>) {
    let mut model = FauxProvider::new(FauxScript::new()).default_model().clone();
    model.cost.rates.input = 3.0;
    model.cost.rates.output = 15.0;
    let provider = FauxProvider::with_models(script, vec![model]);
    let cell: crate::tools::agent::HarnessCell = Arc::new(OnceLock::new());
    let mut tools = crate::tools::builtin_tools_in(cwd);
    tools.push(HarnessTool::new(crate::tools::agent::create_agent_tool(
        cell.clone(),
        cwd.to_path_buf(),
    )));
    let active_tool_names = tools
        .iter()
        .map(|tool| tool.tool.schema().name.clone())
        .collect();
    let session = Session::new(
        Arc::new(InMemorySessionStorage::new(
            SessionMetadata {
                id: format!("turn-totals-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
                created_at: 0,
                parent_session_id: None,
            },
            Arc::new(SystemClock),
            Arc::new(DefaultIdGenerator::new()),
        )),
        None,
    );
    let (events, receiver) = evo_agent::events::BroadcastEmitter::new(8192);
    let harness = AgentHarness::create(AgentHarnessOptions {
        model: provider.default_model().clone(),
        models: vec![provider],
        tools,
        active_tool_names,
        session,
        agent_emitter: Some(Arc::new(events)),
        ..Default::default()
    })
    .await
    .unwrap();
    let _ = cell.set(harness.downgrade());
    (harness, receiver)
}

/// Run `harness` as an RPC engine; returns its stdin and its output.
fn serve(
    harness: AgentHarness,
    events: tokio::sync::broadcast::Receiver<AgentEvent>,
) -> (
    tokio::io::DuplexStream,
    RpcOutput,
    tokio::task::JoinHandle<i32>,
) {
    let (client, input) = tokio::io::duplex(1 << 16);
    let output = RpcOutput::default();
    let captured = output.clone();
    let task = tokio::spawn(async move {
        rpc_with_io(
            &harness,
            Some(events),
            vec![],
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

/// The `n`th `agent_settled` line (from 0), waited for.
async fn settled(output: &RpcOutput, n: usize) -> Value {
    tokio::time::timeout(Duration::from_secs(20), async {
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

/// Sums of the parent's own responses (`message_end`) and of tool results'
/// usage (subagents), as the window's transcript adds them.
fn sums(lines: &[Value]) -> ((i64, i64, f64), (i64, f64)) {
    let (mut input, mut output, mut cost) = (0, 0, 0.0);
    let (mut child_tokens, mut child_cost) = (0, 0.0);
    for line in lines {
        if line["type"] == "message_end" && line["message"]["role"] == "assistant" {
            let usage = &line["message"]["usage"];
            input += usage["input"].as_i64().unwrap();
            output += usage["output"].as_i64().unwrap();
            cost += usage["cost"]["total"].as_f64().unwrap();
        }
        if line["type"] == "tool_execution_end" && line["result"]["usage"].is_object() {
            let usage = &line["result"]["usage"];
            child_tokens += ["input", "output", "cacheRead", "cacheWrite"]
                .iter()
                .map(|key| usage[*key].as_i64().unwrap_or(0))
                .sum::<i64>();
            child_cost += usage["cost"]["total"].as_f64().unwrap();
        }
    }
    ((input, output, cost), (child_tokens, child_cost))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn turn_totals_count_a_run_with_tools_and_a_foreground_subagent() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.md"), "the answer is 42\n").unwrap();
    // The parent asks a subagent (which reads a file), reads the file
    // itself, then answers.
    let script = FauxScript::new()
        .with_tool_call(
            "agent",
            json!({"description":"Find the answer","prompt":"Read notes.md and report the answer.","agentType":"explore"}),
        )
        .with_tool_call("read", json!({"path":"notes.md"}))
        .with_text("The answer in notes.md is 42.")
        .with_tool_call("read", json!({"path":"notes.md"}))
        .with_text("All done: the answer is 42.");
    let (harness, events) = parent(dir.path(), script).await;
    let (mut client, output, task) = serve(harness, events);
    let ready = output.wait_for(|m| m["type"] == "ready").await;
    assert_eq!(ready["turnTotals"], true);
    send(
        &mut client,
        json!({"type":"prompt","id":1,"message":"What is the answer?"}),
    )
    .await;
    let done = settled(&output, 0).await;
    assert_eq!(done["outcome"], "completed");
    let turn = &done["turn"];
    let lines = output.messages();
    let ((input, out, cost), (child_tokens, child_cost)) = sums(&lines);
    // The parent's three responses.
    assert_eq!(turn["input"].as_i64(), Some(input));
    assert_eq!(turn["output"].as_i64(), Some(out));
    assert_eq!(
        (turn["cacheRead"].as_i64(), turn["cacheWrite"].as_i64()),
        (Some(0), Some(0))
    );
    assert!(cost > 0.0);
    assert!(
        close(turn["costUsd"].as_f64().unwrap(), cost),
        "{turn} vs {cost}"
    );
    // The subagent's usage, once: what its call's result carried.
    assert!(child_cost > 0.0);
    assert!(
        close(turn["childCostUsd"].as_f64().unwrap(), child_cost),
        "{turn} vs {child_cost}"
    );
    assert_eq!(turn["childTokens"].as_i64(), Some(child_tokens));
    assert_eq!(turn["provider"], "faux");
    assert!(turn["model"]
        .as_str()
        .is_some_and(|model| !model.is_empty()));
    assert_eq!(turn["preview"], "All done: the answer is 42.");
    // The fixture the webview's parity test reads (T24): set
    // EVO_TURN_FIXTURE to a path to write it again.
    if let Some(path) = std::env::var_os("EVO_TURN_FIXTURE") {
        let text: String = lines
            .iter()
            .filter(|line| line["type"] != "message_update")
            .map(|line| format!("{line}\n"))
            .collect();
        std::fs::write(path, text).unwrap();
    }

    // A compaction is not a turn.
    send(&mut client, json!({"type":"compact","id":2})).await;
    let compacted = settled(&output, 1).await;
    assert_eq!(compacted["compact"], true);
    assert!(compacted.get("turn").is_none(), "{compacted}");
    send(&mut client, json!({"type":"stop","id":3})).await;
    assert_eq!(task.await.unwrap(), 0);
}

fn user_texts(ctx: &Context) -> Vec<String> {
    ctx.messages
        .iter()
        .filter_map(|m| match m {
            Message::User(u) => Some(match &u.content {
                UserContent::Text(text) => text.clone(),
                UserContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| match b {
                        Content::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect(),
            }),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn turn_totals_count_a_background_child_that_ends_after_its_turn_in_the_next() {
    let dir = tempfile::tempdir().unwrap();
    // The child answers only once `gate` is set: after the parent's first turn.
    let gate = Arc::new(AtomicBool::new(false));
    let child_gate = gate.clone();
    let step = FauxStep::factory(move |ctx: &Context| {
        let users = user_texts(ctx);
        let first = users.first().cloned().unwrap_or_default();
        if first.contains("You are a subagent") {
            tokio::task::block_in_place(|| {
                while !child_gate.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
            return faux_assistant_message("background report: three call sites", StopReason::Stop);
        }
        if matches!(ctx.messages.last(), Some(Message::ToolResult(_))) {
            return faux_assistant_message("Started it.", StopReason::Stop);
        }
        if users.len() == 1 {
            return faux_assistant_message(
                vec![FauxBlock::tool_call(
                    "agent",
                    json!({"description":"Count call sites","prompt":"Count the call sites.","agentType":"explore","background":true}),
                )],
                StopReason::ToolUse,
            );
        }
        faux_assistant_message("Got the report.", StopReason::Stop)
    });
    let script = FauxScript::new();
    script.append_responses(vec![step; 20]);
    let (harness, events) = parent(dir.path(), script).await;
    let session = harness.session().storage().metadata().id.clone();
    let (mut client, output, task) = serve(harness, events);
    output.wait_for(|m| m["type"] == "ready").await;
    send(
        &mut client,
        json!({"type":"prompt","id":1,"message":"Count the call sites in the background."}),
    )
    .await;
    let first = settled(&output, 0).await;
    // The child still works: nothing of it in this turn.
    assert_eq!(first["turn"]["childCostUsd"].as_f64(), Some(0.0), "{first}");
    assert_eq!(first["turn"]["childTokens"].as_i64(), Some(0));
    assert!(first["turn"]["costUsd"].as_f64().unwrap() > 0.0);
    // It ends while the parent is idle.
    gate.store(true, Ordering::Release);
    let node = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let node = crate::mesh::router()
                .list(crate::mesh::ListScope::All, None)
                .unwrap()
                .into_iter()
                .find(|node| node.title == "Count call sites" && node.id.session_id() == session);
            if let Some(node) = node.filter(|node| node.state == crate::mesh::NodeState::Settled) {
                return node;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the child settled");
    assert!(node.usage.cost > 0.0);
    // The next turn (the wake) takes its usage, once.
    send(
        &mut client,
        json!({"type":"prompt","id":2,"message":"<background-agents-done>"}),
    )
    .await;
    let second = settled(&output, 1).await;
    let turn = &second["turn"];
    assert!(
        close(turn["childCostUsd"].as_f64().unwrap(), node.usage.cost),
        "{turn} vs {:?}",
        node.usage
    );
    assert_eq!(
        turn["childTokens"].as_u64(),
        Some(node.usage.input + node.usage.output + node.usage.cache_read + node.usage.cache_write)
    );
    send(
        &mut client,
        json!({"type":"prompt","id":3,"message":"Anything else?"}),
    )
    .await;
    let third = settled(&output, 2).await;
    assert_eq!(third["turn"]["childCostUsd"].as_f64(), Some(0.0), "{third}");
    send(&mut client, json!({"type":"stop","id":4})).await;
    assert_eq!(task.await.unwrap(), 0);
}
