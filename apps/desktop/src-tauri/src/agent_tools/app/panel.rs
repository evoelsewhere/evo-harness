//! `app_panel`: open, close or read the side panel tools of the agent's chat.
//!
//! The shell cannot touch the webview's panel state, so opening and closing are
//! an event (`agent://panel { chat, panel, action, window }`) that the webview
//! applies (`stores/agent-events.ts`). What the panels look like comes back the
//! other way: the webview reports each chat's open tabs with
//! [`agent_panels_report`] whenever they change, and the shell keeps that
//! mirror. `status` reads the mirror and the tool confirms an open or close by
//! watching it. A tool never touches another chat's tabs: it always names the
//! calling chat.
//!
//! Spec: docs/features/agent-browser (FR-18, US-4), plan 3.3, 3.5.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::super::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};
use super::{choice, required_str};

/// The side panel's tools, as the webview names them.
pub const PANELS: [&str; 5] = ["terminal", "browser", "files", "memory", "side-chat"];
const ACTIONS: [&str; 3] = ["open", "close", "status"];

/// How long an open or close waits for the webview to report the new state.
const CONFIRM_MS: u64 = 2000;
/// A browser page needs a moment to take its place before pixels can be used.
const SETTLE_MS: u64 = 350;

/// What the webview last reported about one chat's side panel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatPanels {
    pub open_tabs: Vec<String>,
    pub active: Option<String>,
    /// The chat is the one a window shows.
    pub showing: bool,
    /// The side panel itself is open (not collapsed to its rail); unknown for old reports.
    pub panel_open: Option<bool>,
}

static MIRROR: Mutex<Option<HashMap<String, ChatPanels>>> = Mutex::new(None);

fn mirror_of(chat: &str) -> Option<ChatPanels> {
    MIRROR.lock().expect("panel mirror lock").as_ref().and_then(|map| map.get(chat).cloned())
}

/// Keep `state` for `chat`; a chat with nothing open that is not showing is forgotten.
fn record(map: &mut HashMap<String, ChatPanels>, chat: &str, state: ChatPanels) {
    if state.open_tabs.is_empty() && !state.showing {
        map.remove(chat);
    } else {
        map.insert(chat.to_string(), state);
    }
}

/// The webview reports a chat's side panel: its open tabs, the visible one, and
/// whether a window shows the chat (`stores/agent-events.ts`).
#[tauri::command]
pub fn agent_panels_report(chat: String, open_tabs: Vec<String>, active_panel: Option<String>, showing: bool, panel_open: Option<bool>) {
    let open_tabs: Vec<String> = open_tabs.into_iter().filter(|tab| PANELS.contains(&tab.as_str())).collect();
    let active = active_panel.filter(|panel| open_tabs.contains(panel));
    let state = ChatPanels { open_tabs, active, showing, panel_open };
    record(MIRROR.lock().expect("panel mirror lock").get_or_insert_with(HashMap::new), &chat, state);
}

fn title_of(panel: &str) -> &'static str {
    match panel {
        "terminal" => "Terminal",
        "browser" => "Browser",
        "files" => "Files",
        "memory" => "Memory",
        _ => "Side chat",
    }
}

/// The panel state as the agent reads it.
pub fn format_status(state: Option<&ChatPanels>) -> String {
    let Some(state) = state else {
        return "No tool is open in this chat's side panel, and no window reports showing this chat. A tool opened now is marked for when the chat shows.".into();
    };
    let mut lines = Vec::new();
    lines.push(match (state.showing, state.panel_open) {
        (false, _) => "This chat is not showing in a window; its tools show when the person opens it.".to_string(),
        (true, Some(false)) => "The side panel is collapsed to its rail.".to_string(),
        (true, _) => "The side panel is open.".to_string(),
    });
    if state.open_tabs.is_empty() {
        lines.push("Open tools: none.".into());
    } else {
        let open: Vec<String> = state
            .open_tabs
            .iter()
            .map(|tab| if state.active.as_deref() == Some(tab) { format!("{tab} (visible)") } else { tab.clone() })
            .collect();
        lines.push(format!("Open tools: {}.", open.join(", ")));
    }
    let closed: Vec<&str> = PANELS.iter().copied().filter(|panel| !state.open_tabs.iter().any(|tab| tab == panel)).collect();
    if !closed.is_empty() {
        lines.push(format!("Not open: {}.", closed.join(", ")));
    }
    lines.join("\n")
}

/// Whether the reported state shows `action` done for `panel`.
fn reached(state: Option<&ChatPanels>, panel: &str, action: &str) -> bool {
    let open = state.is_some_and(|state| state.open_tabs.iter().any(|tab| tab == panel));
    match action {
        "open" => open && state.is_some_and(|state| state.active.as_deref() == Some(panel) && state.panel_open != Some(false)),
        _ => !open,
    }
}

async fn wait_reached(ctx: &ToolCtx, panel: &str, action: &str) -> bool {
    let deadline = Instant::now() + Duration::from_millis(CONFIRM_MS);
    loop {
        if reached(mirror_of(&ctx.chat).as_ref(), panel, action) {
            return true;
        }
        if ctx.cancelled() || Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "panel": { "type": "string", "enum": PANELS, "description": "The tool: terminal, browser, files, memory or side-chat." },
            "action": { "type": "string", "enum": ACTIONS, "description": "open (default): show it and make it the visible tool; close: close its tab; status: which tools are open." }
        },
        "required": ["panel"],
        "additionalProperties": false
    })
}

async fn run(ctx: ToolCtx, args: Value) -> ToolOutput {
    let (panel, action) = match (|| -> Result<(&'static str, &'static str), String> {
        let panel = required_str(&args, "panel")?;
        let panel = PANELS.iter().copied().find(|known| known.eq_ignore_ascii_case(panel)).ok_or_else(|| format!("`panel` must be one of: {}.", PANELS.join(", ")))?;
        Ok((panel, choice(&args, "action", &ACTIONS, "open")?))
    })() {
        Ok(parsed) => parsed,
        Err(message) => return ToolOutput::error(message),
    };
    let title = title_of(panel);
    if action == "status" {
        return ToolOutput::text(format_status(mirror_of(&ctx.chat).as_ref()));
    }
    if action == "close" && reached(mirror_of(&ctx.chat).as_ref(), panel, "close") {
        return ToolOutput::text(format!("{title} is not open for this chat."));
    }
    ctx.emit("agent://panel", json!({ "chat": ctx.chat, "panel": panel, "action": action, "window": ctx.window() }));
    let confirmed = wait_reached(&ctx, panel, action).await;
    if ctx.cancelled() {
        return ToolOutput::text("");
    }
    if !confirmed {
        return ToolOutput::text(format!(
            "Asked the app to {action} {title} for this chat; no window confirmed it (the app may have no window open). It applies when a window shows this chat."
        ));
    }
    let state = mirror_of(&ctx.chat);
    if action == "close" {
        return ToolOutput::text(format!("Closed {title} for this chat.\n{}", format_status(state.as_ref())));
    }
    let showing = state.as_ref().is_some_and(|state| state.showing);
    if showing && panel == "browser" {
        // Let the page take its place and size before the agent works on pixels.
        tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;
    }
    let note = if showing { "It shows now." } else { "This chat is not showing; the tool shows when the person opens the chat." };
    ToolOutput::text(format!("Opened {title} for this chat. {note}\n{}", format_status(state.as_ref())))
}

fn run_tool(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(run(ctx, args))
}

pub fn tools() -> Vec<Tool> {
    vec![Tool {
        name: "app_panel",
        description: "Open, focus or close one of the app's side panel tools (terminal, browser, files, memory, side-chat) for this chat, or read which are open. The panel shows when the person looks at this chat; use it to put a page, a shell or a file in front of the person. It never touches another chat's tools.",
        schema,
        // Moves the app's view on; nothing the person keeps is altered, so it never asks.
        class: Class::Act,
        run: run_tool,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panels(tabs: &[&str], active: Option<&str>, showing: bool, open: Option<bool>) -> ChatPanels {
        ChatPanels { open_tabs: tabs.iter().map(|t| t.to_string()).collect(), active: active.map(str::to_string), showing, panel_open: open }
    }

    #[test]
    fn status_lists_open_and_closed_tools() {
        let text = format_status(Some(&panels(&["terminal", "files"], Some("files"), true, Some(true))));
        assert!(text.contains("The side panel is open."), "{text}");
        assert!(text.contains("Open tools: terminal, files (visible)."), "{text}");
        assert!(text.contains("Not open: browser, memory, side-chat."), "{text}");
    }

    #[test]
    fn status_says_when_the_chat_is_not_showing_or_the_panel_is_collapsed() {
        let hidden = format_status(Some(&panels(&["browser"], Some("browser"), false, Some(true))));
        assert!(hidden.contains("not showing"), "{hidden}");
        let collapsed = format_status(Some(&panels(&["browser"], Some("browser"), true, Some(false))));
        assert!(collapsed.contains("collapsed"), "{collapsed}");
        let empty = format_status(Some(&panels(&[], None, true, Some(false))));
        assert!(empty.contains("Open tools: none."), "{empty}");
        assert!(format_status(None).contains("No tool is open"));
    }

    #[test]
    fn open_is_reached_when_the_tool_is_visible_and_the_panel_open() {
        assert!(!reached(None, "files", "open"));
        assert!(!reached(Some(&panels(&["files"], None, true, Some(true))), "files", "open"));
        assert!(!reached(Some(&panels(&["files"], Some("files"), true, Some(false))), "files", "open"));
        assert!(reached(Some(&panels(&["files"], Some("files"), true, Some(true))), "files", "open"));
        // A chat that is not showing: the tab is marked, the panel flag is not asked about.
        assert!(reached(Some(&panels(&["files"], Some("files"), false, Some(true))), "files", "open"));
        assert!(!reached(Some(&panels(&["terminal", "files"], Some("terminal"), true, Some(true))), "files", "open"));
    }

    #[test]
    fn close_is_reached_when_the_tab_is_gone() {
        assert!(reached(None, "files", "close"));
        assert!(reached(Some(&panels(&["terminal"], Some("terminal"), true, None)), "files", "close"));
        assert!(!reached(Some(&panels(&["files"], Some("files"), true, None)), "files", "close"));
    }

    #[test]
    fn the_mirror_forgets_a_hidden_chat_with_nothing_open() {
        let mut map = HashMap::new();
        record(&mut map, "a", panels(&["files"], Some("files"), false, Some(true)));
        assert!(map.contains_key("a"));
        record(&mut map, "b", panels(&[], None, true, Some(false)));
        assert!(map.contains_key("b"), "a showing chat stays, so status can say it shows nothing");
        record(&mut map, "a", panels(&[], None, false, None));
        assert!(!map.contains_key("a"));
    }

    #[test]
    fn a_report_drops_unknown_tools_and_a_hidden_active_tab() {
        agent_panels_report("panel-test".into(), vec!["files".into(), "plugins".into()], Some("plugins".into()), true, Some(true));
        let state = mirror_of("panel-test").unwrap();
        assert_eq!(state.open_tabs, ["files"]);
        assert_eq!(state.active, None);
        agent_panels_report("panel-test".into(), vec![], None, false, None);
        assert!(mirror_of("panel-test").is_none());
    }

    #[test]
    fn the_schema_names_the_five_panels() {
        let schema = schema();
        assert_eq!(schema["properties"]["panel"]["enum"].as_array().unwrap().len(), 5);
        assert_eq!(schema["required"][0], "panel");
        assert_eq!(schema["additionalProperties"], false);
    }
}
