//! What the agent is doing in a page or a shell, for the cards over the chat
//! (docs/features/floating-cards, FR-2): `agent://activity`, sent when a browser
//! or terminal tool call starts and when it ends.
//!
//! The payload names the chat, the kind (`browser` or `terminal`), the tool, the
//! tab when it is known (a browser tab resolved the way the tool resolves it, a
//! terminal tab from the call's own `tabId`) and a few short arguments for the
//! card's line ("Clicking ref_4"). The window draws the words; nothing here
//! decides anything.

use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter};

/// Which cards a tool's activity belongs to.
pub fn kind_of(tool: &str) -> Option<&'static str> {
    if tool.starts_with("browser_") {
        Some("browser")
    } else if tool.starts_with("terminal_") {
        Some("terminal")
    } else if tool.starts_with("computer_") {
        Some("computer")
    } else {
        None
    }
}

/// The arguments worth showing on a card, each cut short. For the computer app
/// tool, the batch's first acting step as the person reads it.
pub fn brief_args(args: &Value) -> Value {
    if let Some(actions) = args.get("actions").and_then(Value::as_array) {
        let step = super::computer::tool::approval_lines(actions).into_iter().next();
        return json!({ "action": step.map(|s| s.chars().take(40).collect::<String>()) });
    }
    const KEYS: [(&str, usize); 11] = [
        ("ref", 40),
        ("url", 120),
        ("query", 60),
        ("text", 60),
        ("key", 30),
        ("direction", 12),
        ("action", 20),
        ("command", 80),
        ("tabId", 20),
        ("button", 10),
        ("frame", 10),
    ];
    let mut out = Map::new();
    for (key, max) in KEYS {
        match args.get(key) {
            Some(Value::String(text)) => {
                let cut: String = text.chars().take(max).collect();
                out.insert(key.to_string(), Value::String(cut));
            }
            Some(Value::Bool(b)) => {
                out.insert(key.to_string(), Value::Bool(*b));
            }
            _ => {}
        }
    }
    Value::Object(out)
}

/// The payload of one activity event.
pub fn payload(chat: &str, kind: &str, tool: &str, request_id: &str, phase: &str, tab: Option<&str>, args: &Value) -> Value {
    json!({
        "chat": chat,
        "kind": kind,
        "tool": tool,
        "requestId": request_id,
        "phase": phase,
        "tab": tab,
        "args": brief_args(args),
    })
}

/// The tab an activity event names: a browser tab as the tool resolves it (without marking), a
/// terminal tab from the call's `tabId`.
fn tab_of(app: &AppHandle, chat: &str, kind: &str, args: &Value) -> Option<String> {
    let given = args.get("tabId").and_then(Value::as_str);
    if kind == "browser" {
        crate::browser::find_tab(app, chat, given)
    } else {
        given.map(str::to_string)
    }
}

/// Tell the windows that `tool` of `chat` started (`phase` "start") or ended ("end").
pub fn announce(app: &AppHandle, chat: &str, tool: &str, request_id: &str, phase: &str, args: &Value) {
    let Some(kind) = kind_of(tool) else {
        return;
    };
    let tab = tab_of(app, chat, kind, args);
    let _ = app.emit("agent://activity", payload(chat, kind, tool, request_id, phase, tab.as_deref(), args));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_browser_and_terminal_tools_have_cards() {
        assert_eq!(kind_of("browser_click"), Some("browser"));
        assert_eq!(kind_of("terminal_run"), Some("terminal"));
        assert_eq!(kind_of("usage_get"), None);
        assert_eq!(kind_of("notify"), None);
        assert_eq!(kind_of("mcp__github__browser_x"), None);
    }

    #[test]
    fn the_arguments_on_a_card_are_few_and_short() {
        let args = json!({
            "ref": "ref_4",
            "text": "x".repeat(200),
            "command": "echo hi",
            "paths": ["C:/secret.txt"],
            "value": "a password",
            "button": "right",
            "accept": true,
            "steps": [1, 2, 3],
        });
        let brief = brief_args(&args);
        assert_eq!(brief["ref"], "ref_4");
        assert_eq!(brief["text"].as_str().unwrap().chars().count(), 60);
        assert_eq!(brief["command"], "echo hi");
        assert_eq!(brief["button"], "right");
        // What a card does not need is not sent: file paths, form values, step lists, flags other than the few.
        for key in ["paths", "value", "steps", "accept"] {
            assert!(brief.get(key).is_none(), "{key} is left out");
        }
        assert_eq!(brief_args(&Value::Null), json!({}));
    }

    #[test]
    fn an_event_names_the_chat_the_tool_the_phase_and_the_tab() {
        let event = payload("chat-a", "browser", "browser_click", "req-1", "start", Some("browser-2"), &json!({ "ref": "ref_1" }));
        assert_eq!(
            event,
            json!({ "chat": "chat-a", "kind": "browser", "tool": "browser_click", "requestId": "req-1", "phase": "start", "tab": "browser-2", "args": { "ref": "ref_1" } })
        );
        let unknown = payload("c", "terminal", "terminal_run", "r", "end", None, &json!({}));
        assert!(unknown["tab"].is_null());
    }
}
