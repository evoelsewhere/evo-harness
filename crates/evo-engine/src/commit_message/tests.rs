//! `draft_commit_message` (docs/features/source-control/spec.md, FR-7).

use super::*;
use std::sync::{Arc, Mutex};

use evo_ai::providers::faux::{faux_assistant_message, FauxProvider, FauxScript, FauxStep};
use evo_ai::types::{Context, Message, StopReason, UserContent};
use evo_harness::agent_harness::AgentLane;
use evo_harness::session::memory::{InMemorySessionStorage, SystemClock};
use evo_harness::session::session::DefaultIdGenerator;
use evo_harness::session::types::SessionMetadata;
use evo_harness::session::Session;
use evo_harness::types::AgentHarnessOptions;
use serde_json::json;

async fn chat(script: FauxScript) -> AgentHarness {
    let provider = FauxProvider::new(script);
    AgentHarness::create(AgentHarnessOptions {
        model: provider.default_model().clone(),
        models: vec![provider],
        session: Session::new(
            Arc::new(InMemorySessionStorage::new(
                SessionMetadata {
                    id: "commit-message-chat".into(),
                    created_at: 0,
                    parent_session_id: None,
                },
                Arc::new(SystemClock),
                Arc::new(DefaultIdGenerator::new()),
            )),
            None,
        ),
        ..Default::default()
    })
    .await
    .unwrap()
}

fn last_user_text(ctx: &Context) -> String {
    ctx.messages
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::User(user) => Some(match &user.content {
                UserContent::Text(text) => text.clone(),
                UserContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|block| match block {
                        evo_ai::types::Content::Text(text) => Some(text.text.clone()),
                        _ => None,
                    })
                    .collect(),
            }),
            _ => None,
        })
        .unwrap_or_default()
}

/// A step that records what the model was asked and answers `answer`.
fn answering(seen: Arc<Mutex<Vec<(usize, String)>>>, answer: &'static str) -> FauxStep {
    FauxStep::factory(move |ctx: &Context| {
        seen.lock()
            .unwrap()
            .push((ctx.tools.len(), last_user_text(ctx)));
        faux_assistant_message(answer, StopReason::Stop)
    })
}

const DIFF: &str = "diff --git a/src/scm.rs b/src/scm.rs\n--- a/src/scm.rs\n+++ b/src/scm.rs\n@@ -1 +1,2 @@\n fn a() {}\n+fn stage_hunk() {}\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commit_message_is_drafted_tool_free_on_the_chat_model_and_writes_nothing() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let script = FauxScript::new().with_text("an earlier answer");
    script.append_responses(vec![answering(
        seen.clone(),
        "```\nfeat(scm): stage a single hunk\n\nAdds stage_hunk.\n```",
    )]);
    let harness = chat(script).await;
    harness.prompt_text("hello", vec![]).await.unwrap();
    let tip = harness.get_tip_id().await.unwrap();
    let entries = harness
        .find_entries(&Default::default())
        .await
        .unwrap()
        .len();

    let reply = draft(
        &harness,
        &json!({"diff": DIFF, "subjects": ["feat(chat): add turn row", "fix(files): keep selection"]}),
    )
    .await
    .unwrap();
    assert_eq!(
        reply,
        json!({"message": "feat(scm): stage a single hunk\n\nAdds stage_hunk."})
    );
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    let (tools, asked) = &seen[0];
    assert_eq!(*tools, 0, "the draft has no tools");
    assert!(asked.starts_with(INSTRUCTION), "{asked}");
    assert!(asked.contains("- feat(chat): add turn row\n- fix(files): keep selection"));
    assert!(asked.contains("+fn stage_hunk() {}"));
    // The chat's transcript and context are untouched.
    assert_eq!(harness.get_tip_id().await.unwrap(), tip);
    assert_eq!(
        harness
            .find_entries(&Default::default())
            .await
            .unwrap()
            .len(),
        entries
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commit_message_input_over_64_kib_is_cut_at_a_file_boundary() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let script = FauxScript::new();
    script.set_responses(vec![answering(seen.clone(), "chore: update files")]);
    let harness = chat(script).await;
    let file = |name: &str, lines: usize| {
        let mut text = format!(
            "diff --git a/{name} b/{name}\n--- a/{name}\n+++ b/{name}\n@@ -0,0 +1,{lines} @@\n"
        );
        for n in 0..lines {
            text.push_str(&format!("+line {n} of {name}\n"));
        }
        text
    };
    let first = file("one.txt", 1400);
    let second = file("two.txt", 1400);
    let third = file("three.txt", 1400);
    let diff = format!("{first}{second}{third}");
    assert!(diff.len() > MAX_DIFF_BYTES);
    let cut_to = cut(&diff, MAX_DIFF_BYTES);
    assert!(cut_to.len() <= MAX_DIFF_BYTES);
    assert!(cut_to.ends_with('\n'));
    assert!(cut_to.contains("diff --git a/two.txt"));
    assert!(!cut_to.contains("three.txt"), "cut at the file boundary");

    draft(&harness, &json!({"diff": diff, "subjects": []}))
        .await
        .unwrap();
    let (_, asked) = seen.lock().unwrap()[0].clone();
    assert!(asked.contains("+line 1399 of two.txt"));
    assert!(!asked.contains("three.txt"));
    assert!(asked.contains("(none: the repository has no commits yet)"));
}

#[test]
fn commit_message_cut_falls_back_to_a_line_and_keeps_utf8() {
    let one_file = format!("diff --git a/x b/x\n{}", "+é\n".repeat(100));
    let cut_to = cut(&one_file, 50);
    assert!(cut_to.len() <= 50);
    assert!(cut_to.ends_with('\n'));
    assert_eq!(cut("short", 50), "short");
    assert_eq!(cut("éééé", 3), "é");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commit_message_refuses_an_empty_diff() {
    let harness = chat(FauxScript::new()).await;
    let error = draft(&harness, &json!({"diff": "  \n", "subjects": []}))
        .await
        .unwrap_err();
    assert_eq!(error, "nothing is staged");
}
