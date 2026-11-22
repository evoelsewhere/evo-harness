//! What the agent is told about `run_code`: when to use it, the API, and a
//! compact signature of each tool a program can call, from its schema
//! (docs/features/code-mode/spec.md, FR-18). Kept short: it rides on every
//! request, and `tools.describe(name)` gives a tool's full parameters.

use evo_ai::types::Tool;
use serde_json::Value;

use crate::tools::info::callable_from_code;

/// The tool's description, without the signatures.
pub const DESCRIPTION: &str = "\
Run one JavaScript program that does a whole multi-step task and returns a short summary. \
Use it instead of separate calls whenever a task needs more than one tool call: find, read, \
change, run the check and collect what your answer needs in the same program, then answer \
from its result without checking again. Use a direct call only when one call is enough.

Plain JavaScript with top-level await; `return x` or the last expression is the result. \
There is no require, import, fs or process: use these instead.
- files(dir, ext?) -> sorted relative paths; read(path) -> the whole file's text (all pages; \
throws if it cannot); read(path, {offset, limit}) -> that range; write(path, text); \
edit(path, oldText, newText).
- $`cmd ${value}` runs bash (values quoted; throws on non-zero exit); .lines() .json() \
.nothrow() -> {exitCode, output, ok, truncated}. Use grep -rl / find, not rg.
- tools.<name>(args) -> Promise<string> for any other tool; a refused call throws ToolError.
- Promise.all([...]) runs up to 8 calls at once; console.log lines come back.
Inspect and act in the same program: read inside it and decide in code instead of printing \
samples to look at first. Example: const changed = []; for (const f of await files(\"src\", \
\".js\")) { const t = await read(f); if (t.includes(\"old\")) { await edit(f, \"old\", \"new\"); \
changed.push(f); } } return { changed, check: await $`sh scripts/check.sh`.nothrow() };
Calls are checked like direct calls (approvals, hooks, sandbox). Limits: 10 min, 128 calls.";

/// The description with a signature of each tool in `tools` appended.
pub fn description_with(tools: &[&Tool]) -> String {
    let signatures = render(tools);
    if signatures.is_empty() {
        DESCRIPTION.to_owned()
    } else {
        format!("{DESCRIPTION}\nTools: {signatures}")
    }
}

/// `tools.read({path, offset?, limit?})` per tool a program may call, on one
/// line.
pub fn render(tools: &[&Tool]) -> String {
    tools
        .iter()
        .filter(|tool| callable_from_code(&tool.name))
        .map(|tool| format!("tools.{}({})", tool.name, fields(&tool.parameters.0)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `{path, offset?}`: the schema's top-level fields, optional ones marked.
fn fields(schema: &Value) -> String {
    let Some(properties) = schema["properties"].as_object() else {
        return String::new();
    };
    if properties.is_empty() {
        return String::new();
    }
    let required: Vec<&str> = schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let names: Vec<String> = properties
        .keys()
        .map(|name| {
            if required.contains(&name.as_str()) {
                name.clone()
            } else {
                format!("{name}?")
            }
        })
        .collect();
    format!("{{{}}}", names.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_ai::types::Schema;
    use serde_json::json;

    fn tool(name: &str, parameters: Value) -> Tool {
        Tool {
            name: name.into(),
            description: "A tool. With more text.".into(),
            parameters: Schema::new(parameters),
            constrained_sampling: None,
        }
    }

    #[test]
    fn signatures_follow_the_schemas_and_skip_excluded_tools() {
        let edit = tool(
            "edit",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "edits": { "type": "array" },
                    "mode": { "enum": ["a", "b"] }
                },
                "required": ["path", "edits"]
            }),
        );
        let ask = tool("ask_user", json!({ "type": "object" }));
        let bare = tool("docs", json!({ "type": "object" }));
        assert_eq!(
            render(&[&edit, &ask, &bare]),
            "tools.edit({path, edits, mode?}), tools.docs()"
        );
    }

    #[test]
    fn the_description_carries_the_signatures_and_stays_short() {
        let read = tool(
            "read",
            json!({ "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }),
        );
        let text = description_with(&[&read]);
        assert!(text.starts_with(DESCRIPTION));
        assert!(text.ends_with("Tools: tools.read({path})"));
        assert_eq!(description_with(&[]), DESCRIPTION);
        // It rides on every request (first benchmark run, tasks.md SC-1).
        assert!(DESCRIPTION.len() < 1_700, "{}", DESCRIPTION.len());
    }
}
