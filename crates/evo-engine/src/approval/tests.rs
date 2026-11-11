use super::*;
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn kind(tool: &str, args: Value) -> Kind {
    classify(tool, &args)
}

#[test]
fn classification_table() {
    for tool in [
        "read",
        "grep",
        "find",
        "ls",
        "lsp",
        "docs",
        "web_search",
        "web_fetch",
        "ask_user",
        "update_todos",
        "skill",
        "agent",
        "message_agent",
        "send_message",
        "ask",
        "reply",
        "list_nodes",
        "cancel_node",
        "visualize_read_me",
        "show_widget",
        "document_preview",
    ] {
        assert_eq!(kind(tool, json!({})), Kind::Read, "{tool}");
    }
    assert_eq!(kind("write", json!({ "path": "a" })), Kind::Edit);
    assert_eq!(kind("edit", json!({ "path": "a" })), Kind::Edit);
    assert_eq!(kind("memory", json!({ "action": "save" })), Kind::Edit);
    assert_eq!(kind("memory", json!({ "action": "delete" })), Kind::Edit);
    // docs/features/memory-topics-and-chat-recall/spec.md (FR-3): a topic update is an edit.
    assert_eq!(kind("memory", json!({ "action": "update" })), Kind::Edit);
    assert_eq!(kind("memory", json!({ "action": "read" })), Kind::Read);
    assert_eq!(kind("memory", json!({ "action": "list" })), Kind::Read);
}

/// Commands that ran without a prompt in Ask mode (found by running the app).
#[test]
fn commands_the_checker_misread_are_commands() {
    for command in [
        "echo hi >&2file",
        "GIT_EXTERNAL_DIFF='touch x;:' git diff",
        "opts='-o out'; sort $opts in",
    ] {
        assert_eq!(
            kind("bash", json!({ "command": command })),
            Kind::Command,
            "{command}"
        );
    }
    for command in [
        "Get-ChildItem <# c #> ; Remove-Item x",
        "git log ('--output=x')",
        "[IO.StreamWriter]'x.txt'",
    ] {
        assert_eq!(
            kind("powershell", json!({ "command": command })),
            Kind::Command,
            "{command}"
        );
    }
}

#[test]
fn shell_commands_split_into_reads_and_commands() {
    for tool in ["bash", "powershell"] {
        assert_eq!(kind(tool, json!({ "command": "git status" })), Kind::Read);
        assert_eq!(
            kind(tool, json!({ "command": "git log --oneline -5 | head" })),
            Kind::Read
        );
        assert_eq!(
            kind(tool, json!({ "command": "npm install" })),
            Kind::Command
        );
        assert_eq!(
            kind(tool, json!({ "command": "echo hi > out.txt" })),
            Kind::Command
        );
        // Not a read just because the checker once took it for one.
        assert_eq!(
            kind(tool, json!({ "command": "git log $opts" })),
            Kind::Command
        );
        // A call with no command cannot be proven to read.
        assert_eq!(kind(tool, json!({})), Kind::Read);
    }
    assert_eq!(
        kind("bash", json!({ "command": "echo $(rm -rf x)" })),
        Kind::Command
    );
    assert_eq!(
        kind("powershell", json!({ "command": "$f.Delete()" })),
        Kind::Command
    );
}

#[test]
fn unknown_tools_are_commands() {
    assert_eq!(kind("brand_new_tool", json!({})), Kind::Command);
    assert_eq!(kind("", json!({})), Kind::Command);
}

#[test]
fn classifying_takes_well_under_a_millisecond() {
    let args = json!({ "command": "git log --oneline -20 -- crates/evo-engine | head -50" });
    let started = std::time::Instant::now();
    for _ in 0..1000 {
        std::hint::black_box(classify("bash", &args));
    }
    // 1000 calls in under a second: far below 1 ms each, with room for a slow CI box.
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(started.elapsed() / 1000 < Duration::from_millis(1));
}

#[test]
fn each_mode_asks_for_the_kinds_it_says() {
    let none = Grants::default();
    let table = [
        (ApprovalMode::Ask, Kind::Read, false),
        (ApprovalMode::Ask, Kind::Edit, true),
        (ApprovalMode::Ask, Kind::Command, true),
        (ApprovalMode::AutoEdits, Kind::Read, false),
        (ApprovalMode::AutoEdits, Kind::Edit, false),
        (ApprovalMode::AutoEdits, Kind::Command, true),
        (ApprovalMode::Auto, Kind::Read, false),
        (ApprovalMode::Auto, Kind::Edit, false),
        (ApprovalMode::Auto, Kind::Command, false),
    ];
    for (mode, kind, expected) in table {
        assert_eq!(asks(mode, kind, none), expected, "{mode:?} {kind:?}");
    }
}

#[test]
fn grants_stop_prompts_of_their_kind_only() {
    let edits = Grants {
        edit: true,
        command: false,
    };
    assert!(!asks(ApprovalMode::Ask, Kind::Edit, edits));
    assert!(asks(ApprovalMode::Ask, Kind::Command, edits));
    let commands = Grants {
        edit: false,
        command: true,
    };
    assert!(!asks(ApprovalMode::Ask, Kind::Command, commands));
    assert!(asks(ApprovalMode::Ask, Kind::Edit, commands));
}

#[test]
fn the_mode_round_trips_through_text() {
    for mode in [
        ApprovalMode::Ask,
        ApprovalMode::AutoEdits,
        ApprovalMode::Auto,
    ] {
        assert_eq!(ApprovalMode::parse(mode.as_str()), Some(mode));
    }
    assert_eq!(ApprovalMode::parse("sometimes"), None);
}

#[test]
fn engines_start_in_the_mode_they_were_given() {
    use crate::args::Mode;
    assert_eq!(
        initial_mode(Some("auto-edits"), Mode::Rpc),
        Ok(ApprovalMode::AutoEdits)
    );
    assert_eq!(initial_mode(Some("ask"), Mode::Json), Ok(ApprovalMode::Ask));
    assert_eq!(initial_mode(None, Mode::Rpc), Ok(ApprovalMode::Ask));
    assert_eq!(initial_mode(None, Mode::Json), Ok(ApprovalMode::Auto));
    assert_eq!(initial_mode(None, Mode::Text), Ok(ApprovalMode::Auto));
    assert!(initial_mode(Some("sometimes"), Mode::Rpc).is_err());
}

#[test]
fn prompts_summarise_the_call() {
    let (title, text) = describe("bash", Kind::Command, &json!({ "command": "npm install" }));
    assert_eq!(
        (title.as_str(), text.as_str()),
        ("Run a command?", "npm install")
    );
    let (title, text) = describe(
        "edit",
        Kind::Edit,
        &json!({ "path": "src/app.ts", "edits": [
            { "oldText": "a\nb\nc", "newText": "1\n2\n3\n4\n5" },
            { "oldText": "x", "newText": "" },
        ] }),
    );
    assert_eq!(
        (title.as_str(), text.as_str()),
        ("Edit file?", "src/app.ts (+5 −4)")
    );
    let (_, text) = describe(
        "write",
        Kind::Edit,
        &json!({ "path": "notes.md", "content": "one\ntwo" }),
    );
    assert_eq!(text, "notes.md (2 lines)");
    let (title, _) = describe("plugin_tool", Kind::Command, &json!({ "a": 1 }));
    assert_eq!(title, "Run plugin_tool?");
}

// ── The gate ────────────────────────────────────────────────────────────────

fn message() -> evo_ai::types::AssistantMessage {
    evo_ai::providers::faux::faux_assistant_message("approval", evo_ai::types::StopReason::ToolUse)
}

async fn call(
    hook: &evo_agent::BeforeToolCall,
    id: &str,
    tool: &str,
    args: Value,
    signal: CancellationToken,
) -> Option<evo_agent::BeforeToolCallResult> {
    let evo_ai::types::Content::ToolCall(call) = evo_ai::types::Content::tool_call(id, tool, args)
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
        signal,
    )
    .await
}

fn gate(
    mode: ApprovalMode,
) -> (
    evo_agent::BeforeToolCall,
    evo_extensions::UiDialogMailbox,
    Arc<ApprovalState>,
) {
    gate_after(None, mode)
}

fn gate_after(
    previous: Option<evo_agent::BeforeToolCall>,
    mode: ApprovalMode,
) -> (
    evo_agent::BeforeToolCall,
    evo_extensions::UiDialogMailbox,
    Arc<ApprovalState>,
) {
    let mailbox = evo_extensions::UiDialogMailbox::new();
    mailbox.attach();
    let state = Arc::new(ApprovalState::new(mode));
    let hook = before_tool_call(previous, mailbox.clone(), state.clone()).expect("installed");
    (hook, mailbox, state)
}

/// Wait for the prompt and answer it.
async fn answer(mailbox: &evo_extensions::UiDialogMailbox, index: u64) -> Value {
    for _ in 0..200 {
        if let Some(request) = mailbox.take_pending() {
            mailbox
                .respond(&request.request_id, json!({ "index": index }))
                .expect("respond");
            return request.ui;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no prompt appeared");
}

fn no_prompt(mailbox: &evo_extensions::UiDialogMailbox) {
    assert_eq!(mailbox.pending_len(), 0, "no prompt expected");
}

#[tokio::test]
async fn ask_mode_asks_for_a_command_and_runs_it_when_allowed() {
    let (hook, mailbox, _) = gate(ApprovalMode::Ask);
    let running = call(
        &hook,
        "c1",
        "bash",
        json!({ "command": "npm install" }),
        CancellationToken::new(),
    );
    let (result, ui) = tokio::join!(running, answer(&mailbox, 0));
    assert!(result.is_none(), "allowed call is not blocked");
    assert_eq!(ui["title"], "Run a command?");
    assert_eq!(ui["description"], "npm install");
    assert_eq!(ui["approval"]["kind"], "command");
    assert_eq!(ui["options"].as_array().unwrap().len(), 3);
    assert_eq!(ui["options"][1]["title"], "Allow commands for this chat");
}

#[tokio::test]
async fn a_denied_call_is_blocked_with_the_reason() {
    let (hook, mailbox, _) = gate(ApprovalMode::Ask);
    let running = call(
        &hook,
        "c1",
        "write",
        json!({ "path": "a.txt", "content": "x" }),
        CancellationToken::new(),
    );
    let (result, ui) = tokio::join!(running, answer(&mailbox, 2));
    let result = result.expect("blocked");
    assert!(result.block);
    assert_eq!(
        result.reason.as_deref(),
        Some("The user declined this edit.")
    );
    assert_eq!(ui["title"], "Write file?");
}

#[tokio::test]
async fn reads_never_ask_in_any_mode() {
    for mode in [
        ApprovalMode::Ask,
        ApprovalMode::AutoEdits,
        ApprovalMode::Auto,
    ] {
        let (hook, mailbox, _) = gate(mode);
        for (tool, args) in [
            ("read", json!({ "path": "a" })),
            ("grep", json!({ "pattern": "x" })),
            ("bash", json!({ "command": "git status" })),
        ] {
            let result = call(&hook, "r", tool, args, CancellationToken::new()).await;
            assert!(result.is_none());
            no_prompt(&mailbox);
        }
    }
}

#[tokio::test]
async fn auto_edits_runs_edits_and_asks_for_commands() {
    let (hook, mailbox, _) = gate(ApprovalMode::AutoEdits);
    let edit = call(
        &hook,
        "e",
        "edit",
        json!({ "path": "a", "edits": [] }),
        CancellationToken::new(),
    )
    .await;
    assert!(edit.is_none());
    no_prompt(&mailbox);
    let running = call(
        &hook,
        "c",
        "bash",
        json!({ "command": "make" }),
        CancellationToken::new(),
    );
    let (result, ui) = tokio::join!(running, answer(&mailbox, 0));
    assert!(result.is_none());
    assert_eq!(ui["title"], "Run a command?");
}

#[tokio::test]
async fn auto_mode_never_asks() {
    let (hook, mailbox, _) = gate(ApprovalMode::Auto);
    let result = call(
        &hook,
        "c",
        "bash",
        json!({ "command": "rm -rf build" }),
        CancellationToken::new(),
    )
    .await;
    assert!(result.is_none());
    no_prompt(&mailbox);
}

#[tokio::test]
async fn allow_for_this_chat_stops_that_kind_until_cleared() {
    let (hook, mailbox, state) = gate(ApprovalMode::Ask);
    let running = call(
        &hook,
        "c1",
        "bash",
        json!({ "command": "npm test" }),
        CancellationToken::new(),
    );
    let (result, _) = tokio::join!(running, answer(&mailbox, 1));
    assert!(result.is_none());
    assert_eq!(state.granted(), vec!["command"]);
    assert_eq!(
        state.snapshot(),
        json!({ "mode": "ask", "grants": ["command"] })
    );

    // The next command runs without a prompt…
    let again = call(
        &hook,
        "c2",
        "bash",
        json!({ "command": "npm run build" }),
        CancellationToken::new(),
    )
    .await;
    assert!(again.is_none());
    no_prompt(&mailbox);

    // …but an edit still asks.
    let edit = call(
        &hook,
        "e1",
        "write",
        json!({ "path": "a", "content": "x" }),
        CancellationToken::new(),
    );
    let (result, ui) = tokio::join!(edit, answer(&mailbox, 0));
    assert!(result.is_none());
    assert_eq!(ui["options"][1]["title"], "Allow edits for this chat");

    // Clearing brings the prompts back.
    state.clear_grants();
    assert!(state.granted().is_empty());
    let running = call(
        &hook,
        "c3",
        "bash",
        json!({ "command": "npm test" }),
        CancellationToken::new(),
    );
    let (result, _) = tokio::join!(running, answer(&mailbox, 0));
    assert!(result.is_none());
}

#[tokio::test]
async fn a_mode_change_applies_from_the_next_call() {
    let (hook, mailbox, state) = gate(ApprovalMode::Auto);
    let first = call(
        &hook,
        "c1",
        "bash",
        json!({ "command": "make" }),
        CancellationToken::new(),
    )
    .await;
    assert!(first.is_none());
    no_prompt(&mailbox);
    state.set_mode(ApprovalMode::Ask);
    let second = call(
        &hook,
        "c2",
        "bash",
        json!({ "command": "make" }),
        CancellationToken::new(),
    );
    let (result, _) = tokio::join!(second, answer(&mailbox, 2));
    assert!(result.expect("blocked").block);
}

#[tokio::test]
async fn without_a_ui_a_call_that_needs_approval_fails_clearly() {
    let mailbox = evo_extensions::UiDialogMailbox::new(); // never attached
    let state = Arc::new(ApprovalState::new(ApprovalMode::Ask));
    let hook = before_tool_call(None, mailbox, state).expect("installed");
    let result = call(
        &hook,
        "c",
        "bash",
        json!({ "command": "make" }),
        CancellationToken::new(),
    )
    .await
    .expect("blocked");
    assert!(result.block);
    assert!(result
        .reason
        .unwrap()
        .contains("no one can approve in this mode (use --approval-mode auto)"));
}

#[tokio::test]
async fn stopping_the_turn_closes_the_prompt() {
    let (hook, mailbox, _) = gate(ApprovalMode::Ask);
    let signal = CancellationToken::new();
    let running = call(
        &hook,
        "c",
        "bash",
        json!({ "command": "make" }),
        signal.clone(),
    );
    let stop = async {
        for _ in 0..200 {
            if mailbox.pending_len() > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        signal.cancel();
    };
    let (result, ()) = tokio::join!(running, stop);
    assert!(result.expect("blocked").block);
}

fn previous_blocking() -> evo_agent::BeforeToolCall {
    Arc::new(|_, _| {
        Box::pin(async {
            Some(evo_agent::BeforeToolCallResult {
                block: true,
                reason: Some("Blocked by a hook: no".into()),
                terminate: false,
                args: None,
            })
        })
    })
}

#[tokio::test]
async fn a_block_from_an_earlier_link_wins_even_in_auto() {
    let (hook, mailbox, _) = gate_after(Some(previous_blocking()), ApprovalMode::Auto);
    let result = call(
        &hook,
        "c",
        "bash",
        json!({ "command": "make" }),
        CancellationToken::new(),
    )
    .await
    .expect("blocked");
    assert_eq!(result.reason.as_deref(), Some("Blocked by a hook: no"));
    no_prompt(&mailbox);
}

#[tokio::test]
async fn a_call_a_hook_allowed_does_not_ask_again() {
    let (hook, mailbox, state) = gate(ApprovalMode::Ask);
    state.cover("c");
    let result = call(
        &hook,
        "c",
        "bash",
        json!({ "command": "npm test" }),
        CancellationToken::new(),
    )
    .await;
    assert!(result.is_none());
    no_prompt(&mailbox);
    // Covered once: the same id is not remembered.
    assert!(!state.take_covered("c"));
}

#[tokio::test]
async fn rewritten_arguments_are_what_gets_classified() {
    // A hook that rewrites a command into a read-only one.
    let rewrite: evo_agent::BeforeToolCall = Arc::new(|_, _| {
        Box::pin(async {
            Some(evo_agent::BeforeToolCallResult {
                block: false,
                reason: None,
                terminate: false,
                args: Some(json!({ "command": "git status" })),
            })
        })
    });
    let (hook, mailbox, _) = gate_after(Some(rewrite), ApprovalMode::Ask);
    let result = call(
        &hook,
        "c",
        "bash",
        json!({ "command": "rm -rf x" }),
        CancellationToken::new(),
    )
    .await
    .expect("keeps the rewrite");
    assert_eq!(result.args, Some(json!({ "command": "git status" })));
    no_prompt(&mailbox);
}

#[tokio::test]
async fn tools_the_app_runs_are_left_to_the_app() {
    let (hook, mailbox, state) = gate(ApprovalMode::Ask);
    state.set_external_tools(["browser_open".to_string()]);
    let result = call(
        &hook,
        "c",
        "browser_open",
        json!({ "url": "https://example.com" }),
        CancellationToken::new(),
    )
    .await;
    assert!(result.is_none());
    no_prompt(&mailbox);
}
