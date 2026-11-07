//! The page driver of the browser tools (docs/features/agent-browser FR-25,
//! T7), ported from the webview's `driver.ts`: script and DevTools calls on a
//! tab, the `window.__evo` page helper, page state, keys, navigation waits.
//! Every tool goes through a [`Page`], so a tool does not care which transport
//! reaches the page.
//!
//! A call never hangs on a page dialog: `eval` and `cdp` race against a dialog
//! opening (checked every 150 ms) and return an error that names it. A page a
//! tool uses is marked as agent-driven for two minutes, so the webview leaves
//! its dialogs to the agent (`browser_dialog`).

use std::future::Future;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::agent_tools::{ToolCtx, ToolOutput};
use crate::browser as shell;

// ── Results ─────────────────────────────────────────────────────────────────

/// Why a tool stopped: the result to return (an error message, or the answer of the gate).
pub struct Stop(pub ToolOutput);

impl From<String> for Stop {
    fn from(message: String) -> Self {
        Stop(ToolOutput::error(explain(&message)))
    }
}

impl From<&str> for Stop {
    fn from(message: &str) -> Self {
        Stop::from(message.to_string())
    }
}

impl From<ToolOutput> for Stop {
    fn from(output: ToolOutput) -> Self {
        Stop(output)
    }
}

/// What a tool body returns: its result, or the reason it stopped (use `?` on `Result<_, String>` calls).
pub type ToolResult = Result<ToolOutput, Stop>;

/// The tool's final result.
pub fn finish(result: ToolResult) -> ToolOutput {
    match result {
        Ok(output) | Err(Stop(output)) => output,
    }
}

/// A shell error as the agent should read it: a page that went away under the call is "gone".
pub fn explain(message: &str) -> String {
    if let Some(start) = message.find("browser tab ") {
        let rest = &message[start + "browser tab ".len()..];
        if let Some((id, tail)) = rest.split_once(char::is_whitespace) {
            if !id.is_empty() && tail.starts_with("is closed") {
                return format!("Browser tab {id} is gone. Use browser_tabs or browser_navigate.");
            }
        }
    }
    message.to_string()
}

/// `value` as JavaScript source (a string literal, a number, an object…).
pub fn js<T: Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// A JSON value as a template literal would print it: strings bare, `undefined` for none.
pub fn show(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "undefined".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// The first `max` characters of `text`.
pub fn cut_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => text[..end].to_string(),
        None => text.to_string(),
    }
}

// ── Tab resolution ──────────────────────────────────────────────────────────

/// The `tabId` argument as the id of a tab (`None` when absent or empty).
pub fn tab_id_arg(args: &Value) -> Option<String> {
    match args.get("tabId") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if id.is_empty() => None,
        Some(Value::String(id)) => Some(id.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// The tab a call is for: `tabId` of THIS chat's pages, else the chat's visible one.
/// A page of another chat is "No browser tab". Marks the page as agent-driven.
pub fn resolve_tab(ctx: &ToolCtx, args: &Value) -> Result<String, String> {
    shell::resolve_tab(&ctx.app, &ctx.chat, tab_id_arg(args).as_deref())
}

/// [`resolve_tab`] as a [`Page`] that stops when the call is dropped.
pub fn resolve_page(ctx: &ToolCtx, args: &Value) -> Result<Page, String> {
    let tab = resolve_tab(ctx, args)?;
    Ok(Page::for_ctx(ctx, &tab))
}

// ── The page ────────────────────────────────────────────────────────────────

/// What is loaded in the tab: its address, title, state, and a load error if any.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PageInfo {
    pub href: String,
    pub title: String,
    pub ready: String,
    pub status: Option<u32>,
    pub error: Option<String>,
}

pub const PAGE_INFO: &str = r##"({
  href: location.href,
  title: document.title,
  ready: document.readyState,
  status: performance.getEntriesByType("navigation")[0]?.responseStatus || undefined,
  error: location.protocol === "chrome-error:"
    ? ((document.querySelector(".error-code")?.textContent || "") + " " + (document.querySelector("#main-message h1, h1")?.textContent || "")).trim() || document.body?.innerText.slice(0, 200) || "unknown error"
    : undefined,
})"##;

/// The script that reads the state of the focused text field (`null` when none).
pub const FOCUSED_STATE: &str = "(() => { const t = window.__evo?.typingTarget(); return t ? window.__evo.state(t.ref) : null; })()";

/// How often a call in flight looks for a page dialog.
const DIALOG_POLL: Duration = Duration::from_millis(150);

/// One browser tab, driven by a tool.
#[derive(Clone)]
pub struct Page {
    pub app: AppHandle,
    pub tab: String,
    /// Cancelled when the engine drops the call; waits end early.
    pub cancel: Option<CancellationToken>,
}

/// The error of a call that met an open dialog.
pub fn dialog_error(dialog: &shell::dialog::DialogInfo) -> String {
    format!("{} is open: \"{}\"; answer it with browser_dialog", super::dialog::kind_phrase(&dialog.kind), cut_chars(&dialog.message, 200))
}

/// Whether `error` is the one [`dialog_error`] builds.
pub fn is_dialog_error(error: &str) -> bool {
    (error.starts_with("a ") || error.starts_with("an ")) && error.contains(" dialog is open: \"")
}

impl Page {
    pub fn new(app: &AppHandle, tab: &str) -> Self {
        Self { app: app.clone(), tab: tab.to_string(), cancel: None }
    }

    /// A page of the call's chat that stops when the call is dropped.
    pub fn for_ctx(ctx: &ToolCtx, tab: &str) -> Self {
        Self { app: ctx.app.clone(), tab: tab.to_string(), cancel: Some(ctx.cancel.clone()) }
    }

    #[allow(dead_code)]
    pub fn id(&self) -> &str {
        &self.tab
    }

    /// The tab is still open.
    pub fn exists(&self) -> bool {
        shell::tab_record(&self.app, &self.tab).is_some()
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(CancellationToken::is_cancelled)
    }

    /// The chat the tab belongs to.
    #[allow(dead_code)]
    pub fn chat(&self) -> Option<String> {
        shell::tab_record(&self.app, &self.tab).and_then(|tab| tab.chat)
    }

    /// Whether the page is loading, by the shell's load events.
    pub fn is_loading(&self) -> bool {
        shell::is_loading(&self.app, &self.tab)
    }

    /// The error naming the dialog the page waits on, if one is open.
    pub fn dialog_error(&self) -> Option<String> {
        shell::dialog_pending(&self.app, &self.tab).map(|dialog| dialog_error(&dialog))
    }

    /// A line for a result when the page has a dialog open (the call may have opened it):
    /// what it says and how to answer.
    pub fn dialog_note(&self) -> Option<String> {
        shell::dialog_pending(&self.app, &self.tab)
            .map(|dialog| format!("{} is open: \"{}\". Answer it with browser_dialog.", super::dialog::capitalize(&super::dialog::kind_phrase(&dialog.kind)), cut_chars(&dialog.message, 200)))
    }

    /// Run `call`, but give up with the dialog's error when the page opens one
    /// (the page's script waits for it, so the call would not come back).
    async fn guard<T>(&self, call: impl Future<Output = Result<T, String>>) -> Result<T, String> {
        if let Some(error) = self.dialog_error() {
            return Err(error);
        }
        tokio::pin!(call);
        loop {
            tokio::select! {
                result = &mut call => return result,
                _ = tokio::time::sleep(DIALOG_POLL) => {
                    if let Some(error) = self.dialog_error() {
                        return Err(error);
                    }
                }
            }
        }
    }

    /// Run a script in the page; its JSON result (`null` for undefined). With `statements`, a script
    /// that is not one expression runs as a program and yields its last value.
    pub async fn eval(&self, script: &str, statements: bool) -> Result<Value, String> {
        self.guard(shell::eval(&self.app, &self.tab, script, statements)).await
    }

    /// A DevTools Protocol command (Windows).
    pub async fn cdp(&self, method: &str, params: Value) -> Result<Value, String> {
        self.guard(shell::cdp(&self.app, &self.tab, method, params)).await
    }

    /// `window.__evo.<call>`, or a clear error on pages without the helper (error pages, about:blank).
    pub async fn helper<T: DeserializeOwned>(&self, call: &str) -> Result<T, String> {
        let ready = match self.eval("typeof window.__evo === 'object'", false).await {
            Ok(value) => value.as_bool().unwrap_or(false),
            Err(error) if is_dialog_error(&error) => return Err(error),
            Err(_) => false,
        };
        if !ready {
            let info = self.info().await;
            if let Some(error) = info.as_ref().and_then(|info| info.error.as_deref()) {
                return Err(format!("The page failed to load: {error}. Use browser_navigate to try again."));
            }
            let href = info.as_ref().map(|info| info.href.clone()).unwrap_or_else(|| self.recorded_url());
            let loading = info.as_ref().is_some_and(|info| info.ready != "complete");
            return Err(format!("The page at {href} cannot be read{}.", if loading { " yet (still loading)" } else { "" }));
        }
        let value = self.eval(&format!("window.__evo.{call}"), false).await?;
        serde_json::from_value(value).map_err(|error| format!("the page helper answered something unexpected: {error}"))
    }

    /// The address, title and state of what the tab shows (`None` when it cannot be read).
    pub async fn info(&self) -> Option<PageInfo> {
        let value = self.eval(PAGE_INFO, false).await.ok()?;
        let mut info: PageInfo = serde_json::from_value(value).ok()?;
        if let Some(error) = info.error.take() {
            info.error = Some(error.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|e| !e.is_empty());
        }
        Some(info)
    }

    fn recorded_url(&self) -> String {
        shell::tab_record(&self.app, &self.tab).map(|tab| tab.url).unwrap_or_default()
    }

    /// The address the tab's webview shows now (the last one it reported when the webview is gone).
    pub fn native_url(&self) -> String {
        shell::native_url(&self.app, &self.tab).unwrap_or_else(|_| self.recorded_url())
    }

    /// [`Self::native_url`], or `fallback` when the webview cannot say.
    pub fn native_url_or(&self, fallback: &str) -> String {
        shell::native_url(&self.app, &self.tab).unwrap_or_else(|_| fallback.to_string())
    }

    /// Press a key or combination (`Enter`, `ctrl+a`, `shift+Tab`) with real input (Windows).
    pub async fn press_key(&self, combo: &str) -> Result<(), String> {
        let events = key_events(combo);
        self.cdp("Input.dispatchKeyEvent", events.down).await?;
        self.cdp("Input.dispatchKeyEvent", events.up).await?;
        Ok(())
    }

    /// Real mouse press and release at a point, `count` times, with `button` (`left`, `right`, `middle`).
    pub async fn click_at(&self, x: f64, y: f64, count: u32, button: &str) -> Result<(), String> {
        let mask = button_mask(button);
        self.cdp("Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": x, "y": y })).await?;
        for n in 1..=count.max(1) {
            self.cdp("Input.dispatchMouseEvent", json!({ "type": "mousePressed", "x": x, "y": y, "button": button, "buttons": mask, "clickCount": n })).await?;
            self.cdp("Input.dispatchMouseEvent", json!({ "type": "mouseReleased", "x": x, "y": y, "button": button, "buttons": 0, "clickCount": n })).await?;
        }
        Ok(())
    }

    /// What `window.__evo.state(ref)` says about an element now; `None` when it is gone.
    pub async fn state_of(&self, element: &str) -> Option<String> {
        let value = self.eval(&format!("window.__evo?.state({}) ?? null", js(element)), false).await.ok()?;
        value.as_str().map(str::to_string)
    }

    /// What the focused text field looks like now, if one is focused.
    pub async fn focused_state(&self) -> Option<String> {
        self.eval(FOCUSED_STATE, false).await.ok()?.as_str().map(str::to_string)
    }

    /// A short line on what the tab shows.
    pub async fn summary(&self) -> String {
        let info = self.info().await;
        let fallback = shell::tab_record(&self.app, &self.tab).map(|tab| tab.title).unwrap_or_default();
        summary_line(info.as_ref(), &self.native_url(), &fallback)
    }

    /// Wait until the tab's load events say loading ended (or `timeout`).
    pub async fn wait_for_load(&self, timeout: Duration) {
        let started = Instant::now();
        while self.is_loading() && self.exists() && started.elapsed() < timeout && !self.cancelled() {
            settle(50).await;
        }
    }

    /// Wait for a navigation of the tab to begin (its URL changes or a load starts), then to finish.
    pub async fn wait_for_navigation(&self, before: &str, timeout: Duration) {
        let started = Instant::now();
        while started.elapsed() < timeout && !self.cancelled() {
            if self.is_loading() || self.native_url_or(before) != before {
                break;
            }
            settle(100).await;
        }
        self.wait_for_load(Duration::from_secs(15)).await;
        // Load events can trail the document: give it a moment to be complete.
        for _ in 0..20 {
            match self.info().await {
                Some(info) if info.ready != "complete" && info.error.is_none() => settle(150).await,
                _ => break,
            }
        }
    }

    /// back / forward / reload; a message when there is no page to go to.
    pub async fn go_history(&self, action: &str) -> Result<Option<String>, String> {
        let before = self.native_url();
        if action != "reload" && cfg!(windows) {
            let history = self.cdp("Page.getNavigationHistory", json!({})).await.ok();
            if let Some(history) = history {
                let index = history.get("currentIndex").and_then(Value::as_i64).unwrap_or(-1);
                if index >= 0 {
                    let entries = history.get("entries").and_then(Value::as_array).cloned().unwrap_or_default();
                    return match history_entry(&entries, index, action) {
                        Some(id) => {
                            self.cdp("Page.navigateToHistoryEntry", json!({ "entryId": id })).await?;
                            self.wait_for_navigation(&before, Duration::from_secs(3)).await;
                            Ok(None)
                        }
                        None => Ok(Some(format!("There is no {} page in this tab's history.", if action == "back" { "earlier" } else { "later" }))),
                    };
                }
            }
        }
        shell::history_tab(&self.app, &self.tab, action)?;
        self.wait_for_navigation(&before, Duration::from_secs(3)).await;
        Ok(None)
    }

    /// Console messages logged since `after`, as lines.
    pub async fn console_since(&self, after: u64) -> Vec<String> {
        let value = self.eval(&format!("window.__evo?.console(50, false, {after}) ?? []"), false).await.unwrap_or(Value::Null);
        match value.as_array() {
            Some(logs) => logs
                .iter()
                .map(|log| format!("[{}] {}", log.get("level").map(|v| show(Some(v))).unwrap_or_default(), log.get("text").map(|v| show(Some(v))).unwrap_or_default()))
                .collect(),
            None => Vec::new(),
        }
    }
}

pub async fn settle(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

/// `title — url (HTTP 404, still loading)`, or the load failure.
pub fn summary_line(info: Option<&PageInfo>, native_url: &str, fallback_title: &str) -> String {
    if let Some(error) = info.and_then(|info| info.error.as_deref()) {
        return format!("failed to load: {error} — {native_url}");
    }
    let url = info.map_or(native_url, |info| info.href.as_str());
    let title = info.map_or(fallback_title, |info| info.title.as_str());
    let mut notes = Vec::new();
    if let Some(status) = info.and_then(|info| info.status).filter(|status| *status >= 400) {
        notes.push(format!("HTTP {status}"));
    }
    if info.is_some_and(|info| info.ready != "complete") {
        notes.push("still loading".to_string());
    }
    let notes = if notes.is_empty() { String::new() } else { format!(" ({})", notes.join(", ")) };
    format!("{} — {url}{notes}", if title.is_empty() { "(untitled page)" } else { title })
}

/// The id of the history entry `action` (back or forward) moves to from `index`.
pub fn history_entry(entries: &[Value], index: i64, action: &str) -> Option<i64> {
    let target = if action == "back" { index - 1 } else { index + 1 };
    if target < 0 {
        return None;
    }
    entries.get(target as usize)?.get("id")?.as_i64()
}

// ── Keys and mouse buttons ──────────────────────────────────────────────────

/// Named keys: lowercase name → (DevTools `key`, virtual key code).
pub const KEYS: &[(&str, &str, u32)] = &[
    ("enter", "Enter", 13),
    ("escape", "Escape", 27),
    ("esc", "Escape", 27),
    ("tab", "Tab", 9),
    ("backspace", "Backspace", 8),
    ("delete", "Delete", 46),
    ("space", " ", 32),
    ("arrowup", "ArrowUp", 38),
    ("arrowdown", "ArrowDown", 40),
    ("arrowleft", "ArrowLeft", 37),
    ("arrowright", "ArrowRight", 39),
    ("home", "Home", 36),
    ("end", "End", 35),
    ("pageup", "PageUp", 33),
    ("pagedown", "PageDown", 34),
];

/// The two `Input.dispatchKeyEvent` calls of a key press.
pub struct KeyEvents {
    pub down: Value,
    pub up: Value,
}

/// `Enter`, `ctrl+a`, `shift+Tab`… as DevTools key events.
pub fn key_events(combo: &str) -> KeyEvents {
    let mut parts: Vec<&str> = combo.split('+').map(str::trim).filter(|part| !part.is_empty()).collect();
    let key_name = parts.pop().unwrap_or("");
    let mods: Vec<String> = parts.iter().map(|part| part.to_lowercase()).collect();
    let has = |names: &[&str]| mods.iter().any(|m| names.contains(&m.as_str()));
    let modifiers: u32 = u32::from(has(&["alt"]))
        | (u32::from(has(&["ctrl", "control"])) << 1)
        | (u32::from(has(&["meta", "cmd"])) << 2)
        | (u32::from(has(&["shift"])) << 3);
    let known = KEYS.iter().find(|(name, _, _)| *name == key_name.to_lowercase());
    let key = known.map_or(key_name, |(_, key, _)| *key);
    let code = match known {
        Some((_, _, code)) => *code,
        None => key_name.to_uppercase().encode_utf16().next().map_or(0, u32::from),
    };
    let typed: Option<String> = if known.is_some() {
        match key {
            "Enter" => Some("\r".to_string()),
            " " => Some(" ".to_string()),
            _ => None,
        }
    } else if modifiers & 6 != 0 {
        None
    } else {
        Some(key_name.to_string())
    }
    .filter(|text| !text.is_empty());
    let mut down = json!({
        "type": if typed.is_some() { "keyDown" } else { "rawKeyDown" },
        "key": key,
        "windowsVirtualKeyCode": code,
        "modifiers": modifiers,
    });
    if let Some(text) = typed {
        down["text"] = json!(text);
        down["unmodifiedText"] = json!(text);
    }
    let up = json!({ "type": "keyUp", "key": key, "windowsVirtualKeyCode": code, "modifiers": modifiers });
    KeyEvents { down, up }
}

/// The `buttons` bit of a mouse button.
pub fn button_mask(button: &str) -> u32 {
    match button {
        "right" => 2,
        "middle" => 4,
        _ => 1,
    }
}

// ── Showing the panel ───────────────────────────────────────────────────────

/// Show the Browser tool: the agent works on a page the person can watch.
/// When the panel was hidden, ask the webview to open it (`agent://panel`)
/// and wait for the page to take its place and size (a page laid out at
/// another size puts elements somewhere else): up to 600 ms for the panel's
/// bounds. With no page yet (`tab` is `None`) the wait is a fixed 350 ms.
/// A chat the webview does not show gets its panel opened for later; the wait ends at the limit.
pub async fn show_browser(ctx: &ToolCtx, tab: Option<&str>) {
    if shell::any_shown(&ctx.app, &ctx.chat) {
        return;
    }
    let seq = shell::bounds_seq(&ctx.app);
    ctx.emit("agent://panel", json!({ "chat": ctx.chat, "panel": "browser", "action": "open", "window": ctx.window() }));
    let Some(tab) = tab else {
        settle(350).await;
        return;
    };
    let started = Instant::now();
    loop {
        if shell::is_shown(&ctx.app, tab) {
            return;
        }
        if shell::bounds_seq(&ctx.app) != seq {
            // The panel took its place; the page is shown right after its bounds are set.
            settle(100).await;
            return;
        }
        if started.elapsed() >= Duration::from_millis(600) || ctx.cancelled() {
            return;
        }
        settle(50).await;
    }
}

/// The host of `url` for a line to the person ("Click ref_12 on example.com").
pub fn site_of(url: &str) -> String {
    match tauri::Url::parse(url) {
        Ok(parsed) => match (parsed.host_str(), parsed.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            _ => url.to_string(),
        },
        Err(_) => url.to_string(),
    }
}

/// Gate an act or change on the page `page` shows now.
pub async fn gate_page(ctx: &ToolCtx, page: &Page, what: &str) -> Result<(), Stop> {
    let url = page.native_url();
    let summary = if url.is_empty() { what.to_string() } else { format!("{what} on {}", site_of(&url)) };
    ctx.gate(&url, &summary).await.map_err(Stop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(combo: &str) -> Value {
        key_events(combo).down
    }

    #[test]
    fn named_keys_use_their_codes_and_only_enter_and_space_type_text() {
        let enter = down("Enter");
        assert_eq!(enter["type"], "keyDown");
        assert_eq!(enter["key"], "Enter");
        assert_eq!(enter["windowsVirtualKeyCode"], 13);
        assert_eq!(enter["text"], "\r");
        let space = down("space");
        assert_eq!((space["key"].as_str(), space["text"].as_str()), (Some(" "), Some(" ")));
        let esc = down("esc");
        assert_eq!(esc["key"], "Escape");
        assert_eq!(esc["type"], "rawKeyDown");
        assert!(esc.get("text").is_none());
        assert_eq!(down("PageDown")["windowsVirtualKeyCode"], 34);
        assert_eq!(KEYS.len(), 15);
    }

    #[test]
    fn a_letter_types_itself_unless_ctrl_or_meta_is_held() {
        let a = down("a");
        assert_eq!((a["type"].as_str(), a["text"].as_str(), a["windowsVirtualKeyCode"].as_u64()), (Some("keyDown"), Some("a"), Some(65)));
        let ctrl_a = down("ctrl+a");
        assert_eq!(ctrl_a["type"], "rawKeyDown");
        assert_eq!(ctrl_a["modifiers"], 2);
        assert!(ctrl_a.get("text").is_none());
        assert_eq!(ctrl_a["key"], "a");
        // Shift alone still types.
        let shift_a = down("shift+a");
        assert_eq!(shift_a["modifiers"], 8);
        assert_eq!(shift_a["text"], "a");
        assert_eq!(down("cmd+c")["modifiers"], 4);
    }

    #[test]
    fn modifiers_combine_and_names_ignore_case_and_spaces() {
        let all = down("Alt + Control + Meta + Shift + Tab");
        assert_eq!(all["modifiers"], 1 | 2 | 4 | 8);
        assert_eq!(all["key"], "Tab");
        assert_eq!(down("shift+Tab")["modifiers"], 8);
        let up = key_events("ctrl+Enter").up;
        assert_eq!(up["type"], "keyUp");
        assert_eq!(up["modifiers"], 2);
        assert!(up.get("text").is_none());
    }

    #[test]
    fn an_empty_combo_presses_nothing_in_particular() {
        let none = down("");
        assert_eq!(none["type"], "rawKeyDown");
        assert_eq!(none["windowsVirtualKeyCode"], 0);
    }

    #[test]
    fn mouse_buttons_have_their_bits() {
        assert_eq!((button_mask("left"), button_mask("right"), button_mask("middle")), (1, 2, 4));
    }

    #[test]
    fn a_page_summary_names_the_title_url_and_trouble() {
        let ok = PageInfo { href: "https://a.test/".into(), title: "A".into(), ready: "complete".into(), ..Default::default() };
        assert_eq!(summary_line(Some(&ok), "x", "y"), "A — https://a.test/");
        let loading = PageInfo { ready: "loading".into(), status: Some(404), ..ok.clone() };
        assert_eq!(summary_line(Some(&loading), "x", "y"), "A — https://a.test/ (HTTP 404, still loading)");
        let untitled = PageInfo { title: String::new(), ..ok.clone() };
        assert_eq!(summary_line(Some(&untitled), "x", "y"), "(untitled page) — https://a.test/");
        let failed = PageInfo { error: Some("ERR_NAME_NOT_RESOLVED".into()), ..ok.clone() };
        assert_eq!(summary_line(Some(&failed), "https://nope.test/", "y"), "failed to load: ERR_NAME_NOT_RESOLVED — https://nope.test/");
        // The page cannot be read: what the shell knows.
        assert_eq!(summary_line(None, "https://n.test/", "Native"), "Native — https://n.test/");
        assert_eq!(summary_line(None, "https://n.test/", ""), "(untitled page) — https://n.test/");
        let fine = PageInfo { status: Some(200), ..ok };
        assert_eq!(summary_line(Some(&fine), "x", "y"), "A — https://a.test/");
    }

    #[test]
    fn history_moves_to_the_entry_before_or_after() {
        let entries = vec![json!({ "id": 10 }), json!({ "id": 11 }), json!({ "id": 12 })];
        assert_eq!(history_entry(&entries, 1, "back"), Some(10));
        assert_eq!(history_entry(&entries, 1, "forward"), Some(12));
        assert_eq!(history_entry(&entries, 0, "back"), None);
        assert_eq!(history_entry(&entries, 2, "forward"), None);
    }

    #[test]
    fn dialog_errors_name_the_dialog_and_are_recognised() {
        let dialog = shell::dialog::DialogInfo { kind: "confirm".into(), message: "Delete it?".into(), default_text: None, url: "https://x.test/".into() };
        let error = dialog_error(&dialog);
        assert_eq!(error, "a confirm dialog is open: \"Delete it?\"; answer it with browser_dialog");
        assert!(is_dialog_error(&error));
        assert!(!is_dialog_error("the page did not answer within 30s"));
        let long = shell::dialog::DialogInfo { message: "x".repeat(500), ..dialog };
        assert!(dialog_error(&long).len() < 300);
    }

    #[test]
    fn a_closed_tab_error_reads_as_gone() {
        assert_eq!(explain("browser tab browser-3 is closed"), "Browser tab browser-3 is gone. Use browser_tabs or browser_navigate.");
        assert_eq!(explain("the page went away"), "the page went away");
        let stop = Stop::from("browser tab browser-9 is closed".to_string());
        assert!(stop.0.is_error);
    }

    #[test]
    fn javascript_literals_and_template_style_display() {
        assert_eq!(js("a\"b"), "\"a\\\"b\"");
        assert_eq!(js(&json!({ "filter": "all", "maxChars": 30000 })), "{\"filter\":\"all\",\"maxChars\":30000}");
        assert_eq!(show(Some(&json!("ref_1"))), "ref_1");
        assert_eq!(show(Some(&json!(true))), "true");
        assert_eq!(show(None), "undefined");
        assert_eq!(cut_chars("héllo", 3), "hél");
        assert_eq!(cut_chars("hi", 3), "hi");
    }

    #[test]
    fn the_tab_id_argument_is_a_non_empty_string() {
        assert_eq!(tab_id_arg(&json!({ "tabId": "browser-2" })).as_deref(), Some("browser-2"));
        assert_eq!(tab_id_arg(&json!({ "tabId": "" })), None);
        assert_eq!(tab_id_arg(&json!({})), None);
        assert_eq!(tab_id_arg(&json!({ "tabId": null })), None);
        assert_eq!(tab_id_arg(&json!({ "tabId": 7 })).as_deref(), Some("7"), "a wrong type names a tab that does not exist");
    }

    #[test]
    fn the_site_in_a_line_is_the_host_and_port() {
        assert_eq!(site_of("https://example.com/a?b=1"), "example.com");
        assert_eq!(site_of("http://localhost:3000/x"), "localhost:3000");
        assert_eq!(site_of("not a url"), "not a url");
    }
}
