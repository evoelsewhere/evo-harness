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

/// Where a Webbridge tab's agent work is refused before it reaches the browser.
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

fn check(app: &AppHandle, tab: &str, method: &str) -> Result<(String, i64), String> {
    let (profile, number) =
        split(tab).ok_or_else(|| format!("No browser tab {tab}; see browser_tabs."))?;
    let state = app.state::<Webbridge>();
    // A Block site: nothing but leaving it (spec FR-22).
    if let Some(url) = crate::browser::tab_record(app, tab)
        .map(|t| t.url)
        .filter(|u| !u.is_empty())
    {
        if super::sites::permission(&url) == super::sites::Permission::Block && !leaves_page(method)
        {
            return Err(format!(
                "The person blocked {} for Evo.",
                super::sites::site_of(&url)
            ));
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
        Ok::<(), String>(())
    }
    .await;
    match result {
        Ok(()) => {
            state.holds.lock().expect("webbridge lock").touch(tab);
            Ok(())
        }
        Err(error) => {
            state.prepared.lock().expect("webbridge lock").remove(tab);
            Err(error)
        }
    }
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
    let target = browser::normalize_url(url)?;
    if super::sites::permission(target.as_str()) == super::sites::Permission::Block {
        return Err(format!(
            "The person blocked {} for Evo.",
            super::sites::site_of(target.as_str())
        ));
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
    let target = browser::normalize_url(url)?;
    if super::sites::permission(target.as_str()) == super::sites::Permission::Block {
        return Err(format!(
            "The person blocked {} for Evo.",
            super::sites::site_of(target.as_str())
        ));
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
        _ => browser::external_event(app, &tab, method, params),
    }
}

/// A tab joined, left, changed or closed.
pub fn on_tab(app: &AppHandle, profile_key: &str, message: &Value) {
    let Some(number) = message["tab"].as_i64() else {
        return;
    };
    let tab = groups::tab_id(profile_key, number);
    let url = message["url"].as_str();
    let title = message["title"].as_str();
    match message["change"].as_str().unwrap_or("") {
        "joined" => {
            if let Some(chat) = message["chat"].as_str() {
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
        "updated" => browser::update_external(app, &tab, url, title, message["loading"].as_bool()),
        "left" | "closed" => forget(app, &tab),
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
        app.state::<Webbridge>()
            .prepared
            .lock()
            .expect("webbridge lock")
            .remove(&tab);
        if let Some((profile, number)) = split(&tab) {
            let (app, profile) = (app.clone(), profile.to_string());
            tauri::async_runtime::spawn(async move {
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
    }
}
