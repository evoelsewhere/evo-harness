//! Shared wire protocol for evo's headless RPC mode (`--mode rpc`, the server)
//! and the remote client (`--connect`).
//!
//! Single source of truth: the server serializes these types and the client
//! deserializes them, so the contract cannot drift between the two ends.
//!
//! Wire shape (one JSON object per line):
//! - the server opens with `{"type":"ready"}`, then
//! - streams [`RemoteEvent`]s (`{"type":"agent_start"}`, `{"type":"message_update",…}`, …), and
//! - answers each [`RemoteCommand`] with a [`RemoteResponse`] (`{"type":"response",…}`).
//!
//! These are intentionally **not** the core `evo-agent`/`evo-ai` types: `AgentEvent`
//! is documented as process-local (see `evo-agent/src/events.rs`) and deliberately
//! has no serde impl. This module projects the process-local events into a
//! stable, wire-safe shape, and translates back only the subset the client needs.
//! That keeps the core crates untouched and the client free of local resources.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use evo_agent::events::AgentEvent;
use evo_agent::types::AgentToolResult;

/// A fine-grained lifecycle event streamed from the server to the client.
///
/// Mirrors the shape produced by the previous hand-rolled projection in
/// `modes.rs`; kept field-for-field compatible so existing clients keep working.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RemoteEvent {
    UiDialogRequest {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "toolCallId", default)]
        tool_call_id: Option<String>,
        ui: Value,
    },
    UiDialogClosed {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    AgentStart,
    AgentEnd {
        #[serde(rename = "messageCount")]
        message_count: usize,
        messages: Value,
    },
    RetryScheduled {
        attempt: u32,
        #[serde(rename = "maxRetries")]
        max_retries: u32,
        #[serde(rename = "delayMs")]
        delay_ms: u64,
        error: String,
    },
    TurnStart,
    TurnEnd {
        message: Value,
        #[serde(rename = "toolResultCount")]
        tool_result_count: usize,
        #[serde(rename = "toolResults")]
        tool_results: Value,
        /// Queue depth when this turn settled, so a client can render
        /// "N queued". Defaulted for wire compatibility with older peers.
        #[serde(default, rename = "pendingMessages")]
        pending_messages: usize,
    },
    MessageStart {
        #[serde(rename = "messageId", default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        #[serde(default)]
        message: Value,
    },
    MessageEnd {
        #[serde(rename = "messageId", default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        #[serde(default)]
        message: Value,
    },
    MessageUpdate {
        #[serde(rename = "messageId", default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        #[serde(default)]
        message: Value,
        #[serde(rename = "assistantMessageEvent", default)]
        assistant_message_event: Value,
        #[serde(rename = "eventType", default)]
        event_type: String,
        /// Convenience mirror of the delta's content index (text/thinking/tool-call
        /// deltas only), so the client can apply deltas without re-parsing the
        /// nested event.
        #[serde(
            rename = "contentIndex",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        content_index: Option<u64>,
        /// Convenience mirror of the appended text for text/thinking deltas.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delta: Option<String>,
    },
    ToolExecutionStart {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(default)]
        args: Value,
    },
    ToolExecutionUpdate {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(default)]
        args: Value,
        #[serde(rename = "partialResult")]
        partial_result: Value,
    },
    ToolExecutionEnd {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(rename = "isError")]
        is_error: bool,
        result: Value,
    },
}

impl RemoteEvent {
    /// The wire `type` tag (kept in sync with the serde `rename_all`).
    pub fn type_tag(&self) -> &'static str {
        match self {
            RemoteEvent::UiDialogRequest { .. } => "ui_dialog_request",
            RemoteEvent::UiDialogClosed { .. } => "ui_dialog_closed",
            RemoteEvent::AgentStart => "agent_start",
            RemoteEvent::AgentEnd { .. } => "agent_end",
            RemoteEvent::RetryScheduled { .. } => "retry_scheduled",
            RemoteEvent::TurnStart => "turn_start",
            RemoteEvent::TurnEnd { .. } => "turn_end",
            RemoteEvent::MessageStart { .. } => "message_start",
            RemoteEvent::MessageUpdate { .. } => "message_update",
            RemoteEvent::MessageEnd { .. } => "message_end",
            RemoteEvent::ToolExecutionStart { .. } => "tool_execution_start",
            RemoteEvent::ToolExecutionUpdate { .. } => "tool_execution_update",
            RemoteEvent::ToolExecutionEnd { .. } => "tool_execution_end",
        }
    }

    /// True for the terminal `AgentEnd` event.
    pub fn is_terminal(&self) -> bool {
        matches!(self, RemoteEvent::AgentEnd { .. })
    }

    /// Project a process-local [`AgentEvent`] onto the wire shape.
    pub fn from_agent_event(event: &AgentEvent) -> Self {
        use evo_ai::types::AssistantMessageEvent;
        match event {
            AgentEvent::AgentStart => RemoteEvent::AgentStart,
            AgentEvent::AgentEnd { messages } => RemoteEvent::AgentEnd {
                message_count: messages.len(),
                messages: serde_json::to_value(messages).unwrap_or(Value::Null),
            },
            AgentEvent::RetryScheduled {
                attempt,
                max_retries,
                delay_ms,
                error,
            } => RemoteEvent::RetryScheduled {
                attempt: *attempt,
                max_retries: *max_retries,
                delay_ms: *delay_ms,
                error: error.clone(),
            },
            AgentEvent::TurnStart => RemoteEvent::TurnStart,
            AgentEvent::TurnEnd {
                message,
                tool_results,
                pending_messages,
                ..
            } => RemoteEvent::TurnEnd {
                message: serde_json::to_value(message).unwrap_or(Value::Null),
                tool_result_count: tool_results.len(),
                tool_results: serde_json::to_value(tool_results).unwrap_or(Value::Null),
                pending_messages: *pending_messages,
            },
            AgentEvent::MessageStart {
                message,
                message_id,
            } => RemoteEvent::MessageStart {
                message_id: message_id.clone(),
                message: serde_json::to_value(message).unwrap_or(Value::Null),
            },
            AgentEvent::MessageEnd {
                message,
                message_id,
            } => RemoteEvent::MessageEnd {
                message_id: message_id.clone(),
                message: serde_json::to_value(message).unwrap_or(Value::Null),
            },
            AgentEvent::MessageUpdate {
                message_id,
                message,
                assistant_message_event,
            } => {
                let (content_index, delta) = match assistant_message_event {
                    AssistantMessageEvent::TextDelta {
                        content_index,
                        delta,
                        ..
                    }
                    | AssistantMessageEvent::ThinkingDelta {
                        content_index,
                        delta,
                        ..
                    }
                    | AssistantMessageEvent::ToolCallDelta {
                        content_index,
                        delta,
                        ..
                    } => (Some(*content_index as u64), Some(delta.clone())),
                    _ => (None, None),
                };
                RemoteEvent::MessageUpdate {
                    message_id: message_id.clone(),
                    // `assistant_json` keeps this identical to the
                    // `AgentMessage` encoding without copying the message.
                    message: evo_agent::message::assistant_json(message),
                    assistant_message_event: serde_json::to_value(assistant_message_event)
                        .unwrap_or(Value::Null),
                    event_type: assistant_message_event.type_tag().to_string(),
                    content_index,
                    delta,
                }
            }
            AgentEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
            } => RemoteEvent::ToolExecutionStart {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                args: args.clone(),
            },
            AgentEvent::ToolExecutionUpdate {
                tool_call_id,
                tool_name,
                args,
                partial_result,
            } => RemoteEvent::ToolExecutionUpdate {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                args: args.clone(),
                partial_result: tool_result_json(partial_result),
            },
            AgentEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
            } => RemoteEvent::ToolExecutionEnd {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                is_error: *is_error,
                result: tool_result_json(result),
            },
        }
    }
}

/// Serialize a tool result into the stable wire shape (content blocks, details,
/// usage, and the batch hints).
pub fn tool_result_json(result: &AgentToolResult) -> Value {
    serde_json::to_value(result)
        .expect("AgentToolResult contains only JSON-serializable native content, details and usage")
}

/// A command sent from the client to the server (`{"id":…,"type":…,…}`).
///
/// The `id` is attached by the client when sending (see
/// [`with_id`](RemoteCommand::with_id)) so responses can be correlated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RemoteCommand {
    UiDialogResponse {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answer: Option<Value>,
        #[serde(default)]
        cancelled: bool,
    },
    /// Run a prompt on the remote `main` lane; events stream until `agent_end`.
    Prompt { content: String },
    /// Abort the in-flight run.
    Abort,
    /// Fetch the current session state (model, thinking, tools, ids).
    GetState,
    /// Switch the active model by id.
    SetModel { model: String },
    /// Set the thinking level (`off`…`max`).
    SetThinkingLevel { level: String },
    /// Replace the active tool set.
    SetActiveTools { tools: Vec<String> },
    /// Liveness probe.
    Ping,
    /// Ask the server to end the session loop.
    Stop,
}

impl RemoteCommand {
    /// Serialize the command with an `id` field for request/response correlation.
    pub fn with_id(&self, id: impl Into<Value>) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Some(object) = value.as_object_mut() {
            object.insert("id".to_string(), id.into());
        }
        value
    }
}

/// Response status for a [`RemoteResponse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Ok,
    Error,
}

/// The server's answer to a single [`RemoteCommand`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteResponse {
    #[serde(default)]
    pub id: Value,
    pub status: ResponseStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RemoteResponse {
    /// A successful response carrying an optional result payload.
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            id,
            status: ResponseStatus::Ok,
            result: Some(result),
            error: None,
        }
    }

    /// A failed response carrying an error message.
    pub fn error(id: Value, error: impl Into<String>) -> Self {
        Self {
            id,
            status: ResponseStatus::Error,
            result: None,
            error: Some(error.into()),
        }
    }

    /// True when the request succeeded.
    pub fn is_ok(&self) -> bool {
        matches!(self.status, ResponseStatus::Ok)
    }

    /// Serialize to the wire shape, adding the `"type":"response"` tag.
    pub fn to_json(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Some(object) = value.as_object_mut() {
            object.insert("type".to_string(), Value::String("response".to_string()));
        }
        value
    }
}

/// The `get_state` result payload, as a typed view over the response.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(rename = "thinkingLevel", default)]
    pub thinking_level: Option<Value>,
    #[serde(rename = "activeTools", default)]
    pub active_tools: Vec<String>,
    #[serde(rename = "leafId", default)]
    pub leaf_id: Option<String>,
    #[serde(rename = "sessionId", default)]
    pub session_id: Option<String>,
}

/// The `prompt` result payload (`{outcome, finalText}`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptOutcome {
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(rename = "finalText", default)]
    pub final_text: Option<String>,
}

// ── Turn checkpoints and the commit-message draft ──────────────────────────
//
// docs/features/source-control/plan.md, 3.3 (spec FR-7, FR-9 – FR-13).
// Commands: `list_checkpoints`, `revert_turn`, `draft_commit_message`. A
// top-level run in a repository adds `checkpoint` to its `agent_settled`
// line, and the same object is kept as a session entry of custom type
// [`CHECKPOINT_ENTRY`]; a revert appends the object again with `reverted`
// set (the newest entry of an id wins).

/// The custom type of the session entry that holds a [`Checkpoint`].
pub const CHECKPOINT_ENTRY: &str = "checkpoint";

/// The error `revert_turn` answers while a run is active (FR-13).
pub const REVERT_BUSY: &str = "busy";

/// The reason a file was left as it is by `revert_turn`.
pub const SKIPPED_CHANGED_SINCE: &str = "changed-since";

/// The id of turn `seq`'s checkpoint: `c-<seq>`.
pub fn checkpoint_id(seq: u64) -> String {
    format!("c-{seq}")
}

/// The id that undoes the revert of turn `seq`: `c-<seq>-undo`.
pub fn checkpoint_undo_id(seq: u64) -> String {
    format!("c-{seq}-undo")
}

/// `c-17` → `(17, false)`, `c-17-undo` → `(17, true)`.
pub fn parse_checkpoint_id(id: &str) -> Option<(u64, bool)> {
    let rest = id.strip_prefix("c-")?;
    let (number, undo) = match rest.strip_suffix("-undo") {
        Some(number) => (number, true),
        None => (rest, false),
    };
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number.parse().ok().map(|seq| (seq, undo))
}

/// One top-level turn's checkpoint: the files it changed between the
/// snapshots taken when it started and when it settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    /// `c-<seq>`.
    pub id: String,
    /// The turn's number in its chat (kept across engine restarts).
    pub seq: u64,
    /// The session entry of the turn's prompt, to place it in the transcript.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_entry_id: Option<String>,
    #[serde(default)]
    pub run_id: String,
    /// Milliseconds since the epoch.
    pub started_at: i64,
    /// The commit of the start snapshot (for a turn's diff).
    pub start: String,
    /// The commit of the end snapshot; absent while the turn is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    #[serde(default)]
    pub files: Vec<CheckpointFile>,
    /// `false`: the start snapshot was late or the turn never settled, so
    /// changes made before it may be missing.
    pub complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverted: Option<CheckpointRevert>,
}

/// A file a turn changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointFile {
    pub path: String,
    /// `added`, `modified`, `deleted` or `renamed`.
    pub status: String,
    /// A renamed file's old path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Lines added and removed; absent for a binary file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<u64>,
}

/// A revert of the turn that can still be undone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointRevert {
    /// Milliseconds since the epoch.
    pub at: i64,
    /// The id that undoes it (`c-<seq>-undo`).
    pub undo: String,
    /// The files it put back (an undo restores these).
    #[serde(default)]
    pub paths: Vec<String>,
}

/// `list_checkpoints` → the chat's kept checkpoints, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListCheckpointsResult {
    pub checkpoints: Vec<Checkpoint>,
}

/// `revert_turn`: put the turn's files back (`checkpoint` is `c-<seq>`), or
/// undo that revert (`c-<seq>-undo`). `force` reverts files changed since;
/// `paths` narrows it to some of the turn's files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevertTurnRequest {
    pub checkpoint: String,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub paths: Option<Vec<String>>,
}

/// What `revert_turn` did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevertTurnResult {
    pub reverted: Vec<String>,
    pub skipped: Vec<SkippedFile>,
    /// The id that undoes this revert; `null` after an undo or when nothing
    /// was reverted.
    pub undo: Option<String>,
}

/// A file `revert_turn` left as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedFile {
    pub path: String,
    /// [`SKIPPED_CHANGED_SINCE`].
    pub reason: String,
}

/// `draft_commit_message`: the staged diff (at most 64 KiB is read) and the
/// repository's recent commit subjects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftCommitMessageRequest {
    pub diff: String,
    #[serde(default)]
    pub subjects: Vec<String>,
}

/// The drafted message, for the person to edit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftCommitMessageResult {
    pub message: String,
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn checkpoint_ids_round_trip() {
        assert_eq!(checkpoint_id(17), "c-17");
        assert_eq!(checkpoint_undo_id(17), "c-17-undo");
        assert_eq!(parse_checkpoint_id("c-17"), Some((17, false)));
        assert_eq!(parse_checkpoint_id("c-17-undo"), Some((17, true)));
        for bad in ["c-", "c--1", "17", "c-1x", "c-undo", "c-+1"] {
            assert_eq!(parse_checkpoint_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn checkpoint_wire_shape_matches_the_plan() {
        let checkpoint = Checkpoint {
            id: "c-2".into(),
            seq: 2,
            prompt_entry_id: Some("e1".into()),
            run_id: "r1".into(),
            started_at: 5,
            start: "aaa".into(),
            end: Some("bbb".into()),
            files: vec![
                CheckpointFile {
                    path: "src/a.ts".into(),
                    status: "renamed".into(),
                    from: Some("src/old.ts".into()),
                    added: Some(1),
                    removed: Some(0),
                },
                CheckpointFile {
                    path: "logo.png".into(),
                    status: "added".into(),
                    from: None,
                    added: None,
                    removed: None,
                },
            ],
            complete: true,
            reverted: Some(CheckpointRevert {
                at: 9,
                undo: "c-2-undo".into(),
                paths: vec!["src/a.ts".into()],
            }),
        };
        let value = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(
            value,
            json!({
                "id": "c-2", "seq": 2, "promptEntryId": "e1", "runId": "r1",
                "startedAt": 5, "start": "aaa", "end": "bbb",
                "files": [
                    {"path": "src/a.ts", "status": "renamed", "from": "src/old.ts", "added": 1, "removed": 0},
                    {"path": "logo.png", "status": "added"}
                ],
                "complete": true,
                "reverted": {"at": 9, "undo": "c-2-undo", "paths": ["src/a.ts"]}
            })
        );
        assert_eq!(
            serde_json::from_value::<Checkpoint>(value).unwrap(),
            checkpoint
        );
    }

    #[test]
    fn command_payloads_parse_with_their_defaults() {
        let revert: RevertTurnRequest =
            serde_json::from_value(json!({"checkpoint": "c-1", "paths": null})).unwrap();
        assert_eq!(
            revert,
            RevertTurnRequest {
                checkpoint: "c-1".into(),
                force: false,
                paths: None
            }
        );
        let result = RevertTurnResult {
            reverted: vec!["a".into()],
            skipped: vec![SkippedFile {
                path: "b".into(),
                reason: SKIPPED_CHANGED_SINCE.into(),
            }],
            undo: None,
        };
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"reverted": ["a"], "skipped": [{"path": "b", "reason": "changed-since"}], "undo": null})
        );
        let draft: DraftCommitMessageRequest =
            serde_json::from_value(json!({"diff": "d"})).unwrap();
        assert!(draft.subjects.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_agent::types::AgentToolResult;

    fn signed_native_result() -> AgentToolResult {
        use evo_agent::types::TextContentOrImage;
        AgentToolResult {
            content: vec![
                TextContentOrImage::Text(
                    serde_json::from_value(serde_json::json!({
                        "type":"text", "text":"signed 界😀", "textSignature":
                            "{ \"version\": 1, \"id\": \"sig-😀\" }",
                    }))
                    .unwrap(),
                ),
                TextContentOrImage::Image(
                    serde_json::from_value(serde_json::json!({
                        "type":"image", "data":"AAEC+/==", "mimeType":"image/png",
                    }))
                    .unwrap(),
                ),
            ],
            details: serde_json::json!({"runtimeKey":"business", "runtimeHandle":{"id":7},
                "__evo":{"owner":"customer"}, "__evoRuntimeKey":"unchanged"}),
            usage: Some(evo_ai::types::Usage::zero()),
            added_tool_names: vec!["read".into(), "read".into()],
            terminate: true,
        }
    }

    #[test]
    fn tool_update_preserves_canonical_native_result_and_text_signature() {
        let result = signed_native_result();
        let expected = serde_json::to_value(&result).unwrap();
        let event = AgentEvent::ToolExecutionUpdate {
            tool_call_id: "call".into(),
            tool_name: "read".into(),
            args: serde_json::json!({"path":"file"}),
            partial_result: result.into(),
        };
        let wire = serde_json::to_value(RemoteEvent::from_agent_event(&event)).unwrap();
        assert_eq!(wire["type"], "tool_execution_update");
        assert_eq!(wire["partialResult"], expected);
        assert_eq!(
            wire["partialResult"]["content"][0]["textSignature"],
            "{ \"version\": 1, \"id\": \"sig-😀\" }"
        );
        assert!(wire["partialResult"].get("isError").is_none());
    }

    #[test]
    fn tool_end_preserves_canonical_native_result_and_separate_error_authority() {
        let result = signed_native_result();
        let expected = serde_json::to_value(&result).unwrap();
        let event = AgentEvent::ToolExecutionEnd {
            tool_call_id: "call".into(),
            tool_name: "read".into(),
            result,
            is_error: true,
        };
        let wire = serde_json::to_value(RemoteEvent::from_agent_event(&event)).unwrap();
        assert_eq!(wire["type"], "tool_execution_end");
        assert_eq!(wire["result"], expected);
        assert!(wire["result"]["content"][0]["textSignature"].is_string());
        assert_eq!(wire["isError"], true);
        assert!(wire["result"].get("isError").is_none());
        let plain = tool_result_json(&AgentToolResult::text("unsigned"));
        assert!(plain["content"][0].get("textSignature").is_none());
        assert!(plain["usage"].is_null());
    }

    /// The queue depth must survive the native remote projection: a client that
    /// never sees it cannot render the "N queued" indicator.
    #[test]
    fn turn_end_projects_pending_messages() {
        let event = AgentEvent::TurnEnd {
            message: evo_agent::AgentMessage::Assistant(Box::new(
                evo_ai::types::AssistantMessage::empty(
                    evo_ai::types::Api::AnthropicMessages,
                    "p",
                    "m",
                    0,
                ),
            )),
            tool_results: vec![],
            pending_messages: 3,
        };
        let projected = RemoteEvent::from_agent_event(&event);
        assert_eq!(projected.type_tag(), "turn_end");
        let wire = serde_json::to_value(&projected).unwrap();
        assert_eq!(wire["pendingMessages"], 3, "{wire}");

        // Zero is a real value, not an absent field.
        let zero = AgentEvent::TurnEnd {
            message: evo_agent::AgentMessage::Assistant(Box::new(
                evo_ai::types::AssistantMessage::empty(
                    evo_ai::types::Api::AnthropicMessages,
                    "p",
                    "m",
                    0,
                ),
            )),
            tool_results: vec![],
            pending_messages: 0,
        };
        let wire = serde_json::to_value(RemoteEvent::from_agent_event(&zero)).unwrap();
        assert_eq!(wire["pendingMessages"], 0, "{wire}");

        // A peer that predates the field still deserializes.
        let legacy: RemoteEvent = serde_json::from_value(serde_json::json!({
            "type":"turn_end", "message":null, "toolResultCount":0, "toolResults":[],
        }))
        .unwrap();
        assert!(matches!(
            legacy,
            RemoteEvent::TurnEnd {
                pending_messages: 0,
                ..
            }
        ));
    }

    #[test]
    fn dialog_wire_shapes_round_trip_with_request_ids() {
        let event: RemoteEvent = serde_json::from_value(serde_json::json!({
            "type":"ui_dialog_request", "requestId":"q", "toolCallId":"c",
            "ui":{"question":"Allow?", "options":["Yes","No"]},
        }))
        .unwrap();
        assert_eq!(event.type_tag(), "ui_dialog_request");
        assert_eq!(serde_json::to_value(&event).unwrap()["requestId"], "q");
        let answer = RemoteCommand::UiDialogResponse {
            request_id: "q".into(),
            answer: Some(serde_json::json!({"values":["No"]})),
            cancelled: false,
        }
        .with_id(7);
        assert_eq!(answer["type"], "ui_dialog_response");
        assert_eq!(answer["requestId"], "q");
        assert_eq!(answer["answer"]["values"][0], "No");
        let cancelled = RemoteCommand::UiDialogResponse {
            request_id: "q".into(),
            answer: None,
            cancelled: true,
        }
        .with_id(8);
        assert_eq!(cancelled["cancelled"], true);
        assert!(cancelled.get("answer").is_none());
        assert!(
            matches!(serde_json::from_value::<RemoteEvent>(serde_json::json!({
            "type":"ui_dialog_closed", "requestId":"q",
        })).unwrap(), RemoteEvent::UiDialogClosed { request_id } if request_id == "q")
        );
    }

    #[test]
    fn event_projection_matches_wire_shape() {
        let end = serde_json::to_value(RemoteEvent::from_agent_event(&AgentEvent::AgentEnd {
            messages: vec![],
        }))
        .unwrap();
        assert_eq!(end["type"], "agent_end");
        assert_eq!(end["messages"], serde_json::json!([]));
        assert_eq!(end["messageCount"], 0);

        let tool = serde_json::to_value(RemoteEvent::from_agent_event(
            &AgentEvent::ToolExecutionEnd {
                tool_call_id: "call-1".into(),
                tool_name: "read".into(),
                result: AgentToolResult::text("hello"),
                is_error: false,
            },
        ))
        .unwrap();
        assert_eq!(tool["type"], "tool_execution_end");
        assert_eq!(tool["result"]["content"][0]["text"], "hello");
        assert_eq!(tool["result"]["terminate"], false);

        let retry =
            serde_json::to_value(RemoteEvent::from_agent_event(&AgentEvent::RetryScheduled {
                attempt: 3,
                max_retries: 10,
                delay_ms: 8_000,
                error: "503 service unavailable".into(),
            }))
            .unwrap();
        assert_eq!(retry["type"], "retry_scheduled");
        assert_eq!(retry["attempt"], 3);
        assert_eq!(retry["maxRetries"], 10);
        assert_eq!(retry["delayMs"], 8_000);
    }

    #[test]
    fn native_message_ids_round_trip_and_legacy_messages_remain_optional() {
        for kind in ["message_start", "message_update", "message_end"] {
            let legacy = serde_json::json!({"type": kind, "message": {"role":"assistant"}});
            let parsed: RemoteEvent = serde_json::from_value(legacy).unwrap();
            assert!(serde_json::to_value(parsed)
                .unwrap()
                .get("messageId")
                .is_none());
            let value = serde_json::json!({"type": kind, "messageId": "native-\"🐈", "message": {"role":"assistant"}});
            let parsed: RemoteEvent = serde_json::from_value(value).unwrap();
            assert_eq!(
                serde_json::to_value(parsed).unwrap()["messageId"],
                "native-\"🐈"
            );
        }
    }

    #[test]
    fn events_round_trip_over_the_wire() {
        for event in [
            RemoteEvent::AgentStart,
            RemoteEvent::MessageUpdate {
                message_id: Some("entry-1".into()),
                message: serde_json::json!({"role": "assistant"}),
                assistant_message_event: serde_json::json!({"type": "text_delta"}),
                event_type: "text_delta".into(),
                content_index: Some(0),
                delta: Some("hi".into()),
            },
            RemoteEvent::ToolExecutionEnd {
                tool_call_id: "c1".into(),
                tool_name: "read".into(),
                is_error: true,
                result: tool_result_json(&AgentToolResult::text("nope")),
            },
        ] {
            let json = serde_json::to_value(&event).unwrap();
            let back: RemoteEvent = serde_json::from_value(json).unwrap();
            assert_eq!(event, back);
        }
    }

    #[test]
    fn command_and_response_round_trip() {
        let cmd = RemoteCommand::Prompt {
            content: "hi".into(),
        };
        let value = cmd.with_id("r1");
        assert_eq!(value["type"], "prompt");
        assert_eq!(value["content"], "hi");
        assert_eq!(value["id"], "r1");

        let response =
            RemoteResponse::ok(serde_json::json!("r1"), serde_json::json!({"pong": true}));
        let json = response.to_json();
        assert_eq!(json["type"], "response");
        assert_eq!(json["id"], "r1");
        assert_eq!(json["status"], "ok");
        let parsed: RemoteResponse = serde_json::from_value(json).unwrap();
        assert!(parsed.is_ok());

        let err = RemoteResponse::error(Value::Null, "boom").to_json();
        assert_eq!(err["status"], "error");
        assert_eq!(err["error"], "boom");
    }
}
