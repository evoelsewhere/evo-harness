//! `browser_dialog` (docs/features/agent-browser FR-4, T16): answer the page's
//! open JavaScript dialog (`alert`, `confirm`, `prompt`, `beforeunload`) or
//! report it.
//!
//! On Windows the shell holds a page's dialog open until it is answered
//! (`browser::dialog_pending` / `dialog_answer`), and every other tool returns
//! with a note instead of waiting on it (`Page::dialog_note`). Where the shell
//! cannot hold dialogs the page script answers them by a preset policy
//! (`__evo.dialogPolicy`) and keeps a log (`__evo.dialogLog`).

use serde::Deserialize;
use serde_json::{json, Value};

use super::driver::{cut_chars, finish, gate_page, js, resolve_page, settle, ToolResult};
use crate::agent_tools::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};
use crate::browser as shell;
use crate::browser::dialog::DialogInfo;

const DIALOG: &str = "Answer the page's open JavaScript dialog (alert, confirm, prompt, or the leave-page question): accept it (default) or dismiss it, with text for a prompt. action \"status\" only reports the open dialog (kind, message). While a dialog is open the page is held: a click or key that opens one returns while it is open and says so, then answer it with this tool and read the page again. A dialog nobody answers is dismissed after a minute.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["answer", "status"], "description": "Default answer. status only reports the open dialog." },
            "accept": { "type": "boolean", "description": "Accept (OK) or dismiss (Cancel) the dialog. Default true." },
            "text": { "type": "string", "description": "The answer for a prompt dialog (when accepting); default: the prompt's own default text." },
            "tabId": { "type": "string", "description": "A tab id from browser_tabs; default: the active tab." },
        },
        "additionalProperties": false,
    })
}

fn run(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(async move { finish(dialog(&ctx, &args).await) })
}

/// The tool of this module.
pub fn tools() -> Vec<Tool> {
    vec![Tool { name: "browser_dialog", description: DIALOG, schema, class: Class::Act, run }]
}

// ── Arguments ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Answer,
    Status,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub action: Action,
    pub accept: bool,
    pub text: Option<String>,
}

pub fn parse_args(args: &Value) -> Result<Request, String> {
    let action = match args.get("action") {
        None | Some(Value::Null) => Action::Answer,
        Some(Value::String(a)) if a == "answer" => Action::Answer,
        Some(Value::String(a)) if a == "status" => Action::Status,
        Some(_) => return Err("action must be \"answer\" or \"status\".".into()),
    };
    let accept = match args.get("accept") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(accept)) => *accept,
        Some(_) => return Err("accept must be true or false.".into()),
    };
    let text = match args.get("text") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(_) => return Err("text must be a string.".into()),
    };
    Ok(Request { action, accept, text })
}

// ── Wording ─────────────────────────────────────────────────────────────────

fn quoted(text: &str) -> String {
    let short = cut_chars(text, 200);
    js(&if short.len() < text.len() { format!("{short}…") } else { short })
}

/// `an alert dialog`, `a confirm dialog`, `a beforeunload (leave page?) dialog`.
pub fn kind_phrase(kind: &str) -> String {
    let what = if kind == "beforeunload" { "beforeunload (leave page?)" } else { kind };
    let article = if what.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
    format!("{article} {what} dialog")
}

/// What `action: "status"` says about an open dialog.
pub fn status_line(dialog: &DialogInfo) -> String {
    let mut line = format!("{} is open", capitalize(&kind_phrase(&dialog.kind)));
    if !dialog.message.is_empty() {
        line.push_str(&format!(": {}", quoted(&dialog.message)));
    }
    line.push('.');
    if dialog.kind == "prompt" {
        if let Some(default) = dialog.default_text.as_deref().filter(|d| !d.is_empty()) {
            line.push_str(&format!(" Default text: {}.", quoted(default)));
        }
    }
    line.push_str(" Answer it with browser_dialog {accept: true} or {accept: false}.");
    line
}

pub(super) fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// What was done to the dialog that was open.
pub fn answered_line(dialog: &DialogInfo, accept: bool, text: Option<&str>) -> String {
    let verb = if accept { "Accepted" } else { "Dismissed" };
    let message = if dialog.message.is_empty() { String::new() } else { format!(" {}", quoted(&dialog.message)) };
    let with = if accept && dialog.kind == "prompt" {
        match text {
            Some(text) => format!(" with text {}", quoted(text)),
            None => " with its default text".to_string(),
        }
    } else {
        String::new()
    };
    let mut line = format!("{verb} the {} dialog{message}{with}.", dialog.kind);
    if text.is_some() && dialog.kind != "prompt" {
        line.push_str(" (text is only used by prompt dialogs)");
    }
    line
}

/// One dialog the page script answered itself (platforms where the shell does not hold dialogs).
#[derive(Debug, Deserialize)]
pub struct LogEntry {
    pub kind: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub answer: Value,
}

/// `action: "status"` where the page script answers dialogs.
pub fn script_status(entries: &[LogEntry]) -> String {
    if entries.is_empty() {
        return "No dialog is open, and the page has shown none since it loaded.".into();
    }
    let recent: Vec<String> = entries
        .iter()
        .rev()
        .take(5)
        .rev()
        .map(|entry| format!("{} {} — answered {}", entry.kind, quoted(&cut_chars(&entry.message, 120)), entry.answer))
        .collect();
    format!("No dialog is open. This platform answers dialogs by script. Recent dialogs:\n{}", recent.join("\n"))
}

/// What setting the script policy says.
pub fn policy_line(accept: bool, text: Option<&str>) -> String {
    let prompts = text.map(|t| format!(" (prompts get {})", quoted(t))).unwrap_or_default();
    format!(
        "No dialog is open. This platform answers dialogs by script, so the next dialogs on this page will be {}{prompts}. Use browser_dialog with action \"status\" to see what was answered.",
        if accept { "accepted" } else { "dismissed" }
    )
}

// ── The tool ────────────────────────────────────────────────────────────────

async fn dialog(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let request = parse_args(args)?;
    let page = resolve_page(ctx, args)?;
    if !cfg!(windows) {
        return script_path(&page, &request).await;
    }
    let Some(open) = shell::dialog_pending(&page.app, &page.tab) else {
        return Ok(ToolOutput::text("No dialog is open."));
    };
    if request.action == Action::Status {
        return Ok(ToolOutput::text(status_line(&open)));
    }
    let what = format!("{} the {} dialog", if request.accept { "Accept" } else { "Dismiss" }, open.kind);
    gate_page(ctx, &page, &what).await?;
    // The person may have answered it, or it timed out, while the question was open.
    if !shell::dialog_answer(&page.app, &page.tab, request.accept, request.text.clone()) {
        return Ok(ToolOutput::text("The dialog was already closed."));
    }
    let mut lines = vec![answered_line(&open, request.accept, request.text.as_deref())];
    // Answering can run the page's handler, which may open another dialog.
    settle(250).await;
    if let Some(next) = shell::dialog_pending(&page.app, &page.tab) {
        lines.push(format!("Another dialog opened: {}", status_line(&next)));
    }
    Ok(ToolOutput::text(lines.join("\n")))
}

/// Where the shell cannot hold dialogs: set the page's policy, or read what it answered.
async fn script_path(page: &super::driver::Page, request: &Request) -> ToolResult {
    if request.action == Action::Status {
        let value = page.helper::<Value>("dialogLog()").await?;
        let entries: Vec<LogEntry> = serde_json::from_value(value).unwrap_or_default();
        return Ok(ToolOutput::text(script_status(&entries)));
    }
    let policy = match &request.text {
        Some(text) => json!({ "accept": request.accept, "text": text }),
        None => json!({ "accept": request.accept }),
    };
    page.helper::<Value>(&format!("dialogPolicy({})", js(&policy))).await?;
    Ok(ToolOutput::text(policy_line(request.accept, request.text.as_deref())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn confirm() -> DialogInfo {
        DialogInfo { kind: "confirm".into(), message: "Delete it?".into(), default_text: None, url: "https://x.test/".into() }
    }

    fn prompt() -> DialogInfo {
        DialogInfo { kind: "prompt".into(), message: "Name?".into(), default_text: Some("anon".into()), url: "https://x.test/".into() }
    }

    #[test]
    fn arguments_default_to_accepting() {
        assert_eq!(parse_args(&json!({})).unwrap(), Request { action: Action::Answer, accept: true, text: None });
        let r = parse_args(&json!({ "accept": false, "text": "x", "action": "answer" })).unwrap();
        assert_eq!((r.accept, r.text.as_deref()), (false, Some("x")));
        assert_eq!(parse_args(&json!({ "action": "status" })).unwrap().action, Action::Status);
        assert!(parse_args(&json!({ "action": "close" })).is_err());
        assert!(parse_args(&json!({ "accept": "yes" })).is_err());
        assert!(parse_args(&json!({ "text": 3 })).is_err());
    }

    #[test]
    fn the_status_names_the_dialog_and_how_to_answer() {
        assert_eq!(status_line(&confirm()), "A confirm dialog is open: \"Delete it?\". Answer it with browser_dialog {accept: true} or {accept: false}.");
        let line = status_line(&prompt());
        assert!(line.contains("Default text: \"anon\"."));
        let alert = DialogInfo { kind: "alert".into(), message: String::new(), default_text: None, url: String::new() };
        assert!(status_line(&alert).starts_with("An alert dialog is open."));
        let leave = DialogInfo { kind: "beforeunload".into(), message: "x".into(), default_text: None, url: String::new() };
        assert!(status_line(&leave).starts_with("A beforeunload (leave page?) dialog is open"));
    }

    #[test]
    fn a_long_message_is_cut() {
        let long = DialogInfo { message: "y".repeat(500), ..confirm() };
        assert!(status_line(&long).len() < 400);
    }

    #[test]
    fn answers_say_what_was_done() {
        assert_eq!(answered_line(&confirm(), true, None), "Accepted the confirm dialog \"Delete it?\".");
        assert_eq!(answered_line(&confirm(), false, None), "Dismissed the confirm dialog \"Delete it?\".");
        assert_eq!(answered_line(&prompt(), true, Some("Ada")), "Accepted the prompt dialog \"Name?\" with text \"Ada\".");
        assert_eq!(answered_line(&prompt(), true, None), "Accepted the prompt dialog \"Name?\" with its default text.");
        assert_eq!(answered_line(&prompt(), false, None), "Dismissed the prompt dialog \"Name?\".");
        assert!(answered_line(&confirm(), true, Some("x")).ends_with("(text is only used by prompt dialogs)"));
    }

    #[test]
    fn the_script_path_reports_its_log_and_policy() {
        assert!(script_status(&[]).contains("shown none"));
        let entries = vec![
            LogEntry { kind: "alert".into(), message: "Saved".into(), answer: Value::Null },
            LogEntry { kind: "confirm".into(), message: "Sure?".into(), answer: json!(false) },
        ];
        let text = script_status(&entries);
        assert!(text.contains("alert \"Saved\" — answered null"));
        assert!(text.contains("confirm \"Sure?\" — answered false"));
        let many: Vec<LogEntry> = (0..9).map(|i| LogEntry { kind: "alert".into(), message: format!("m{i}"), answer: Value::Null }).collect();
        let text = script_status(&many);
        assert!(text.contains("m8") && text.contains("m4") && !text.contains("m3"), "the five most recent");
        assert!(policy_line(true, None).contains("will be accepted"));
        assert!(policy_line(false, Some("Ada")).contains("will be dismissed (prompts get \"Ada\")"));
    }

    #[test]
    fn the_description_tells_that_a_click_returns_while_a_dialog_is_open() {
        let tool = &tools()[0];
        assert!(tool.description.contains("returns while it is open"));
        assert_eq!(tool.class, Class::Act);
        assert_eq!((tool.schema)()["additionalProperties"], false);
    }
}
