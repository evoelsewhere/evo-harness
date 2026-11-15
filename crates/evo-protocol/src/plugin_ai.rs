//! The plugin-AI engine's wire (`evo-engine --mode plugin-ai`; JSONL on
//! stdio): docs/features/plugin-ai/spec.md, plan 3.3; ADR 0023.
//!
//! The shell sends [`AiCommand`]s and reads [`AiOutput`]s. One engine serves
//! many runs at once; every run line carries its `runId`. The desktop shell
//! speaks this JSON without linking this crate (as it does with chat
//! engines); the shapes here are the contract.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A model by provider and id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
}

/// What a plugin may use: `None` fields allow every connected one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Allowed {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<ModelRef>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Fast,
    Balanced,
    Best,
}

/// Models the person pinned for each tier.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Pins {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balanced: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best: Option<ModelRef>,
}

impl Pins {
    pub fn get(&self, tier: Tier) -> Option<&ModelRef> {
        match tier {
            Tier::Fast => self.fast.as_ref(),
            Tier::Balanced => self.balanced.as_ref(),
            Tier::Best => self.best.as_ref(),
        }
    }
}

/// What a run needs of its model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Need {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<Tier>,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub tools: bool,
    /// `off`, `low`, `medium`, `high` (a thinking level).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

/// A part of a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Part {
    Text {
        text: String,
    },
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

/// A message's content: plain text or parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<Part>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// `user` or `assistant`.
    pub role: String,
    pub content: Content,
}

/// Where a tool the model may call runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolOrigin {
    /// A function in the view that started the run.
    View,
    /// A tool of the plugin's backend (or another of its servers).
    Backend,
    /// One of evo's app tools (browser, terminal), run by the shell (FR-16).
    App,
}

/// A tool of the plugin the model may call (FR-10, FR-11). The shell runs it
/// and answers with [`AiCommand::AiToolResult`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "object_schema")]
    pub input_schema: Value,
    pub origin: ToolOrigin,
}

fn object_schema() -> Value {
    serde_json::json!({ "type": "object" })
}

/// The plugin a run is for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginRef {
    pub id: String,
    pub name: String,
}

/// Context a run attaches (FR-17): the shell checks the plugin may see it
/// and gathers the chat; the engine reads the files and the skill.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attached {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<AttachedChat>,
    /// Absolute paths inside places the plugin may read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<AttachedSkill>,
}

/// The open chat's last messages (never an incognito chat).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachedChat {
    pub title: String,
    pub messages: Vec<ChatLine>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatLine {
    pub role: String,
    pub text: String,
}

/// One of the plugin's own skills, as the run's instructions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachedSkill {
    pub name: String,
    /// Its SKILL.md.
    pub path: String,
}

/// One run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    pub run_id: String,
    pub plugin: PluginRef,
    #[serde(default)]
    pub allowed: Allowed,
    /// A model the plugin named (must be allowed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    /// The person's default model for this plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<ModelRef>,
    #[serde(default)]
    pub pins: Pins,
    #[serde(default)]
    pub need: Need,
    #[serde(default)]
    pub budget: Budget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    /// Tools the model may call; the shell runs them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<RemoteTool>,
    /// Model calls at most (default 8, at most 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
    /// Context the shell checked and gathered (FR-17).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Attached>,
    /// Groups of evo's own tools the plugin may use (FR-16): `read`,
    /// `edit`, `command`, `web`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evo_tools: Vec<String>,
    /// The folder evo's tools work in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The person's approval mode now: `ask`, `auto-edits` or `auto`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<String>,
    /// The person's sandbox config (as a chat engine gets it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<Value>,
}

/// Shell → engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum AiCommand {
    /// The models a plugin may use, with abilities, prices and tiers.
    AiModels {
        id: Value,
        #[serde(default)]
        allowed: Allowed,
    },
    AiRun(Box<RunRequest>),
    /// What a tool the engine asked for answered.
    AiToolResult {
        run_id: String,
        call_id: String,
        #[serde(default)]
        result: Value,
        #[serde(default)]
        is_error: bool,
    },
    AiCancel {
        run_id: String,
    },
    /// The person's answer to an `ai_question`: the option's index, or -1.
    AiAnswer {
        id: String,
        index: i32,
    },
    /// Credentials or provider settings changed.
    ReloadProviders,
    Stop,
}

/// A model as plugins see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub provider: String,
    pub model: String,
    pub name: String,
    pub vision: bool,
    pub tools: bool,
    pub reasoning: bool,
    pub context: u64,
    pub max_output: u64,
    /// USD per million tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Price>,
    pub tiers: Vec<Tier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub input: f64,
    pub output: f64,
}

/// Tokens and cost.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotal {
    pub input: i64,
    pub output: i64,
    pub cost_usd: f64,
}

/// Why a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stopped {
    End,
    Steps,
    Budget,
    Cancel,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub usage: UsageTotal,
    pub stopped: Stopped,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// What a run streams.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RunEvent {
    Started {
        provider: String,
        model: String,
        /// The thread's oldest messages left out to fit the model (FR-18).
        #[serde(default, skip_serializing_if = "is_zero")]
        dropped_messages: u32,
        /// Attached context left out or shortened, and why (FR-17).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cut: Vec<String>,
    },
    Text {
        text: String,
    },
    Reasoning {
        text: String,
    },
    Retry {
        from: ModelRef,
        to: ModelRef,
        reason: String,
    },
    /// The model calls one of the plugin's tools.
    ToolCall {
        call_id: String,
        tool: String,
        input: Value,
    },
    /// What it answered (cut to a preview).
    ToolResult {
        call_id: String,
        tool: String,
        is_error: bool,
        preview: String,
    },
    Usage {
        usage: UsageTotal,
    },
    Done {
        result: RunResult,
    },
}

/// Engine → shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum AiOutput {
    Ready {
        protocol: String,
    },
    AiEvent {
        run_id: String,
        event: RunEvent,
    },
    /// Run one of the plugin's tools and answer with `ai_tool_result`.
    AiToolCall {
        run_id: String,
        call_id: String,
        tool: String,
        input: Value,
        origin: ToolOrigin,
    },
    /// Ask the person before one of evo's tools runs; answered with
    /// `ai_answer`.
    AiQuestion {
        run_id: String,
        id: String,
        title: String,
        detail: String,
        options: Vec<String>,
    },
    Response {
        id: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// The `protocol` value of [`AiOutput::Ready`].
pub const PROTOCOL: &str = "evo-plugin-ai/1";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn commands_read_the_shells_json() {
        let run: AiCommand = serde_json::from_value(json!({
            "type": "ai_run", "runId": "r1", "plugin": { "id": "p@m", "name": "p" },
            "allowed": { "providers": ["anthropic"] },
            "need": { "tier": "fast", "vision": true },
            "budget": { "maxCostUsd": 0.02 },
            "system": "Be brief.",
            "messages": [
                { "role": "user", "content": "hi" },
                { "role": "user", "content": [{ "type": "text", "text": "look" }, { "type": "image", "data": "AAA", "mimeType": "image/png" }] }
            ],
            "thread": "AAPL"
        }))
        .unwrap();
        let AiCommand::AiRun(run) = run else { panic!() };
        assert_eq!(run.run_id, "r1");
        assert_eq!(run.need.tier, Some(Tier::Fast));
        assert_eq!(run.budget.max_cost_usd, Some(0.02));
        assert!(matches!(&run.messages[1].content, Content::Parts(parts) if parts.len() == 2));
        assert_eq!(
            serde_json::from_value::<AiCommand>(json!({ "type": "ai_cancel", "runId": "r1" }))
                .unwrap(),
            AiCommand::AiCancel {
                run_id: "r1".into()
            }
        );
        assert_eq!(
            serde_json::from_value::<AiCommand>(json!({ "type": "reload_providers" })).unwrap(),
            AiCommand::ReloadProviders
        );
    }

    #[test]
    fn tools_and_their_results_read_the_shells_json() {
        let with_tools: AiCommand = serde_json::from_value(json!({
            "type": "ai_run", "runId": "r2", "plugin": { "id": "p@m", "name": "p" }, "messages": [{ "role": "user", "content": "x" }],
            "tools": [{ "name": "getPrice", "description": "price", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } } }, "origin": "view" }, { "name": "search", "origin": "backend" }],
            "maxSteps": 5
        }))
        .unwrap();
        let AiCommand::AiRun(run) = with_tools else {
            panic!()
        };
        assert_eq!(run.tools.len(), 2);
        assert_eq!(run.tools[1].input_schema, json!({ "type": "object" }));
        assert_eq!(run.max_steps, Some(5));
        let result: AiCommand = serde_json::from_value(json!({ "type": "ai_tool_result", "runId": "r2", "callId": "c1", "result": { "price": 1 }, "isError": false })).unwrap();
        assert!(matches!(result, AiCommand::AiToolResult { ref call_id, .. } if call_id == "c1"));
    }

    #[test]
    fn events_are_the_shape_the_shell_reads() {
        let done = AiOutput::AiEvent {
            run_id: "r1".into(),
            event: RunEvent::Done {
                result: RunResult {
                    text: "ok".into(),
                    json: None,
                    provider: Some("a".into()),
                    model: Some("m".into()),
                    usage: UsageTotal {
                        input: 3,
                        output: 1,
                        cost_usd: 0.001,
                    },
                    stopped: Stopped::End,
                    error: None,
                },
            },
        };
        assert_eq!(
            serde_json::to_value(&done).unwrap(),
            json!({ "type": "ai_event", "runId": "r1", "event": { "type": "done", "result": { "text": "ok", "provider": "a", "model": "m", "usage": { "input": 3, "output": 1, "costUsd": 0.001 }, "stopped": "end" } } })
        );
        let retry = RunEvent::Retry {
            from: ModelRef {
                provider: "a".into(),
                model: "m".into(),
            },
            to: ModelRef {
                provider: "b".into(),
                model: "n".into(),
            },
            reason: "rate limited".into(),
        };
        assert_eq!(serde_json::to_value(&retry).unwrap()["type"], "retry");
    }
}
