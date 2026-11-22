//! `run_code` end to end in the real agent loop, with fake tools and the real
//! approval gate (docs/features/code-mode/spec.md, FR-4 – FR-8, FR-10 –
//! FR-14, FR-17).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use evo_agent::{AgentContext, AgentLoopConfig, AgentMessage, BeforeToolCall};
use evo_ai::event_stream::create_assistant_message_event_stream;
use evo_ai::types::{
    Api, AssistantMessage, AssistantMessageEvent, Content, DoneReason, ImageContent,
    ImageContentType, StopReason, ToolCall, ToolCallType, ToolResultMessage,
};
use futures::future::BoxFuture;

use super::*;
use crate::approval::{ApprovalMode, ApprovalState};

// ── A small agent loop ──────────────────────────────────────────────────────

fn assistant(content: Vec<Content>, stop: StopReason) -> AssistantMessage {
    AssistantMessage {
        role: evo_ai::types::AssistantRole,
        content,
        api: Api::Other("openai-responses".into()),
        provider: "mock".into(),
        model: "mock".into(),
        response_model: None,
        response_id: None,
        usage: evo_ai::types::Usage::zero(),
        stop_reason: stop,
        deferred: None,
        error_message: None,
        raw_stop_reason: None,
        end_turn: None,
        timestamp: 0,
    }
}

fn stream(messages: Vec<AssistantMessage>) -> evo_agent::StreamFn {
    let queue = Arc::new(Mutex::new(VecDeque::from(messages)));
    evo_agent::stream_fn(move |_model, _ctx, _opts| {
        let (mut producer, stream) = create_assistant_message_event_stream();
        let next = queue.lock().unwrap().pop_front();
        tokio::spawn(async move {
            let message = next.unwrap_or_else(|| assistant(vec![], StopReason::Stop));
            producer
                .push(AssistantMessageEvent::Start {
                    partial: Arc::new(message.clone()),
                })
                .await;
            let reason = if message.stop_reason == StopReason::ToolUse {
                DoneReason::ToolUse
            } else {
                DoneReason::Stop
            };
            producer
                .push(AssistantMessageEvent::Done { reason, message })
                .await;
        });
        stream
    })
}

fn config(before: Option<BeforeToolCall>, signal: CancellationToken) -> AgentLoopConfig {
    AgentLoopConfig {
        model: evo_ai::Model::new(
            "mock",
            "mock",
            Api::Other("openai-responses".into()),
            "mock",
            "https://example.invalid",
        ),
        convert_to_llm: Arc::new(|messages: Vec<AgentMessage>| {
            let out: Vec<evo_ai::types::Message> = messages
                .into_iter()
                .filter_map(|m| match m {
                    AgentMessage::User(u) => Some(evo_ai::types::Message::User(u)),
                    AgentMessage::Assistant(a) => Some(evo_ai::types::Message::Assistant(a)),
                    AgentMessage::ToolResult(t) => Some(evo_ai::types::Message::ToolResult(t)),
                    AgentMessage::Custom(_) => None,
                })
                .collect();
            Box::pin(async move { out })
        }),
        convert_to_llm_incremental: None,
        transform_context: None,
        get_api_key: None,
        should_stop_after_turn: None,
        prepare_next_turn: None,
        after_tool_results: None,
        get_steering_messages: None,
        get_follow_up_messages: None,
        pending_message_count: None,
        before_tool_call: before,
        after_tool_call: None,
        tool_execution: ToolExecutionMode::Parallel,
        thinking_level: evo_ai::types::ThinkingLevel::Off,
        api_key: None,
        timeout: None,
        stream_idle_timeout: None,
        max_retries: None,
        max_retry_delay: None,
        cache_retention: evo_ai::provider::CacheRetention::default(),
        session_id: None,
        signal,
    }
}

/// Run one `run_code` call with `code` among `tools`; its tool result.
async fn run_with(
    code: &str,
    limits: runtime::Limits,
    tools: Vec<Arc<dyn AgentTool>>,
    before: Option<BeforeToolCall>,
    signal: CancellationToken,
) -> ToolResultMessage {
    let mut all = vec![create_code_tool_with(limits)];
    all.extend(tools);
    let call = Content::ToolCall(ToolCall {
        kind: ToolCallType,
        id: "code-1".into(),
        name: NAME.into(),
        arguments: json!({ "code": code, "description": "test" }),
        thought_signature: None,
        namespace: None,
        recorded: None,
    });
    let (collector, _events) = evo_agent::CollectorEmitter::new();
    let emit: Arc<dyn evo_agent::AgentEmitter> = Arc::new(collector);
    let messages = evo_agent::run_agent_loop(
        vec![AgentMessage::User(evo_ai::types::UserMessage::new(
            evo_ai::types::UserContent::Text("go".into()),
            0,
        ))],
        AgentContext {
            tools: all,
            ..Default::default()
        },
        config(before, signal),
        emit,
        stream(vec![
            assistant(vec![call], StopReason::ToolUse),
            assistant(vec![Content::text("done")], StopReason::Stop),
        ]),
    )
    .await
    .expect("loop ran");
    let results: Vec<ToolResultMessage> = messages
        .into_iter()
        .filter_map(|m| match m {
            AgentMessage::ToolResult(r) => Some(*r),
            _ => None,
        })
        .collect();
    assert_eq!(
        results.len(),
        1,
        "calls from code never become transcript messages"
    );
    results.into_iter().next().unwrap()
}

async fn run(code: &str) -> ToolResultMessage {
    run_with(
        code,
        runtime::Limits::default(),
        fakes(),
        None,
        CancellationToken::new(),
    )
    .await
}

fn text(result: &ToolResultMessage) -> String {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn details(result: &ToolResultMessage) -> Value {
    result.details.clone().unwrap_or(Value::Null)
}

// ── Fake tools ──────────────────────────────────────────────────────────────

type Behaviour = Arc<
    dyn Fn(Value, CancellationToken) -> BoxFuture<'static, Result<AgentToolResult, AgentError>>
        + Send
        + Sync,
>;

struct Fake {
    schema: Tool,
    behaviour: Behaviour,
}

#[async_trait]
impl AgentTool for Fake {
    fn schema(&self) -> &Tool {
        &self.schema
    }
    fn label(&self) -> &str {
        "fake"
    }
    async fn execute(
        &self,
        _: &str,
        params: Value,
        signal: CancellationToken,
        _: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        (self.behaviour)(params, signal).await
    }
}

fn fake(name: &str, behaviour: Behaviour) -> Arc<dyn AgentTool> {
    Arc::new(Fake {
        schema: Tool {
            name: name.into(),
            description: format!("The {name} test tool."),
            parameters: Schema::new(json!({ "type": "object", "properties": {
                "value": { "type": "string" }, "ms": { "type": "integer" }
            } })),
            constrained_sampling: None,
        },
        behaviour,
    })
}

/// `echo` returns its arguments, `fail` fails, `picture` returns an image,
/// `ask_user` stands in for an excluded tool.
fn fakes() -> Vec<Arc<dyn AgentTool>> {
    vec![
        fake(
            "echo",
            Arc::new(|args, _| {
                Box::pin(async move { Ok(AgentToolResult::text(args["value"].to_string())) })
            }),
        ),
        fake(
            "fail",
            Arc::new(|_, _| Box::pin(async { Err(AgentError::Tool("it broke".into())) })),
        ),
        fake(
            "picture",
            Arc::new(|_, _| {
                Box::pin(async {
                    Ok(AgentToolResult {
                        content: vec![
                            TextContentOrImage::text("a picture"),
                            TextContentOrImage::Image(ImageContent {
                                kind: ImageContentType,
                                data: "aGk=".into(),
                                mime_type: "image/png".into(),
                            }),
                        ],
                        ..Default::default()
                    })
                })
            }),
        ),
        fake(
            "ask_user",
            Arc::new(|_, _| Box::pin(async { Ok(AgentToolResult::text("answered")) })),
        ),
    ]
}

/// `slow` sleeps `ms` and records how many ran at once.
fn slow(peak: Arc<AtomicUsize>) -> Arc<dyn AgentTool> {
    let running = Arc::new(AtomicUsize::new(0));
    fake(
        "slow",
        Arc::new(move |args, signal: CancellationToken| {
            let running = running.clone();
            let peak = peak.clone();
            Box::pin(async move {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                let ms = args["ms"].as_u64().unwrap_or(100);
                // Like real tools, it stops when its call is cancelled.
                let slept = tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(ms)) => true,
                    _ = signal.cancelled() => false,
                };
                running.fetch_sub(1, Ordering::SeqCst);
                if slept {
                    Ok(AgentToolResult::text("slept"))
                } else {
                    Err(AgentError::Tool("cancelled".into()))
                }
            })
        }),
    )
}

// ── The tool ────────────────────────────────────────────────────────────────

// FR-4: the schema the model sees.
#[test]
fn the_schema_takes_code_and_a_description() {
    let tool = create_code_tool();
    let schema = tool.schema();
    assert_eq!(schema.name, "run_code");
    assert_eq!(schema.parameters.0["required"], json!(["code"]));
    assert!(schema.parameters.0["properties"]["description"].is_object());
    assert_eq!(tool.execution_mode(), ToolExecutionMode::Sequential);
}

// FR-5, FR-8: tools are functions; the result is the last value, the
// printed lines and the tools used.
#[tokio::test]
async fn a_program_calls_tools_and_returns_its_last_value() {
    let result =
        run("const a = await tools.echo({ value: 'hi' });\nconsole.log('got', a);\n({ a, n: 2 })")
            .await;
    assert!(!result.is_error, "{}", text(&result));
    let d = details(&result);
    assert_eq!(d["result"], json!({ "a": "\"hi\"", "n": 2 }));
    assert_eq!(d["stdout"], "got \"hi\"");
    assert_eq!(d["toolsUsed"], json!(["echo"]));
    assert_eq!(d["calls"][0]["ok"], true);
    let shown = text(&result);
    assert!(shown.starts_with(r#"{"a":"\"hi\"","n":2}"#), "{shown}");
    assert!(shown.contains("--- console ---\ngot \"hi\""), "{shown}");
    assert!(
        shown.contains("--- tools used: echo (1 call) ---"),
        "{shown}"
    );
}

// FR-5: a failed call rejects with a ToolError the program can catch; an
// uncaught one fails the run with its line.
#[tokio::test]
async fn failed_calls_throw_tool_errors() {
    let caught = run(
        "let m = '';\ntry { await tools.fail({}) } catch (e) { m = e.name + ': ' + e.message }\nm",
    )
    .await;
    assert!(!caught.is_error);
    // The same text a direct call's error has.
    assert_eq!(
        details(&caught)["result"],
        "ToolError: tool error: it broke"
    );

    let uncaught = run("await tools.echo({ value: 'x' });\nawait tools.fail({});").await;
    assert!(uncaught.is_error);
    let d = details(&uncaught);
    assert_eq!(d["error"], "ToolError: tool error: it broke");
    assert_eq!(d["line"], 2);
    assert_eq!(d["toolsUsed"], json!(["echo", "fail"]));
    assert!(text(&uncaught).contains("--- error (line 2) ---\nToolError: tool error: it broke"));
}

// FR-12: excluded and unknown tools.
#[tokio::test]
async fn excluded_and_unknown_tools_cannot_be_called() {
    let result = run("const names = tools.list().map(t => t.name);\n\
         const errors = [];\n\
         for (const name of ['ask_user', 'nope', 'run_code']) {\n\
           try { await tools.call(name, {}) } catch (e) { errors.push(e.message) }\n\
         }\n\
         ({ names, errors })")
    .await;
    assert!(!result.is_error, "{}", text(&result));
    let d = details(&result);
    let names = d["result"]["names"].as_array().unwrap();
    assert!(names.contains(&json!("echo")));
    assert!(!names.contains(&json!("ask_user")));
    assert!(!names.contains(&json!("run_code")));
    assert_eq!(
        d["result"]["errors"],
        json!([
            "ask_user cannot be called from code",
            "nope is not a tool in this chat",
            "run_code cannot be called from code"
        ])
    );
}

// FR-5: tools.describe gives a tool's parameters.
#[tokio::test]
async fn describe_gives_parameters() {
    let result = run("tools.describe('echo').parameters.properties.value.type").await;
    assert_eq!(details(&result)["result"], "string");
}

// FR-7: calls run at the same time, at most eight at once.
#[tokio::test]
async fn calls_run_in_parallel_up_to_eight_at_once() {
    let peak = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();
    let result = run_with(
        "await Promise.all(Array.from({ length: 12 }, () => tools.slow({ ms: 200 })));\n'ok'",
        runtime::Limits::default(),
        vec![slow(peak.clone())],
        None,
        CancellationToken::new(),
    )
    .await;
    let took = started.elapsed();
    assert!(!result.is_error, "{}", text(&result));
    assert_eq!(details(&result)["calls"].as_array().unwrap().len(), 12);
    let peak = peak.load(Ordering::SeqCst);
    assert!((2..=bridge::CONCURRENCY).contains(&peak), "peak {peak}");
    // Twelve 200 ms calls one after another would take 2.4 s.
    assert!(took < Duration::from_millis(1500), "{took:?}");
}

// FR-8: images from calls are attached to the result.
#[tokio::test]
async fn images_are_attached_to_the_result() {
    let result = run("await tools.picture({})").await;
    assert_eq!(details(&result)["result"], "a picture");
    assert!(result
        .content
        .iter()
        .any(|c| matches!(c, Content::Image(_))));
}

// FR-13: the call cap.
#[tokio::test]
async fn the_call_cap_ends_the_run() {
    let result = run_with(
        "for (let i = 0; i < 10; i++) await tools.echo({ value: String(i) });",
        runtime::Limits {
            max_calls: 3,
            ..Default::default()
        },
        fakes(),
        None,
        CancellationToken::new(),
    )
    .await;
    assert!(result.is_error);
    let d = details(&result);
    assert_eq!(d["limit"], "calls");
    assert_eq!(d["calls"].as_array().unwrap().len(), 3);
}

// FR-13: the result is cut to the usual limit.
#[tokio::test]
async fn a_large_result_is_cut() {
    let result = run("'x'.repeat(200000)").await;
    let shown = text(&result);
    assert!(shown.len() < 60_000, "{}", shown.len());
    assert!(
        shown.contains("[Output cut to"),
        "{}",
        &shown[shown.len() - 200..]
    );
}

// FR-14: stopping the turn stops the program and its calls.
#[tokio::test]
async fn stopping_the_turn_stops_the_program() {
    let signal = CancellationToken::new();
    let later = signal.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        later.cancel();
    });
    let started = Instant::now();
    let result = run_with(
        "await tools.slow({ ms: 20000 });\n'never'",
        runtime::Limits::default(),
        vec![slow(Arc::new(AtomicUsize::new(0)))],
        None,
        signal,
    )
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(result.is_error, "{}", text(&result));
    assert_ne!(details(&result)["result"], "never");
}

// ── Approvals for calls from code (FR-10, FR-11) ────────────────────────────

/// A gate in "Ask every time", and the mailbox its prompts go to.
fn ask_gate() -> (BeforeToolCall, evo_extensions::UiDialogMailbox) {
    let mailbox = evo_extensions::UiDialogMailbox::new();
    mailbox.attach();
    let state = Arc::new(ApprovalState::new(ApprovalMode::Ask));
    let gate = crate::approval::before_tool_call(None, mailbox.clone(), state).expect("gate");
    (gate, mailbox)
}

/// Answer the next `n` prompts with `index`, after `delay`; their payloads.
fn answer_prompts(
    mailbox: evo_extensions::UiDialogMailbox,
    answers: Vec<u64>,
    delay: Duration,
) -> tokio::task::JoinHandle<Vec<Value>> {
    tokio::spawn(async move {
        let mut seen = Vec::new();
        for index in answers {
            let request = loop {
                if let Some(request) = mailbox.take_pending() {
                    break request;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            };
            tokio::time::sleep(delay).await;
            mailbox
                .respond(&request.request_id, json!({ "index": index }))
                .unwrap();
            seen.push(request.ui);
        }
        seen
    })
}

/// `deploy` is a command (not in the tool table), so "Ask every time" asks.
fn deploy(count: Arc<AtomicUsize>) -> Arc<dyn AgentTool> {
    fake(
        "deploy",
        Arc::new(move |_, _| {
            let count = count.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(AgentToolResult::text("deployed"))
            })
        }),
    )
}

#[tokio::test]
async fn each_call_from_code_asks_with_the_code_options() {
    let (gate, mailbox) = ask_gate();
    let count = Arc::new(AtomicUsize::new(0));
    // Allow the first, deny the second; the program goes on after the denial.
    let prompts = answer_prompts(mailbox, vec![0, 3], Duration::ZERO);
    let result = run_with(
        "const out = [await tools.deploy({ value: 'a' })];\n\
         try { await tools.deploy({ value: 'b' }) } catch (e) { out.push(e.message) }\n\
         out",
        runtime::Limits::default(),
        vec![deploy(count.clone())],
        Some(gate),
        CancellationToken::new(),
    )
    .await;
    let prompts = prompts.await.unwrap();
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[0]["approval"]["origin"], "code");
    assert_eq!(prompts[0]["options"][2]["title"], "Stop code");
    assert!(!result.is_error, "{}", text(&result));
    assert_eq!(
        details(&result)["result"],
        json!(["deployed", "The user declined this command."])
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stop_code_ends_the_program_and_keeps_finished_calls() {
    let (gate, mailbox) = ask_gate();
    let count = Arc::new(AtomicUsize::new(0));
    let prompts = answer_prompts(mailbox, vec![0, 2], Duration::ZERO);
    let result = run_with(
        "await tools.deploy({ value: 'a' });\n\
         try { await tools.deploy({ value: 'b' }) } catch (e) {}\n\
         await tools.deploy({ value: 'c' });\n\
         'never'",
        runtime::Limits::default(),
        vec![deploy(count.clone())],
        Some(gate),
        CancellationToken::new(),
    )
    .await;
    prompts.await.unwrap();
    assert!(result.is_error, "{}", text(&result));
    let d = details(&result);
    assert_eq!(d["error"], "The user stopped the code.");
    assert_eq!(count.load(Ordering::SeqCst), 1, "only the allowed call ran");
}

// FR-11: time at a prompt does not count toward the time limit.
#[tokio::test]
async fn time_at_a_prompt_does_not_count() {
    let (gate, mailbox) = ask_gate();
    let prompts = answer_prompts(mailbox, vec![0], Duration::from_millis(1200));
    let result = run_with(
        "await tools.deploy({ value: 'a' });\n'done'",
        runtime::Limits {
            time: Duration::from_millis(800),
            ..Default::default()
        },
        vec![deploy(Arc::new(AtomicUsize::new(0)))],
        Some(gate),
        CancellationToken::new(),
    )
    .await;
    prompts.await.unwrap();
    assert!(!result.is_error, "{}", text(&result));
    assert_eq!(details(&result)["result"], "done");
}

// ── Hook view (FR-17) ───────────────────────────────────────────────────────

fn sample() -> AgentToolResult {
    build_result(
        "(await $`cat notes.txt`.text())",
        "read notes",
        Run {
            outcome: runtime::Outcome {
                value: Some(json!("deploy token: abc123")),
                ..Default::default()
            },
            served: bridge::Served {
                calls: vec![bridge::CallRecord {
                    name: "bash".into(),
                    ok: Some(true),
                    ms: 3,
                    args: json!({ "command": "cat notes.txt" }),
                    output: "deploy token: abc123".into(),
                    details: Value::Null,
                }],
                ..Default::default()
            },
            stopped: None,
            limits: runtime::Limits::default(),
        },
    )
}

#[test]
fn hooks_see_the_structured_response() {
    let view = hook_view(&sample()).unwrap();
    assert_eq!(
        view,
        json!({
            "code": "(await $`cat notes.txt`.text())",
            "result": "deploy token: abc123",
            "stdout": "", "stderr": "", "toolsUsed": ["bash"],
        })
    );
}

#[test]
fn a_hook_can_hide_a_secret_and_a_malformed_copy_is_ignored() {
    let original = sample();
    let mut copy = hook_view(&original).unwrap();
    copy["result"] = json!("deploy token: [hidden]");
    let changed = hook_apply(&original, &copy).unwrap();
    let shown = match &changed.content[0] {
        TextContentOrImage::Text(t) => t.text.clone(),
        _ => unreachable!(),
    };
    assert!(shown.starts_with("deploy token: [hidden]"), "{shown}");
    assert!(!shown.contains("abc123"));
    assert_eq!(changed.details["result"], "deploy token: [hidden]");

    for bad in [
        json!({ "code": 1, "stdout": "", "stderr": "" }),
        json!({ "code": "x", "stdout": "", "stderr": "", "error": 3 }),
        json!({ "code": "x", "stdout": "", "stderr": "", "toolsUsed": [1] }),
        json!({ "code": "x", "stderr": "" }),
    ] {
        assert!(hook_apply(&original, &bad).is_none(), "{bad}");
    }
}

// FR-10: an earlier link of the chain (a guard, a `PreToolUse` hook, a plugin
// handler) blocks a call from code with its reason, as for a direct call.
#[tokio::test]
async fn an_earlier_link_blocks_a_call_from_code() {
    let count = Arc::new(AtomicUsize::new(0));
    let block: BeforeToolCall = Arc::new(|ctx: evo_agent::BeforeToolCallContext<'_>, _signal| {
        let blocked = ctx.tool_call.name == "deploy";
        Box::pin(async move {
            blocked.then(|| evo_agent::BeforeToolCallResult {
                block: true,
                reason: Some("Blocked by a hook: no deploys".into()),
                terminate: false,
                args: None,
            })
        })
    });
    let result = run_with(
        "let reason = '';\n\
         try { await tools.deploy({}) } catch (e) { reason = e.message }\n\
         [reason, await tools.echo({ value: 'still running' })]",
        runtime::Limits::default(),
        {
            let mut tools = fakes();
            tools.push(deploy(count.clone()));
            tools
        },
        Some(block),
        CancellationToken::new(),
    )
    .await;
    assert!(!result.is_error, "{}", text(&result));
    assert_eq!(
        details(&result)["result"],
        json!(["Blocked by a hook: no deploys", "\"still running\""])
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

/// A fake shell that answers with the command it got; `exit 3` fails like the
/// real one ("…\n\nCommand exited with code 3").
fn fake_shell() -> Arc<dyn AgentTool> {
    fake(
        "bash",
        Arc::new(|args, _| {
            Box::pin(async move {
                let command = args["command"].as_str().unwrap_or_default().to_owned();
                if command.contains("exit 3") {
                    Err(AgentError::Tool(
                        "partial output\n\nCommand exited with code 3".into(),
                    ))
                } else {
                    Ok(AgentToolResult::text(format!(
                        "{command}|timeout={}",
                        args["timeout"]
                    )))
                }
            })
        }),
    )
}

/// Fake file tools that answer with their arguments.
fn fake_files() -> Vec<Arc<dyn AgentTool>> {
    ["read", "write", "edit"]
        .into_iter()
        .map(|name| {
            fake(
                name,
                Arc::new(move |args, _| {
                    Box::pin(async move { Ok(AgentToolResult::text(args.to_string())) })
                }),
            )
        })
        .collect()
}

// FR-6: `$` quotes values as single arguments, offers .nothrow() with the
// exit code and $.timeout; the file shortcuts are one ordinary call each.
#[tokio::test]
async fn shell_and_file_shortcuts() {
    let mut tools = vec![fake_shell()];
    tools.extend(fake_files());
    let result = run_with(
        "const quoted = await $`echo ${'a b'} ${\"it's\"} ${['x', 'y z']}`;\n\
         const failed = await $`exit 3`.nothrow();\n\
         const slow = await $.timeout(600)`npm test`;\n\
         let thrown = '';\n\
         try { await $`exit 3` } catch (e) { thrown = e.name }\n\
         const r = JSON.parse(await read('a.txt'));\n\
         const w = JSON.parse(await write('b.txt', 'hi'));\n\
         const e = JSON.parse(await edit('c.txt', 'old', 'new'));\n\
         ({ quoted, failed, slow, thrown, r, w, e })",
        runtime::Limits::default(),
        tools,
        None,
        CancellationToken::new(),
    )
    .await;
    assert!(!result.is_error, "{}", text(&result));
    let d = details(&result);
    let r = &d["result"];
    assert_eq!(r["quoted"], r"echo 'a b' 'it'\''s' x 'y z'|timeout=null");
    assert_eq!(
        r["failed"],
        json!({ "exitCode": 3, "output": "tool error: partial output", "ok": false })
    );
    assert_eq!(r["slow"], "npm test|timeout=600");
    assert_eq!(r["thrown"], "ToolError");
    assert_eq!(r["r"], json!({ "path": "a.txt" }));
    assert_eq!(r["w"], json!({ "path": "b.txt", "content": "hi" }));
    assert_eq!(
        r["e"],
        json!({ "path": "c.txt", "edits": [{ "oldText": "old", "newText": "new" }] })
    );
    // One ordinary call per shortcut.
    assert_eq!(d["toolsUsed"], json!(["bash", "read", "write", "edit"]));
    assert_eq!(d["calls"].as_array().unwrap().len(), 7);
}

// FR-6: grep / glob need the grep / find tools.
#[tokio::test]
async fn grep_and_glob_need_their_tools() {
    let result = run("let m = '';\ntry { await grep('x') } catch (e) { m = e.message }\nm").await;
    assert_eq!(
        details(&result)["result"],
        "grep is not a tool in this chat; use $`rg ...` instead"
    );
}

// FR-15: each call keeps a bounded copy of its arguments and output for the
// transcript's Code row.
#[tokio::test]
async fn calls_keep_a_bounded_copy_for_the_transcript() {
    let result = run("await tools.echo({ value: 'short' });\n\
         await tools.echo({ value: 'x'.repeat(5000) });\n\
         try { await tools.fail({}) } catch {}\n0")
    .await;
    let calls = details(&result)["calls"].as_array().unwrap().clone();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0]["args"], json!({ "value": "short" }));
    assert_eq!(calls[0]["output"], "\"short\"");
    let long = calls[1]["args"]["value"].as_str().unwrap();
    assert!(
        long.ends_with("…[cut]") && long.chars().count() < 1100,
        "{}",
        long.len()
    );
    assert!(calls[1]["output"].as_str().unwrap().ends_with("…[cut]"));
    assert_eq!(calls[2]["ok"], false);
    assert_eq!(calls[2]["output"], "tool error: it broke");
}

// FR-6: files() lists files through one shell call.
#[tokio::test]
async fn files_lists_through_the_shell() {
    let shell = fake(
        "bash",
        Arc::new(|args, _| {
            Box::pin(async move {
                assert_eq!(args["command"], "find src -type f");
                Ok(AgentToolResult::text("./src/b.js\n./src/a.js\n./src/c.md"))
            })
        }),
    );
    let result = run_with(
        "return { all: await files('src'), js: await files('src', '.js') };",
        runtime::Limits::default(),
        vec![shell],
        None,
        CancellationToken::new(),
    )
    .await;
    assert!(!result.is_error, "{}", text(&result));
    assert_eq!(
        details(&result)["result"],
        json!({ "all": ["src/a.js", "src/b.js", "src/c.md"], "js": ["src/a.js", "src/b.js"] })
    );
}

// FR-6: read() gives the file's text as it is, so a program can parse it.
#[tokio::test]
async fn read_returns_the_raw_file_text() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("c.json"), "{\n  \"port\": 3000\n}\n").unwrap();
    let tools: Vec<Arc<dyn AgentTool>> = crate::tools::builtin_tools_in(dir.path())
        .into_iter()
        .map(|tool| tool.tool)
        .filter(|tool| tool.schema().name == "read")
        .collect();
    let path = dir
        .path()
        .join("c.json")
        .display()
        .to_string()
        .replace('\\', "/");
    let result = run_with(
        &format!(
            "const text = await read({path:?});\nreturn {{ text, port: JSON.parse(text).port }};"
        ),
        runtime::Limits::default(),
        tools,
        None,
        CancellationToken::new(),
    )
    .await;
    assert!(!result.is_error, "{}", text(&result));
    assert_eq!(
        details(&result)["result"]["port"],
        3000,
        "{}",
        details(&result)
    );
}
