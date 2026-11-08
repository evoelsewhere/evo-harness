//! WebView2 wiring for the agent browser (docs/features/agent-browser, FR-4,
//! FR-13, FR-14, FR-16): the dialog deferral, the download handler and the
//! DevTools event receivers. Everything here runs on the main (UI) thread
//! because WebView2 objects belong to it; the state they feed is in
//! `BrowserState` (browser.rs), the pure logic in the sibling modules.

use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::{AppHandle, Manager};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    take_pwstr, BytesReceivedChangedEventHandler, CallDevToolsProtocolMethodCompletedHandler, DevToolsProtocolEventReceivedEventHandler,
    DownloadStartingEventHandler, NavigationCompletedEventHandler, ScriptDialogOpeningEventHandler, StateChangedEventHandler,
};
use windows_core::{Interface, HSTRING, PWSTR};

use super::dialog::{Answer, DialogInfo};
use super::downloads::State;
use super::{BrowserState, Completer};

/// A WebView2 object carried to a closure that only ever touches it on the
/// main thread (the answer to a dialog is applied there).
struct MainThreadOnly<T>(T);

// SAFETY: the value is only unwrapped inside `run_on_main_thread` closures.
unsafe impl<T> Send for MainThreadOnly<T> {}

impl<T> MainThreadOnly<T> {
    /// A method (not a field access) so closures capture the whole wrapper.
    fn into_inner(self) -> T {
        self.0
    }
}

/// Hook a new tab: hold page dialogs for the app and route downloads. Errors
/// are logged; the tab still works with WebView2's own behaviour.
pub fn install(app: &AppHandle, tab: &str, webview: &tauri::Webview) {
    let (app, tab) = (app.clone(), tab.to_string());
    let scheduled = webview.with_webview(move |platform| {
        let result = (|| -> Result<(), String> {
            let core = unsafe { platform.controller().CoreWebView2() }.map_err(|e| e.to_string())?;
            // With the default dialogs on, WebView2 never raises the event.
            let settings = unsafe { core.Settings() }.map_err(|e| e.to_string())?;
            unsafe { settings.SetAreDefaultScriptDialogsEnabled(false) }.map_err(|e| e.to_string())?;
            unsafe { hold_dialogs(&app, &tab, &core) }?;
            unsafe { dialog_world(&core) }?;
            unsafe { route_downloads(&app, &tab, &core) }?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("browser hooks for {tab} failed: {error}");
        }
    });
    if let Err(error) = scheduled {
        eprintln!("browser hooks could not be installed: {error}");
    }
}

/// The app's dialog plugin replaces `window.alert` and `window.confirm` in the
/// page's own world, so the page script (browser_page.js) cannot reach the real
/// ones. An isolated world shares the DOM but not the globals: its functions are
/// the real ones, and a DOM event from the page's world calls them synchronously,
/// which raises ScriptDialogOpening for `hold_dialogs`.
const DIALOG_WORLD: &str = "evo-dialogs";
const DIALOG_WORLD_SOURCE: &str = r#"if (!window.__evoDialogWorld) {
  window.__evoDialogWorld = true;
  document.addEventListener("evo-dialog", (event) => {
    const slot = event.relatedTarget;
    if (!slot || typeof slot.getAttribute !== "function") return;
    const message = slot.getAttribute("data-message") || "";
    try {
      const result = slot.getAttribute("data-kind") === "confirm" ? window.confirm(message) : (window.alert(message), true);
      slot.setAttribute("data-result", String(result));
    } catch (_) {}
  });
}"#;

/// A DevTools call whose answer `then` receives (nothing happens on failure: the
/// page then keeps the app's own dialogs).
unsafe fn devtools(core: &ICoreWebView2, method: &str, params: Value, then: impl FnOnce(Value) + 'static) -> Result<(), String> {
    let name = method.to_string();
    let mut then = Some(then);
    let done = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
        match (result, then.take()) {
            (Ok(()), Some(then)) => then(serde_json::from_str(&json).unwrap_or(Value::Null)),
            (Err(error), _) => eprintln!("browser dialog world: {name} failed: {error}"),
            _ => {}
        }
        Ok(())
    }));
    unsafe { core.CallDevToolsProtocolMethod(&HSTRING::from(method), &HSTRING::from(params.to_string()), &done) }.map_err(|e| e.to_string())
}

/// Give every document of the tab the isolated world. A document gets it from a
/// new-document script (WebView2 honours those once the Page domain is on), but
/// not the tab's first one, which exists before the script does; after each
/// navigation the world is therefore also created in the current document (the
/// source guards against running twice).
unsafe fn dialog_world(core: &ICoreWebView2) -> Result<(), String> {
    unsafe { devtools(core, "Page.enable", serde_json::json!({}), |_| {}) }?;
    unsafe {
        devtools(
            core,
            "Page.addScriptToEvaluateOnNewDocument",
            serde_json::json!({ "source": DIALOG_WORLD_SOURCE, "worldName": DIALOG_WORLD }),
            |_| {},
        )
    }?;
    let core_again = core.clone();
    let handler = NavigationCompletedEventHandler::create(Box::new(move |_, _| {
        let core = core_again.clone();
        let _ = unsafe {
            devtools(&core_again, "Page.getFrameTree", serde_json::json!({}), move |tree| {
                let Some(frame) = tree.pointer("/frameTree/frame/id").and_then(Value::as_str) else {
                    return;
                };
                let core_next = core.clone();
                let _ = devtools(&core, "Page.createIsolatedWorld", serde_json::json!({ "frameId": frame, "worldName": DIALOG_WORLD }), move |world| {
                    let Some(context) = world.get("executionContextId").and_then(Value::as_i64) else {
                        return;
                    };
                    let _ = devtools(
                        &core_next,
                        "Runtime.evaluate",
                        serde_json::json!({ "contextId": context, "expression": DIALOG_WORLD_SOURCE }),
                        |_| {},
                    );
                });
            })
        };
        Ok(())
    }));
    let mut token = 0i64;
    unsafe { core.add_NavigationCompleted(&handler, &mut token) }.map_err(|e| e.to_string())
}

unsafe fn hold_dialogs(app: &AppHandle, tab: &str, core: &ICoreWebView2) -> Result<(), String> {
    let (app, tab) = (app.clone(), tab.to_string());
    let handler = ScriptDialogOpeningEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut kind = COREWEBVIEW2_SCRIPT_DIALOG_KIND::default();
        let (mut message, mut url, mut default_text) = (PWSTR::null(), PWSTR::null(), PWSTR::null());
        unsafe {
            args.Kind(&mut kind)?;
            args.Message(&mut message)?;
            args.Uri(&mut url)?;
            args.DefaultText(&mut default_text)?;
        }
        let (message, url, default_text) = (take_pwstr(message), take_pwstr(url), take_pwstr(default_text));
        let kind = if kind == COREWEBVIEW2_SCRIPT_DIALOG_KIND_CONFIRM {
            "confirm"
        } else if kind == COREWEBVIEW2_SCRIPT_DIALOG_KIND_PROMPT {
            "prompt"
        } else if kind == COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD {
            "beforeunload"
        } else {
            "alert"
        };
        // The page waits until the deferral completes.
        let deferral = unsafe { args.GetDeferral() }?;
        let held = MainThreadOnly((args, deferral));
        let apply_app = app.clone();
        let complete: Completer = Box::new(move |answer: Answer| {
            let _ = apply_app.run_on_main_thread(move || {
                let (args, deferral) = held.into_inner();
                unsafe {
                    if answer.accept {
                        if let Some(text) = &answer.text {
                            let _ = args.SetResultText(&HSTRING::from(text.as_str()));
                        }
                        let _ = args.Accept();
                    }
                    // Not accepted: completing the deferral dismisses the dialog.
                    let _ = deferral.Complete();
                }
            });
        });
        let info = DialogInfo {
            kind: kind.to_string(),
            message,
            default_text: (kind == "prompt").then_some(default_text),
            url,
        };
        app.state::<BrowserState>().dialog_opened(&app, &tab, info, complete);
        Ok(())
    }));
    let mut token = 0i64;
    unsafe { core.add_ScriptDialogOpening(&handler, &mut token) }.map_err(|e| e.to_string())
}

unsafe fn route_downloads(app: &AppHandle, tab: &str, core: &ICoreWebView2) -> Result<(), String> {
    let core4: ICoreWebView2_4 = core.cast().map_err(|e| e.to_string())?;
    let (app, tab) = (app.clone(), tab.to_string());
    let handler = DownloadStartingEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let operation = unsafe { args.DownloadOperation() }?;
        let (mut url, mut proposed) = (PWSTR::null(), PWSTR::null());
        unsafe {
            operation.Uri(&mut url)?;
            args.ResultFilePath(&mut proposed)?;
        }
        let (url, proposed) = (take_pwstr(url), take_pwstr(proposed));
        let state = app.state::<BrowserState>();
        let page = crate::browser::native_url(&app, &tab).unwrap_or_default();
        let record = state.begin_download(&tab, &url, &proposed, &page);
        // The record exists in either case: a blocked one tells the agent why.
        state.emit_owner(&app, &tab, "browser://download", record.clone());
        if record.state == State::Blocked {
            unsafe {
                args.SetCancel(true)?;
                args.SetHandled(true)?;
            }
            return Ok(());
        }
        unsafe {
            args.SetResultFilePath(&HSTRING::from(record.path.as_str()))?;
            args.SetHandled(true)?;
            watch_download(&app, &operation, record.id)?;
        }
        Ok(())
    }));
    let mut token = 0i64;
    unsafe { core4.add_DownloadStarting(&handler, &mut token) }.map_err(|e| e.to_string())
}

/// Follow one download: bytes (at most twice a second) and its final state.
unsafe fn watch_download(app: &AppHandle, operation: &ICoreWebView2DownloadOperation, id: u64) -> windows_core::Result<()> {
    let progress_app = app.clone();
    let mut last_emit: Option<Instant> = None;
    let progress = BytesReceivedChangedEventHandler::create(Box::new(move |operation, _| {
        let Some(operation) = operation else {
            return Ok(());
        };
        if last_emit.is_some_and(|at| at.elapsed() < Duration::from_millis(500)) {
            return Ok(());
        }
        last_emit = Some(Instant::now());
        let mut bytes = 0i64;
        unsafe { operation.BytesReceived(&mut bytes) }?;
        progress_app.state::<BrowserState>().download_changed(&progress_app, id, Some(bytes.max(0) as u64), None);
        Ok(())
    }));
    let mut token = 0i64;
    unsafe { operation.add_BytesReceivedChanged(&progress, &mut token) }?;

    let done_app = app.clone();
    let changed = StateChangedEventHandler::create(Box::new(move |operation, _| {
        let Some(operation) = operation else {
            return Ok(());
        };
        let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
        let mut bytes = 0i64;
        unsafe {
            operation.State(&mut state)?;
            operation.BytesReceived(&mut bytes)?;
        }
        let state = if state == COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS {
            None
        } else if state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED {
            Some(State::Completed)
        } else {
            Some(State::Failed)
        };
        done_app.state::<BrowserState>().download_changed(&done_app, id, Some(bytes.max(0) as u64), state);
        Ok(())
    }));
    let mut token = 0i64;
    unsafe { operation.add_StateChanged(&changed, &mut token) }
}

/// Register DevTools event receivers for `events` on the tab's webview.
pub async fn subscribe(app: &AppHandle, webview: &tauri::Webview, tab: &str, events: Vec<String>) -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    let (app, tab) = (app.clone(), tab.to_string());
    webview
        .with_webview(move |platform| {
            let result = (|| -> Result<(), String> {
                let core = unsafe { platform.controller().CoreWebView2() }.map_err(|e| e.to_string())?;
                let state = app.state::<BrowserState>();
                for event in events {
                    if !state.mark_subscribed(&tab, &event) {
                        continue;
                    }
                    if let Err(error) = unsafe { receive(&app, &tab, &core, &event) } {
                        state.unmark_subscribed(&tab, &event);
                        return Err(format!("could not subscribe to {event}: {error}"));
                    }
                }
                Ok(())
            })();
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;
    tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .map_err(|_| "the browser did not answer within 10s".to_string())?
        .map_err(|_| "the browser call was dropped".to_string())?
}

unsafe fn receive(app: &AppHandle, tab: &str, core: &ICoreWebView2, event: &str) -> Result<(), String> {
    let receiver = unsafe { core.GetDevToolsProtocolEventReceiver(&HSTRING::from(event)) }.map_err(|e| e.to_string())?;
    let (app, tab, method) = (app.clone(), tab.to_string(), event.to_string());
    let handler = DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut json = PWSTR::null();
        unsafe { args.ParameterObjectAsJson(&mut json) }?;
        let params: Value = serde_json::from_str(&take_pwstr(json)).unwrap_or(Value::Null);
        app.state::<BrowserState>().cdp_event(&app, &tab, &method, params);
        Ok(())
    }));
    let mut token = 0i64;
    unsafe { receiver.add_DevToolsProtocolEventReceived(&handler, &mut token) }.map_err(|e| e.to_string())
}
