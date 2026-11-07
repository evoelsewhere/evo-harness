//! The core browser tools: navigate, tabs, screenshot, read, find, click, type,
//! key, form input, scroll, eval, console and network (docs/features/agent-browser
//! T8, FR-24, FR-25). Ported from the webview's `tools/core.ts` with the same
//! descriptions, schemas and behaviour; they work on the tabs of the calling
//! chat and run through the page driver (`driver.rs`).
//!
//! Classes: navigate, click, type, key, form_input and scroll are acts (gated on
//! the page's site); eval is a change; the rest observe and never ask.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use super::driver::{
    finish, gate_page, is_dialog_error, js, resolve_page, resolve_tab, settle, show, show_browser, tab_id_arg, Page, Stop, ToolResult,
};
use crate::agent_tools::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};
use crate::browser as shell;

// ── Arguments and schemas ───────────────────────────────────────────────────

fn text(value: impl Into<String>) -> ToolOutput {
    ToolOutput::text(value)
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn num_arg(args: &Value, key: &str) -> Option<f64> {
    args.get(key).and_then(Value::as_f64)
}

fn flag(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// `String(args.<key> ?? "")`.
fn string_arg(args: &Value, key: &str) -> String {
    match args.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// `args.<key> ? String(args.<key>) : ""`.
fn ref_arg(args: &Value, key: &str) -> String {
    match args.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None | Some(Value::Bool(false)) => String::new(),
        Some(other) => other.to_string(),
    }
}

/// A number as JavaScript prints it in a template.
fn num_js(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// `Math.round`: halves go up.
fn js_round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

/// `properties` plus the `tabId` every tool takes; no other properties are allowed.
fn schema(properties: Value, required: &[&str]) -> Value {
    let mut props = properties.as_object().cloned().unwrap_or_default();
    props.insert("tabId".into(), json!({ "type": "string", "description": "A tab id from browser_tabs; default: the active tab." }));
    let mut object = json!({ "type": "object", "properties": props, "additionalProperties": false });
    if !required.is_empty() {
        object["required"] = json!(required);
    }
    object
}

const NAVIGATE: &str = "Open a URL in the app's built-in browser, which the user sees in the side panel (for previewing a local dev server, checking a page, reading docs). Also takes \"back\", \"forward\" or \"reload\". Opens a tab when none is open (or with newTab). Waits for the page to load and returns its title, URL and HTTP status, or an error when the page failed to load. Then use browser_read_page or browser_screenshot to see it.";
const TABS: &str = "List, select or close the built-in browser's tabs (shared with the user).";
const SCREENSHOT: &str = "A screenshot of the visible part of the page in the built-in browser.";
const READ_PAGE: &str = "The page as an outline: URL, title, headings, text, and interactive elements (links, buttons, fields) each with a ref like [ref_12] to use with browser_click, browser_type, browser_form_input and browser_scroll. Links inside text show as [link text](ref_12). filter \"interactive\" lists only headings and elements.";
const FIND: &str = "Find elements on the page whose role, label or text contains the query; returns them with refs.";
const CLICK: &str = "Click an element by ref (from browser_read_page / browser_find), or a point (x, y in CSS pixels of the screenshot). Returns the element's state afterwards (checked, focused, value) and the page.";
const TYPE: &str = "Type text into ref (focused first, text added at the end), or into the focused element. Returns which element got the text and its value. Date, time, color and range fields are set directly. submit presses Enter afterwards.";
const KEY: &str = "Press a key or combination in the page: Enter, Escape, Tab, Backspace, ArrowDown, PageDown, ctrl+a, shift+Tab…";
const FORM_INPUT: &str = "Set a form field's value directly (text, select option by value or label, checkbox true/false).";
const SCROLL: &str = "Scroll the page: to an element (ref) or the first text matching text, or up/down (amount in screens, default 1), or to the top or bottom.";
const EVAL: &str = "Run JavaScript in the page and return its value as JSON: an expression, or statements (the last one's value). A returned promise is awaited (up to 30s). Console messages it logs are shown too. For inspection and debugging.";
const CONSOLE: &str = "The page's console messages and uncaught errors since it loaded.";
const NETWORK: &str = "Requests the page made (URL, type, status, duration, size), from the browser's resource timing.";

fn navigate_schema() -> Value {
    schema(
        json!({
            "url": { "type": "string", "description": "URL, host (localhost:3000), local file path, or back/forward/reload." },
            "newTab": { "type": "boolean", "description": "Open in a new tab instead of the active one." },
        }),
        &["url"],
    )
}

fn tabs_schema() -> Value {
    schema(json!({ "action": { "type": "string", "enum": ["list", "select", "close"], "description": "Default list." } }), &[])
}

fn screenshot_schema() -> Value {
    schema(json!({}), &[])
}

fn read_page_schema() -> Value {
    schema(
        json!({
            "filter": { "type": "string", "enum": ["all", "interactive"] },
            "maxChars": { "type": "number", "description": "Default 30000." },
        }),
        &[],
    )
}

fn find_schema() -> Value {
    schema(json!({ "query": { "type": "string" } }), &["query"])
}

fn click_schema() -> Value {
    schema(
        json!({
            "ref": { "type": "string" },
            "x": { "type": "number" },
            "y": { "type": "number" },
            "doubleClick": { "type": "boolean" },
        }),
        &[],
    )
}

fn type_schema() -> Value {
    schema(json!({ "text": { "type": "string" }, "ref": { "type": "string" }, "submit": { "type": "boolean" } }), &["text"])
}

fn key_schema() -> Value {
    schema(json!({ "key": { "type": "string" } }), &["key"])
}

fn form_input_schema() -> Value {
    schema(
        json!({
            "ref": { "type": "string" },
            "value": { "type": "string", "description": "Text, an option value or label, or true/false for a checkbox." },
        }),
        &["ref", "value"],
    )
}

fn scroll_schema() -> Value {
    schema(
        json!({
            "direction": { "type": "string", "enum": ["up", "down", "top", "bottom"] },
            "amount": { "type": "number" },
            "ref": { "type": "string", "description": "Scroll this element to the middle of the view." },
            "text": { "type": "string", "description": "Scroll the first visible text containing this (case-insensitive) into view." },
        }),
        &[],
    )
}

fn eval_schema() -> Value {
    schema(json!({ "expression": { "type": "string" } }), &["expression"])
}

fn console_schema() -> Value {
    schema(json!({ "onlyErrors": { "type": "boolean" }, "limit": { "type": "number" } }), &[])
}

fn network_schema() -> Value {
    schema(json!({ "limit": { "type": "number" } }), &[])
}

macro_rules! runner {
    ($name:ident, $body:path) => {
        fn $name(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
            Box::pin(async move { finish($body(&ctx, &args).await) })
        }
    };
}

runner!(navigate_run, navigate);
runner!(tabs_run, tabs);
runner!(screenshot_run, screenshot);
runner!(read_page_run, read_page);
runner!(find_run, find);
runner!(click_run, click_tool);
runner!(type_run, type_tool);
runner!(key_run, key);
runner!(form_input_run, form_input);
runner!(scroll_run, scroll);
runner!(eval_run, eval);
runner!(console_run, console);
runner!(network_run, network);

/// The 13 core tools.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "browser_navigate", description: NAVIGATE, schema: navigate_schema, class: Class::Edit, run: navigate_run },
        Tool { name: "browser_tabs", description: TABS, schema: tabs_schema, class: Class::Read, run: tabs_run },
        Tool { name: "browser_screenshot", description: SCREENSHOT, schema: screenshot_schema, class: Class::Read, run: screenshot_run },
        Tool { name: "browser_read_page", description: READ_PAGE, schema: read_page_schema, class: Class::Read, run: read_page_run },
        Tool { name: "browser_find", description: FIND, schema: find_schema, class: Class::Read, run: find_run },
        Tool { name: "browser_click", description: CLICK, schema: click_schema, class: Class::Edit, run: click_run },
        Tool { name: "browser_type", description: TYPE, schema: type_schema, class: Class::Edit, run: type_run },
        Tool { name: "browser_key", description: KEY, schema: key_schema, class: Class::Edit, run: key_run },
        Tool { name: "browser_form_input", description: FORM_INPUT, schema: form_input_schema, class: Class::Edit, run: form_input_run },
        Tool { name: "browser_scroll", description: SCROLL, schema: scroll_schema, class: Class::Edit, run: scroll_run },
        Tool { name: "browser_eval", description: EVAL, schema: eval_schema, class: Class::Command, run: eval_run },
        Tool { name: "browser_console", description: CONSOLE, schema: console_schema, class: Class::Read, run: console_run },
        Tool { name: "browser_network", description: NETWORK, schema: network_schema, class: Class::Read, run: network_run },
    ]
}

// ── browser_navigate ────────────────────────────────────────────────────────

const HISTORY_ACTIONS: [&str; 3] = ["back", "forward", "reload"];

async fn navigate(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let url = string_arg(args, "url").trim().to_string();
    if url.is_empty() {
        return Err("browser_navigate needs a url.".into());
    }
    let history = HISTORY_ACTIONS.contains(&url.as_str());
    // A tab id that is not this chat's page is no page: a new one opens (as it always did).
    let existing = if flag(args, "newTab") { None } else { shell::find_tab(&ctx.app, &ctx.chat, tab_id_arg(args).as_deref()) };
    if history && existing.is_none() {
        return Err("No browser tab is open to go back, forward or reload.".into());
    }
    let target = if history { None } else { Some(shell::normalize_url(&url)?.to_string()) };

    // The gate: the page it acts on is the target, or the tab's own for back, forward and reload.
    let (gate_url, summary) = match (&existing, &target) {
        (Some(tab), None) => {
            let current = Page::new(&ctx.app, tab).native_url();
            let verb = match url.as_str() {
                "back" => "Go back",
                "forward" => "Go forward",
                _ => "Reload the page",
            };
            let summary = format!("{verb} on {}", super::driver::site_of(&current));
            (current, summary)
        }
        (_, Some(target)) => (target.clone(), format!("Open {target}")),
        (None, None) => unreachable!("a history action has a tab"),
    };
    ctx.gate(&gate_url, &summary).await.map_err(Stop)?;

    show_browser(ctx, existing.as_deref()).await;
    let page = if history {
        let tab = existing.expect("checked above");
        shell::mark_driven(&ctx.app, &tab);
        let page = Page::for_ctx(ctx, &tab);
        if let Some(problem) = page.go_history(&url).await? {
            return Err(Stop(ToolOutput::error(problem)));
        }
        page
    } else if let Some(tab) = existing {
        shell::mark_driven(&ctx.app, &tab);
        shell::navigate_tab(&ctx.app, &tab, &url)?;
        let page = Page::for_ctx(ctx, &tab);
        page.wait_for_navigation("", Duration::from_secs(3)).await;
        page
    } else {
        let id = shell::create_for_chat(&ctx.app, &ctx.window(), &ctx.chat, &url, false)?;
        shell::mark_driven(&ctx.app, &id);
        let page = Page::for_ctx(ctx, &id);
        page.wait_for_navigation("", Duration::from_secs(3)).await;
        page
    };
    if !page.exists() {
        return Err("The tab closed.".into());
    }
    if let Some(error) = page.info().await.and_then(|info| info.error) {
        return Err(format!("The page failed to load: {error} ({url}, tab {}).", page.tab).into());
    }
    Ok(text(format!("Loaded {} (tab {})", page.summary().await, page.tab)))
}

// ── browser_tabs ────────────────────────────────────────────────────────────

/// The title a listing shows for a tab: the load failure, else the page's title, else a placeholder.
fn tab_title(error: Option<&str>, page_title: Option<&str>, known_title: &str) -> String {
    match error {
        Some(error) => format!("failed to load: {error}"),
        None => match page_title.unwrap_or(known_title) {
            "" => "(untitled page)".to_string(),
            title => title.to_string(),
        },
    }
}

fn tab_line(active: bool, id: &str, title: &str, url: &str, loading: bool) -> String {
    format!("{}{id}: {title} — {url}{}", if active { "* " } else { "  " }, if loading { " (loading)" } else { "" })
}

async fn tabs(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let action = str_arg(args, "action").unwrap_or("list");
    if action == "select" || action == "close" {
        let id = tab_id_arg(args);
        let tab = id.as_deref().and_then(|id| shell::find_tab(&ctx.app, &ctx.chat, Some(id)));
        let Some(tab) = tab else {
            return Err(format!("No tab {}.", id.unwrap_or_default()).into());
        };
        if action == "select" {
            shell::select_tab_for_tool(&ctx.app, &ctx.chat, &tab);
        } else {
            shell::close_tab_for_tool(&ctx.app, &tab);
        }
    }
    let listed = shell::chat_tabs(&ctx.app, &ctx.chat);
    if listed.is_empty() {
        return Ok(text("No browser tabs are open. Use browser_navigate to open one."));
    }
    let active = shell::visible_tab(&ctx.app, &ctx.chat);
    let mut lines = Vec::new();
    for (id, record) in listed {
        let page = Page::for_ctx(ctx, &id);
        // The live page, not what the last events said.
        let info = tokio::time::timeout(Duration::from_secs(2), page.info()).await.ok().flatten();
        let error = info.as_ref().and_then(|info| info.error.as_deref());
        let url = match &info {
            Some(info) if error.is_none() => info.href.clone(),
            Some(_) => page.native_url(),
            None => record.url.clone(),
        };
        let loading = match &info {
            Some(info) => info.ready != "complete" && error.is_none(),
            None => record.loading,
        };
        if info.is_some() && !loading && record.loading {
            shell::set_loading(&ctx.app, &id, false);
        }
        let title = tab_title(error, info.as_ref().map(|info| info.title.as_str()), &record.title);
        lines.push(tab_line(active.as_deref() == Some(id.as_str()), &id, &title, &url, loading));
    }
    Ok(text(lines.join("\n")))
}

// ── Observing ───────────────────────────────────────────────────────────────

async fn screenshot(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    show_browser(ctx, Some(&page.tab)).await;
    if !cfg!(windows) {
        return Err("Screenshots are not available on this platform yet; use browser_read_page.".into());
    }
    settle(250).await;
    let shot = page.cdp("Page.captureScreenshot", json!({ "format": "jpeg", "quality": 80 })).await?;
    let data = show(shot.get("data"));
    if data.is_empty() || data == "undefined" {
        return Err("The browser returned no image.".into());
    }
    Ok(ToolOutput::image(data, "image/jpeg", page.summary().await))
}

/// `filter` and `maxChars` of a read: the page helper wants at least 1,000 and at most 200,000 characters.
fn read_options(args: &Value) -> (&'static str, i64) {
    let filter = if str_arg(args, "filter") == Some("interactive") { "interactive" } else { "all" };
    let max_chars = num_arg(args, "maxChars").map_or(30_000, |n| n.clamp(1000.0, 200_000.0) as i64);
    (filter, max_chars)
}

async fn read_page(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let (filter, max_chars) = read_options(args);
    Ok(text(page.helper::<String>(&format!("readPage({})", js(&json!({ "filter": filter, "maxChars": max_chars })))).await?))
}

async fn find(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    Ok(text(page.helper::<String>(&format!("find({})", js(&string_arg(args, "query")))).await?))
}

// ── browser_click ───────────────────────────────────────────────────────────

/// How a real click at a point is made. The default is a left click, double with `doubleClick`;
/// `pointer.rs` passes the other buttons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickInput {
    /// `left`, `right` or `middle`.
    pub button: &'static str,
    /// 1 to 3.
    pub count: u32,
    /// How the first line of the result starts: `Clicked`, `Double-clicked`, `Right-clicked`…
    pub verb: &'static str,
}

impl ClickInput {
    /// What `browser_click` does with these arguments.
    pub fn of(args: &Value) -> Self {
        Self { button: "left", count: if flag(args, "doubleClick") { 2 } else { 1 }, verb: "Clicked" }
    }
}

#[derive(Debug, Deserialize)]
struct Point {
    x: f64,
    y: f64,
    hits: bool,
    covered: Option<String>,
}

/// The first line of a click result.
fn click_line(verb: &str, reference: &str, x: f64, y: f64, notes: &[String]) -> String {
    let what = if reference.is_empty() { format!("({}, {})", js_round(x), js_round(y)) } else { reference.to_string() };
    let notes = if notes.is_empty() { String::new() } else { format!(" ({})", notes.join("; ")) };
    format!("{verb} {what}{notes}.")
}

async fn click_tool(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let reference = ref_arg(args, "ref");
    let what = if reference.is_empty() {
        format!("Click at ({}, {})", num_arg(args, "x").map_or(0, js_round), num_arg(args, "y").map_or(0, js_round))
    } else {
        format!("Click {reference}")
    };
    gate_page(ctx, &page, &what).await?;
    click(ctx, &page, args, ClickInput::of(args)).await
}

/// Click by ref or point. A ref is clicked with real input at its centre when nothing covers it;
/// when the click does not take (a radio stays unchecked, a field unfocused) the element is
/// clicked or focused by script instead. The caller has run the gate.
pub async fn click(ctx: &ToolCtx, page: &Page, args: &Value, input: ClickInput) -> ToolResult {
    show_browser(ctx, Some(&page.tab)).await;
    let before = page.native_url();
    let mut notes: Vec<String> = Vec::new();
    let reference = ref_arg(args, "ref");
    let mut effect = "none".to_string();
    // A click that opens a dialog does not come back until it is answered: that is a result, not a failure.
    let press = |error: String| if is_dialog_error(&error) { Ok(()) } else { Err(error) };
    if !reference.is_empty() {
        effect = page.helper::<String>(&format!("clickEffect({})", js(&reference))).await?;
        if !cfg!(windows) {
            page.helper::<Value>(&format!("click({})", js(&reference))).await?;
        } else {
            let point = page.helper::<Point>(&format!("point({})", js(&reference))).await?;
            if point.hits {
                settle(60).await;
                if let Err(error) = page.click_at(point.x, point.y, input.count, input.button).await {
                    press(error)?;
                }
            } else {
                notes.push(match &point.covered {
                    Some(covered) if !covered.is_empty() => format!("it was covered by {covered}, so it was clicked by script"),
                    _ => "it was off screen, so it was clicked by script".to_string(),
                });
                page.helper::<Value>(&format!("click({})", js(&reference))).await?;
            }
        }
    } else {
        let (Some(x), Some(y)) = (num_arg(args, "x"), num_arg(args, "y")) else {
            return Err("browser_click needs a ref or x and y.".into());
        };
        if !cfg!(windows) {
            return Err("Clicking a point needs the DevTools protocol (Windows); click by ref instead.".into());
        }
        if let Err(error) = page.click_at(x, y, input.count, input.button).await {
            press(error)?;
        }
    }
    settle(250).await;
    if !reference.is_empty() && effect != "none" {
        let took = match page.eval(&format!("window.__evo.took({}, {})", js(&reference), js(&effect)), false).await {
            Ok(Value::Null | Value::Bool(false)) => false,
            Ok(_) | Err(_) => true,
        };
        if !took {
            let by_script = if effect == "check" { "click" } else { "focus" };
            page.helper::<Value>(&format!("{by_script}({})", js(&reference))).await?;
            notes.push(format!(
                "the click did not {} it, so it was {} by script",
                if effect == "check" { "select" } else { "focus" },
                if effect == "check" { "clicked" } else { "focused" }
            ));
        }
    }
    // A click that navigates: wait for the new page.
    if page.is_loading() || page.native_url_or(&before) != before {
        page.wait_for_navigation(&before, Duration::from_secs(3)).await;
    }
    let state = if reference.is_empty() { None } else { page.state_of(&reference).await };
    let focused = if reference.is_empty() { page.focused_state().await } else { None };
    let mut lines = vec![click_line(input.verb, &reference, num_arg(args, "x").unwrap_or(0.0), num_arg(args, "y").unwrap_or(0.0), &notes)];
    if let Some(state) = state {
        lines.push(format!("Now: {state}"));
    } else if let Some(focused) = focused {
        lines.push(format!("Focused: {focused}"));
    }
    lines.push(format!("Page: {}", page.summary().await));
    lines.extend(page.dialog_note());
    Ok(text(lines.join("\n")))
}

// ── browser_type ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Target {
    #[serde(rename = "ref")]
    reference: String,
    kind: String,
    #[serde(default)]
    value: String,
}

/// `"text"` as the result quotes it: cut at 60 characters.
fn quote(value: &str) -> String {
    let shown = if value.chars().count() > 60 { format!("{}…", value.chars().take(60).collect::<String>()) } else { value.to_string() };
    js(&shown)
}

/// What to do after real typing when the field's value does not look right.
#[derive(Debug, PartialEq)]
enum Fixup {
    /// It looks right.
    None,
    /// Nothing arrived (the focus was not where it seemed): insert by script.
    InsertByScript,
    /// The first characters were dropped: set the whole value.
    Complete(String),
}

/// `before` is the field's value before typing, `now` after it. `expected` is what a clean
/// append would give; it is known only when the field was named by a ref.
fn fixup(before: &str, now: &str, value: &str, named: bool) -> Fixup {
    if now == before {
        return Fixup::InsertByScript;
    }
    if named {
        let expected = format!("{before}{value}");
        if now != expected && now.starts_with(before) && expected.ends_with(&now[before.len()..]) {
            return Fixup::Complete(expected);
        }
    }
    Fixup::None
}

async fn type_tool(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let value = string_arg(args, "text");
    let reference = ref_arg(args, "ref");
    let into = if reference.is_empty() { "the focused field".to_string() } else { reference.clone() };
    gate_page(ctx, &page, &format!("Type {} into {into}", quote(&value))).await?;

    if !reference.is_empty() {
        page.helper::<Value>(&format!("focus({})", js(&reference))).await?;
    }
    let target_call = if reference.is_empty() { "typingTarget()".to_string() } else { format!("typingTarget({})", js(&reference)) };
    let target = page.helper::<Option<Target>>(&target_call).await?;
    let mut notes: Vec<String> = Vec::new();
    let kind = target.as_ref().map(|t| t.kind.as_str());
    if kind == Some("other") {
        notes.push("it is not a text field; pass the ref of one".into());
    }
    if let (Some("set"), Some(target)) = (kind, &target) {
        // Date, time, color, range, select: keys fill them part by part; set the value.
        page.helper::<Value>(&format!("setValue({}, {})", js(&target.reference), js(&value))).await?;
    } else if cfg!(windows) {
        let inserted = page.cdp("Input.insertText", json!({ "text": value })).await;
        if let Err(error) = inserted {
            if !is_dialog_error(&error) {
                return Err(error.into());
            }
        }
        settle(60).await;
        if let (Some("keys"), Some(target)) = (kind, &target) {
            if !value.is_empty() {
                let now = page.helper::<Option<Target>>(&format!("typingTarget({})", js(&target.reference))).await?;
                if let Some(now) = now {
                    match fixup(&target.value, &now.value, &value, !reference.is_empty()) {
                        Fixup::InsertByScript => {
                            page.helper::<Value>(&format!("insert({}, {})", js(&target.reference), js(&value))).await?;
                            notes.push("typed by script".into());
                        }
                        Fixup::Complete(expected) => {
                            page.helper::<Value>(&format!("setValue({}, {})", js(&target.reference), js(&expected))).await?;
                            notes.push("completed by script".into());
                        }
                        Fixup::None => {}
                    }
                }
            }
        }
    } else {
        let Some(target) = target.as_ref().filter(|t| t.kind == "keys") else {
            return Err("Typing needs a ref of a text field on this platform.".into());
        };
        page.helper::<Value>(&format!("insert({}, {})", js(&target.reference), js(&value))).await?;
    }
    let submit = flag(args, "submit");
    if submit {
        let before = page.native_url();
        if cfg!(windows) {
            if let Err(error) = page.press_key("Enter").await {
                if !is_dialog_error(&error) {
                    return Err(error.into());
                }
            }
        } else {
            page.eval("document.activeElement?.form?.requestSubmit()", false).await?;
        }
        settle(300).await;
        if page.is_loading() || page.native_url_or(&before) != before {
            page.wait_for_navigation(&before, Duration::from_secs(3)).await;
        }
    } else {
        settle(100).await;
    }
    let now = match &target {
        Some(target) => page.state_of(&target.reference).await,
        None => None,
    };
    let where_ = match (&now, &target) {
        (Some(state), _) => format!("into {state}"),
        (None, Some(target)) => format!("into {}, which is no longer on the page", target.reference),
        (None, None) => "with no field focused (the keys went to the page)".to_string(),
    };
    let notes = if notes.is_empty() { String::new() } else { format!(" ({})", notes.join("; ")) };
    let tail = if submit { format!(", then pressed Enter. Page: {}", page.summary().await) } else { ".".to_string() };
    let mut out = format!("Typed {} {where_}{notes}{tail}", quote(&value));
    if let Some(note) = page.dialog_note() {
        out.push_str(&format!("\n{note}"));
    }
    Ok(text(out))
}

// ── browser_key, browser_form_input, browser_scroll ─────────────────────────

async fn key(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let key = show(args.get("key"));
    gate_page(ctx, &page, &format!("Press {key}")).await?;
    if !cfg!(windows) {
        return Err("Key presses need the DevTools protocol (Windows).".into());
    }
    let combo = string_arg(args, "key");
    if let Err(error) = page.press_key(&combo).await {
        if !is_dialog_error(&error) {
            return Err(error.into());
        }
    }
    settle(200).await;
    let focused = page.focused_state().await;
    let mut out = format!("Pressed {key}.{}", focused.map(|f| format!(" Focused: {f}.")).unwrap_or_default());
    if let Some(note) = page.dialog_note() {
        out.push_str(&format!("\n{note}"));
    }
    Ok(text(out))
}

fn js_value(value: Option<&Value>) -> String {
    value.map_or_else(|| "undefined".to_string(), js)
}

async fn form_input(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let reference = args.get("ref");
    gate_page(ctx, &page, &format!("Set {}", show(reference))).await?;
    let state = page.helper::<String>(&format!("setValue({}, {})", js_value(reference), js_value(args.get("value")))).await?;
    Ok(text(format!("Set {}: {state}.", show(reference))))
}

/// The page script that scrolls by `direction` (`top`, `bottom`, `up`, `down`); `None` for anything else.
fn scroll_script(direction: Option<&str>, amount: f64) -> Option<String> {
    match direction {
        Some("top") => Some("scrollTo(0, 0)".to_string()),
        Some("bottom") => Some("scrollTo(0, document.documentElement.scrollHeight)".to_string()),
        Some(way @ ("up" | "down")) => {
            Some(format!("scrollBy(0, {} * innerHeight * {} * 0.9)", if way == "up" { -1 } else { 1 }, num_js(amount)))
        }
        _ => None,
    }
}

async fn scroll(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let reference = ref_arg(args, "ref");
    let query = ref_arg(args, "text");
    gate_page(ctx, &page, "Scroll the page").await?;
    let mut target = String::new();
    if !reference.is_empty() || !query.is_empty() {
        let mut options = serde_json::Map::new();
        if !reference.is_empty() {
            options.insert("ref".into(), json!(reference));
        }
        if !query.is_empty() {
            options.insert("query".into(), json!(query));
        }
        target = page.helper::<String>(&format!("scrollTo({})", js(&Value::Object(options)))).await?;
    } else {
        let Some(script) = scroll_script(str_arg(args, "direction"), num_arg(args, "amount").unwrap_or(1.0)) else {
            return Err("browser_scroll needs a direction, a ref or text.".into());
        };
        page.eval(&format!("({script}, 0)"), false).await?;
    }
    let at = page.eval("[Math.round(scrollY), Math.max(0, document.documentElement.scrollHeight - innerHeight)]", false).await?;
    let (now, max) = (show(at.get(0)), show(at.get(1)));
    let head = if target.is_empty() { format!("Scrolled {}", show(args.get("direction"))) } else { format!("Scrolled to {target}") };
    Ok(text(format!("{head}; now at {now} of {max}.")))
}

// ── browser_eval, browser_console, browser_network ──────────────────────────

const EVAL_OUTPUT_MAX: usize = 50_000;

/// The value as indented JSON, cut at 50,000 characters.
fn eval_output(value: &Value) -> String {
    let json = serde_json::to_string_pretty(value).unwrap_or_else(|_| "undefined".to_string());
    if json.chars().count() > EVAL_OUTPUT_MAX {
        format!("{}\n… (truncated)", json.chars().take(EVAL_OUTPUT_MAX).collect::<String>())
    } else {
        json
    }
}

async fn eval(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let expression = args.get("expression").and_then(Value::as_str).unwrap_or("undefined");
    let first_line = expression.lines().next().unwrap_or("");
    gate_page(ctx, &page, &format!("Run a script: {}", super::driver::cut_chars(first_line, 100))).await?;
    let seq = page
        .eval("window.__evo?.logSeq() ?? 0", false)
        .await
        .ok()
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite() && *n > 0.0)
        .map_or(0, |n| n as u64);
    let (out, failed) = match page.eval(expression, true).await {
        Ok(value) => (eval_output(&value), false),
        Err(error) => (format!("Error: {error}"), true),
    };
    let logs = page.console_since(seq).await;
    let out = if logs.is_empty() { out } else { format!("{out}\n\nConsole:\n{}", logs.join("\n")) };
    Ok(if failed { ToolOutput::error(out) } else { text(out) })
}

fn console_lines(logs: &[Value]) -> Vec<String> {
    logs.iter().map(|log| format!("[{}] {}", show(log.get("level")), show(log.get("text")))).collect()
}

async fn console(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let limit = num_arg(args, "limit").unwrap_or(100.0);
    let only_errors = flag(args, "onlyErrors");
    let logs = page.helper::<Option<Vec<Value>>>(&format!("console({}, {only_errors})", num_js(limit))).await?.unwrap_or_default();
    if logs.is_empty() {
        return Ok(text(if only_errors { "No errors since the page loaded." } else { "No console messages since the page loaded." }));
    }
    Ok(text(console_lines(&logs).join("\n")))
}

/// `status type url 12ms 345B`, one request per line.
fn network_lines(requests: &[Value]) -> Vec<String> {
    requests
        .iter()
        .map(|r| {
            let status = match r.get("status") {
                None | Some(Value::Null) => "—".to_string(),
                other => show(other),
            };
            let bytes = match r.get("bytes") {
                Some(Value::Number(n)) if n.as_f64().is_some_and(|b| b != 0.0) => format!(" {n}B"),
                _ => String::new(),
            };
            format!("{status} {} {} {}ms{bytes}", show(r.get("type")), show(r.get("url")), show(r.get("ms")))
        })
        .collect()
}

async fn network(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let page = resolve_page(ctx, args)?;
    let limit = num_arg(args, "limit").unwrap_or(100.0);
    let requests = page.helper::<Option<Vec<Value>>>(&format!("network({})", num_js(limit))).await?.unwrap_or_default();
    if requests.is_empty() {
        return Ok(text("No requests recorded."));
    }
    Ok(text(network_lines(&requests).join("\n")))
}

/// The tab a call works on, for tools of other modules that gate on it.
#[allow(dead_code)]
pub fn tab_of(ctx: &ToolCtx, args: &Value) -> Result<String, String> {
    resolve_tab(ctx, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn by_name(name: &str) -> Tool {
        tools().into_iter().find(|t| t.name == name).unwrap_or_else(|| panic!("no tool {name}"))
    }

    #[test]
    fn the_thirteen_tools_keep_their_names_and_classes() {
        let expected = [
            ("browser_navigate", Class::Edit),
            ("browser_tabs", Class::Read),
            ("browser_screenshot", Class::Read),
            ("browser_read_page", Class::Read),
            ("browser_find", Class::Read),
            ("browser_click", Class::Edit),
            ("browser_type", Class::Edit),
            ("browser_key", Class::Edit),
            ("browser_form_input", Class::Edit),
            ("browser_scroll", Class::Edit),
            ("browser_eval", Class::Command),
            ("browser_console", Class::Read),
            ("browser_network", Class::Read),
        ];
        let all = tools();
        assert_eq!(all.len(), 13);
        for (name, class) in expected {
            let tool = all.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("missing {name}"));
            assert_eq!(tool.class, class, "{name}");
            assert!(!tool.description.is_empty());
        }
        let unique: HashSet<_> = all.iter().map(|t| t.name).collect();
        assert_eq!(unique.len(), 13);
    }

    #[test]
    fn every_schema_is_a_closed_object_with_a_tab_id() {
        for tool in tools() {
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(schema["additionalProperties"], false, "{}", tool.name);
            assert_eq!(schema["properties"]["tabId"]["type"], "string", "{}", tool.name);
            assert_eq!(schema["properties"]["tabId"]["description"], "A tab id from browser_tabs; default: the active tab.");
        }
    }

    #[test]
    fn required_arguments_are_as_before() {
        let required = |name: &str| (by_name(name).schema)().get("required").cloned();
        assert_eq!(required("browser_navigate"), Some(json!(["url"])));
        assert_eq!(required("browser_find"), Some(json!(["query"])));
        assert_eq!(required("browser_type"), Some(json!(["text"])));
        assert_eq!(required("browser_key"), Some(json!(["key"])));
        assert_eq!(required("browser_form_input"), Some(json!(["ref", "value"])));
        assert_eq!(required("browser_eval"), Some(json!(["expression"])));
        for name in ["browser_tabs", "browser_screenshot", "browser_read_page", "browser_click", "browser_scroll", "browser_console", "browser_network"] {
            assert_eq!(required(name), None, "{name}");
        }
        let tabs = (by_name("browser_tabs").schema)();
        assert_eq!(tabs["properties"]["action"]["enum"], json!(["list", "select", "close"]));
        let scroll = (by_name("browser_scroll").schema)();
        assert_eq!(scroll["properties"]["direction"]["enum"], json!(["up", "down", "top", "bottom"]));
    }

    #[test]
    fn descriptions_are_the_ones_the_model_always_saw() {
        assert!(by_name("browser_navigate").description.starts_with("Open a URL in the app's built-in browser"));
        assert!(by_name("browser_navigate").description.contains("Also takes \"back\", \"forward\" or \"reload\"."));
        assert_eq!(by_name("browser_tabs").description, "List, select or close the built-in browser's tabs (shared with the user).");
        assert!(by_name("browser_read_page").description.contains("filter \"interactive\" lists only headings and elements."));
        assert!(by_name("browser_key").description.ends_with("ctrl+a, shift+Tab…"));
        assert!(by_name("browser_eval").description.contains("(up to 30s)"));
    }

    #[test]
    fn a_read_asks_for_between_1000_and_200000_characters() {
        assert_eq!(read_options(&json!({})), ("all", 30_000));
        assert_eq!(read_options(&json!({ "filter": "interactive", "maxChars": 5 })), ("interactive", 1000));
        assert_eq!(read_options(&json!({ "maxChars": 9_999_999 })), ("all", 200_000));
        assert_eq!(read_options(&json!({ "filter": "other", "maxChars": 4000 })), ("all", 4000));
    }

    #[test]
    fn scrolling_builds_the_same_scripts() {
        assert_eq!(scroll_script(Some("top"), 1.0).as_deref(), Some("scrollTo(0, 0)"));
        assert_eq!(scroll_script(Some("bottom"), 1.0).as_deref(), Some("scrollTo(0, document.documentElement.scrollHeight)"));
        assert_eq!(scroll_script(Some("down"), 1.0).as_deref(), Some("scrollBy(0, 1 * innerHeight * 1 * 0.9)"));
        assert_eq!(scroll_script(Some("up"), 2.5).as_deref(), Some("scrollBy(0, -1 * innerHeight * 2.5 * 0.9)"));
        assert_eq!(scroll_script(Some("sideways"), 1.0), None);
        assert_eq!(scroll_script(None, 1.0), None);
    }

    #[test]
    fn typed_text_is_quoted_and_cut_at_sixty_characters() {
        assert_eq!(quote("hello"), "\"hello\"");
        assert_eq!(quote("say \"hi\""), "\"say \\\"hi\\\"\"");
        let long = "x".repeat(70);
        assert_eq!(quote(&long), format!("\"{}…\"", "x".repeat(60)));
        assert_eq!(quote(&"é".repeat(60)), format!("\"{}\"", "é".repeat(60)));
    }

    #[test]
    fn typing_is_checked_against_what_the_field_holds() {
        // Nothing arrived.
        assert_eq!(fixup("ab", "ab", "cd", true), Fixup::InsertByScript);
        assert_eq!(fixup("", "", "cd", false), Fixup::InsertByScript);
        // All of it arrived.
        assert_eq!(fixup("ab", "abcd", "cd", true), Fixup::None);
        // The first characters were dropped: complete it.
        assert_eq!(fixup("ab", "abd", "cd", true), Fixup::Complete("abcd".into()));
        // Typing into the focused element (no ref): no expectation to check.
        assert_eq!(fixup("ab", "abd", "cd", false), Fixup::None);
        // Something else changed the field: leave it.
        assert_eq!(fixup("ab", "zz", "cd", true), Fixup::None);
        // Multi-byte text: the prefix check is on characters and must not panic.
        assert_eq!(fixup("héllo ", "héllo wörld", "wörld", true), Fixup::None);
        assert_eq!(fixup("é", "éö", "xö", true), Fixup::Complete("éxö".into()));
    }

    #[test]
    fn the_click_line_names_the_element_or_the_point_and_the_notes() {
        assert_eq!(click_line("Clicked", "ref_12", 0.0, 0.0, &[]), "Clicked ref_12.");
        assert_eq!(click_line("Clicked", "", 10.4, 20.5, &[]), "Clicked (10, 21).");
        assert_eq!(
            click_line("Double-clicked", "ref_3", 0.0, 0.0, &["it was off screen, so it was clicked by script".into(), "x".into()]),
            "Double-clicked ref_3 (it was off screen, so it was clicked by script; x)."
        );
        assert_eq!(js_round(-0.5), 0);
        assert_eq!(js_round(2.5), 3);
    }

    #[test]
    fn the_default_click_is_left_and_double_with_the_flag() {
        assert_eq!(ClickInput::of(&json!({})), ClickInput { button: "left", count: 1, verb: "Clicked" });
        assert_eq!(ClickInput::of(&json!({ "doubleClick": true })).count, 2);
    }

    #[test]
    fn tab_listings_mark_the_active_tab_and_say_what_failed() {
        assert_eq!(tab_line(true, "browser-1", "A", "https://a.test/", false), "* browser-1: A — https://a.test/");
        assert_eq!(tab_line(false, "browser-2", "B", "https://b.test/", true), "  browser-2: B — https://b.test/ (loading)");
        assert_eq!(tab_title(Some("ERR_FAILED"), Some("x"), "y"), "failed to load: ERR_FAILED");
        assert_eq!(tab_title(None, Some("Page"), "old"), "Page");
        assert_eq!(tab_title(None, None, "old"), "old");
        assert_eq!(tab_title(None, Some(""), "old"), "(untitled page)");
        assert_eq!(tab_title(None, None, ""), "(untitled page)");
    }

    #[test]
    fn eval_results_are_indented_json_cut_at_fifty_thousand() {
        assert_eq!(eval_output(&json!(null)), "null");
        assert_eq!(eval_output(&json!({ "a": [1, 2] })), "{\n  \"a\": [\n    1,\n    2\n  ]\n}");
        let long = eval_output(&json!("x".repeat(60_000)));
        assert!(long.ends_with("\n… (truncated)"));
        assert_eq!(long.chars().count(), EVAL_OUTPUT_MAX + "\n… (truncated)".chars().count());
        let fits = eval_output(&json!("y".repeat(100)));
        assert!(!fits.contains("truncated"));
    }

    #[test]
    fn console_and_network_lines_read_as_before() {
        let logs = vec![json!({ "level": "error", "at": 1, "text": "boom" }), json!({ "level": "log", "at": 2, "text": "hi" })];
        assert_eq!(console_lines(&logs), ["[error] boom", "[log] hi"]);
        let requests = vec![
            json!({ "url": "https://a.test/x.js", "type": "script", "status": 200, "ms": 12, "bytes": 345 }),
            json!({ "url": "https://a.test/y", "type": "fetch", "ms": 7 }),
            json!({ "url": "https://a.test/z", "type": "img", "status": 404, "ms": 3, "bytes": 0 }),
        ];
        assert_eq!(
            network_lines(&requests),
            ["200 script https://a.test/x.js 12ms 345B", "— fetch https://a.test/y 7ms", "404 img https://a.test/z 3ms"]
        );
    }

    #[test]
    fn numbers_print_like_a_template_literal() {
        assert_eq!(num_js(100.0), "100");
        assert_eq!(num_js(2.5), "2.5");
        assert_eq!(num_js(-1.0), "-1");
    }

    #[test]
    fn arguments_follow_javascript_truthiness() {
        let args = json!({ "ref": "ref_1", "empty": "", "no": false, "n": 5 });
        assert_eq!(ref_arg(&args, "ref"), "ref_1");
        assert_eq!(ref_arg(&args, "empty"), "");
        assert_eq!(ref_arg(&args, "no"), "");
        assert_eq!(ref_arg(&args, "missing"), "");
        assert_eq!(ref_arg(&args, "n"), "5");
        assert!(!flag(&args, "ref"));
        assert_eq!(string_arg(&args, "n"), "5");
        assert_eq!(string_arg(&args, "missing"), "");
        assert_eq!(string_arg(&json!({ "a": null }), "a"), "");
    }
}
