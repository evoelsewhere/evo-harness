//! The checkpoint link and the revert note (docs/features/source-control,
//! FR-9, FR-12). The runs themselves are tested over RPC in
//! `modes/checkpoints_tests.rs`.

use super::*;
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn message() -> evo_ai::types::AssistantMessage {
    evo_ai::providers::faux::faux_assistant_message(
        "checkpoint",
        evo_ai::types::StopReason::ToolUse,
    )
}

async fn call(
    hook: &evo_agent::BeforeToolCall,
    tool: &str,
    args: Value,
) -> Option<evo_agent::BeforeToolCallResult> {
    let evo_ai::types::Content::ToolCall(call) =
        evo_ai::types::Content::tool_call("t1", tool, args)
    else {
        unreachable!()
    };
    let message = message();
    hook(
        evo_agent::BeforeToolCallContext {
            assistant_message: &message,
            tool_call: &call,
            args: &call.arguments,
            context: &evo_agent::AgentContext::default(),
        },
        CancellationToken::new(),
    )
    .await
}

/// A gate whose start snapshot finishes when the sender is used.
fn pending(gate: &Gate) -> (tokio::sync::oneshot::Sender<()>, Arc<AtomicBool>) {
    let (done, wait) = tokio::sync::oneshot::channel::<()>();
    let start: Start = async move {
        let _ = wait.await;
        Some((1, "commit".to_string()))
    }
    .boxed()
    .shared();
    (done, gate.open(start))
}

fn blocking_hook() -> evo_agent::BeforeToolCall {
    Arc::new(|_, _| {
        Box::pin(async {
            Some(evo_agent::BeforeToolCallResult {
                block: true,
                reason: Some("declined".into()),
                terminate: false,
                args: None,
            })
        })
    })
}

#[tokio::test]
async fn checkpoints_edits_and_commands_wait_for_the_start_snapshot_and_reads_do_not() {
    let gate = Arc::new(Gate::with_wait(Duration::from_secs(30)));
    let hook = before_tool_call(None, Arc::clone(&gate)).unwrap();
    let (done, late) = pending(&gate);

    // A read goes on at once.
    let read = tokio::time::timeout(
        Duration::from_millis(500),
        call(&hook, "read", json!({"path": "a.txt"})),
    )
    .await;
    assert!(matches!(read, Ok(None)), "a read must not wait");
    let ls = tokio::time::timeout(
        Duration::from_millis(500),
        call(&hook, "bash", json!({"command": "git status"})),
    )
    .await;
    assert!(matches!(ls, Ok(None)), "a read-only command must not wait");

    // An edit, a command and an unknown tool wait.
    for (tool, args) in [
        ("write", json!({"path": "a.txt", "content": "x"})),
        ("bash", json!({"command": "rm -rf build"})),
        ("some_plugin_tool", json!({})),
    ] {
        let waited =
            tokio::time::timeout(Duration::from_millis(200), call(&hook, tool, args)).await;
        assert!(waited.is_err(), "{tool} must wait for the snapshot");
    }

    let edit = tokio::spawn({
        let hook = hook.clone();
        async move { call(&hook, "edit", json!({"path": "a.txt", "edits": []})).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!edit.is_finished());
    done.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), edit)
        .await
        .expect("the edit goes on once the snapshot exists")
        .unwrap();
    assert!(result.is_none());
    assert!(!late.load(Ordering::Acquire));

    // Taken already: no wait. Closed (between runs): no wait.
    assert!(call(&hook, "write", json!({"path": "b"})).await.is_none());
    gate.close();
    assert!(call(&hook, "write", json!({"path": "b"})).await.is_none());
}

#[tokio::test]
async fn checkpoints_a_blocked_call_never_waits_and_stays_blocked() {
    let gate = Arc::new(Gate::with_wait(Duration::from_secs(30)));
    let hook = before_tool_call(Some(blocking_hook()), Arc::clone(&gate)).unwrap();
    let (_done, late) = pending(&gate);
    let result = tokio::time::timeout(
        Duration::from_millis(500),
        call(&hook, "write", json!({"path": "a.txt", "content": "x"})),
    )
    .await
    .expect("a blocked call must not wait for the snapshot")
    .expect("the block is kept");
    assert!(result.block);
    assert_eq!(result.reason.as_deref(), Some("declined"));
    assert!(!late.load(Ordering::Acquire));
}

#[tokio::test]
async fn checkpoints_a_late_snapshot_lets_the_call_go_and_marks_the_turn_incomplete() {
    let gate = Arc::new(Gate::with_wait(Duration::from_millis(100)));
    let hook = before_tool_call(None, Arc::clone(&gate)).unwrap();
    let (_done, late) = pending(&gate);
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        call(&hook, "write", json!({"path": "a.txt", "content": "x"})),
    )
    .await
    .expect("the call goes on after the wait");
    assert!(result.is_none());
    assert!(started.elapsed() >= Duration::from_millis(100));
    assert!(late.load(Ordering::Acquire), "the turn is incomplete");
}

#[test]
fn checkpoints_revert_note_names_the_turn_files_and_skipped_ones() {
    let text = note(
        17,
        Some("Rename <the> \"helper\"\n and   update the tests please, then run them all again and again and again"),
        false,
        &["src/a.ts".into(), "src/b.ts".into()],
        &["src/c.ts".into()],
    );
    assert!(
        text.starts_with("<turn-reverted checkpoint=\"c-17\">"),
        "{text}"
    );
    assert!(text.ends_with("</turn-reverted>"), "{text}");
    assert!(text.contains(
        "The person reverted your changes from an earlier turn (\"Rename the helper and update"
    ));
    assert!(text.contains("…\"): src/a.ts, src/b.ts are back as they were before it."));
    assert!(text.contains("Skipped (changed since): src/c.ts."));

    let one = note(2, None, false, &["a".into()], &[]);
    assert_eq!(
        one,
        "<turn-reverted checkpoint=\"c-2\">The person reverted your changes from an earlier turn: a is back as it was before it.</turn-reverted>"
    );
    let undone = note(2, Some("Fix it"), true, &["a".into(), "b".into()], &[]);
    assert_eq!(
        undone,
        "<turn-reverted checkpoint=\"c-2\">The person undid their revert of an earlier turn (\"Fix it\"): a, b have that turn's changes again.</turn-reverted>"
    );
}

#[test]
fn checkpoints_only_start_in_a_repository() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!in_repository(dir.path()));
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join("src/deep")).unwrap();
    assert!(in_repository(&dir.path().join("src/deep")));
}
