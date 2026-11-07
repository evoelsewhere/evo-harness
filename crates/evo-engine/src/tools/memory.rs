//! The `memory` tool: save, read, list and delete the agent's memories. The
//! storage, the index and the prompt block are in [`crate::memory`].
//!
//! Spec: `docs/features/agent-tools-layers/spec.md` (FR-2).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use evo_agent::agent_tool::AgentTool;
use evo_agent::error::AgentError;
use evo_agent::types::{AgentToolResult, TextContentOrImage, ToolExecutionMode, ToolResultPartial};
use evo_ai::types::{Schema, Tool};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::memory::{
    delete_in, dir, enabled, entry, find_in, index_lines, list_dir, save_in, Scope, TYPES,
};

pub struct MemoryTool {
    schema: Tool,
    cwd: PathBuf,
}

pub fn create_memory_tool(cwd: PathBuf) -> Arc<dyn AgentTool> {
    Arc::new(MemoryTool {
        schema: Tool {
            name: "memory".into(),
            description: "Your memory across chats (see # Memory in the system prompt). Actions: \
                 `save` (name, description, type, content, scope; the same name replaces it), \
                 `read` (name, scope), `list` (scope, optional), `delete` (name, scope)."
                .into(),
            parameters: Schema::new(json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["save", "read", "list", "delete"] },
                    "scope": { "type": "string", "enum": ["personal", "project"], "description": "personal: every project; project: this one. Default project." },
                    "name": { "type": "string", "description": "Short kebab-case name, e.g. prefers-small-commits." },
                    "description": { "type": "string", "description": "One line saying what it holds, used to judge relevance later." },
                    "type": { "type": "string", "enum": TYPES },
                    "content": { "type": "string", "description": "The memory. For feedback and project memories, add why and how to apply it." }
                },
                "required": ["action"],
                "additionalProperties": false
            })),
            constrained_sampling: None,
        },
        cwd,
    })
}

fn arg<'a>(params: &'a Value, key: &str) -> &'a str {
    params.get(key).and_then(Value::as_str).unwrap_or("")
}

impl MemoryTool {
    fn run(&self, params: &Value) -> Result<(String, Value), String> {
        if !enabled() {
            return Err("memory is switched off in the app".into());
        }
        let action = arg(params, "action");
        let scope_text = arg(params, "scope");
        let scope = if scope_text.is_empty() {
            Scope::Project
        } else {
            Scope::parse(scope_text)?
        };
        let folder = dir(scope, &self.cwd)?;
        match action {
            "save" => {
                let kind = match arg(params, "type") {
                    "" => "project",
                    k => k,
                };
                let existed = find_in(&folder, arg(params, "name")).is_some();
                let m = save_in(
                    &folder,
                    arg(params, "name"),
                    arg(params, "description"),
                    kind,
                    arg(params, "content"),
                    None,
                )?;
                let verb = if existed { "Updated" } else { "Saved" };
                Ok((
                    format!("{verb} {} memory `{}`.", scope.label(), m.name),
                    json!({ "memory": entry(&m, scope), "action": if existed { "updated" } else { "saved" } }),
                ))
            }
            "read" => {
                let m = find_in(&folder, arg(params, "name")).ok_or_else(|| {
                    format!(
                        "no {} memory named `{}`",
                        scope.label(),
                        arg(params, "name")
                    )
                })?;
                Ok((
                    format!(
                        "{} ({}, {}): {}\n\n{}",
                        m.name,
                        m.kind,
                        scope.label(),
                        m.description,
                        m.content
                    ),
                    json!({ "memory": entry(&m, scope), "action": "read" }),
                ))
            }
            "list" => {
                let scopes: Vec<Scope> = if scope_text.is_empty() {
                    vec![Scope::Personal, Scope::Project]
                } else {
                    vec![scope]
                };
                let mut text = String::new();
                for s in scopes {
                    let items = list_dir(&dir(s, &self.cwd)?);
                    text.push_str(&format!("## {}\n{}\n\n", s.label(), index_lines(&items)));
                }
                Ok((text.trim_end().to_string(), json!({ "action": "list" })))
            }
            "delete" => {
                delete_in(&folder, arg(params, "name"))?;
                Ok((
                    format!(
                        "Deleted {} memory `{}`.",
                        scope.label(),
                        arg(params, "name")
                    ),
                    json!({ "action": "deleted", "memory": { "name": arg(params, "name"), "scope": scope.label() } }),
                ))
            }
            other => Err(format!("unknown action `{other}`")),
        }
    }
}

#[async_trait]
impl AgentTool for MemoryTool {
    fn schema(&self) -> &Tool {
        &self.schema
    }

    fn label(&self) -> &str {
        "memory"
    }

    fn execution_mode(&self) -> ToolExecutionMode {
        ToolExecutionMode::Sequential
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        params: Value,
        _signal: CancellationToken,
        _on_update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let (text, details) = self.run(&params).map_err(AgentError::Validation)?;
        Ok(AgentToolResult {
            content: vec![TextContentOrImage::text(text)],
            details,
            ..Default::default()
        })
    }
}
