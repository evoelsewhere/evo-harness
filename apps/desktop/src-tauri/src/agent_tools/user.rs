//! The tools that reach the person, answered by the shell (ADR 0013, ADR 0014):
//! `send_file` puts files in the chat as cards, `notify` raises a notification
//! (the bell, and the OS when no window is in front), `show_file` opens a file in
//! the Files panel.
//!
//! The shell cannot touch the webview's stores, so what the person sees comes from
//! events the webview applies (`stores/agent-events.ts`):
//!
//! * `agent://notify { chat, title, body, window }` — the in-app notification
//!   entry (and toast); the OS notification is sent from here, so it works with
//!   the window hidden. The webview reports whether notifications are muted
//!   ([`agent_notify_muted`]: Do not disturb, or the `agent` source muted).
//! * `agent://show-file { chat, path, line, window }` — open `path` in Files.
//! * `agent://file-sent { chat, paths, window }` — files `send_file` delivered: an
//!   Office document opens in Files for a live look.
//!
//! Behaviour and wording are those of the webview tools this replaces
//! (`features/chat/user-tools.ts`, `features/browser/show-file.ts`).
//!
//! Spec: docs/features/agent-tools-layers (FR-3, T9); docs/features/agent-tools
//! (FR-11 `send_file`, FR-12 `notify`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde_json::{json, Number, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use super::{Class, Tool, ToolCtx, ToolOutput};

/// At most this many files in one `send_file`.
const MAX_FILES: usize = 10;
/// At most one `notify` per chat this often (FR-12).
const NOTIFY_EVERY_MS: u64 = 30_000;
/// A notification's title and body are cut to this many characters.
const TITLE_MAX: usize = 120;
const BODY_MAX: usize = 400;

/// The tools of this group.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "send_file",
            description: "Give the person files you made or found — a report, an image, a build, an export — as cards in the chat they can open, show in their folder, or view in the app. Use it for deliverables, not for every file you touch.",
            schema: send_file_schema,
            // Only checks that the files exist; never reads them.
            class: Class::Read,
            run: |ctx, args| Box::pin(run_send_file(ctx, args)),
        },
        Tool {
            name: "notify",
            description: "Send the person a notification (on their desktop when the app is in the background). Use it when a long run finishes or needs their decision while they may be away — not for every step.",
            schema: notify_schema,
            class: Class::Read,
            run: |ctx, args| Box::pin(run_notify(ctx, args)),
        },
        Tool {
            name: "show_file",
            description: "Show a file to the user in the app's Files panel, scrolled to a line. Use it to point the user at code you are talking about instead of pasting it.",
            schema: show_file_schema,
            // Moves the app's view on; nothing the person keeps is altered, so it never asks.
            class: Class::Edit,
            run: |ctx, args| Box::pin(run_show_file(ctx, args)),
        },
    ]
}

fn send_file_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "paths": { "type": "array", "items": { "type": "string" }, "description": "Files, absolute or relative to the working folder (1–10)." },
            "caption": { "type": "string", "description": "One line about them, e.g. \"The report, with the chart on page 2\"." }
        },
        "required": ["paths"],
        "additionalProperties": false
    })
}

fn notify_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "Short: what happened or what you need." },
            "body": { "type": "string", "description": "One or two sentences." }
        },
        "required": ["title"],
        "additionalProperties": false
    })
}

fn show_file_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Absolute, or relative to the working folder." },
            "line": { "type": "number", "description": "1-based line to scroll to." }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

// ── Pure helpers (the TypeScript's semantics, kept exact) ───────────────────

/// A number as JavaScript prints it (`3`, not `3.0`).
fn number_text(number: &Number) -> String {
    if let Some(whole) = number.as_i64() {
        return whole.to_string();
    }
    match number.as_f64() {
        Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => (f as i64).to_string(),
        Some(f) => f.to_string(),
        None => number.to_string(),
    }
}

/// `String(value ?? "")`.
fn js_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number_text(number),
        Some(other) => other.to_string(),
    }
}

/// The first `max` characters.
fn first_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Whether `path` stands on its own: a drive (`C:\`), a root (`/`, `\`) or the home folder (`~`).
fn is_rooted(path: &str) -> bool {
    let bytes = path.as_bytes();
    match bytes.first() {
        Some(b'/' | b'\\' | b'~') => true,
        Some(first) if first.is_ascii_alphabetic() => bytes.get(1) == Some(&b':') && matches!(bytes.get(2), Some(b'/' | b'\\')),
        _ => false,
    }
}

/// `resolvePath` of `features/files/fs.ts`: a rooted path stays; another joins `root`
/// with the separator `root` uses.
pub fn resolve_path(root: &str, path: &str) -> String {
    if is_rooted(path) {
        return path.to_string();
    }
    let separator = if root.contains('\\') { '\\' } else { '/' };
    let relative = path.strip_prefix("./").or_else(|| path.strip_prefix(".\\")).unwrap_or(path);
    let relative: String = relative.chars().map(|c| if c == '/' || c == '\\' { separator } else { c }).collect();
    format!("{}{separator}{relative}", root.trim_end_matches(['/', '\\']))
}

/// A path to resolve against the working folder (the raw path when there is no folder).
fn resolve_in(folder: &str, raw: &str) -> String {
    if folder.is_empty() {
        raw.to_string()
    } else {
        resolve_path(folder, raw)
    }
}

// ── send_file ───────────────────────────────────────────────────────────────

/// The non-blank strings of `paths` (a lone value counts as a list of one).
fn paths_arg(args: &Value) -> Vec<String> {
    let items: Vec<&Value> = match args.get("paths") {
        None => Vec::new(),
        Some(Value::Array(items)) => items.iter().collect(),
        Some(single) => vec![single],
    };
    items.into_iter().filter_map(Value::as_str).filter(|path| !path.trim().is_empty()).map(str::to_string).collect()
}

/// The last part of a path, split on either separator.
fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

enum Stat {
    Missing,
    Folder,
    File(u64),
}

/// Whether `path` is a file and how big (metadata only, never the contents).
fn stat(path: &str) -> Stat {
    match std::fs::metadata(Path::new(path)) {
        Ok(meta) if meta.is_dir() => Stat::Folder,
        Ok(meta) => Stat::File(meta.len()),
        Err(_) => Stat::Missing,
    }
}

/// The result of a `send_file` call in `folder`: the text and `structuredContent`
/// (`{ files: [{ path, name, size }], caption? }`) the transcript renders as cards.
fn send_files(folder: &str, args: &Value) -> ToolOutput {
    let paths = paths_arg(args);
    if paths.is_empty() {
        return ToolOutput::error("send_file needs at least one path.");
    }
    if paths.len() > MAX_FILES {
        return ToolOutput::error("Send at most 10 files at once.");
    }
    let mut files = Vec::new();
    let mut problems = Vec::new();
    for raw in &paths {
        let path = resolve_in(folder, raw);
        match stat(&path) {
            Stat::Missing => problems.push(format!("{raw}: not found")),
            Stat::Folder => problems.push(format!("{raw}: a folder (send files)")),
            Stat::File(size) => files.push(json!({ "name": file_name(&path), "path": path, "size": size })),
        }
    }
    if files.is_empty() {
        return ToolOutput::error(problems.join("\n"));
    }
    let names: Vec<&str> = files.iter().filter_map(|file| file["name"].as_str()).collect();
    let text = format!(
        "Sent {} file{} to the chat: {}.{}",
        files.len(),
        if files.len() == 1 { "" } else { "s" },
        names.join(", "),
        if problems.is_empty() { String::new() } else { format!("\nNot sent:\n{}", problems.join("\n")) }
    );
    let mut structured = json!({ "files": files });
    if let Some(caption) = args.get("caption").and_then(Value::as_str) {
        structured["caption"] = json!(caption);
    }
    ToolOutput::text(text).with_structured(structured)
}

async fn run_send_file(ctx: ToolCtx, args: Value) -> ToolOutput {
    let folder = ctx.folder().to_string_lossy().to_string();
    let output = send_files(&folder, &args);
    let sent: Vec<&str> = output
        .structured
        .as_ref()
        .and_then(|structured| structured["files"].as_array())
        .map(|files| files.iter().filter_map(|file| file["path"].as_str()).collect())
        .unwrap_or_default();
    if !sent.is_empty() {
        // A delivered Office file opens in Files for a live look (the webview decides).
        ctx.emit("agent://file-sent", json!({ "chat": ctx.chat, "paths": sent, "window": ctx.window() }));
    }
    output
}

// ── notify ──────────────────────────────────────────────────────────────────

/// What `notify` says.
#[derive(Debug, PartialEq)]
struct Notice {
    title: String,
    body: Option<String>,
}

/// The notification of a call's arguments, or what is wrong with them.
fn notice(args: &Value) -> Result<Notice, &'static str> {
    let title = first_chars(js_string(args.get("title")).trim(), TITLE_MAX);
    if title.is_empty() {
        return Err("notify needs a title.");
    }
    let body = args.get("body").and_then(Value::as_str).map(|body| first_chars(body, BODY_MAX));
    Ok(Notice { title, body })
}

/// Whether `chat` may notify at `now_ms` (and if so, remember it): at most one per `every_ms`.
fn take_slot(last: &mut HashMap<String, u64>, chat: &str, now_ms: u64, every_ms: u64) -> bool {
    if last.get(chat).is_some_and(|at| now_ms.saturating_sub(*at) < every_ms) {
        return false;
    }
    last.insert(chat.to_string(), now_ms);
    true
}

/// The OS gets a notification when the person is not looking at the app and has not muted it.
fn system_wanted(window_focused: bool, muted: bool) -> bool {
    !window_focused && !muted
}

/// `agent://notify`: the bell entry the webview adds (its title and body as the OS shows them).
fn notify_event(chat: &str, window: &str, notice: &Notice) -> Value {
    json!({ "chat": chat, "title": notice.title, "body": notice.body, "window": window })
}

/// Do not disturb, or the agent's notifications muted: the webview reports it on every change.
static MUTED: AtomicBool = AtomicBool::new(false);

/// The webview reports whether `notify` is muted (Do not disturb, or the `agent` source
/// muted), so the OS notification the shell sends follows it (`stores/agent-events.ts`).
#[tauri::command]
pub fn agent_notify_muted(muted: bool) {
    MUTED.store(muted, Ordering::Relaxed);
}

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Whether the person muted the shell's OS notifications (Do not disturb, or
/// the agent muted); the shell's run notices follow it too
/// (docs/features/shell-hosted-engines/spec.md, plan 3.7).
pub(crate) fn muted() -> bool {
    MUTED.load(Ordering::Relaxed)
}

/// Whether a window is visible and in front of the person.
pub(crate) fn any_window_focused(app: &AppHandle) -> bool {
    app.webview_windows().values().any(|window| window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false))
}

/// Send an OS notification. With `chat`, clicking it opens that chat
/// (docs/features/shell-hosted-engines/spec.md, FR-9; `supervisor::notices::setup`).
pub(crate) fn send_os(app: &AppHandle, title: &str, body: Option<&str>, chat: Option<&str>) {
    let notification = app.notification();
    let allowed = match notification.permission_state() {
        Ok(tauri::plugin::PermissionState::Granted) => true,
        _ => matches!(notification.request_permission(), Ok(tauri::plugin::PermissionState::Granted)),
    };
    if !allowed {
        return;
    }
    let mut builder = notification.builder().title(title);
    if let Some(body) = body.filter(|body| !body.is_empty()) {
        builder = builder.body(body);
    }
    if let Some(chat) = chat {
        builder = builder.extra("chat", chat);
    }
    let _ = builder.show();
}

fn send_system(app: &AppHandle, notice: &Notice) {
    send_os(app, &notice.title, notice.body.as_deref(), None);
}

async fn run_notify(ctx: ToolCtx, args: Value) -> ToolOutput {
    let notice = match notice(&args) {
        Ok(notice) => notice,
        Err(message) => return ToolOutput::error(message),
    };
    static LAST: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    let allowed = take_slot(&mut LAST.get_or_init(Default::default).lock().expect("notify lock"), &ctx.chat, now_ms(), NOTIFY_EVERY_MS);
    if !allowed {
        return ToolOutput::error("A notification went out moments ago; not sending another yet.");
    }
    // The bell entry exists whether or not the OS shows anything.
    ctx.emit("agent://notify", notify_event(&ctx.chat, &ctx.window(), &notice));
    if system_wanted(any_window_focused(&ctx.app), MUTED.load(Ordering::Relaxed)) {
        send_system(&ctx.app, &notice);
    }
    ToolOutput::text("Notified.")
}

// ── show_file ───────────────────────────────────────────────────────────────

/// The file to show (resolved against `folder`) and the line, or what is wrong.
fn show_request(folder: &str, args: &Value) -> Result<(String, Option<Number>), &'static str> {
    let path = js_string(args.get("path"));
    if path.is_empty() {
        return Err("show_file needs a path.");
    }
    let line = match args.get("line") {
        Some(Value::Number(number)) => Some(number.clone()),
        _ => None,
    };
    Ok((resolve_in(folder, &path), line))
}

/// What the agent reads back (a line of 0 is no line, as in the webview tool).
fn show_text(full: &str, line: Option<&Number>) -> String {
    match line.filter(|line| line.as_f64().is_some_and(|n| n != 0.0)) {
        Some(line) => format!("Showing {full} at line {} in the Files panel.", number_text(line)),
        None => format!("Showing {full} in the Files panel."),
    }
}

async fn run_show_file(ctx: ToolCtx, args: Value) -> ToolOutput {
    let folder = ctx.folder().to_string_lossy().to_string();
    let (full, line) = match show_request(&folder, &args) {
        Ok(request) => request,
        Err(message) => return ToolOutput::error(message),
    };
    ctx.emit("agent://show-file", json!({ "chat": ctx.chat, "path": full, "line": line, "window": ctx.window() }));
    ToolOutput::text(show_text(&full, line.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder with files of known sizes.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("evo-user-tools-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("sub")).unwrap();
            Self(dir)
        }
        fn write(&self, relative: &str, bytes: usize) {
            std::fs::write(self.0.join(relative), vec![b'x'; bytes]).unwrap();
        }
        fn folder(&self) -> String {
            self.0.to_string_lossy().to_string()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn text_of(output: &ToolOutput) -> String {
        match &output.content[0] {
            super::super::Content::Text(text) => text.clone(),
            _ => panic!("not text"),
        }
    }

    #[test]
    fn the_group_registers_three_tools_with_closed_schemas() {
        let tools = tools();
        let names: Vec<_> = tools.iter().map(|tool| (tool.name, tool.class)).collect();
        assert_eq!(names, [("send_file", Class::Read), ("notify", Class::Read), ("show_file", Class::Edit)]);
        for tool in &tools {
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(schema["additionalProperties"], false, "{}", tool.name);
            assert!(!tool.description.is_empty());
        }
    }

    #[test]
    fn the_schemas_are_those_of_the_webview_tools() {
        assert_eq!(
            send_file_schema(),
            json!({
                "type": "object",
                "properties": {
                    "paths": { "type": "array", "items": { "type": "string" }, "description": "Files, absolute or relative to the working folder (1–10)." },
                    "caption": { "type": "string", "description": "One line about them, e.g. \"The report, with the chart on page 2\"." }
                },
                "required": ["paths"],
                "additionalProperties": false
            })
        );
        assert_eq!(notify_schema()["required"], json!(["title"]));
        assert_eq!(notify_schema()["properties"]["body"]["description"], "One or two sentences.");
        assert_eq!(show_file_schema()["properties"]["line"], json!({ "type": "number", "description": "1-based line to scroll to." }));
        assert_eq!(show_file_schema()["required"], json!(["path"]));
        let descriptions: Vec<_> = tools().iter().map(|tool| tool.description).collect();
        assert!(descriptions[0].starts_with("Give the person files you made or found — a report"));
        assert!(descriptions[1].starts_with("Send the person a notification (on their desktop when the app is in the background)."));
        assert!(descriptions[2].ends_with("instead of pasting it."));
    }

    #[test]
    fn paths_resolve_like_the_webview() {
        assert_eq!(resolve_path("C:\\work\\app", "src\\a.rs"), "C:\\work\\app\\src\\a.rs");
        assert_eq!(resolve_path("C:\\work\\app\\", "./src/a.rs"), "C:\\work\\app\\src\\a.rs");
        assert_eq!(resolve_path("C:\\work\\app", ".\\a.rs"), "C:\\work\\app\\a.rs");
        assert_eq!(resolve_path("/home/me/app/", "src/a.rs"), "/home/me/app/src/a.rs");
        assert_eq!(resolve_path("/home/me/app", "../a.rs"), "/home/me/app/../a.rs");
        for rooted in ["D:\\x\\y.txt", "d:/x/y.txt", "/etc/hosts", "\\\\server\\share\\f", "~/notes.md"] {
            assert_eq!(resolve_path("C:\\work", rooted), rooted);
        }
        // A drive letter alone, or a name with a colon, is not rooted.
        assert_eq!(resolve_path("/w", "c:file"), "/w/c:file");
        assert_eq!(resolve_in("", "a.txt"), "a.txt");
    }

    #[test]
    fn send_file_paths_are_validated() {
        let args = json!({ "paths": ["a", "  ", 3, null, "b"] });
        assert_eq!(paths_arg(&args), ["a", "b"]);
        assert_eq!(paths_arg(&json!({ "paths": "one" })), ["one"]);
        assert!(paths_arg(&json!({})).is_empty());
        let none = send_files("/w", &json!({ "paths": [] }));
        assert!(none.is_error);
        assert_eq!(text_of(&none), "send_file needs at least one path.");
        let many = send_files("/w", &json!({ "paths": (0..11).map(|n| format!("f{n}")).collect::<Vec<_>>() }));
        assert!(many.is_error);
        assert_eq!(text_of(&many), "Send at most 10 files at once.");
    }

    #[test]
    fn send_file_result_has_the_text_and_structure_of_the_webview_tool() {
        let scratch = Scratch::new("golden");
        scratch.write("report.txt", 12);
        scratch.write("sub/chart.png", 3);
        let folder = scratch.folder();
        let sep = if folder.contains('\\') { "\\" } else { "/" };
        let result = send_files(&folder, &json!({ "paths": ["report.txt", format!("sub{sep}chart.png")], "caption": "The report, with the chart" }));
        assert!(!result.is_error);
        assert_eq!(
            text_of(&result),
            "Sent 2 files to the chat: report.txt, chart.png."
        );
        // What the TypeScript returned: `{ files: [{ path, name, size }], caption }`.
        let golden = json!({
            "files": [
                { "path": format!("{folder}{sep}report.txt"), "name": "report.txt", "size": 12 },
                { "path": format!("{folder}{sep}sub{sep}chart.png"), "name": "chart.png", "size": 3 }
            ],
            "caption": "The report, with the chart"
        });
        assert_eq!(result.structured, Some(golden));
        let wire = result.to_json();
        assert_eq!(wire["structuredContent"]["files"][0]["name"], "report.txt");
        assert_eq!(wire["content"][0]["type"], "text");
        assert!(wire.get("isError").is_none());
    }

    #[test]
    fn send_file_says_what_it_could_not_send() {
        let scratch = Scratch::new("partial");
        scratch.write("one.txt", 1);
        let folder = scratch.folder();
        let sep = if folder.contains('\\') { "\\" } else { "/" };
        let result = send_files(&folder, &json!({ "paths": ["one.txt", "gone.txt", "sub"] }));
        assert!(!result.is_error);
        assert_eq!(
            text_of(&result),
            "Sent 1 file to the chat: one.txt.\nNot sent:\ngone.txt: not found\nsub: a folder (send files)"
        );
        let golden = json!({ "files": [{ "path": format!("{folder}{sep}one.txt"), "name": "one.txt", "size": 1 }] });
        assert_eq!(result.structured, Some(golden), "no caption key without a caption");

        let failed = send_files(&folder, &json!({ "paths": ["gone.txt", "sub"] }));
        assert!(failed.is_error);
        assert_eq!(text_of(&failed), "gone.txt: not found\nsub: a folder (send files)");
        assert!(failed.structured.is_none());
        // An empty caption is still a caption, as in the webview tool.
        let empty = send_files(&folder, &json!({ "paths": ["one.txt"], "caption": "" }));
        assert_eq!(empty.structured.unwrap()["caption"], "");
    }

    #[test]
    fn a_notice_needs_a_title_and_is_cut_to_size() {
        assert_eq!(notice(&json!({})), Err("notify needs a title."));
        assert_eq!(notice(&json!({ "title": "   " })), Err("notify needs a title."));
        assert_eq!(notice(&json!({ "title": "  Done  " })).unwrap(), Notice { title: "Done".into(), body: None });
        let long = notice(&json!({ "title": "t".repeat(200), "body": "é".repeat(500) })).unwrap();
        assert_eq!(long.title.chars().count(), 120);
        assert_eq!(long.body.unwrap().chars().count(), 400);
        // A body that is not a string is left out; an empty one stays.
        assert_eq!(notice(&json!({ "title": "t", "body": 5 })).unwrap().body, None);
        assert_eq!(notice(&json!({ "title": "t", "body": "" })).unwrap().body, Some(String::new()));
        // `String(value)` for a title that is not a string.
        assert_eq!(notice(&json!({ "title": 42 })).unwrap().title, "42");
    }

    #[test]
    fn a_chat_notifies_once_in_the_window() {
        let mut last = HashMap::new();
        assert!(take_slot(&mut last, "a", 0, NOTIFY_EVERY_MS), "the first goes out, even at clock zero");
        assert!(!take_slot(&mut last, "a", 1, NOTIFY_EVERY_MS));
        assert!(!take_slot(&mut last, "a", 29_999, NOTIFY_EVERY_MS));
        assert!(take_slot(&mut last, "b", 10, NOTIFY_EVERY_MS), "each chat has its own window");
        assert!(take_slot(&mut last, "a", 30_000, NOTIFY_EVERY_MS));
        assert!(!take_slot(&mut last, "a", 59_999, NOTIFY_EVERY_MS), "a refused call does not restart the window");
        assert!(take_slot(&mut last, "a", 60_000, NOTIFY_EVERY_MS));
    }

    #[test]
    fn the_os_is_told_only_when_nobody_is_looking_and_nothing_is_muted() {
        assert!(system_wanted(false, false));
        assert!(!system_wanted(true, false));
        assert!(!system_wanted(false, true));
        assert!(!system_wanted(true, true));
    }

    #[test]
    fn the_notify_event_carries_the_chat_the_text_and_the_window() {
        let event = notify_event("c1", "main", &Notice { title: "Build done".into(), body: Some("3 warnings".into()) });
        assert_eq!(event, json!({ "chat": "c1", "title": "Build done", "body": "3 warnings", "window": "main" }));
        let bare = notify_event("c1", "win-2", &Notice { title: "Hi".into(), body: None });
        assert!(bare["body"].is_null());
    }

    #[test]
    fn show_file_resolves_and_reports_like_the_webview_tool() {
        let (full, line) = show_request("C:\\work", &json!({ "path": "src\\lib.rs", "line": 42 })).unwrap();
        assert_eq!(full, "C:\\work\\src\\lib.rs");
        assert_eq!(show_text(&full, line.as_ref()), "Showing C:\\work\\src\\lib.rs at line 42 in the Files panel.");
        let (full, line) = show_request("/w", &json!({ "path": "/abs/a.rs" })).unwrap();
        assert_eq!(show_text(&full, line.as_ref()), "Showing /abs/a.rs in the Files panel.");
        // A line of 0 is no line in the text; a fraction prints as JavaScript prints it.
        let (full, line) = show_request("/w", &json!({ "path": "a.rs", "line": 0 })).unwrap();
        assert_eq!(show_text(&full, line.as_ref()), "Showing /w/a.rs in the Files panel.");
        let (full, line) = show_request("/w", &json!({ "path": "a.rs", "line": 3.5 })).unwrap();
        assert_eq!(show_text(&full, line.as_ref()), "Showing /w/a.rs at line 3.5 in the Files panel.");
        let (full, line) = show_request("/w", &json!({ "path": "a.rs", "line": 7.0 })).unwrap();
        assert_eq!(show_text(&full, line.as_ref()), "Showing /w/a.rs at line 7 in the Files panel.");
        // A line that is not a number is ignored.
        assert!(show_request("/w", &json!({ "path": "a.rs", "line": "9" })).unwrap().1.is_none());
        assert_eq!(show_request("/w", &json!({})), Err("show_file needs a path."));
        assert_eq!(show_request("/w", &json!({ "path": "" })), Err("show_file needs a path."));
    }
}
