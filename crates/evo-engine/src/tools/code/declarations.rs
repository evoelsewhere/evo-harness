//! What the agent is told about `run_code`: when to use it, the API, and
//! TypeScript declarations of each tool a program can call, generated from
//! its JSON Schema (docs/features/code-mode/spec.md, FR-18; plan section 5).
//! Doc comments keep only the first sentence of a description: the text
//! rides on every request next to the direct tools, which carry the full
//! descriptions, and `tools.describe(name)` gives a tool's whole schema.

use evo_ai::types::Tool;
use serde_json::{Map, Value};

use crate::tools::info::callable_from_code;

/// The tool's description, without the declarations.
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

/// Nesting past this depth is `unknown`; it also ends `$ref` cycles.
const MAX_DEPTH: usize = 6;
/// A doc comment's most characters.
const DOC_CHARS: usize = 160;
/// An object whose fields have no docs stays on one line up to this width.
const INLINE_WIDTH: usize = 72;

/// The description with the declarations of `tools` appended.
pub fn description_with(tools: &[&Tool]) -> String {
    let declarations = render(tools);
    if declarations.is_empty() {
        DESCRIPTION.to_owned()
    } else {
        format!(
            "{DESCRIPTION}\n\nThe tools a program can call (each resolves to the tool's text output):\n```ts\n{declarations}\n```"
        )
    }
}

/// `declare const tools: { … }` with a method per tool a program may call;
/// empty when there is none.
pub fn render(tools: &[&Tool]) -> String {
    let methods: String = tools
        .iter()
        .filter(|tool| callable_from_code(&tool.name))
        .map(|tool| method(tool))
        .collect();
    if methods.is_empty() {
        String::new()
    } else {
        format!("declare const tools: {{\n{methods}}};")
    }
}

/// One tool's method: its doc comment and signature.
fn method(tool: &Tool) -> String {
    let mut out = String::new();
    push_doc(&mut out, 1, &tool.description);
    out.push_str(&format!(
        "  {}({}): Promise<string>;\n",
        key(&tool.name),
        arguments(&tool.parameters.0)
    ));
    out
}

/// `args: {…}`, `args?: {…}` when every field is optional, or nothing for a
/// tool without fields.
fn arguments(schema: &Value) -> String {
    let has_fields = schema["properties"]
        .as_object()
        .is_some_and(|properties| !properties.is_empty());
    let open = !matches!(
        schema.get("additionalProperties"),
        None | Some(Value::Bool(false))
    );
    if !has_fields && !open {
        return String::new();
    }
    let optional = if required(schema).is_empty() { "?" } else { "" };
    let ty = Renderer { root: schema }.object(schema, 1, 0);
    format!("args{optional}: {ty}")
}

/// Turns schemas into TypeScript types; `root` is the tool's whole schema,
/// which `$ref`s point into.
struct Renderer<'a> {
    root: &'a Value,
}

impl Renderer<'_> {
    /// The type of `schema`, written at `indent` (two spaces each).
    fn ty(&self, schema: &Value, indent: usize, depth: usize) -> String {
        if depth > MAX_DEPTH {
            return "unknown".into();
        }
        let Some(map) = schema.as_object() else {
            return if schema == &Value::Bool(false) {
                "never"
            } else {
                "unknown"
            }
            .into();
        };
        let ty = self.base(map, schema, indent, depth);
        if map.get("nullable") == Some(&Value::Bool(true)) {
            union(vec![ty, "null".into()])
        } else {
            ty
        }
    }

    fn base(
        &self,
        map: &Map<String, Value>,
        schema: &Value,
        indent: usize,
        depth: usize,
    ) -> String {
        if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
            return match self.resolve(reference) {
                Some(target) => self.ty(target, indent, depth + 1),
                None => "unknown".into(),
            };
        }
        if let Some(value) = map.get("const") {
            return literal(value);
        }
        if let Some(values) = map.get("enum").and_then(Value::as_array) {
            return union(values.iter().map(literal).collect());
        }
        // Branches that only list required fields say nothing about the type.
        for keyword in ["anyOf", "oneOf"] {
            if let Some(branches) = map.get(keyword).and_then(Value::as_array) {
                let ty = union(
                    branches
                        .iter()
                        .map(|branch| self.ty(branch, indent, depth + 1))
                        .collect(),
                );
                if ty != "unknown" {
                    return ty;
                }
            }
        }
        if let Some(parts) = map.get("allOf").and_then(Value::as_array) {
            let parts: Vec<String> = parts
                .iter()
                .map(|part| self.ty(part, indent, depth + 1))
                .filter(|ty| ty != "unknown")
                .collect();
            if !parts.is_empty() {
                return parts
                    .iter()
                    .map(|part| parenthesized(part))
                    .collect::<Vec<_>>()
                    .join(" & ");
            }
        }
        match map.get("type") {
            Some(Value::String(name)) => self.named(name, map, schema, indent, depth),
            Some(Value::Array(names)) => union(
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|name| self.named(name, map, schema, indent, depth))
                    .collect(),
            ),
            _ if map.contains_key("properties") || map.contains_key("additionalProperties") => {
                self.object(schema, indent, depth)
            }
            _ if map.contains_key("items") || map.contains_key("prefixItems") => {
                self.array(map, indent, depth)
            }
            _ => "unknown".into(),
        }
    }

    /// The type for one JSON Schema `type` name.
    fn named(
        &self,
        name: &str,
        map: &Map<String, Value>,
        schema: &Value,
        indent: usize,
        depth: usize,
    ) -> String {
        match name {
            "string" => "string".into(),
            "number" | "integer" => "number".into(),
            "boolean" => "boolean".into(),
            "null" => "null".into(),
            "array" => self.array(map, indent, depth),
            "object" => self.object(schema, indent, depth),
            _ => "unknown".into(),
        }
    }

    /// `T[]`, or a tuple for `prefixItems` or a list of `items`.
    fn array(&self, map: &Map<String, Value>, indent: usize, depth: usize) -> String {
        let tuple = map
            .get("prefixItems")
            .or_else(|| map.get("items").filter(|items| items.is_array()))
            .and_then(Value::as_array);
        if let Some(items) = tuple {
            let items: Vec<String> = items
                .iter()
                .map(|item| self.ty(item, indent, depth + 1))
                .collect();
            return format!("[{}]", items.join(", "));
        }
        match map.get("items") {
            Some(items) => format!("{}[]", parenthesized(&self.ty(items, indent, depth + 1))),
            None => "unknown[]".into(),
        }
    }

    /// A type literal: one field per property, `?` unless required, and an
    /// index signature for `additionalProperties`.
    fn object(&self, schema: &Value, indent: usize, depth: usize) -> String {
        let required = required(schema);
        let mut fields: Vec<(String, String)> = Vec::new();
        for (name, property) in schema["properties"].as_object().into_iter().flatten() {
            let optional = if required.contains(&name.as_str()) {
                ""
            } else {
                "?"
            };
            let ty = self.ty(property, indent + 1, depth + 1);
            let doc = property["description"]
                .as_str()
                .map(summary)
                .unwrap_or_default();
            fields.push((doc, format!("{}{optional}: {ty}", key(name))));
        }
        let rest = match schema.get("additionalProperties") {
            Some(Value::Bool(true)) => Some("unknown".to_owned()),
            Some(extra @ Value::Object(_)) => Some(self.ty(extra, indent + 1, depth + 1)),
            None if fields.is_empty() => Some("unknown".to_owned()),
            _ => None,
        };
        if let Some(ty) = rest {
            fields.push((String::new(), format!("[key: string]: {ty}")));
        }
        if fields.is_empty() {
            return "{}".into();
        }
        if fields
            .iter()
            .all(|(doc, line)| doc.is_empty() && !line.contains('\n'))
        {
            let lines: Vec<&str> = fields.iter().map(|(_, line)| line.as_str()).collect();
            let inline = format!("{{ {} }}", lines.join("; "));
            if inline.chars().count() <= INLINE_WIDTH {
                return inline;
            }
        }
        let pad = "  ".repeat(indent + 1);
        let mut out = String::from("{\n");
        for (doc, line) in &fields {
            if !doc.is_empty() {
                out.push_str(&format!("{pad}/** {doc} */\n"));
            }
            out.push_str(&format!("{pad}{line};\n"));
        }
        out.push_str(&"  ".repeat(indent));
        out.push('}');
        out
    }

    /// The schema a local `$ref` (`#/$defs/Name`, `#/definitions/Name`)
    /// points to.
    fn resolve(&self, reference: &str) -> Option<&Value> {
        let pointer = reference.strip_prefix('#')?;
        self.root.pointer(pointer)
    }
}

/// The names in the schema's `required` list.
fn required(schema: &Value) -> Vec<&str> {
    schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// `a | b` with repeats dropped; anything joined with `unknown` is `unknown`.
fn union(parts: Vec<String>) -> String {
    let mut seen: Vec<String> = Vec::new();
    for part in parts {
        if part == "unknown" {
            return part;
        }
        if !seen.contains(&part) {
            seen.push(part);
        }
    }
    if seen.is_empty() {
        "unknown".into()
    } else {
        seen.join(" | ")
    }
}

/// `(a | b)` when a union or intersection is an array element or part of an
/// intersection.
fn parenthesized(ty: &str) -> String {
    let mut depth = 0i32;
    let mut quoted = false;
    let mut escaped = false;
    let mut joined = false;
    for c in ty.chars() {
        if quoted {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => quoted = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            '|' | '&' if depth == 0 => joined = true,
            _ => {}
        }
    }
    if joined {
        format!("({ty})")
    } else {
        ty.to_owned()
    }
}

/// A `const` or `enum` value as a TypeScript literal type.
fn literal(value: &Value) -> String {
    match value {
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => value.to_string(),
        _ => "unknown".into(),
    }
}

/// A field or method name, quoted when it is not an identifier.
fn key(name: &str) -> String {
    let mut chars = name.chars();
    let identifier = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if identifier {
        name.to_owned()
    } else {
        Value::String(name.to_owned()).to_string()
    }
}

/// `/** … */` at `indent` with the first sentence of `text`, if any.
fn push_doc(out: &mut String, indent: usize, text: &str) {
    let doc = summary(text);
    if !doc.is_empty() {
        out.push_str(&format!("{}/** {doc} */\n", "  ".repeat(indent)));
    }
}

/// The first sentence of the first paragraph, on one line, at most
/// [`DOC_CHARS`] characters, safe inside a block comment.
fn summary(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let paragraph = text
        .split("\n\n")
        .map(str::trim)
        .find(|part| !part.is_empty())
        .unwrap_or("");
    let line = paragraph.split_whitespace().collect::<Vec<_>>().join(" ");
    let sentence = first_sentence(&line);
    let mut doc: String = sentence.chars().take(DOC_CHARS).collect();
    if sentence.chars().count() > DOC_CHARS {
        doc = format!("{}…", doc.trim_end());
    }
    doc.replace("*/", "*\\/")
}

/// Up to the first `.`, `!` or `?` followed by a space and a capital letter,
/// not counting abbreviations such as "e.g.".
fn first_sentence(line: &str) -> &str {
    let bytes = line.as_bytes();
    for (at, c) in line.char_indices() {
        if !matches!(c, '.' | '!' | '?') {
            continue;
        }
        let next_is_capital = bytes.get(at + 1) == Some(&b' ')
            && bytes.get(at + 2).is_some_and(u8::is_ascii_uppercase);
        if !next_is_capital {
            continue;
        }
        let word = line[..at].rsplit(' ').next().unwrap_or("");
        if ["e.g", "i.e", "etc", "vs"].contains(&word.to_ascii_lowercase().as_str()) {
            continue;
        }
        return &line[..=at];
    }
    line
}

/// The most bytes the declarations of the engine's own tools may take.
#[cfg(test)]
const DECLARATIONS_BUDGET: usize = 8_000;

#[cfg(test)]
mod tests {
    use super::*;
    use evo_ai::types::Schema;
    use serde_json::json;

    fn tool(name: &str, description: &str, parameters: Value) -> Tool {
        Tool {
            name: name.into(),
            description: description.into(),
            parameters: Schema::new(parameters),
            constrained_sampling: None,
        }
    }

    // FR-18: each JSON Schema form becomes its TypeScript type.
    #[test]
    fn each_schema_form_becomes_its_typescript_type() {
        let edit = tool(
            "edit_many",
            "Edit files in place. Each edit replaces text.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File to change. Relative to the project." },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": { "oldText": { "type": "string" }, "newText": { "type": "string" } },
                            "required": ["oldText", "newText"]
                        }
                    },
                    "mode": { "enum": ["replace", "append"] },
                    "count": { "type": "integer" },
                    "dry": { "type": "boolean", "default": false },
                    "tags": { "type": ["string", "null"] },
                    "env": { "type": "object", "additionalProperties": { "type": "string" } },
                    "pair": { "type": "array", "prefixItems": [{ "type": "string" }, { "type": "number" }] },
                    "either": { "anyOf": [{ "type": "string" }, { "type": "array", "items": { "type": "string" } }] },
                    "level": { "const": 2 },
                    "any": {}
                },
                "required": ["path", "edits"]
            }),
        );
        assert_eq!(
            render(&[&edit]),
            "declare const tools: {
  /** Edit files in place. */
  edit_many(args: {
    /** File to change. */
    path: string;
    edits: { oldText: string; newText: string }[];
    mode?: \"replace\" | \"append\";
    count?: number;
    dry?: boolean;
    tags?: string | null;
    env?: { [key: string]: string };
    pair?: [string, number];
    either?: string | string[];
    level?: 2;
    any?: unknown;
  }): Promise<string>;
};"
        );
    }

    // FR-18, FR-12: quoted names, no or optional arguments, excluded tools.
    #[test]
    fn names_and_arguments_follow_the_tool() {
        let issue = tool(
            "mcp__gh__create-issue",
            "Create an issue",
            json!({ "type": "object", "properties": { "title": { "type": "string" } }, "required": ["title"] }),
        );
        let docs = tool("docs", "", json!({ "type": "object" }));
        let find = tool(
            "find",
            "Find files.\n\nMore text.",
            json!({ "type": "object", "properties": { "pattern": { "type": "string" } } }),
        );
        let ask = tool("ask_user", "Ask.", json!({ "type": "object" }));
        assert_eq!(
            render(&[&issue, &docs, &find, &ask]),
            "declare const tools: {
  /** Create an issue */
  \"mcp__gh__create-issue\"(args: { title: string }): Promise<string>;
  docs(): Promise<string>;
  /** Find files. */
  find(args?: { pattern?: string }): Promise<string>;
};"
        );
        assert_eq!(render(&[&ask]), "");
    }

    // `anyOf` branches that only list required fields leave the object as is.
    #[test]
    fn required_only_branches_keep_the_object() {
        let pick = tool(
            "pick",
            "",
            json!({
                "type": "object",
                "properties": { "a": { "type": "string" }, "b": { "type": "string" } },
                "anyOf": [{ "required": ["a"] }, { "required": ["b"] }]
            }),
        );
        assert!(render(&[&pick])
            .contains("  pick(args?: { a?: string; b?: string }): Promise<string>;\n"));
    }

    #[test]
    fn the_description_carries_the_declarations_and_stays_short() {
        let read = tool(
            "read",
            "Read a file.",
            json!({ "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }),
        );
        let text = description_with(&[&read]);
        assert!(text.starts_with(DESCRIPTION));
        assert!(text.ends_with(
            "```ts\ndeclare const tools: {\n  /** Read a file. */\n  read(args: { path: string }): Promise<string>;\n};\n```"
        ));
        assert_eq!(description_with(&[]), DESCRIPTION);
        // It rides on every request (first benchmark run, tasks.md SC-1).
        assert!(DESCRIPTION.len() < 1_700, "{}", DESCRIPTION.len());
    }

    // The declarations for the engine's real tools ride on every request next
    // to the direct tools, so their size is held to a budget (plan section 5).
    #[tokio::test]
    async fn the_real_tool_set_stays_within_the_budget() {
        let env = std::sync::Arc::new(evo_tools::OsExecutionEnv::with_cwd(".".into()));
        let ctx = evo_tools::ExecutionToolContext::new(env.clone(), Some(env));
        let cell: crate::tools::agent::HarnessCell = Default::default();
        let registered = crate::tools::register_all(&ctx, &cell);
        let schemas: Vec<&Tool> = registered.iter().map(|(_, t)| t.tool.schema()).collect();
        let text = render(&schemas);
        eprintln!("{text}\n-- {} tools, {} bytes", schemas.len(), text.len());
        assert!(text.contains("  read(args: {"), "{text}");
        assert!(!text.contains("ask_user("), "{text}");
        assert!(text.len() <= DECLARATIONS_BUDGET, "{} bytes", text.len());
    }

    fn ty(schema: Value) -> String {
        Renderer { root: &schema }.ty(&schema, 1, 0)
    }

    // FR-18: forms the tool test above does not reach.
    #[test]
    fn unions_intersections_and_nullable_types() {
        assert_eq!(
            ty(json!({ "type": "string", "nullable": true })),
            "string | null"
        );
        assert_eq!(
            ty(json!({ "allOf": [
                { "type": "object", "properties": { "a": { "type": "string" } }, "required": ["a"] },
                { "type": "object", "properties": { "b": { "type": "number" } } }
            ] })),
            "{ a: string } & { b?: number }"
        );
        assert_eq!(
            ty(json!({ "type": "array", "items": { "enum": ["a", "b"] } })),
            "(\"a\" | \"b\")[]"
        );
        // A `|` inside a string literal is not a union.
        assert_eq!(
            ty(json!({ "type": "array", "items": { "const": "x | y" } })),
            "\"x | y\"[]"
        );
        assert_eq!(
            ty(json!({ "type": "array", "items": [{ "type": "boolean" }] })),
            "[boolean]"
        );
        assert_eq!(ty(json!({ "type": "array" })), "unknown[]");
        assert_eq!(
            ty(json!({ "anyOf": [{ "type": "string" }, {}] })),
            "unknown"
        );
        assert_eq!(
            ty(json!({ "enum": ["a", 1, true, null] })),
            "\"a\" | 1 | true | null"
        );
        assert_eq!(
            ty(json!({ "type": "object", "additionalProperties": true })),
            "{ [key: string]: unknown }"
        );
        assert_eq!(
            ty(json!({ "type": "object", "additionalProperties": false })),
            "{}"
        );
        assert_eq!(
            ty(
                json!({ "type": "object", "properties": { "a-b": { "type": "string" } }, "required": ["a-b"] })
            ),
            "{ \"a-b\": string }"
        );
    }

    // `$ref`s resolve inside the tool's schema; a cycle ends in `unknown`.
    #[test]
    fn references_resolve_and_cycles_end() {
        let schema = json!({
            "type": "object",
            "$defs": {
                "Node": {
                    "type": "object",
                    "properties": { "name": { "type": "string" }, "child": { "$ref": "#/$defs/Node" } }
                }
            },
            "properties": {
                "root": { "$ref": "#/$defs/Node" },
                "missing": { "$ref": "#/$defs/Nope" },
                "old": { "$ref": "#/definitions/Old" }
            },
            "definitions": { "Old": { "type": "integer" } },
            "required": ["root"]
        });
        let text = Renderer { root: &schema }.object(&schema, 1, 0);
        assert!(text.contains("name?: string"), "{text}");
        assert!(text.contains("child?: unknown"), "{text}");
        assert!(text.contains("missing?: unknown"), "{text}");
        assert!(text.contains("old?: number"), "{text}");
        assert!(text.contains("root: {"), "{text}");
    }

    // Doc comments: the first sentence of the first paragraph, cut and safe.
    #[test]
    fn docs_keep_the_first_sentence() {
        assert_eq!(
            summary("Read a file, e.g. Foo. Then more."),
            "Read a file, e.g. Foo."
        );
        assert_eq!(
            summary("  Multi\n line   text.\n\nSecond paragraph."),
            "Multi line text."
        );
        assert_eq!(summary("Version 1.2 is out. Next"), "Version 1.2 is out.");
        assert_eq!(summary("Ends */ here. More"), "Ends *\\/ here.");
        assert_eq!(summary(""), "");
        let long = "word ".repeat(60);
        let doc = summary(&long);
        assert!(doc.ends_with('…'), "{doc}");
        assert!(doc.chars().count() <= DOC_CHARS + 1, "{doc}");
    }
}
