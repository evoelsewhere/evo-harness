//! The code tool, `run_code`: the agent writes a JavaScript program that
//! calls tools, runs them in parallel and returns only what it needs
//! (docs/features/code-mode/spec.md; plan 3.2 – 3.6; ADR 0025).
//!
//! The program runs in [`runtime`] (QuickJS, no abilities of its own). Each
//! tool call it makes goes through [`bridge`] to the tool program capability,
//! so it is checked like a direct call (FR-10). The API it sees is
//! `prelude.js` (FR-5, FR-6).

pub mod bridge;
pub mod declarations;
pub mod runtime;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use evo_agent::agent_tool::AgentTool;
use evo_agent::error::AgentError;
use evo_agent::tool_program::{self, ToolProgram};
use evo_agent::types::{AgentToolResult, TextContentOrImage, ToolExecutionMode, ToolResultPartial};
use evo_ai::types::{Schema, Tool};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::tools::info::callable_from_code;

/// The tool's name.
pub const NAME: &str = crate::approval::CODE_TOOL;

/// The API a program sees.
pub const PRELUDE: &str = include_str!("prelude.js");

/// The result given to the agent is cut to the usual tool output limit (FR-13).
const OUTPUT_BYTES: usize = evo_tools::truncate::DEFAULT_MAX_BYTES;
const OUTPUT_LINES: usize = evo_tools::truncate::DEFAULT_MAX_LINES;

pub struct CodeTool {
    schema: Tool,
    limits: runtime::Limits,
}

/// The tool with the spec's limits (FR-13).
pub fn create_code_tool() -> Arc<dyn AgentTool> {
    create_code_tool_with(runtime::Limits::default())
}

/// The tool with typed declarations of `tools` in its description (FR-18).
pub fn create_code_tool_for(tools: &[&Tool]) -> Arc<dyn AgentTool> {
    let tool = CodeTool {
        schema: Tool {
            description: declarations::description_with(tools),
            ..schema()
        },
        limits: runtime::Limits::default(),
    };
    register_view();
    Arc::new(tool)
}

fn register_view() {
    crate::hooks::register_output_view(
        NAME,
        crate::hooks::OutputView {
            view: hook_view,
            apply: hook_apply,
        },
    );
}

/// The tool with other limits (tests).
pub fn create_code_tool_with(limits: runtime::Limits) -> Arc<dyn AgentTool> {
    register_view();
    Arc::new(CodeTool {
        schema: schema(),
        limits,
    })
}

fn schema() -> Tool {
    Tool {
        name: NAME.into(),
        description: declarations::DESCRIPTION.into(),
        parameters: Schema::new(json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "The JavaScript program. Top-level await works; the value of the last expression is the result."
                },
                "description": {
                    "type": "string",
                    "description": "One line saying what the program is for, shown to the person."
                }
            },
            "required": ["code"],
            "additionalProperties": false
        })),
        constrained_sampling: None,
    }
}

#[async_trait]
impl AgentTool for CodeTool {
    fn schema(&self) -> &Tool {
        &self.schema
    }

    fn label(&self) -> &str {
        "Code"
    }

    /// The tool program capability starts nested calls only from a
    /// Sequential tool.
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
        let code = params["code"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("code is required".into()))?
            .to_owned();
        let description = params["description"].as_str().unwrap_or("").to_owned();
        let program = tool_program::current().ok_or_else(|| {
            AgentError::Tool("run_code can only run inside the agent loop".into())
        })?;
        let run = run_program(&program, code.clone(), self.limits.clone()).await;
        let result = build_result(&code, &description, run);
        if result.details["error"].is_string() {
            Err(AgentError::StructuredTool(Box::new(result)))
        } else {
            Ok(result)
        }
    }
}

/// What one run produced: the interpreter's outcome and the bridge's record.
struct Run {
    outcome: runtime::Outcome,
    served: bridge::Served,
    /// The person's "Stop code" reason, if they stopped it (FR-11).
    stopped: Option<String>,
    limits: runtime::Limits,
}

/// A nested call's own timeout. The program's time limit, which leaves out
/// time at approval prompts, is enforced by the interpreter and ends every
/// call with it; shell commands keep their own timeout (FR-11, FR-13).
const CALL_TIMEOUT_MS: u64 = 24 * 60 * 60 * 1000;

async fn run_program(program: &Arc<ToolProgram>, code: String, limits: runtime::Limits) -> Run {
    let timeout_ms = CALL_TIMEOUT_MS;
    program.set_timeout_limit_ms(timeout_ms);
    let prelude = format!(
        "globalThis.__evo_catalog = JSON.parse({});\n{PRELUDE}",
        serde_json::to_string(&catalog(program).await.to_string()).unwrap_or_default()
    );
    let (calls_tx, calls_rx) = tokio::sync::mpsc::unbounded_channel();
    let served = tokio::spawn(bridge::serve(program.clone(), calls_rx, timeout_ms));
    let cancel = program.cancellation();
    let weak = Arc::downgrade(program);
    let paused: Arc<dyn Fn() -> Duration + Send + Sync> =
        Arc::new(move || weak.upgrade().map(|p| p.paused()).unwrap_or_default());
    let worker_limits = limits.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        runtime::run(runtime::Program {
            code: &code,
            prelude: &prelude,
            limits: worker_limits,
            cancel,
            paused,
            calls: calls_tx,
        })
    })
    .await
    .unwrap_or_else(|error| runtime::Outcome {
        error: Some(runtime::ProgramError {
            message: format!("The interpreter failed: {error}"),
            line: None,
        }),
        ..Default::default()
    });
    let stopped = program.stopped();
    // Calls the program started but did not wait for stop with it (FR-14).
    program.cancel_all();
    let served = served.await.unwrap_or_default();
    Run {
        outcome,
        served,
        stopped,
        limits,
    }
}

/// The tools a program may call: the effective tool set, minus the code tool
/// and the excluded tools (FR-12), with their parameters for `tools.describe`.
async fn catalog(program: &Arc<ToolProgram>) -> Value {
    let mut tools = Vec::new();
    let mut offset = 0u64;
    loop {
        let Ok(page) = program
            .handle(json!({ "op": "list", "offset": offset, "limit": 128 }))
            .await
        else {
            break;
        };
        for tool in page["tools"].as_array().into_iter().flatten() {
            if tool["name"].as_str().is_some_and(callable_from_code) {
                tools.push(tool.clone());
            }
        }
        match page["nextOffset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    Value::Array(tools)
}

/// The result's `details`, which is also what hooks see (FR-8, FR-17).
fn build_result(code: &str, description: &str, run: Run) -> AgentToolResult {
    let Run {
        outcome,
        served,
        stopped,
        limits,
    } = run;
    let mut tools_used: Vec<String> = Vec::new();
    for call in &served.calls {
        if !tools_used.contains(&call.name) {
            tools_used.push(call.name.clone());
        }
    }
    // Once the person stopped the code, that is why it ended, whatever the
    // program did after the refused call (FR-11).
    let error = match (&stopped, &outcome.error) {
        (Some(reason), error) => Some((reason.clone(), error.as_ref().and_then(|e| e.line))),
        (None, Some(error)) => Some((error.message.clone(), error.line)),
        (None, None) => None,
    };
    let mut details = json!({
        "code": code,
        "description": description,
        "result": outcome.value.clone().unwrap_or(Value::Null),
        "stdout": outcome.stdout.join("\n"),
        "stderr": outcome.stderr.join("\n"),
        "toolsUsed": tools_used,
        // A bounded copy of each call for the transcript's Code row (FR-15).
        "calls": served.calls.iter().map(|call| json!({
            "name": call.name, "ok": call.ok, "ms": call.ms,
            "args": call.args, "output": call.output, "details": call.details,
        })).collect::<Vec<_>>(),
    });
    if let Some((message, line)) = &error {
        details["error"] = json!(message);
        if let Some(line) = line {
            details["line"] = json!(line);
        }
    }
    if let Some(limit) = outcome.limit {
        details["limit"] = json!(format!("{limit:?}").to_lowercase());
        details["limits"] = json!({
            "seconds": limits.time.as_secs(),
            "memoryMb": limits.memory_bytes / (1024 * 1024),
            "calls": limits.max_calls,
        });
    }
    let mut content = vec![TextContentOrImage::text(render(&details))];
    content.extend(served.images);
    AgentToolResult {
        content,
        details,
        ..Default::default()
    }
}

/// The text the agent reads: the result, then what the program printed, the
/// tools it used and its error (FR-8), cut to the usual limit (FR-13).
fn render(details: &Value) -> String {
    let mut parts = Vec::new();
    match &details["result"] {
        Value::Null => {}
        Value::String(text) => parts.push(text.clone()),
        other => parts.push(other.to_string()),
    }
    let console: Vec<String> = [("stdout", ""), ("stderr", "[stderr] ")]
        .iter()
        .filter_map(|(key, prefix)| {
            let text = details[*key].as_str().unwrap_or_default();
            (!text.is_empty()).then(|| {
                text.lines()
                    .map(|line| format!("{prefix}{line}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        })
        .collect();
    if !console.is_empty() {
        parts.push(format!("--- console ---\n{}", console.join("\n")));
    }
    let calls = details["calls"].as_array().map_or(0, Vec::len);
    if calls > 0 {
        let used: Vec<&str> = details["toolsUsed"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let failed = details["calls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|call| call["ok"] == false)
            .count();
        parts.push(format!(
            "--- tools used: {} ({calls} call{}{}) ---",
            used.join(", "),
            if calls == 1 { "" } else { "s" },
            if failed > 0 {
                format!(", {failed} failed")
            } else {
                String::new()
            }
        ));
    }
    if let Some(error) = details["error"].as_str() {
        let at = details["line"]
            .as_u64()
            .map(|line| format!(" (line {line})"))
            .unwrap_or_default();
        parts.push(format!("--- error{at} ---\n{error}"));
    }
    let text = if parts.is_empty() {
        "(no result)".to_owned()
    } else {
        parts.join("\n\n")
    };
    let cut = evo_tools::truncate::truncate_head(
        &text,
        evo_tools::truncate::TruncationOptions {
            max_lines: Some(OUTPUT_LINES),
            max_bytes: Some(OUTPUT_BYTES),
        },
    );
    if cut.truncated {
        format!(
            "{}\n\n[Output cut to {} of {}; return less from the program.]",
            cut.content,
            evo_tools::truncate::format_size(cut.output_bytes),
            evo_tools::truncate::format_size(cut.total_bytes)
        )
    } else {
        cut.content
    }
}

/// What `PostToolUse` hooks get as `tool_response` (FR-17).
fn hook_view(result: &AgentToolResult) -> Option<Value> {
    let details = &result.details;
    details["code"].as_str()?;
    let mut view = json!({
        "code": details["code"],
        "result": details["result"],
        "stdout": details["stdout"],
        "stderr": details["stderr"],
        "toolsUsed": details["toolsUsed"],
    });
    if let Some(error) = details["error"].as_str() {
        view["error"] = json!(error);
    }
    Some(view)
}

/// The result rebuilt from a hook's changed copy; `None` when the copy is
/// malformed: `code`, `stdout` and `stderr` must be strings, `error` a string
/// or absent, `toolsUsed` a list of strings (FR-17).
fn hook_apply(result: &AgentToolResult, copy: &Value) -> Option<AgentToolResult> {
    let text = |key: &str| copy.get(key).and_then(Value::as_str).map(str::to_owned);
    let code = text("code")?;
    let stdout = text("stdout")?;
    let stderr = text("stderr")?;
    let error = match copy.get("error") {
        None | Some(Value::Null) => None,
        Some(Value::String(error)) => Some(error.clone()),
        Some(_) => return None,
    };
    let tools_used = match copy.get("toolsUsed") {
        None => result.details["toolsUsed"].clone(),
        Some(Value::Array(names)) if names.iter().all(Value::is_string) => {
            Value::Array(names.clone())
        }
        Some(_) => return None,
    };
    let mut details = result.details.clone();
    details["code"] = json!(code);
    details["result"] = copy.get("result").cloned().unwrap_or(Value::Null);
    details["stdout"] = json!(stdout);
    details["stderr"] = json!(stderr);
    details["toolsUsed"] = tools_used;
    match error {
        Some(error) => details["error"] = json!(error),
        None => {
            if let Some(map) = details.as_object_mut() {
                map.remove("error");
                map.remove("line");
            }
        }
    }
    let mut content = vec![TextContentOrImage::text(render(&details))];
    content.extend(
        result
            .content
            .iter()
            .filter(|part| matches!(part, TextContentOrImage::Image(_)))
            .cloned(),
    );
    Some(AgentToolResult {
        content,
        details,
        ..result.clone()
    })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
