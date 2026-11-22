//! A provider answering HTTP 413 must not leave the chat stuck: the harness
//! trims the older images of the request and retries, while the stored session
//! keeps every image.
use std::sync::{Arc, Mutex};

use evo_ai::providers::faux::{faux_assistant_message, FauxProvider, FauxScript, FauxStep};
use evo_ai::types::{Content, ImageContent, ImageContentType, Message, StopReason, UserContent};
use evo_ai::Provider;
use evo_harness::agent_harness::{AgentHarness, AgentLane, HarnessRunOutcome};
use evo_harness::session::memory::{InMemorySessionStorage, SystemClock};
use evo_harness::session::types::SessionMetadata;
use evo_harness::session::{DefaultIdGenerator, Session};
use evo_harness::types::{AgentHarnessOptions, RetryPolicy};

fn images_in(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| match m {
            Message::User(u) => match &u.content {
                UserContent::Blocks(b) => {
                    b.iter().filter(|c| matches!(c, Content::Image(_))).count()
                }
                UserContent::Text(_) => 0,
            },
            Message::ToolResult(t) => t
                .content
                .iter()
                .filter(|c| matches!(c, Content::Image(_)))
                .count(),
            Message::Assistant(_) => 0,
        })
        .sum()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_413_trims_older_images_and_the_run_recovers() {
    let seen: Arc<Mutex<Vec<usize>>> = Default::default();
    let record = |seen: &Arc<Mutex<Vec<usize>>>| {
        let seen = Arc::clone(seen);
        move |ctx: &evo_ai::types::Context| {
            seen.lock().unwrap().push(images_in(&ctx.messages));
        }
    };
    let (first, second) = (record(&seen), record(&seen));
    let script = FauxScript::new();
    script.set_responses(vec![
        FauxStep::factory(move |ctx| {
            first(ctx);
            let mut failure = faux_assistant_message("", StopReason::Error);
            failure.error_message = Some("http error 413: Request Entity Too Large".into());
            failure
        }),
        FauxStep::factory(move |ctx| {
            second(ctx);
            faux_assistant_message("recovered", StopReason::Stop)
        }),
    ]);
    let provider = FauxProvider::new(script);
    let session = Session::new(
        Arc::new(InMemorySessionStorage::new(
            SessionMetadata {
                id: "too-large".into(),
                created_at: 0,
                parent_session_id: None,
            },
            Arc::new(SystemClock),
            Arc::new(DefaultIdGenerator::new()),
        )),
        None,
    );
    let options = AgentHarnessOptions {
        model: provider.default_model().clone(),
        thinking_level: Default::default(),
        active_tool_names: vec![],
        tools: vec![],
        system_prompt: None,
        resources: Default::default(),
        stream_options: Default::default(),
        retry: RetryPolicy {
            base_delay_ms: 1,
            ..Default::default()
        },
        compaction: evo_harness::types::CompactionSettings {
            enabled: false,
            reserve_tokens: 20,
            keep_recent_tokens: 1,
        },
        steering_mode: Default::default(),
        follow_up_mode: Default::default(),
        tool_execution: Default::default(),
        drive: Default::default(),
        session,
        allow_existing_session: false,
        models: vec![provider.clone() as Arc<dyn Provider>],
        to_provider_messages: None,
        entry_projectors: Default::default(),
        agent_emitter: None,
        before_tool_call: None,
        after_tool_call: None,
        transform_context: None,
        entry_transforms: Vec::new(),
        provider_hooks: None,
    };
    let harness = AgentHarness::create(options).await.unwrap();
    // A complete 1x1 GIF, small enough to keep its original bytes.
    let image = ImageContent {
        kind: ImageContentType,
        data: "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7".into(),
        mime_type: "image/gif".into(),
    };
    let result = harness.prompt_text("look", vec![image; 30]).await.unwrap();
    assert!(
        matches!(result.outcome, HarnessRunOutcome::Completed { .. }),
        "{:?}",
        result.outcome
    );
    // 30 images: the first request is capped at 24, the retry at 12.
    assert_eq!(*seen.lock().unwrap(), vec![24, 12]);
}
