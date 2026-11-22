//! The person's own messages pass through compaction word for word: kept in a
//! block after the summary, taken out of the previous summary before the
//! model rewrites it, and carried on by every later compaction.

use std::sync::{Arc, Mutex};

use evo_agent::message::AgentMessage;
use evo_ai::providers::faux::{faux_assistant_message, FauxProvider, FauxScript, FauxStep};
use evo_ai::types::{Context, StopReason, UserContent, UserMessage};
use evo_ai::Provider;
use evo_harness::compaction::{compact, CompactionLlmOptions, CompactionPreparation};
use evo_harness::types::DEFAULT_COMPACTION_SETTINGS;
use tokio_util::sync::CancellationToken;

fn user(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage::new(UserContent::Text(text.to_string()), 0))
}

fn assistant(text: &str) -> AgentMessage {
    AgentMessage::Assistant(Box::new(faux_assistant_message(
        text.to_string(),
        StopReason::Stop,
    )))
}

fn preparation(messages: Vec<AgentMessage>, previous: Option<String>) -> CompactionPreparation {
    CompactionPreparation {
        messages_to_summarize: messages,
        turn_prefix_messages: Vec::new(),
        retained_tail: Vec::new(),
        is_split_turn: false,
        tokens_before: 1_000,
        previous_summary: previous,
        file_ops: evo_harness::compaction::FileOperations::default(),
        settings: DEFAULT_COMPACTION_SETTINGS,
    }
}

/// A summariser that answers `reply` and records the prompt it was given.
fn summariser(reply: &'static str, seen: Arc<Mutex<String>>) -> CompactionLlmOptions {
    let script = FauxScript::new();
    script.set_responses(vec![FauxStep::factory(move |ctx: &Context| {
        *seen.lock().unwrap() = serde_json::to_string(&ctx.messages).unwrap();
        faux_assistant_message(reply.to_string(), StopReason::Stop)
    })]);
    let provider = FauxProvider::new(script);
    let model = provider.default_model().clone();
    CompactionLlmOptions {
        provider: provider as Arc<dyn Provider>,
        model,
        api_key: None,
        signal: CancellationToken::new(),
        thinking_level: None,
        retry: None,
        custom_instructions: None,
        stream_options: Default::default(),
    }
}

const REQUEST: &str = "Create 8 changes under openspec/changes/. Hard rule: never run sed -i \
                       across */proposal.md, only touch the 8 folders you create.";

#[tokio::test]
async fn the_persons_words_survive_two_compactions() {
    let seen = Arc::new(Mutex::new(String::new()));
    let first = compact(
        &preparation(
            vec![
                user(REQUEST),
                assistant("Starting."),
                user("<background-agent id=\"a1\" status=\"done\">\nreport\n</background-agent>"),
            ],
            None,
        ),
        &summariser("## Goal\nCreate changes.", seen.clone()),
    )
    .await
    .unwrap();
    assert!(first.summary.starts_with("## Goal\nCreate changes."));
    assert!(
        first.summary.contains("<user-messages>"),
        "{}",
        first.summary
    );
    assert!(first.summary.contains(REQUEST), "{}", first.summary);
    assert!(
        !first.summary.contains("report\n"),
        "engine notes are not kept"
    );

    let second = compact(
        &preparation(
            vec![user("Also validate each change."), assistant("Done.")],
            Some(first.summary.clone()),
        ),
        &summariser("## Goal\nCreate and validate changes.", seen.clone()),
    )
    .await
    .unwrap();
    // The model rewrote the summary without the block in front of it...
    let prompt = seen.lock().unwrap().clone();
    assert!(prompt.contains("Create changes."), "{prompt}");
    assert!(!prompt.contains("user-messages"), "{prompt}");
    // ...and the block came back with both messages, in order, once.
    let summary = &second.summary;
    assert!(summary.starts_with("## Goal\nCreate and validate changes."));
    assert_eq!(summary.matches("<user-messages>").count(), 1, "{summary}");
    let request = summary.find(REQUEST).expect("the original request");
    let later = summary
        .find("Also validate each change.")
        .expect("the later message");
    assert!(request < later, "{summary}");
}
