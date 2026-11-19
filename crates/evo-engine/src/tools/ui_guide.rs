//! The `ui_guide` tool: how to write the UI blocks (`evo-ui` fences) Evo
//! draws in a reply.
//!
//! Spec: `docs/features/rich-replies/spec.md` (FR-10, FR-11, FR-12,
//! FR-15–FR-22); plan 3.3 "Tool ui_guide" and "The index". The block format
//! itself is read by the desktop (`apps/desktop/src/lib/ui-blocks/parse.ts`);
//! every example here must be a block that parser draws.
//!
//! The guides are constant text compiled into the engine: the tool takes no
//! path and reads nothing, so it needs no approval, is safe for read-only
//! agents and safe to replay. The prompt index (`ui_guide/index.md`) goes
//! into the system prompt through `session::builtin_tool_guidelines` while
//! this tool is active.

use std::sync::Arc;

use async_trait::async_trait;
use evo_agent::agent_tool::AgentTool;
use evo_agent::error::AgentError;
use evo_agent::types::{AgentToolResult, TextContentOrImage, ToolExecutionMode, ToolResultPartial};
use evo_ai::types::{Schema, Tool};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

pub const TOOL: &str = "ui_guide";

/// The guides `ui_guide` knows, in the order the schema lists them.
pub const COMPONENTS: [&str; 8] = [
    "stats", "bars", "chart", "table", "steps", "callout", "actions", "formats",
];

/// What every answer starts with: the fence, the shared fields and rules.
const COMMON: &str = include_str!("ui_guide/common.md");

/// The prompt index (FR-10), one guideline per `- ` line.
const INDEX: &str = include_str!("ui_guide/index.md");

fn guide_of(component: &str) -> Option<&'static str> {
    Some(match component {
        "stats" => include_str!("ui_guide/stats.md"),
        "bars" => include_str!("ui_guide/bars.md"),
        "chart" => include_str!("ui_guide/chart.md"),
        "table" => include_str!("ui_guide/table.md"),
        "steps" => include_str!("ui_guide/steps.md"),
        "callout" => include_str!("ui_guide/callout.md"),
        "actions" => include_str!("ui_guide/actions.md"),
        "formats" => include_str!("ui_guide/formats.md"),
        _ => return None,
    })
}

fn lf(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_owned()
}

/// The index lines for the system prompt's guidelines, without their `- `
/// marks. A rule's continuation lines keep their indent.
pub fn index_rules() -> Vec<String> {
    let mut rules: Vec<String> = Vec::new();
    for line in lf(INDEX).lines() {
        match line.strip_prefix("- ") {
            Some(start) => rules.push(start.to_owned()),
            None => match rules.last_mut() {
                Some(rule) if !line.trim().is_empty() => {
                    rule.push('\n');
                    rule.push_str(line);
                }
                _ => {}
            },
        }
    }
    rules
}

/// What `ui_guide` returns for `components`, and the components it explained.
/// The shared rules, then each known guide once in request order, then the
/// unknown names; with no known name, the list of known ones.
pub fn guide(components: &[String]) -> (String, Vec<String>) {
    let mut known: Vec<String> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for name in components {
        let key = name.trim().to_ascii_lowercase();
        if guide_of(&key).is_some() {
            if !known.contains(&key) {
                known.push(key);
            }
        } else if !unknown.contains(name) {
            unknown.push(name.clone());
        }
    }
    let unknown_line = (!unknown.is_empty()).then(|| format!("Unknown: {}", unknown.join(", ")));
    if known.is_empty() {
        let mut text = unknown_line.map_or_else(String::new, |line| format!("{line}\n\n"));
        text.push_str(&format!(
            "Known components: {}. Call ui_guide with the ones you need.",
            COMPONENTS.join(", ")
        ));
        return (text, known);
    }
    let mut parts = vec![lf(COMMON)];
    parts.extend(known.iter().filter_map(|name| guide_of(name)).map(lf));
    parts.extend(unknown_line);
    (parts.join("\n\n"), known)
}

pub struct UiGuideTool {
    schema: Tool,
}

pub fn create_ui_guide_tool() -> Arc<dyn AgentTool> {
    Arc::new(UiGuideTool {
        schema: Tool {
            name: TOOL.into(),
            description:
                "Read how to write Evo UI blocks (evo-ui fences) before using a component: its \
                 fields, an example and its rules. Read-only."
                    .into(),
            parameters: Schema::new(json!({
                "type": "object",
                "properties": {
                    "components": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": format!("Components to read: {}", COMPONENTS.join(", "))
                    }
                },
                "required": ["components"]
            })),
            constrained_sampling: None,
        },
    })
}

fn components_of(params: &Value) -> Result<Vec<String>, AgentError> {
    let invalid =
        |message: &str| AgentError::Validation(format!("input invalid: components: {message}"));
    match params.get("components") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid("Input should be a valid string"))
            })
            .collect(),
        // A lone name is taken as a list of one.
        Some(Value::String(name)) => Ok(vec![name.clone()]),
        Some(_) => Err(invalid("Input should be a valid list")),
        None => Err(invalid("Field required")),
    }
}

#[async_trait]
impl AgentTool for UiGuideTool {
    fn schema(&self) -> &Tool {
        &self.schema
    }

    fn label(&self) -> &str {
        TOOL
    }

    fn execution_mode(&self) -> ToolExecutionMode {
        ToolExecutionMode::Parallel
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        params: Value,
        _signal: CancellationToken,
        _on_update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let requested = components_of(&params)?;
        let (text, components) = guide(&requested);
        Ok(AgentToolResult {
            content: vec![TextContentOrImage::text(text)],
            details: json!({ "components": components }),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests;
