//! Webbridge pages for the browser tools (docs/features/browser-bridge/spec.md
//! FR-5, FR-6, FR-20, FR-22; plan 3.2 `webbridge/page.rs`, 3.5).
//!
//! The page functions in `browser.rs` send a Webbridge tab here. Commands go
//! to the extension as `cdp` requests; the first one on a tab attaches the
//! debugging API and adds the page helper (`browser_page.js`) to the page and
//! to every document it loads next. Page dialogs, subscribed events and load
//! state land in the same tables the built-in browser uses, so the tools read
//! them the same way.

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use super::groups::{self, split};
use super::{Webbridge, REQUEST_TIMEOUT};
use crate::browser::{self, dialog};

/// Commands that take a tab away from the page it shows.
fn leaves_page(method: &str) -> bool {
    matches!(
        method,
        "Page.navigate"
            | "Page.navigateToHistoryEntry"
            | "Page.getNavigationHistory"
            | "Page.stopLoading"
    )
}

/// What the agent is told for a page the browser keeps from extensions.
pub(crate) const FORBIDDEN: &str = "The browser does not let extensions use this page.";

/// Pages the browser does not let extensions debug (spec FR-22): its own pages,
/// other extensions' pages, and the extension stores.
pub(crate) fn forbidden_by_browser(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    const SCHEMES: [&str; 8] = [
        "chrome:",
        "edge:",
        "chrome-extension:",
        "extension:",
        "chrome-search:",
        "chrome-untrusted:",
        "devtools:",
        "view-source:",
    ];
    if SCHEMES.iter().any(|scheme| lower.starts_with(scheme)) {
        return true;
    }
    // about:blank is where every new tab starts; the other about: pages are the browser's.
    if lower.starts_with("about:") && lower != "about:blank" && !lower.starts_with("about:blank#") {
        return true;
    }
    let origin = crate::agent_tools::gate::origin_of(&lower);
    origin == "https://chromewebstore.google.com"
        || origin == "https://microsoftedge.microsoft.com"
            && lower.contains("://microsoftedge.microsoft.com/addons")
        || origin == "https://chrome.google.com" && lower.contains("://chrome.google.com/webstore")
}

/// Why the agent may not use a tab showing `url`, if it may not (spec FR-22).
/// `leaving`: the call takes the tab away from that page, which is always allowed.
pub(crate) fn refusal(
    url: &str,
    permission: super::sites::Permission,
    leaving: bool,
) -> Option<String> {
    if leaving || url.is_empty() {
        return None;
    }
    if forbidden_by_browser(url) {
        return Some(FORBIDDEN.to_string());
    }
    (permission == super::sites::Permission::Block)
        .then(|| format!("The person blocked {} for Evo.", super::sites::site_of(url)))
}

/// Why the agent may not open `url` in a Webbridge tab, if it may not.
fn refusal_to_open(url: &str) -> Option<String> {
    refusal(url, super::sites::permission(url), false)
}

/// Where a Webbridge tab's agent work is refused before it reaches the browser.
fn check(app: &AppHandle, tab: &str, method: &str) -> Result<(String, i64), String> {
    let (profile, number) =
        split(tab).ok_or_else(|| format!("No browser tab {tab}; see browser_tabs."))?;
    let state = app.state::<Webbridge>();
    // A page the browser forbids or a Block site: nothing but leaving it (spec FR-22).
    if let Some(url) = crate::browser::tab_record(app, tab).map(|t| t.url) {
        if let Some(why) = refusal(&url, super::sites::permission(&url), leaves_page(method)) {
            return Err(why);
        }
    }
    if state.holds.lock().expect("webbridge lock").is_taken(tab) {
        return Err("The person has control of this tab. Wait for them to hand it back (browser_wait), or work in another tab.".into());
    }
    Ok((profile.to_string(), number))
}

/// Attach and add the page helper, once per attachment.
async fn prepare(app: &AppHandle, tab: &str, profile: &str, number: i64) -> Result<(), String> {
    let state = app.state::<Webbridge>();
    if !state
        .prepared
        .lock()
        .expect("webbridge lock")
        .insert(tab.to_string())
    {
        state.holds.lock().expect("webbridge lock").touch(tab);
        return Ok(());
    }
    let result = async {
        state
            .request(
                profile,
                json!({ "t": "attach", "tab": number }),
                REQUEST_TIMEOUT,
            )
            .await?;
        let script = browser::page_script();
        raw(app, profile, number, "Page.enable", json!({})).await?;
        // A background tab is hidden, and a hidden page does not take input (each event waits ~5 s
        // and does nothing): focus emulation makes it act as the focused, visible page.
        raw(
            app,
            profile,
            number,
            "Emulation.setFocusEmulationEnabled",
            json!({ "enabled": true }),
        )
        .await?;
        raw(
            app,
            profile,
            number,
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": script }),
        )
        .await?;
        // The page already loaded: give it the helper now (a guard inside makes this safe to repeat).
        let _ = raw(
            app,
            profile,
            number,
            "Runtime.evaluate",
            json!({ "expression": script }),
        )
        .await;
        // The indicator (FR-15): on every document, its buttons reach the shell through a binding.
        // Bindings reach new documents only with the Runtime domain on.
        raw(app, profile, number, "Runtime.enable", json!({})).await?;
        raw(
            app,
            profile,
            number,
            "Runtime.addBinding",
            json!({ "name": BINDING }),
        )
        .await?;
        raw(
            app,
            profile,
            number,
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": OVERLAY }),
        )
        .await?;
        let _ = raw(
            app,
            profile,
            number,
            "Runtime.evaluate",
            json!({ "expression": OVERLAY }),
        )
        .await;
        Ok::<(), String>(())
    }
    .await;
    match result {
        Ok(()) => {
            state.holds.lock().expect("webbridge lock").touch(tab);
            show_overlay(app, tab);
            Ok(())
        }
        Err(error) => {
            state.prepared.lock().expect("webbridge lock").remove(tab);
            Err(error)
        }
    }
}

/// The indicator's script and the binding its buttons call.
const OVERLAY: &str = include_str!("overlay.js");
const BINDING: &str = "__evoWebbridge";

/// Set the indicator of `tab` to what the shell knows: working, taken over, or off.
pub fn show_overlay(app: &AppHandle, tab: &str) {
    let state = app.state::<Webbridge>();
    let attached = state.prepared.lock().expect("webbridge lock").contains(tab);
    let taken = state.holds.lock().expect("webbridge lock").is_taken(tab);
    let mode = if !attached {
        "off"
    } else if taken {
        "taken"
    } else {
        "working"
    };
    set_overlay(app, tab, mode);
}

fn set_overlay(app: &AppHandle, tab: &str, mode: &str) {
    let Some(expression) = overlay_expression(app, tab, mode) else {
        return;
    };
    let Some((profile, number)) = split(tab) else {
        return;
    };
    let (app, profile) = (app.clone(), profile.to_string());
    tauri::async_runtime::spawn(async move {
        let _ = raw(
            &app,
            &profile,
            number,
            "Runtime.evaluate",
            json!({ "expression": expression }),
        )
        .await;
    });
}

/// The page statement that sets `tab`'s indicator to `mode`.
fn overlay_expression(app: &AppHandle, tab: &str, mode: &str) -> Option<String> {
    split(tab)?;
    let chat = crate::browser::tab_record(app, tab)
        .and_then(|t| t.chat)
        .map(|c| super::chat_title(app, &c))
        .unwrap_or_default();
    Some(format!(
        "window.__evoOverlay && window.__evoOverlay.set({})",
        json!({ "state": mode, "chat": chat })
    ))
}

async fn raw(
    app: &AppHandle,
    profile: &str,
    number: i64,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    app.state::<Webbridge>()
        .request(
            profile,
            json!({ "t": "cdp", "tab": number, "method": method, "params": params }),
            REQUEST_TIMEOUT,
        )
        .await
}

/// A DevTools command on a Webbridge tab.
pub async fn cdp(app: &AppHandle, tab: &str, method: &str, params: Value) -> Result<Value, String> {
    let (profile, number) = check(app, tab, method)?;
    prepare(app, tab, &profile, number).await?;
    if method.starts_with("Input.") {
        wait_for_person(app, tab).await?;
        let state = app.state::<Webbridge>();
        state.holds.lock().expect("webbridge lock").agent_input(tab);
        let reply = raw(app, &profile, number, method, params).await;
        state
            .holds
            .lock()
            .expect("webbridge lock")
            .agent_input_done(tab, super::now_ms());
        return reply;
    }
    if method == "Page.captureScreenshot" {
        // A background tab draws a new frame only when something changes, so a capture may wait for
        // one that never comes: a tiny screencast around it makes the tab draw (measured: every
        // capture in ~70 ms instead of one in two timing out).
        let _ = raw(
            app,
            &profile,
            number,
            "Page.startScreencast",
            json!({ "format": "jpeg", "quality": 10, "maxWidth": 32, "maxHeight": 32 }),
        )
        .await;
        let shot = raw(app, &profile, number, method, params).await;
        let _ = raw(app, &profile, number, "Page.stopScreencast", json!({})).await;
        return shot;
    }
    raw(app, &profile, number, method, params).await
}

/// The person used the tab a moment ago: wait until they have left it alone
/// for [`groups::QUIET_MS`] (spec FR-17), or give up after [`groups::PATIENCE_MS`].
async fn wait_for_person(app: &AppHandle, tab: &str) -> Result<(), String> {
    let started = super::now_ms();
    loop {
        let wait = app
            .state::<Webbridge>()
            .holds
            .lock()
            .expect("webbridge lock")
            .wait_for_person(tab, super::now_ms());
        if wait == 0 {
            return Ok(());
        }
        if super::now_ms().saturating_sub(started) + wait > groups::PATIENCE_MS {
            return Err("The person is using this tab. Wait a moment and try again (browser_wait), or work in another tab.".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(wait.min(1_000))).await;
        check(app, tab, "Input")?;
    }
}

/// Run `script` in the page, as `browser::eval` does for built-in pages: one
/// expression (a returned promise is awaited), or with `statements` a program
/// whose last value is the result.
pub async fn eval(
    app: &AppHandle,
    tab: &str,
    script: &str,
    statements: bool,
) -> Result<Value, String> {
    let body = format!("(() => {{ return ({script}\n); }})()");
    let answer = evaluate(app, tab, &body).await?;
    let answer = match answer {
        Evaluated::Syntax(_) if statements => {
            let source = serde_json::to_string(script).map_err(|e| e.to_string())?;
            match evaluate(app, tab, &format!("(0, eval)({source})")).await? {
                Evaluated::Syntax(message) => {
                    return Err(format!("the script could not run: {message}"))
                }
                other => other,
            }
        }
        Evaluated::Syntax(_) => {
            return Err("the script is not a valid JavaScript expression".into())
        }
        other => other,
    };
    let Evaluated::Value(value) = answer else {
        unreachable!("syntax errors are handled above")
    };
    if let Some(error) = value.get("__error").and_then(Value::as_str) {
        if error.starts_with("EvalError") {
            return Err(format!(
                "{error}\nThis page's security policy blocks running statements; pass one expression instead, e.g. (() => {{ …; return value; }})()"
            ));
        }
        return Err(error.to_string());
    }
    Ok(value)
}

enum Evaluated {
    Value(Value),
    /// The wrapper did not compile: the script is not what was asked for.
    Syntax(String),
}

async fn evaluate(app: &AppHandle, tab: &str, body: &str) -> Result<Evaluated, String> {
    let expression = format!(
        r#"(async () => {{
  const __ser = {serialize};
  try {{
    return __ser(await {body});
  }} catch (e) {{
    return {{ __error: e instanceof Error ? `${{e.name}}: ${{e.message}}` : String(e) }};
  }}
}})()"#,
        serialize = browser::EVAL_SERIALIZE
    );
    let reply = cdp(
        app,
        tab,
        "Runtime.evaluate",
        json!({ "expression": expression, "awaitPromise": true, "returnByValue": true, "userGesture": true }),
    )
    .await?;
    if let Some(details) = reply.get("exceptionDetails") {
        let message = details["exception"]["description"]
            .as_str()
            .or_else(|| details["text"].as_str())
            .unwrap_or("the script failed")
            .to_string();
        if details["exception"]["className"].as_str() == Some("SyntaxError")
            || message.starts_with("SyntaxError")
        {
            return Ok(Evaluated::Syntax(message));
        }
        return Err(message);
    }
    Ok(Evaluated::Value(
        reply["result"].get("value").cloned().unwrap_or(Value::Null),
    ))
}

/// Navigate the tab (FR-22: the browser refuses its own pages and Block sites are refused before).
pub async fn navigate(app: &AppHandle, tab: &str, url: &str) -> Result<String, String> {
    if forbidden_by_browser(url) {
        return Err(FORBIDDEN.to_string());
    }
    let target = browser::normalize_url(url)?;
    if let Some(why) = refusal_to_open(target.as_str()) {
        return Err(why);
    }
    let reply = cdp(app, tab, "Page.navigate", json!({ "url": target.as_str() })).await?;
    if let Some(error) = reply.get("errorText").and_then(Value::as_str) {
        return Err(format!("The page failed to load: {error}"));
    }
    browser::set_external_loading(app, tab, Some(target.as_str()), true);
    Ok(target.to_string())
}

/// back | forward | reload | stop
pub async fn history(app: &AppHandle, tab: &str, action: &str) -> Result<(), String> {
    match action {
        "reload" => {
            cdp(app, tab, "Page.reload", json!({})).await?;
            browser::set_external_loading(app, tab, None, true);
            Ok(())
        }
        "stop" => cdp(app, tab, "Page.stopLoading", json!({}))
            .await
            .map(|_| ()),
        "back" | "forward" => {
            let list = cdp(app, tab, "Page.getNavigationHistory", json!({})).await?;
            let index =
                list["currentIndex"].as_i64().unwrap_or(0) + if action == "back" { -1 } else { 1 };
            let Some(entry) = list["entries"]
                .as_array()
                .and_then(|e| e.get(usize::try_from(index).ok()?))
            else {
                return Err(format!("There is no page to go {action} to."));
            };
            cdp(
                app,
                tab,
                "Page.navigateToHistoryEntry",
                json!({ "entryId": entry["id"] }),
            )
            .await?;
            browser::set_external_loading(app, tab, entry["url"].as_str(), true);
            Ok(())
        }
        other => Err(format!("unknown browser action {other}")),
    }
}

/// Close a Webbridge tab in the browser.
pub fn close(app: &AppHandle, tab: &str) {
    let Some((profile, number)) = split(tab) else {
        return;
    };
    let (app, profile) = (app.clone(), profile.to_string());
    tauri::async_runtime::spawn(async move {
        let _ = app
            .state::<Webbridge>()
            .request(
                &profile,
                json!({ "t": "close", "tab": number }),
                REQUEST_TIMEOUT,
            )
            .await;
    });
}

/// Open `url` in a new background tab of the chat's group in `profile_key`'s browser.
pub async fn open(
    app: &AppHandle,
    chat: &str,
    profile_key: &str,
    url: &str,
) -> Result<String, String> {
    if forbidden_by_browser(url) {
        return Err(FORBIDDEN.to_string());
    }
    let target = browser::normalize_url(url)?;
    if let Some(why) = refusal_to_open(target.as_str()) {
        return Err(why);
    }
    let title = super::chat_title(app, chat);
    // A blank tab first: the page helper, the console capture and the network recording are in place
    // before the page's own scripts run, as in the built-in browser.
    let reply = app
        .state::<Webbridge>()
        .request(profile_key, json!({ "t": "open", "group": { "chat": chat, "title": title }, "url": "about:blank", "background": true }), REQUEST_TIMEOUT)
        .await?;
    let number = reply["tab"]
        .as_i64()
        .ok_or("the browser did not open a tab")?;
    let id = groups::tab_id(profile_key, number);
    browser::register_external(app, &id, chat, "about:blank", "", false);
    browser::select_tab_for_tool(app, chat, &id);
    let _ = crate::agent_tools::browser::network::record_from_start(app, &id).await;
    super::push_working(app, profile_key);
    if target.as_str() != "about:blank" {
        navigate(app, &id, target.as_str()).await?;
    }
    Ok(id)
}

// ── What the extension reports ──────────────────────────────────────────────

/// A DevTools event of an attached tab.
pub fn on_event(app: &AppHandle, profile_key: &str, message: &Value) {
    let Some(number) = message["tab"].as_i64() else {
        return;
    };
    let tab = groups::tab_id(profile_key, number);
    let method = message["method"].as_str().unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "Page.javascriptDialogOpening" => {
            let info = dialog::DialogInfo {
                kind: params["type"].as_str().unwrap_or("alert").to_string(),
                message: params["message"].as_str().unwrap_or("").to_string(),
                default_text: params["defaultPrompt"].as_str().map(str::to_string),
                url: params["url"].as_str().unwrap_or("").to_string(),
            };
            let (app2, tab2) = (app.clone(), tab.clone());
            let complete: Box<dyn FnOnce(dialog::Answer) + Send> =
                Box::new(move |answer: dialog::Answer| {
                    tauri::async_runtime::spawn(async move {
                        let mut params = json!({ "accept": answer.accept });
                        if let Some(text) = answer.text {
                            params["promptText"] = json!(text);
                        }
                        let _ = cdp(&app2, &tab2, "Page.handleJavaScriptDialog", params).await;
                    });
                });
            browser::external_dialog(app, &tab, info, complete);
        }
        "Page.javascriptDialogClosed" => browser::external_dialog_closed(app, &tab),
        "Runtime.bindingCalled" if params["name"].as_str() == Some(BINDING) => {
            let payload: Value = params["payload"]
                .as_str()
                .and_then(|p| serde_json::from_str(p).ok())
                .unwrap_or(Value::Null);
            match payload["action"].as_str() {
                // The person used the page (FR-17): the agent's next act waits.
                Some("input") => {
                    let state = app.state::<Webbridge>();
                    state
                        .holds
                        .lock()
                        .expect("webbridge lock")
                        .person_input(&tab, super::now_ms());
                }
                Some(action) => super::control(app, &tab, action),
                None => {}
            }
        }
        _ => browser::external_event(app, &tab, method, params),
    }
}

/// A tab joined, left, changed or closed.
pub fn on_tab(app: &AppHandle, profile_key: &str, message: &Value) {
    on_tab_change(app, profile_key, message);
    if message["change"].as_str() != Some("updated") {
        super::push_working(app, profile_key);
    }
}

fn on_tab_change(app: &AppHandle, profile_key: &str, message: &Value) {
    let Some(number) = message["tab"].as_i64() else {
        return;
    };
    let tab = groups::tab_id(profile_key, number);
    let url = message["url"].as_str();
    let title = message["title"].as_str();
    match message["change"].as_str().unwrap_or("") {
        "joined" => {
            if let Some(chat) = message["chat"].as_str() {
                app.state::<Webbridge>()
                    .gone
                    .lock()
                    .expect("webbridge lock")
                    .clear(&tab);
                browser::register_external(
                    app,
                    &tab,
                    chat,
                    url.unwrap_or(""),
                    title.unwrap_or(""),
                    message["loading"].as_bool().unwrap_or(false),
                );
            }
        }
        "updated" => {
            browser::update_external(app, &tab, url, title, message["loading"].as_bool());
            // A new document has the indicator off until it is told.
            if message["loading"].as_bool() == Some(false) {
                show_overlay(app, &tab);
            }
        }
        change @ ("left" | "closed") => {
            let reason = if change == "left" {
                groups::TAKEN_BACK
            } else {
                groups::CLOSED
            };
            app.state::<Webbridge>()
                .gone
                .lock()
                .expect("webbridge lock")
                .note(&tab, reason);
            forget(app, &tab);
        }
        _ => {}
    }
}

/// The browser let go of the debugging API on a tab.
pub fn on_detached(app: &AppHandle, profile_key: &str, message: &Value) {
    let Some(number) = message["tab"].as_i64() else {
        return;
    };
    let tab = groups::tab_id(profile_key, number);
    let state = app.state::<Webbridge>();
    state.prepared.lock().expect("webbridge lock").remove(&tab);
    state.holds.lock().expect("webbridge lock").forget(&tab);
}

/// A tab is no longer the chat's (closed, taken back, its browser went away).
pub fn forget(app: &AppHandle, tab: &str) {
    let state = app.state::<Webbridge>();
    state.prepared.lock().expect("webbridge lock").remove(tab);
    state.holds.lock().expect("webbridge lock").forget(tab);
    state.shared.lock().expect("webbridge lock").remove(tab);
    browser::remove_external(app, tab);
}

/// Let go of tabs whose lease ran out (every few seconds).
pub fn release_due(app: &AppHandle, now_ms: u64) {
    let due = app
        .state::<Webbridge>()
        .holds
        .lock()
        .expect("webbridge lock")
        .due(now_ms);
    for tab in due {
        super::log(app, "debug", format!("let go of {tab}: its lease ran out"));
        let_go(app, &tab);
    }
}

/// Turn the tab's indicator off, then detach the debugging API (the browser's notice goes).
/// In that order: a command after the detach would attach the tab again.
pub fn let_go(app: &AppHandle, tab: &str) {
    let off = overlay_expression(app, tab, "off");
    app.state::<Webbridge>()
        .prepared
        .lock()
        .expect("webbridge lock")
        .remove(tab);
    let Some((profile, number)) = split(tab) else {
        return;
    };
    let (app, profile) = (app.clone(), profile.to_string());
    tauri::async_runtime::spawn(async move {
        if let Some(expression) = off {
            let _ = raw(
                &app,
                &profile,
                number,
                "Runtime.evaluate",
                json!({ "expression": expression }),
            )
            .await;
        }
        let _ = app
            .state::<Webbridge>()
            .request(
                &profile,
                json!({ "t": "detach", "tab": number }),
                REQUEST_TIMEOUT,
            )
            .await;
    });
}

#[cfg(test)]
mod tests {
    use super::super::sites::Permission;
    use super::*;

    #[test]
    fn the_browsers_own_pages_and_the_stores_are_forbidden() {
        for url in [
            "chrome://settings",
            "edge://extensions/",
            "EDGE://settings/privacy",
            "chrome-extension://abc/page.html",
            "devtools://devtools/bundled/inspector.html",
            "view-source:https://example.com",
            "about:version",
            "https://chromewebstore.google.com/detail/x",
            "https://chrome.google.com/webstore/category/extensions",
            "https://microsoftedge.microsoft.com/addons/detail/x",
        ] {
            assert!(forbidden_by_browser(url), "{url} is the browser's");
        }
        for url in [
            "about:blank",
            "https://example.com/chrome://settings",
            "https://chrome.google.com/search",
            "https://microsoftedge.microsoft.com/insider",
            "http://localhost:5173/",
            "file:///C:/x.html",
        ] {
            assert!(!forbidden_by_browser(url), "{url} is a page");
        }
    }

    #[test]
    fn forbidden_pages_and_block_sites_are_refused_with_the_reason() {
        assert_eq!(
            refusal("edge://settings", Permission::Ask, false).as_deref(),
            Some(FORBIDDEN)
        );
        assert_eq!(
            refusal("https://bank.example/login", Permission::Block, false).as_deref(),
            Some("The person blocked https://bank.example for Evo.")
        );
        assert_eq!(refusal("https://example.com", Permission::Ask, false), None);
        assert_eq!(
            refusal("https://example.com", Permission::Allow, false),
            None
        );
        // A tab with no address yet, and a call that leaves the page, are not refused.
        assert_eq!(refusal("", Permission::Block, false), None);
        assert_eq!(
            refusal("https://bank.example/", Permission::Block, true),
            None
        );
        assert_eq!(refusal("chrome://settings", Permission::Ask, true), None);
        assert!(leaves_page("Page.navigate") && !leaves_page("Runtime.evaluate"));
    }

    #[test]
    fn the_browsers_refusals_read_as_reasons() {
        assert_eq!(
            super::super::error_text(
                &json!({ "code": "restricted", "message": "Cannot access a chrome:// URL" })
            ),
            FORBIDDEN
        );
        assert_eq!(
            super::super::error_text(&json!({ "code": "gone" })),
            "The tab is gone."
        );
    }
}
