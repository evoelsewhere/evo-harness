//! `browser_tabs` with `create`, and `browser_status` (docs/features/agent-browser
//! FR-9, FR-10, T20).
//!
//! `browser_tabs` keeps what core's tool does (list, select, close, `*` marks
//! the visible tab, every tab with title, address and loading state) by running
//! it, and adds `create`: a blank tab, or a tab on a URL (opened through
//! `browser_navigate`, so the approval gate sees it). `browser_status` says
//! which tools work on this platform and what the active tab is doing.

use serde_json::{json, Value};

use super::core;
use super::driver::{finish, resolve_page, show_browser, tab_id_arg, Page, ToolResult};
use crate::agent_tools::{call_tool, BoxFuture, Class, Content, Tool, ToolCtx, ToolOutput};
use crate::browser as shell;

const TABS: &str = "List, select, close or create the built-in browser's tabs (shared with the user). The list shows each tab's id, title, address and whether it is loading; * marks the visible tab. create opens a new tab (url, default a blank page) and makes it the visible one.";
const STATUS: &str = "Which browser tools work on this platform (real mouse and key input, screenshots, frames, dialogs, uploads, emulation, downloads) and what falls back to page scripts or is unavailable, plus whether the active tab is ready (page helper, loading, open dialog). Needs no tab.";

fn schema(properties: Value) -> Value {
    let mut props = properties.as_object().cloned().unwrap_or_default();
    props.insert("tabId".into(), json!({ "type": "string", "description": "A tab id from browser_tabs; default: the active tab." }));
    json!({ "type": "object", "properties": props, "additionalProperties": false })
}

fn tabs_schema() -> Value {
    schema(json!({
        "action": { "type": "string", "enum": ["list", "select", "close", "create"], "description": "Default list." },
        "url": { "type": "string", "description": "With create: the address to open (default a blank page)." },
    }))
}

fn status_schema() -> Value {
    schema(json!({}))
}

fn tabs_run(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(async move { finish(tabs(&ctx, &args).await) })
}

fn status_run(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(async move { finish(status(&ctx, &args).await) })
}

/// The `browser_tabs` that replaces core's, and `browser_status`.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "browser_tabs", description: TABS, schema: tabs_schema, class: Class::Read, run: tabs_run },
        Tool { name: "browser_status", description: STATUS, schema: status_schema, class: Class::Read, run: status_run },
    ]
}

// ── browser_tabs ────────────────────────────────────────────────────────────

const ACTIONS: [&str; 4] = ["list", "select", "close", "create"];

/// `action`, validated; `list` by default.
pub fn parse_action(args: &Value) -> Result<&'static str, String> {
    match args.get("action") {
        None | Some(Value::Null) => Ok("list"),
        Some(Value::String(action)) => ACTIONS.iter().find(|known| **known == action).copied().ok_or_else(|| format!("action must be one of {}.", ACTIONS.join(", "))),
        Some(_) => Err("action must be a string.".into()),
    }
}

/// The address `create` opens: none for a blank tab.
pub fn create_url(args: &Value) -> Result<Option<String>, String> {
    match args.get("url") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(url)) => {
            let url = url.trim();
            Ok(if url.is_empty() || url == "about:blank" { None } else { Some(url.to_string()) })
        }
        Some(_) => Err("url must be a string.".into()),
    }
}

/// The text of a result (its text parts, joined).
fn text_of(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(|part| match part {
            Content::Text(text) => Some(text.as_str()),
            Content::Image { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Core's `browser_tabs`, run as it is.
async fn core_tabs(ctx: &ToolCtx, args: Value) -> ToolOutput {
    match core::tools().into_iter().find(|tool| tool.name == "browser_tabs") {
        Some(tool) => (tool.run)(ctx.clone(), args).await,
        None => ToolOutput::error("The tab list is not available."),
    }
}

async fn tabs(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let action = parse_action(args)?;
    let url = create_url(args)?;
    if action != "create" {
        let mut forwarded = args.clone();
        forwarded["action"] = json!(action);
        return Ok(core_tabs(ctx, forwarded).await);
    }
    let opened = match url {
        // Opening an address is a navigation: its own tool asks the gate and waits for the load.
        Some(url) => {
            let result = call_tool(ctx, "browser_navigate", json!({ "url": url, "newTab": true })).await;
            if result.is_error || ctx.cancelled() {
                return Ok(result);
            }
            text_of(&result)
        }
        None => {
            show_browser(ctx, None).await;
            let id = shell::create_for_chat(&ctx.app, &ctx.window(), &ctx.chat, "about:blank", false)?;
            shell::mark_driven(&ctx.app, &id);
            format!("Opened a blank tab {id}.")
        }
    };
    let listing = core_tabs(ctx, json!({ "action": "list" })).await;
    Ok(ToolOutput::text(format!("{opened}\n{}", text_of(&listing))))
}

// ── browser_status ──────────────────────────────────────────────────────────

/// What the shell can do with a page on this platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caps {
    /// DevTools protocol commands (real input, screenshots, emulation, uploads).
    pub devtools: bool,
    /// The shell holds page dialogs until they are answered.
    pub dialogs: bool,
    pub downloads: bool,
    pub platform: String,
}

impl Caps {
    pub fn from_status(status: &shell::BrowserStatus) -> Self {
        Self { devtools: status.devtools, dialogs: status.dialogs, downloads: status.downloads, platform: status.platform.to_string() }
    }

    /// What this build can do when there is no page to ask.
    pub fn for_platform() -> Self {
        let windows = cfg!(windows);
        Self {
            devtools: windows,
            dialogs: windows,
            downloads: windows,
            platform: if windows {
                "windows"
            } else if cfg!(target_os = "macos") {
                "macos"
            } else {
                "linux"
            }
            .to_string(),
        }
    }
}

/// The active tab's state.
#[derive(Debug, Clone, PartialEq)]
pub struct Readiness {
    pub tab: String,
    /// `title — address (notes)`.
    pub summary: String,
    /// `window.__evo` is there (error pages and blank tabs have none).
    pub helper: bool,
    pub loading: bool,
    /// The open dialog, if any, as a line.
    pub dialog: Option<String>,
}

/// One line per capability: what works, what falls back, what is unavailable.
pub fn capability_lines(caps: &Caps) -> Vec<String> {
    let yes = caps.devtools;
    let by = |works: &str, fallback: &str| if yes { works.to_string() } else { fallback.to_string() };
    vec![
        format!(
            "- Real mouse and key input (browser_click with button or clickCount, point clicks, browser_key, typing): {}",
            by("available (DevTools)", "not available; browser_click by ref and browser_type into a ref work by script")
        ),
        format!("- browser_hover and browser_drag: {}", by("real mouse events (DevTools)", "by script events; CSS :hover does not apply, and a point cannot be used")),
        format!("- browser_screenshot (visible part, full page, element, region): {}", by("available (DevTools)", "not available; use browser_read_page")),
        "- Frames (browser_frames, the frame argument, refs like f1:ref_3): same-origin iframes by script; cross-origin iframes cannot be reached".to_string(),
        format!(
            "- Dialogs (browser_dialog): {}",
            if caps.dialogs {
                "held open until answered; calls that open one return and say so"
            } else {
                "answered by a script policy (browser_dialog sets it and shows what was answered)"
            }
        ),
        format!("- browser_upload: {}", by("available (DevTools)", "not available")),
        format!("- Emulation (viewport, device, colour scheme): {}", by("available (DevTools)", "not available")),
        format!("- Downloads (browser_downloads): {}", if caps.downloads { "available" } else { "not available" }),
        "- Reading, finding, scrolling, forms, waiting, eval, console, network, tabs, batch: available (page scripts)".to_string(),
    ]
}

/// The whole report of `browser_status`.
pub fn status_text(caps: &Caps, ready: Option<&Readiness>) -> String {
    let mut lines = vec![format!(
        "Browser status: {} ({}).",
        caps.platform,
        if caps.devtools { "the DevTools protocol is available" } else { "no DevTools access in the webview here; page scripts only" }
    )];
    lines.push("Tools on this platform:".to_string());
    lines.extend(capability_lines(caps));
    match ready {
        None => lines.push("Active tab: none is open. browser_navigate opens one, or browser_tabs with action create.".to_string()),
        Some(ready) => {
            lines.push(format!("Active tab: {} — {}", ready.tab, ready.summary));
            lines.push(format!(
                "- Page helper: {}",
                if ready.helper { "ready" } else { "not ready (blank tabs, error pages and pages still loading have none: reading and finding fail until the page loads)" }
            ));
            lines.push(format!("- Loading: {}", if ready.loading { "yes (browser_wait with networkIdleMs or a text waits for it)" } else { "no" }));
            lines.push(format!("- Open dialog: {}", ready.dialog.as_deref().unwrap_or("none")));
        }
    }
    lines.join("\n")
}

/// The active tab's readiness (does not mark it as driven).
async fn readiness(ctx: &ToolCtx, args: &Value) -> Option<(Readiness, Caps)> {
    let tab = shell::find_tab(&ctx.app, &ctx.chat, tab_id_arg(args).as_deref())?;
    let page = Page::for_ctx(ctx, &tab);
    let caps = shell::status(&ctx.app, &tab).map(|status| Caps::from_status(&status)).unwrap_or_else(|_| Caps::for_platform());
    let helper = page.eval("typeof window.__evo === 'object'", false).await.ok().and_then(|v| v.as_bool()).unwrap_or(false);
    let loading = page.is_loading();
    let dialog = shell::dialog_pending(&ctx.app, &tab).map(|d| format!("a {} dialog: {}", d.kind, super::driver::cut_chars(&d.message, 120)));
    let summary = page.summary().await;
    Some((Readiness { tab, summary, helper, loading, dialog }, caps))
}

async fn status(ctx: &ToolCtx, args: &Value) -> ToolResult {
    // A tab id that is not this chat's page is an error, as for every tool.
    if tab_id_arg(args).is_some() {
        resolve_page(ctx, args)?;
    }
    Ok(ToolOutput::text(match readiness(ctx, args).await {
        Some((ready, caps)) => status_text(&caps, Some(&ready)),
        None => status_text(&Caps::for_platform(), None),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows() -> Caps {
        Caps { devtools: true, dialogs: true, downloads: true, platform: "windows".into() }
    }

    fn linux() -> Caps {
        Caps { devtools: false, dialogs: false, downloads: false, platform: "linux".into() }
    }

    #[test]
    fn the_tab_action_is_one_of_four() {
        assert_eq!(parse_action(&json!({})), Ok("list"));
        assert_eq!(parse_action(&json!({ "action": "create" })), Ok("create"));
        assert_eq!(parse_action(&json!({ "action": "close" })), Ok("close"));
        assert!(parse_action(&json!({ "action": "reload" })).unwrap_err().contains("list, select, close, create"));
        assert!(parse_action(&json!({ "action": 2 })).is_err());
    }

    #[test]
    fn create_opens_a_blank_tab_unless_given_an_address() {
        assert_eq!(create_url(&json!({})), Ok(None));
        assert_eq!(create_url(&json!({ "url": "" })), Ok(None));
        assert_eq!(create_url(&json!({ "url": "about:blank" })), Ok(None));
        assert_eq!(create_url(&json!({ "url": " localhost:3000 " })), Ok(Some("localhost:3000".into())));
        assert!(create_url(&json!({ "url": 3 })).is_err());
    }

    #[test]
    fn text_of_a_result_skips_pictures() {
        let mut out = ToolOutput::text("");
        out.content = vec![Content::Image { data: "AA".into(), mime_type: "image/png".into() }, Content::Text("a".into()), Content::Text("b".into())];
        assert_eq!(text_of(&out), "a\nb");
    }

    #[test]
    fn status_on_windows_lists_everything_as_available() {
        let text = status_text(&windows(), None);
        assert!(text.starts_with("Browser status: windows (the DevTools protocol is available)."));
        assert!(text.contains("browser_screenshot (visible part, full page, element, region): available (DevTools)"));
        assert!(text.contains("browser_upload: available (DevTools)"));
        assert!(text.contains("Dialogs (browser_dialog): held open until answered"));
        assert!(text.contains("Downloads (browser_downloads): available"));
        assert!(text.contains("Active tab: none is open"));
        assert!(!text.contains("not available"), "{text}");
    }

    #[test]
    fn status_without_devtools_names_each_fallback() {
        let text = status_text(&linux(), None);
        assert!(text.starts_with("Browser status: linux (no DevTools access"));
        assert!(text.contains("browser_hover and browser_drag: by script events; CSS :hover does not apply"));
        assert!(text.contains("browser_screenshot (visible part, full page, element, region): not available; use browser_read_page"));
        assert!(text.contains("browser_upload: not available"));
        assert!(text.contains("Dialogs (browser_dialog): answered by a script policy"));
        assert!(text.contains("Emulation (viewport, device, colour scheme): not available"));
        assert!(text.contains("Downloads (browser_downloads): not available"));
        assert!(text.contains("Frames (browser_frames, the frame argument, refs like f1:ref_3): same-origin iframes by script"));
    }

    #[test]
    fn status_reports_the_active_tab() {
        let ready = Readiness {
            tab: "browser-2".into(),
            summary: "Shop — https://shop.test/ (still loading)".into(),
            helper: true,
            loading: true,
            dialog: Some("a confirm dialog: Delete it?".into()),
        };
        let text = status_text(&windows(), Some(&ready));
        assert!(text.contains("Active tab: browser-2 — Shop — https://shop.test/ (still loading)"));
        assert!(text.contains("- Page helper: ready"));
        assert!(text.contains("- Loading: yes"));
        assert!(text.contains("- Open dialog: a confirm dialog: Delete it?"));
        let blank = Readiness { tab: "browser-3".into(), summary: "(untitled page) — about:blank".into(), helper: false, loading: false, dialog: None };
        let text = status_text(&windows(), Some(&blank));
        assert!(text.contains("- Page helper: not ready"));
        assert!(text.contains("- Loading: no"));
        assert!(text.contains("- Open dialog: none"));
    }

    #[test]
    fn the_platform_caps_follow_the_build() {
        let caps = Caps::for_platform();
        assert_eq!(caps.devtools, cfg!(windows));
        assert_eq!(caps.dialogs, cfg!(windows));
        assert!(["windows", "macos", "linux"].contains(&caps.platform.as_str()));
    }

    #[test]
    fn the_tools_are_observe_and_strict() {
        let tools = tools();
        assert_eq!(tools.iter().map(|t| t.name).collect::<Vec<_>>(), ["browser_tabs", "browser_status"]);
        for tool in &tools {
            assert_eq!(tool.class, Class::Read);
            let schema = (tool.schema)();
            assert_eq!(schema["additionalProperties"], false);
            assert!(schema["properties"]["tabId"].is_object());
        }
        assert_eq!((tools[0].schema)()["properties"]["action"]["enum"], json!(["list", "select", "close", "create"]));
        assert!((tools[0].schema)()["properties"]["url"].is_object());
    }
}
