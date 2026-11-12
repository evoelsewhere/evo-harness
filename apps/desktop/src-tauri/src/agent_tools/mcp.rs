//! Connector tools (MCP servers) as agent tools, answered by the shell (ADR 0013,
//! ADR 0014): `mcp__<server>__<tool>` for every connected, allowed tool of the
//! chat, or past a size the pair `mcp_search_tools` / `mcp_call_tool` instead.
//!
//! They are *dynamic* per chat, so they are not in the static registry: this
//! module is a [`Provider`] (agent_tools/mod.rs). It lists a chat's definitions
//! and claims and runs the calls, on top of the shell's one MCP pool
//! (`crate::mcp`, `crates/evo-mcp`):
//!
//! * which connectors exist for the chat: the person's own plus the approved
//!   servers of the project of the chat's folder (the pool decides);
//! * the per-chat switches (Customize → composer `+` → Connectors): the webview
//!   reports the switched-off connectors with [`agent_mcp_chat_config`] before it
//!   asks for a chat's definitions, and whenever a switch changes;
//! * permissions Allow / Ask / Block per tool: Block tools are not offered and
//!   are refused, Ask tools raise the composer's Allow once / Always allow / Deny
//!   prompt through [`ToolCtx::ask`], Always allow saves Allow in the pool.
//!
//! Behaviour, names, schemas and wording are those of the webview code this
//! replaces (`stores/engine-store.ts` `loadMcpTools` / `runMcpTool`, `lib/mcp.ts`
//! `shouldDefer` / `searchToolDefinitions` / `searchTools`). The approval mode is
//! not consulted: as before, only the tool's own permission asks.
//!
//! Spec: docs/features/agent-tools-layers (FR-4, T10); docs/features/
//! customize-skills-plugins-connectors (FR-19 – FR-21, FR-27, FR-28).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use evo_mcp::Permission;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};

use super::{chat_folder, BoxFuture, Content, Provider, ToolCtx, ToolOutput};

/// The search tool a chat gets instead of every connector tool.
pub const SEARCH_TOOL: &str = "mcp_search_tools";
/// The tool that runs a connector tool found with [`SEARCH_TOOL`].
pub const CALL_TOOL: &str = "mcp_call_tool";
/// Defer past this many tools...
const DEFER_COUNT: usize = 40;
/// ...or this many characters of names, descriptions and schemas.
const DEFER_CHARS: usize = 60_000;
/// The ask prompt shows at most this many characters of the arguments.
const PREVIEW_CHARS: usize = 600;
/// Results of a search, by default.
const DEFAULT_LIMIT: f64 = 8.0;
/// Results of a search, at most.
const MAX_LIMIT: f64 = 25.0;

// ── Per-chat state: the switched-off connectors ─────────────────────────────

fn chat_off() -> &'static Mutex<HashMap<String, HashSet<String>>> {
    static OFF: OnceLock<Mutex<HashMap<String, HashSet<String>>>> = OnceLock::new();
    OFF.get_or_init(Default::default)
}

fn off_of(chat: &str) -> HashSet<String> {
    chat_off().lock().expect("connector switches lock").get(chat).cloned().unwrap_or_default()
}

/// The connectors (pool keys) switched off for `chat`: the webview reports them
/// when a chat's tools are set up and whenever a switch changes. An empty list
/// switches every connector on.
#[tauri::command]
pub fn agent_mcp_chat_config(chat: String, off: Vec<String>) {
    let mut all = chat_off().lock().expect("connector switches lock");
    if off.is_empty() {
        all.remove(&chat);
    } else {
        all.insert(chat, off.into_iter().collect());
    }
}

// ── A chat's connector tools ────────────────────────────────────────────────

/// One tool of `mcp_chat_tools`: what the chat sees, plus where it comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub permission: Permission,
    /// The pool's key of the server (`name`, or `name#project`).
    pub server: String,
    /// The tool's own name on the server.
    pub tool: String,
    /// The server marks it as only reading (`readOnlyHint`).
    pub read_only: bool,
}

impl ChatTool {
    fn from_value(value: &Value) -> Option<Self> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        Some(Self {
            name: text("name")?,
            description: text("description").unwrap_or_default(),
            input_schema: value.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object" })),
            permission: value.get("permission").and_then(|p| serde_json::from_value(p.clone()).ok()).unwrap_or(Permission::Ask),
            server: text("server").unwrap_or_default(),
            tool: text("tool").unwrap_or_default(),
            read_only: value.get("readOnly").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    /// `{name, description, inputSchema}`: the definition the engine takes.
    fn definition(&self) -> Value {
        json!({ "name": self.name, "description": self.description, "inputSchema": self.input_schema })
    }
}

/// The pool's tools for a chat, without those of the connectors switched off.
fn parse_tools(list: &[Value], off: &HashSet<String>) -> Vec<ChatTool> {
    list.iter().filter_map(ChatTool::from_value).filter(|tool| !off.contains(&tool.server)).collect()
}

/// The tools of `chat` working in `folder` now. `open_project` first connects the
/// approved servers of the folder's project (a chat started there).
async fn chat_tools(app: &AppHandle, chat: &str, folder: &Path, open_project: bool) -> Vec<ChatTool> {
    let Some(state) = app.try_state::<crate::mcp::McpState>() else {
        return Vec::new();
    };
    if open_project {
        let _ = state.0.open_project(folder).await;
    }
    parse_tools(&state.0.chat_tools(Some(folder)), &off_of(chat))
}

// ── Names ───────────────────────────────────────────────────────────────────

/// `mcp__<server>__<tool>` names a connector's tool.
pub fn is_mcp_tool(name: &str) -> bool {
    name.starts_with("mcp__")
}

/// The server and tool parts of an exposed name: `mcp__a__b__c` is `("a", "b__c")`.
pub fn label(name: &str) -> (String, String) {
    let parts: Vec<&str> = name.split("__").collect();
    (parts.get(1).copied().unwrap_or("").to_string(), parts.get(2..).unwrap_or(&[]).join("__"))
}

// ── Deferral and search ─────────────────────────────────────────────────────

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Whether `tools` should sit behind search rather than be listed: past 40 tools,
/// or about 60k characters of names, descriptions and schemas.
pub fn should_defer(tools: &[ChatTool]) -> bool {
    if tools.len() > DEFER_COUNT {
        return true;
    }
    let size: usize = tools.iter().map(|t| utf16_len(&t.name) + utf16_len(&t.description) + utf16_len(&t.input_schema.to_string())).sum();
    size > DEFER_CHARS
}

/// The two tools a chat gets instead of `tools`.
pub fn search_tool_definitions(tools: &[ChatTool]) -> Vec<Value> {
    let mut servers: Vec<(String, usize)> = Vec::new();
    for tool in tools {
        let server = label(&tool.name).0;
        match servers.iter_mut().find(|(s, _)| *s == server) {
            Some((_, count)) => *count += 1,
            None => servers.push((server, 1)),
        }
    }
    let listing = servers.iter().map(|(server, count)| format!("{server} ({count})")).collect::<Vec<_>>().join(", ");
    vec![
        json!({
            "name": SEARCH_TOOL,
            "description": format!(
                "Find tools of the connected MCP servers \u{2014} {listing} \u{2014} and get their input schemas. Search by what you want to do (\"create issue\", \"query database\"), or pass \"select:<name>,<name>\" for exact tool names. Then run one with {CALL_TOOL}."
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Words describing the task, or \"select:name1,name2\"." },
                    "limit": { "type": "number", "description": "At most this many results (default 8)." },
                },
                "required": ["query"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": CALL_TOOL,
            "description": format!("Run a connector tool found with {SEARCH_TOOL}, by its full name (mcp__server__tool), with arguments matching its input schema."),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "The tool's full name, e.g. mcp__github__create_issue." },
                    "arguments": { "type": "object", "description": "The tool's arguments." },
                },
                "required": ["name"],
                "additionalProperties": false,
            },
        }),
    ]
}

/// The definitions a chat gets for `tools`: all of them, or the search pair.
pub fn definitions(tools: &[ChatTool]) -> Vec<Value> {
    if should_defer(tools) {
        search_tool_definitions(tools)
    } else {
        tools.iter().map(ChatTool::definition).collect()
    }
}

/// Order like `String.prototype.localeCompare` does for tool names: punctuation
/// before digits before letters, letters without regard to case, a lower-case
/// letter before its capital when nothing else differs.
fn locale_cmp(a: &str, b: &str) -> Ordering {
    fn rank(c: char) -> u8 {
        if c.is_alphabetic() {
            2
        } else if c.is_numeric() {
            1
        } else {
            0
        }
    }
    let (mut left, mut right) = (a.chars(), b.chars());
    let mut tertiary = Ordering::Equal;
    loop {
        match (left.next(), right.next()) {
            (None, None) => return tertiary,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let primary = rank(x).cmp(&rank(y)).then_with(|| x.to_lowercase().cmp(y.to_lowercase()));
                if primary != Ordering::Equal {
                    return primary;
                }
                if tertiary == Ordering::Equal && x != y {
                    // Lower case first.
                    tertiary = if x.is_lowercase() { Ordering::Less } else { Ordering::Greater };
                }
            }
        }
    }
}

/// `Array.prototype.slice(0, end)` for a number that may be fractional or negative.
fn slice_end(len: usize, end: f64) -> usize {
    let end = end.trunc();
    if end.is_nan() {
        0
    } else if end < 0.0 {
        (len as f64 + end).max(0.0) as usize
    } else {
        end.min(len as f64) as usize
    }
}

/// The best matches for `query` among `tools`: `select:a,b` picks tools by exact
/// name (full or the tool's own); otherwise by words, three points for a hit in
/// the name and one in the description.
pub fn search_tools<'a>(tools: &'a [ChatTool], query: &str, limit: f64) -> Vec<&'a ChatTool> {
    let q = query.trim();
    if q.to_lowercase().starts_with("select:") {
        let rest: String = q.chars().skip(7).collect();
        let names: Vec<&str> = rest.split(',').map(str::trim).filter(|n| !n.is_empty()).collect();
        return tools.iter().filter(|t| names.contains(&t.name.as_str()) || names.contains(&label(&t.name).1.as_str())).collect();
    }
    let lower = q.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit())).filter(|w| w.len() > 1).collect();
    if words.is_empty() {
        return tools.iter().take(slice_end(tools.len(), limit)).collect();
    }
    let mut scored: Vec<(usize, &ChatTool)> = tools
        .iter()
        .map(|tool| {
            let name = tool.name.to_lowercase();
            let description = tool.description.to_lowercase();
            let score = words.iter().map(|w| usize::from(name.contains(w)) * 3 + usize::from(description.contains(w))).sum::<usize>();
            (score, tool)
        })
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| locale_cmp(&a.1.name, &b.1.name)));
    let take = limit.min(MAX_LIMIT).max(1.0).trunc() as usize;
    scored.into_iter().take(take).map(|(_, tool)| tool).collect()
}

/// `Number(value) || 8`: the search limit as the model gave it.
fn limit_of(value: Option<&Value>) -> f64 {
    let number = match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => {
            let s = s.trim();
            if s.is_empty() {
                0.0
            } else {
                s.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
        Some(Value::Bool(true)) => 1.0,
        Some(Value::Bool(false)) | Some(Value::Null) => 0.0,
        _ => f64::NAN,
    };
    if number == 0.0 || number.is_nan() {
        DEFAULT_LIMIT
    } else {
        number
    }
}

/// `String(value ?? "")`.
fn js_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items.iter().map(|v| js_string(Some(v))).collect::<Vec<_>>().join(","),
        Some(Value::Object(_)) => "[object Object]".to_string(),
        Some(other) => other.to_string(),
    }
}

/// The answer to `mcp_search_tools`.
fn search_reply(tools: &[ChatTool], args: &Value) -> ToolOutput {
    let query = js_string(args.get("query"));
    let found = search_tools(tools, &query, limit_of(args.get("limit")));
    if found.is_empty() {
        return ToolOutput::text(format!("No connector tool matches \"{query}\". Try other words."));
    }
    let text = found
        .iter()
        .map(|t| format!("{}: {}\n{}", t.name, if t.description.is_empty() { "(no description)" } else { t.description.as_str() }, t.input_schema))
        .collect::<Vec<_>>()
        .join("\n\n");
    let results: Vec<Value> = found.iter().map(|t| t.definition()).collect();
    ToolOutput::text(text).with_structured(json!({ "tools": results }))
}

// ── Permissions and prompts ─────────────────────────────────────────────────

/// What the person's answer to the Ask prompt means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    /// "Allow once".
    Once,
    /// "Always allow": run, and save Allow.
    Always,
    /// "Deny".
    Deny,
    /// The question became moot (the call was dropped).
    Gone,
}

const OPTIONS: [&str; 3] = ["Allow once", "Always allow", "Deny"];

fn answer_of(index: i32) -> Answer {
    match index {
        -1 => Answer::Gone,
        1 => Answer::Always,
        2 => Answer::Deny,
        _ => Answer::Once,
    }
}

/// Whether a tool with `permission` asks before it runs. A blocked tool is not
/// offered; if one is called anyway the pool refuses it.
fn asks(permission: Permission) -> bool {
    permission == Permission::Ask
}

/// The arguments as the Ask prompt shows them: pretty JSON, cut at 600 characters.
fn ask_description(args: &Value) -> String {
    let preview = serde_json::to_string_pretty(args).unwrap_or_default();
    if preview.chars().count() > PREVIEW_CHARS {
        let cut: String = preview.chars().take(PREVIEW_CHARS).collect();
        format!("{cut}\u{2026}")
    } else {
        preview
    }
}

// ── Results ─────────────────────────────────────────────────────────────────

/// An MCP `tools/call` result as the engine reads one: text and image parts,
/// `structuredContent` beside them, `isError`. The pool already cut very long
/// text (100k characters). Parts that are neither text nor image (resources,
/// audio, links) become text, since the engine refuses any other part.
pub fn shape_result(result: &Value) -> ToolOutput {
    let mut content = Vec::new();
    for part in result.get("content").and_then(Value::as_array).into_iter().flatten() {
        let field = |key: &str| part.get(key).and_then(Value::as_str);
        match field("type") {
            Some("text") => content.push(Content::Text(field("text").unwrap_or("").to_string())),
            Some("image") => content.push(Content::Image {
                data: field("data").unwrap_or("").to_string(),
                mime_type: field("mimeType").unwrap_or("").to_string(),
            }),
            Some("resource") => {
                let resource = part.get("resource").unwrap_or(&Value::Null);
                let text = resource.get("text").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| part.to_string());
                content.push(Content::Text(text));
            }
            Some("resource_link") => content.push(Content::Text(field("uri").map(str::to_string).unwrap_or_else(|| part.to_string()))),
            _ => content.push(Content::Text(part.to_string())),
        }
    }
    let structured = result.get("structuredContent").filter(|s| s.is_object()).cloned();
    ToolOutput { content, is_error: result.get("isError") == Some(&Value::Bool(true)), structured }
}

// ── The provider ────────────────────────────────────────────────────────────

/// The connector tools of every chat.
pub struct Connectors;

impl Provider for Connectors {
    fn definitions_for(&self, app: AppHandle, chat: String, folder: Option<PathBuf>) -> BoxFuture<Vec<Value>> {
        Box::pin(async move {
            let folder = folder.unwrap_or_else(|| chat_folder(&app, &chat));
            let mut tools = chat_tools(&app, &chat, &folder, true).await;
            // A read-only session gets only the tools their server marks
            // read-only (session-roles-and-mail FR-13).
            if super::read_only_chat(&app, &chat) {
                tools.retain(|tool| tool.read_only);
            }
            definitions(&tools)
        })
    }

    fn claims(&self, name: &str) -> bool {
        is_mcp_tool(name) || name == SEARCH_TOOL || name == CALL_TOOL
    }

    fn run(&self, ctx: ToolCtx, name: String, args: Value) -> BoxFuture<ToolOutput> {
        Box::pin(async move {
            match name.as_str() {
                SEARCH_TOOL => {
                    let tools = chat_tools(&ctx.app, &ctx.chat, &ctx.folder(), false).await;
                    search_reply(&tools, &args)
                }
                CALL_TOOL => {
                    let name = js_string(args.get("name"));
                    if !is_mcp_tool(&name) {
                        let shown = if name.is_empty() { "(no name)" } else { &name };
                        return ToolOutput::error(format!("{shown} is not a connector tool; find one with {SEARCH_TOOL}."));
                    }
                    let tool_args = args.get("arguments").filter(|a| a.is_object()).cloned().unwrap_or_else(|| json!({}));
                    run_tool(ctx, &name, tool_args).await
                }
                _ => run_tool(ctx, &name, args).await,
            }
        })
    }
}

/// A connector's tool: ask when its permission says so, then call it through the pool.
async fn run_tool(ctx: ToolCtx, name: &str, args: Value) -> ToolOutput {
    let folder = ctx.folder();
    let mut tool = find_tool(&chat_tools(&ctx.app, &ctx.chat, &folder, false).await, name);
    if tool.is_none() {
        // Maybe the chat's project was not open yet (as the webview reloaded the list).
        tool = find_tool(&chat_tools(&ctx.app, &ctx.chat, &folder, true).await, name);
    }
    let Some(tool) = tool else {
        return ToolOutput::error(format!("{name} is not available: its connector is off or disconnected."));
    };
    if !tool.read_only && super::read_only_chat(&ctx.app, &ctx.chat) {
        return ToolOutput::error(super::READ_ONLY_REFUSAL.replace("{tool}", name));
    }
    if asks(tool.permission) {
        let (server, tool_name) = label(name);
        let answer = ctx.ask(&format!("Allow {server} to run {tool_name}?"), &ask_description(&args), &OPTIONS).await;
        match answer_of(answer) {
            Answer::Gone => return ToolOutput::text(""),
            Answer::Deny => return ToolOutput::error("The user declined this tool call."),
            Answer::Always => {
                if let Some(state) = ctx.app.try_state::<crate::mcp::McpState>() {
                    let _ = state.0.set_permission(&tool.server, &tool.tool, Permission::Allow);
                }
            }
            Answer::Once => {}
        }
    }
    let Some(state) = ctx.app.try_state::<crate::mcp::McpState>() else {
        return ToolOutput::error(format!("{name} is not available: its connector is off or disconnected."));
    };
    let pool = state.0.clone();
    let arguments: Map<String, Value> = args.as_object().cloned().unwrap_or_default();
    let call = pool.call(name, Some(arguments), Some(folder.as_path()));
    tokio::select! {
        result = call => match result {
            Ok(result) => shape_result(&result),
            Err(error) => ToolOutput::error(error),
        },
        // The engine dropped the call: nobody waits for the answer.
        _ = ctx.cancel.cancelled() => ToolOutput::text(""),
    }
}

fn find_tool(tools: &[ChatTool], name: &str) -> Option<ChatTool> {
    tools.iter().find(|t| t.name == name).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, description: &str) -> ChatTool {
        ChatTool {
            name: name.into(),
            description: description.into(),
            input_schema: json!({ "type": "object", "properties": { "q": { "type": "string" } } }),
            permission: Permission::Allow,
            server: name.split("__").nth(1).unwrap_or("").into(),
            tool: label(name).1,
            read_only: false,
        }
    }

    // docs/features/session-roles-and-mail/spec.md (FR-13).
    #[test]
    fn read_only_chat_keeps_the_servers_read_only_hint() {
        let hinted = json!({"name": "mcp__docs__search", "server": "docs", "tool": "search", "readOnly": true});
        let plain = json!({"name": "mcp__docs__write", "server": "docs", "tool": "write"});
        assert!(ChatTool::from_value(&hinted).unwrap().read_only);
        assert!(!ChatTool::from_value(&plain).unwrap().read_only, "no hint: it may change something");
    }

    fn many(count: usize) -> Vec<ChatTool> {
        (0..count).map(|i| tool(&format!("mcp__s__t{i}"), "does a thing")).collect()
    }

    #[test]
    fn names_split_into_server_and_tool() {
        assert!(is_mcp_tool("mcp__github__create_issue"));
        assert!(!is_mcp_tool("mcp_search_tools"));
        assert!(!is_mcp_tool("browser_click"));
        assert_eq!(label("mcp__github__create_issue"), ("github".into(), "create_issue".into()));
        // A tool name that holds the separator keeps it.
        assert_eq!(label("mcp__a__b__c"), ("a".into(), "b__c".into()));
        assert_eq!(label("mcp__only"), ("only".into(), String::new()));
        assert_eq!(label("plain"), (String::new(), String::new()));
    }

    #[test]
    fn the_provider_claims_connector_tools_and_the_search_pair_only() {
        let provider = Connectors;
        assert!(provider.claims("mcp__github__create_issue"));
        assert!(provider.claims("mcp_search_tools"));
        assert!(provider.claims("mcp_call_tool"));
        assert!(!provider.claims("browser_click"));
        assert!(!provider.claims("terminal_run"));
        assert!(!provider.claims("mcp_other"));
    }

    #[test]
    fn tools_come_from_the_pool_without_the_switched_off_connectors() {
        let list = vec![
            json!({ "name": "mcp__a__x", "description": "[a] x", "inputSchema": { "type": "object" }, "permission": "ask", "server": "a", "tool": "x" }),
            json!({ "name": "mcp__b__y", "description": "[b] y", "inputSchema": { "type": "object" }, "permission": "allow", "server": "b#proj", "tool": "y" }),
            json!({ "description": "no name" }),
        ];
        let all = parse_tools(&list, &HashSet::new());
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].permission, Permission::Ask);
        assert_eq!(all[1].permission, Permission::Allow);
        assert_eq!(all[1].server, "b#proj");
        // A project server is switched off by its pool key.
        let off: HashSet<String> = ["b#proj".to_string()].into();
        let rest = parse_tools(&list, &off);
        assert_eq!(rest.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["mcp__a__x"]);
    }

    #[test]
    fn switches_are_kept_per_chat_and_cleared_by_an_empty_list() {
        agent_mcp_chat_config("chat-switch-1".into(), vec!["a".into(), "b#p".into()]);
        assert_eq!(off_of("chat-switch-1"), HashSet::from(["a".to_string(), "b#p".to_string()]));
        assert!(off_of("chat-switch-2").is_empty());
        agent_mcp_chat_config("chat-switch-1".into(), vec![]);
        assert!(off_of("chat-switch-1").is_empty());
    }

    #[test]
    fn a_few_tools_are_listed_with_name_description_and_schema() {
        let tools = vec![tool("mcp__gh__issues", "[gh] list issues")];
        assert!(!should_defer(&tools));
        let defs = definitions(&tools);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0]["name"], "mcp__gh__issues");
        assert_eq!(defs[0]["description"], "[gh] list issues");
        assert_eq!(defs[0]["inputSchema"]["type"], "object");
        assert!(defs[0].get("permission").is_none());
        assert!(definitions(&[]).is_empty());
    }

    #[test]
    fn deferral_starts_past_forty_tools() {
        assert!(!should_defer(&many(40)));
        assert!(should_defer(&many(41)));
        let defs = definitions(&many(41));
        assert_eq!(defs.iter().map(|d| d["name"].as_str().unwrap()).collect::<Vec<_>>(), [SEARCH_TOOL, CALL_TOOL]);
    }

    #[test]
    fn deferral_also_starts_past_sixty_thousand_characters() {
        let big = |chars: usize| vec![ChatTool { description: "x".repeat(chars), ..tool("mcp__s__t", "") }];
        let fixed = {
            let t = tool("mcp__s__t", "");
            t.name.len() + t.input_schema.to_string().len()
        };
        assert!(!should_defer(&big(DEFER_CHARS - fixed)));
        assert!(should_defer(&big(DEFER_CHARS - fixed + 1)));
    }

    #[test]
    fn the_search_pair_lists_servers_with_their_counts() {
        let tools = vec![tool("mcp__gh__a", "d"), tool("mcp__db__q", "d"), tool("mcp__gh__b", "d")];
        let defs = search_tool_definitions(&tools);
        assert_eq!(defs[0]["name"], "mcp_search_tools");
        let text = defs[0]["description"].as_str().unwrap();
        assert!(text.starts_with("Find tools of the connected MCP servers \u{2014} gh (2), db (1) \u{2014} and get their input schemas."));
        assert!(text.ends_with("Then run one with mcp_call_tool."));
        assert_eq!(defs[0]["inputSchema"]["required"], json!(["query"]));
        assert_eq!(defs[0]["inputSchema"]["additionalProperties"], false);
        assert_eq!(defs[1]["name"], "mcp_call_tool");
        assert_eq!(defs[1]["inputSchema"]["required"], json!(["name"]));
        assert_eq!(defs[1]["inputSchema"]["properties"]["arguments"]["type"], "object");
    }

    fn sample() -> Vec<ChatTool> {
        vec![
            tool("mcp__gh__create_issue", "[gh] Create an issue in a repository"),
            tool("mcp__gh__list_issues", "[gh] List the issues of a repository"),
            tool("mcp__gh__search_code", "[gh] Search code"),
            tool("mcp__db__run_query", "[db] Run a SQL query against the database"),
        ]
    }

    fn names(found: Vec<&ChatTool>) -> Vec<&str> {
        found.into_iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn search_ranks_name_hits_over_description_hits() {
        let tools = sample();
        // "issue": name (3) + description (1) for the two issue tools; "repository" is only a description hit.
        assert_eq!(names(search_tools(&tools, "issue", 8.0)), ["mcp__gh__create_issue", "mcp__gh__list_issues"]);
        // "query database": run_query matches "query" in name and description, "database" in description.
        assert_eq!(names(search_tools(&tools, "query database", 8.0)), ["mcp__db__run_query"]);
        // Score ties sort by name.
        assert_eq!(names(search_tools(&tools, "gh", 8.0)), ["mcp__gh__create_issue", "mcp__gh__list_issues", "mcp__gh__search_code"]);
        // Name hits (3) beat description-only hits (1).
        assert_eq!(names(search_tools(&tools, "repository", 8.0)), ["mcp__gh__create_issue", "mcp__gh__list_issues"]);
        let mixed = vec![tool("mcp__s__a", "mentions zeta"), tool("mcp__s__zeta", "plain")];
        assert_eq!(names(search_tools(&mixed, "zeta", 8.0)), ["mcp__s__zeta", "mcp__s__a"]);
        assert!(search_tools(&tools, "zebra", 8.0).is_empty());
    }

    #[test]
    fn search_limits_results() {
        let tools = sample();
        assert_eq!(search_tools(&tools, "gh", 2.0).len(), 2);
        // At least one, at most 25.
        assert_eq!(search_tools(&tools, "gh", 0.2).len(), 1);
        assert_eq!(search_tools(&many(30), "thing", 100.0).len(), 25);
        // Words under two characters do not count: the first `limit` tools.
        assert_eq!(names(search_tools(&tools, "a", 2.0)), ["mcp__gh__create_issue", "mcp__gh__list_issues"]);
        assert_eq!(search_tools(&tools, "   ", 8.0).len(), 4);
    }

    #[test]
    fn select_picks_exact_names_in_either_form() {
        let tools = sample();
        assert_eq!(names(search_tools(&tools, "select:mcp__db__run_query", 8.0)), ["mcp__db__run_query"]);
        assert_eq!(names(search_tools(&tools, "SELECT: search_code , mcp__gh__list_issues", 8.0)), ["mcp__gh__list_issues", "mcp__gh__search_code"]);
        assert!(search_tools(&tools, "select:nothing", 8.0).is_empty());
    }

    #[test]
    fn the_search_limit_reads_like_the_scripts_number_or_eight() {
        assert_eq!(limit_of(None), 8.0);
        assert_eq!(limit_of(Some(&json!(3))), 3.0);
        assert_eq!(limit_of(Some(&json!("5"))), 5.0);
        assert_eq!(limit_of(Some(&json!(0))), 8.0);
        assert_eq!(limit_of(Some(&json!("abc"))), 8.0);
        assert_eq!(limit_of(Some(&json!(null))), 8.0);
        assert_eq!(limit_of(Some(&json!({}))), 8.0);
    }

    #[test]
    fn ties_order_like_a_locale_compare() {
        assert_eq!(locale_cmp("mcp__a__b", "mcp__a__b"), Ordering::Equal);
        assert_eq!(locale_cmp("list", "list_issues"), Ordering::Less);
        assert_eq!(locale_cmp("a", "B"), Ordering::Less);
        assert_eq!(locale_cmp("a", "A"), Ordering::Less);
        assert_eq!(locale_cmp("x_y", "x2"), Ordering::Less);
        assert_eq!(locale_cmp("x2", "xa"), Ordering::Less);
    }

    #[test]
    fn a_search_answers_with_text_and_structured_tools() {
        let tools = sample();
        let reply = search_reply(&tools, &json!({ "query": "run query" })).to_json();
        let text = reply["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("mcp__db__run_query: [db] Run a SQL query against the database\n{"));
        assert_eq!(reply["structuredContent"]["tools"][0]["name"], "mcp__db__run_query");
        assert_eq!(reply["structuredContent"]["tools"][0]["inputSchema"]["type"], "object");
        assert!(reply.get("isError").is_none());
        // Two results are separated by a blank line; a tool without a description says so.
        let two = vec![tool("mcp__s__alpha", ""), tool("mcp__s__alpine", "")];
        let reply = search_reply(&two, &json!({ "query": "alp" })).to_json();
        let text = reply["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("mcp__s__alpha: (no description)\n{"));
        assert!(text.contains("}\n\nmcp__s__alpine: (no description)"));
    }

    #[test]
    fn a_search_without_a_match_says_so() {
        let reply = search_reply(&sample(), &json!({ "query": "zebra" })).to_json();
        assert_eq!(reply["content"][0]["text"], "No connector tool matches \"zebra\". Try other words.");
        assert!(reply.get("isError").is_none());
        assert!(reply.get("structuredContent").is_none());
        let none = search_reply(&sample(), &json!({})).to_json();
        assert!(!none["content"][0]["text"].as_str().unwrap().is_empty());
    }

    #[test]
    fn only_ask_tools_ask() {
        assert!(asks(Permission::Ask));
        assert!(!asks(Permission::Allow));
        assert!(!asks(Permission::Block));
    }

    #[test]
    fn answers_map_to_run_remember_deny_or_gone() {
        assert_eq!(OPTIONS, ["Allow once", "Always allow", "Deny"]);
        assert_eq!(answer_of(0), Answer::Once);
        assert_eq!(answer_of(1), Answer::Always);
        assert_eq!(answer_of(2), Answer::Deny);
        assert_eq!(answer_of(-1), Answer::Gone);
    }

    #[test]
    fn the_prompt_shows_the_arguments_cut_at_six_hundred_characters() {
        assert_eq!(ask_description(&json!({})), "{}");
        assert_eq!(ask_description(&json!({ "a": 1 })), "{\n  \"a\": 1\n}");
        let long = ask_description(&json!({ "text": "x".repeat(2000) }));
        assert_eq!(long.chars().count(), PREVIEW_CHARS + 1);
        assert!(long.ends_with('\u{2026}'));
        assert!(long.starts_with("{\n  \"text\": \"xxx"));
    }

    #[test]
    fn a_result_keeps_text_images_structured_content_and_the_error_flag() {
        let out = shape_result(&json!({
            "content": [
                { "type": "text", "text": "hello" },
                { "type": "image", "data": "AAAA", "mimeType": "image/png" },
            ],
            "structuredContent": { "n": 1 },
        }))
        .to_json();
        assert_eq!(out["content"][0], json!({ "type": "text", "text": "hello" }));
        assert_eq!(out["content"][1], json!({ "type": "image", "data": "AAAA", "mimeType": "image/png" }));
        assert_eq!(out["structuredContent"]["n"], 1);
        assert!(out.get("isError").is_none());
        let failed = shape_result(&json!({ "content": [{ "type": "text", "text": "boom" }], "isError": true })).to_json();
        assert_eq!(failed["isError"], true);
        assert_eq!(failed["content"][0]["text"], "boom");
        // `isError: false` and a non-object structured content are not carried.
        let odd = shape_result(&json!({ "content": [], "isError": false, "structuredContent": [1] })).to_json();
        assert!(odd.get("isError").is_none());
        assert!(odd.get("structuredContent").is_none());
        assert_eq!(odd["content"], json!([]));
    }

    #[test]
    fn parts_that_are_not_text_or_images_become_text() {
        let out = shape_result(&json!({
            "content": [
                { "type": "resource", "resource": { "uri": "file:///a", "text": "body" } },
                { "type": "resource_link", "uri": "file:///b" },
                { "type": "audio", "data": "AA" },
            ],
        }))
        .to_json();
        assert_eq!(out["content"][0]["text"], "body");
        assert_eq!(out["content"][1]["text"], "file:///b");
        assert_eq!(out["content"][2]["type"], "text");
        assert!(out["content"][2]["text"].as_str().unwrap().contains("audio"));
    }

    #[test]
    fn a_blocked_or_missing_tool_is_not_in_the_chats_list() {
        // The pool leaves Block tools out of `chat_tools`; a call finds nothing.
        let list = vec![json!({ "name": "mcp__a__x", "permission": "allow", "server": "a", "tool": "x" })];
        let tools = parse_tools(&list, &HashSet::new());
        assert!(find_tool(&tools, "mcp__a__x").is_some());
        assert!(find_tool(&tools, "mcp__a__blocked").is_none());
    }
}
