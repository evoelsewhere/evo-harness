//! The approval mode: ask before a tool call that edits files or runs a
//! command, as the person's mode says.
//!
//! Spec: `docs/features/approval-modes/spec.md` (FR-3 – FR-8, FR-12, FR-13,
//! NFR-1, NFR-3); decision: ADR 0010.
//!
//! * Every call is classified as a **read**, an **edit** or a **command**
//!   ([`classify`]). Reads never ask. `ask` asks for edits and commands,
//!   `auto-edits` for commands only, `auto` for none.
//! * The gate is the last link of the `before_tool_call` chain
//!   ([`before_tool_call`]): the guards, `PreToolUse` hooks and plugin handlers
//!   run first and a block from them always wins.
//! * The mode and the chat's "allow for this chat" grants live in one shared
//!   [`ApprovalState`], so `set_approval_mode` applies from the next call, even
//!   in the middle of a run.
//! * Tools the app registers (`configure_external_tools`) are gated by the app,
//!   not here.
//!
//! The prompt goes through the same dialog mailbox as every other approval
//! (`ui_dialog_request`, kind `confirm`).

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

use crate::read_only::{check_command, Shell};

/// How much the person wants to be asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalMode {
    /// Ask for edits and commands.
    #[default]
    Ask,
    /// Edits run, commands ask.
    AutoEdits,
    /// Everything runs.
    Auto,
}

impl ApprovalMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "ask" => Some(Self::Ask),
            "auto-edits" => Some(Self::AutoEdits),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::AutoEdits => "auto-edits",
            Self::Auto => "auto",
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Self::Ask => 0,
            Self::AutoEdits => 1,
            Self::Auto => 2,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Ask,
            1 => Self::AutoEdits,
            _ => Self::Auto,
        }
    }
}

/// What a tool call does to the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Read,
    Edit,
    Command,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Command => "command",
        }
    }
}

/// Tools that only read, talk to the person or to other agents, or show
/// something. A subagent's own calls are checked one by one, so `agent` and
/// `message_agent` pass; a skill's `` !`command` `` lines are governed by
/// Customize → Skills.
const READ_TOOLS: &[&str] = &[
    "read",
    "grep",
    "find",
    "ls",
    "lsp",
    "docs",
    "document_preview",
    "web_search",
    "web_fetch",
    "ask_user",
    "update_todos",
    "send_file",
    "notify",
    "skill",
    "agent",
    "message_agent",
    "image",
    "show_widget",
    "visualize_read_me",
];

/// Tools that change files.
const EDIT_TOOLS: &[&str] = &["write", "edit"];

/// Classify one call. Unknown tools count as commands, so a new tool asks until
/// it is listed here.
pub fn classify(tool: &str, args: &Value) -> Kind {
    match tool {
        "bash" | "powershell" => {
            let shell = if tool == "bash" {
                Shell::Bash
            } else {
                Shell::PowerShell
            };
            let command = args.get("command").and_then(Value::as_str).unwrap_or("");
            if check_command(command, shell).is_ok() {
                Kind::Read
            } else {
                Kind::Command
            }
        }
        "memory" => match args.get("action").and_then(Value::as_str) {
            Some("save" | "delete") => Kind::Edit,
            _ => Kind::Read,
        },
        name if EDIT_TOOLS.contains(&name) => Kind::Edit,
        name if READ_TOOLS.contains(&name) || crate::mesh_tools::TOOL_NAMES.contains(&name) => {
            Kind::Read
        }
        _ => Kind::Command,
    }
}

/// Whether `mode` asks for a call of `kind`, with the chat's grants applied.
fn asks(mode: ApprovalMode, kind: Kind, grants: Grants) -> bool {
    match kind {
        Kind::Read => false,
        Kind::Edit => mode == ApprovalMode::Ask && !grants.edit,
        Kind::Command => mode != ApprovalMode::Auto && !grants.command,
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Grants {
    edit: bool,
    command: bool,
}

/// The mode and the chat's grants, shared by the gate and the RPC commands.
pub struct ApprovalState {
    mode: AtomicU8,
    grant_edit: AtomicBool,
    grant_command: AtomicBool,
    /// Tools the app registered; the app gates them.
    external: Mutex<HashSet<String>>,
    /// Call ids a `PreToolUse` hook already allowed or asked about.
    covered: Mutex<HashSet<String>>,
}

impl ApprovalState {
    pub fn new(mode: ApprovalMode) -> Self {
        Self {
            mode: AtomicU8::new(mode.to_u8()),
            grant_edit: AtomicBool::new(false),
            grant_command: AtomicBool::new(false),
            external: Mutex::new(HashSet::new()),
            covered: Mutex::new(HashSet::new()),
        }
    }

    pub fn mode(&self) -> ApprovalMode {
        ApprovalMode::from_u8(self.mode.load(Ordering::Relaxed))
    }

    pub fn set_mode(&self, mode: ApprovalMode) {
        self.mode.store(mode.to_u8(), Ordering::Relaxed);
    }

    fn grants(&self) -> Grants {
        Grants {
            edit: self.grant_edit.load(Ordering::Relaxed),
            command: self.grant_command.load(Ordering::Relaxed),
        }
    }

    /// The kinds the person allowed for this chat.
    pub fn granted(&self) -> Vec<&'static str> {
        let grants = self.grants();
        let mut kinds = Vec::new();
        if grants.edit {
            kinds.push("edit");
        }
        if grants.command {
            kinds.push("command");
        }
        kinds
    }

    fn grant(&self, kind: Kind) {
        match kind {
            Kind::Edit => self.grant_edit.store(true, Ordering::Relaxed),
            Kind::Command => self.grant_command.store(true, Ordering::Relaxed),
            Kind::Read => {}
        }
    }

    pub fn clear_grants(&self) {
        self.grant_edit.store(false, Ordering::Relaxed);
        self.grant_command.store(false, Ordering::Relaxed);
    }

    /// The names of the tools the app runs itself.
    pub fn set_external_tools<I: IntoIterator<Item = String>>(&self, names: I) {
        *self.external.lock().unwrap() = names.into_iter().collect();
    }

    fn is_external(&self, tool: &str) -> bool {
        self.external.lock().unwrap().contains(tool)
    }

    /// A `PreToolUse` hook allowed or asked about this call: the mode does not
    /// ask again (FR-13).
    pub fn cover(&self, tool_call_id: &str) {
        let mut covered = self.covered.lock().unwrap();
        if covered.len() > 1024 {
            covered.clear();
        }
        covered.insert(tool_call_id.to_string());
    }

    fn take_covered(&self, tool_call_id: &str) -> bool {
        self.covered.lock().unwrap().remove(tool_call_id)
    }

    /// `{ "mode": "ask", "grants": ["command"] }`, the shape of `get_state`'s
    /// `approval` field.
    pub fn snapshot(&self) -> Value {
        json!({ "mode": self.mode().as_str(), "grants": self.granted() })
    }
}

/// This engine's state. It starts in `auto`; the host sets the mode from
/// `--approval-mode` before any session is built.
pub fn global() -> &'static Arc<ApprovalState> {
    static STATE: OnceLock<Arc<ApprovalState>> = OnceLock::new();
    STATE.get_or_init(|| Arc::new(ApprovalState::new(ApprovalMode::Auto)))
}

/// The mode an engine starts in (FR-2, FR-12): the `--approval-mode` it was
/// given, else `ask` for the RPC host (the desktop) and `auto` without a UI,
/// where there is no person to ask and today's behaviour is kept.
pub fn initial_mode(flag: Option<&str>, run: crate::args::Mode) -> Result<ApprovalMode, String> {
    match flag {
        Some(text) => ApprovalMode::parse(text).ok_or_else(|| {
            format!("--approval-mode: expected ask, auto-edits or auto, got `{text}`")
        }),
        None if run == crate::args::Mode::Rpc => Ok(ApprovalMode::Ask),
        None => Ok(ApprovalMode::Auto),
    }
}

// ── Prompt ──────────────────────────────────────────────────────────────────

/// Lines to keep in a prompt's description.
const SUMMARY_CHARS: usize = 2000;

fn clip(text: &str) -> String {
    if text.chars().count() <= SUMMARY_CHARS {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(SUMMARY_CHARS).collect();
    clipped.push('…');
    clipped
}

fn line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.lines().count().max(1)
    }
}

/// `+added −removed` lines of an `edit` call (or its legacy top-level form).
fn edit_delta(args: &Value) -> (usize, usize) {
    let mut added = 0;
    let mut removed = 0;
    let mut count = |old: Option<&str>, new: Option<&str>| {
        removed += line_count(old.unwrap_or(""));
        added += line_count(new.unwrap_or(""));
    };
    match args.get("edits").and_then(Value::as_array) {
        Some(edits) => {
            for edit in edits {
                count(
                    edit.get("oldText").and_then(Value::as_str),
                    edit.get("newText").and_then(Value::as_str),
                );
            }
        }
        None => count(
            args.get("oldText").and_then(Value::as_str),
            args.get("newText").and_then(Value::as_str),
        ),
    }
    (added, removed)
}

/// What the prompt says: the title and the one-line summary.
fn describe(tool: &str, kind: Kind, args: &Value) -> (String, String) {
    let path = args.get("path").and_then(Value::as_str).unwrap_or("a file");
    match (tool, kind) {
        ("bash" | "powershell", _) => (
            "Run a command?".into(),
            clip(args.get("command").and_then(Value::as_str).unwrap_or("")),
        ),
        ("write", _) => {
            let lines = line_count(args.get("content").and_then(Value::as_str).unwrap_or(""));
            (
                "Write file?".into(),
                format!("{path} ({lines} line{})", if lines == 1 { "" } else { "s" }),
            )
        }
        ("edit", _) => {
            let (added, removed) = edit_delta(args);
            ("Edit file?".into(), format!("{path} (+{added} −{removed})"))
        }
        (_, Kind::Edit) => (format!("Allow {tool}?"), clip(&args.to_string())),
        _ => (format!("Run {tool}?"), clip(&args.to_string())),
    }
}

enum Answer {
    Allow,
    AllowChat,
    Deny,
    /// No one can answer (headless).
    NoUi,
}

async fn ask(
    ui_dialog: &evo_extensions::UiDialogMailbox,
    tool_call_id: &str,
    tool: &str,
    kind: Kind,
    args: &Value,
    signal: &tokio_util::sync::CancellationToken,
) -> Answer {
    let (title, description) = describe(tool, kind, args);
    let request_id = format!("approve:{tool_call_id}");
    let noun = match kind {
        Kind::Edit => "edits",
        _ => "commands",
    };
    let opened = ui_dialog.handle(json!({
        "op": "open",
        "requestId": request_id,
        "toolCallId": tool_call_id,
        "ui": {
            "kind": "confirm",
            "title": title,
            "description": description,
            "approval": { "kind": kind.as_str(), "tool": tool },
            "options": [
                { "title": "Allow" },
                { "title": format!("Allow {noun} for this chat"), "permission": "allow-chat" },
                { "title": "Deny", "permission": "reject" },
            ],
        },
    }));
    if opened.is_err() {
        return Answer::NoUi;
    }
    loop {
        if signal.is_cancelled() {
            let _ = ui_dialog.cancel(&request_id);
            return Answer::Deny;
        }
        match ui_dialog.poll(&request_id) {
            Ok(state) => match state.get("status").and_then(Value::as_str) {
                Some("answered") => {
                    let answer = state.get("answer").cloned().unwrap_or(Value::Null);
                    let index = answer.get("index").and_then(Value::as_u64);
                    let value = answer.get("value").and_then(Value::as_str).unwrap_or("");
                    return match index {
                        Some(0) => Answer::Allow,
                        Some(1) => Answer::AllowChat,
                        Some(_) => Answer::Deny,
                        None if value.eq_ignore_ascii_case("allow") => Answer::Allow,
                        None => Answer::Deny,
                    };
                }
                Some("cancelled") => return Answer::Deny,
                _ => {}
            },
            Err(_) => return Answer::Deny,
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn blocked(reason: String) -> Option<evo_agent::BeforeToolCallResult> {
    Some(evo_agent::BeforeToolCallResult {
        block: true,
        reason: Some(reason),
        terminate: false,
        args: None,
    })
}

/// The approval gate, behind `previous` (the guards, hooks and plugin
/// handlers): it runs after them and never overrides a block of theirs.
pub fn before_tool_call(
    previous: Option<evo_agent::BeforeToolCall>,
    ui_dialog: evo_extensions::UiDialogMailbox,
    state: Arc<ApprovalState>,
) -> Option<evo_agent::BeforeToolCall> {
    Some(Arc::new(
        move |ctx: evo_agent::BeforeToolCallContext<'_>, signal| {
            let tool = ctx.tool_call.name.clone();
            let id = ctx.tool_call.id.clone();
            let args = ctx.args.clone();
            let earlier = previous.as_ref().map(|hook| hook(ctx, signal.clone()));
            let ui_dialog = ui_dialog.clone();
            let state = state.clone();
            Box::pin(async move {
                let inner = match earlier {
                    Some(earlier) => earlier.await,
                    None => None,
                };
                if inner.as_ref().is_some_and(|result| result.block) {
                    return inner;
                }
                // A hook already allowed or asked: one prompt per call.
                let covered = state.take_covered(&id);
                if covered || state.is_external(&tool) {
                    return inner;
                }
                // What will run: a hook may have rewritten the arguments.
                let args = inner
                    .as_ref()
                    .and_then(|result| result.args.clone())
                    .unwrap_or(args);
                let kind = classify(&tool, &args);
                if !asks(state.mode(), kind, state.grants()) {
                    return inner;
                }
                match ask(&ui_dialog, &id, &tool, kind, &args, &signal).await {
                    Answer::Allow => inner,
                    Answer::AllowChat => {
                        state.grant(kind);
                        inner
                    }
                    Answer::Deny => blocked(format!("The user declined this {}.", kind.as_str())),
                    Answer::NoUi => blocked(
                        "Not run: approval needed and no one can approve in this mode \
                         (use --approval-mode auto)."
                            .into(),
                    ),
                }
            })
        },
    ))
}

#[cfg(test)]
mod tests;
