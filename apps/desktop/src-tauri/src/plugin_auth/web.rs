//! A web login (docs/features/plugin-settings/spec.md, FR-24; spike S3): an
//! incognito window (its own storage: no cookies of the app, the person's
//! browser or other plugins) whose title shows the page's real address, with
//! no access to evo (`plugin-login-*` windows are outside the invoke gate's
//! allowed labels). Single sign-on redirects are allowed. When the declared
//! address or cookie appears, the cookies of the declared domains are read
//! (HttpOnly too) and the window closes.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use evo_plugins::accounts::Web;
use serde_json::{json, Value};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Url, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// Cookies of `domains` in `window`'s store: `{name, value, domain, path}`.
fn read_cookies(window: &tauri::WebviewWindow, domains: &[String]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for domain in domains {
        for scheme in ["https", "http"] {
            let Ok(url) = Url::parse(&format!("{scheme}://{domain}/")) else { continue };
            for cookie in window.cookies_for_url(url).unwrap_or_default() {
                let row = json!({
                    "name": cookie.name(),
                    "value": cookie.value(),
                    "domain": cookie.domain().unwrap_or(domain),
                    "path": cookie.path().unwrap_or("/"),
                });
                if !out.iter().any(|r| r["name"] == row["name"] && r["domain"] == row["domain"] && r["path"] == row["path"]) {
                    out.push(row);
                }
            }
        }
    }
    out
}

/// The title: what the window is for, and where it is now.
fn title(service: &str, url: &Url) -> String {
    let shown = format!("{}://{}{}", url.scheme(), url.host_str().unwrap_or_default(), url.path());
    format!("Log in to {service} — {shown}")
}

/// Whether the page at `url` means the login is done by address.
pub fn done_by_url(web: &Web, url: &str) -> bool {
    web.done_url.as_deref().is_some_and(|prefix| url.starts_with(prefix))
}

type Done = Arc<Mutex<Option<oneshot::Sender<Result<Vec<Value>, String>>>>>;

fn finish(done: &Done, result: Result<Vec<Value>, String>) {
    if let Some(tx) = done.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = tx.send(result);
    }
}

/// Open the login window and wait: the declared domains' cookies, or why not.
pub async fn login(app: &AppHandle, service: &str, web: &Web, cancel: &CancellationToken) -> Result<Vec<Value>, String> {
    let label = format!("plugin-login-{}", NEXT.fetch_add(1, Ordering::Relaxed) + 1);
    let start: Url = web.login_url.parse().map_err(|e| format!("bad login address: {e}"))?;
    let (tx, rx) = oneshot::channel();
    let done: Done = Arc::new(Mutex::new(Some(tx)));
    let (page_done, closed_done) = (done.clone(), done.clone());
    let (page_web, page_service) = (web.clone(), service.to_string());
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(start.clone()))
        .title(title(service, &start))
        .inner_size(520.0, 700.0)
        .incognito(true)
        .on_page_load(move |window, payload| {
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            let url = payload.url().clone();
            let _ = window.set_title(&title(&page_service, &url));
            let by_url = done_by_url(&page_web, url.as_str());
            let (web, done) = (page_web.clone(), page_done.clone());
            // Off the main thread: reading cookies there deadlocks on Windows.
            tauri::async_runtime::spawn(async move {
                let domains = web.domains.clone();
                let read = tauri::async_runtime::spawn_blocking(move || read_cookies(&window, &domains)).await;
                let Ok(cookies) = read else { return };
                let by_cookie = web.done_cookie.as_deref().is_some_and(|name| cookies.iter().any(|c| c["name"] == name));
                if (by_url || by_cookie) && !cookies.is_empty() {
                    finish(&done, Ok(cookies));
                }
            });
        })
        .build()
        .map_err(|e| format!("could not open the login window: {e}"))?;
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            finish(&closed_done, Err("cancelled".into()));
        }
    });
    let result = tokio::select! {
        result = rx => result.unwrap_or_else(|_| Err("cancelled".into())),
        _ = cancel.cancelled() => Err("cancelled".into()),
    };
    let _ = window.close();
    result
}
