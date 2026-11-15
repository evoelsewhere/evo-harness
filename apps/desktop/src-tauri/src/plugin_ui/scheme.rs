//! The `evo-plugin://<key>/<path>` scheme (docs/features/plugin-ui/spec.md,
//! FR-9; ADR 0021). On Windows a view's URL is `http://evo-plugin.<key>/…`;
//! either way each key is its own origin.
//!
//! - A plugin view loads only its own plugin's files: paths are decoded,
//!   cleaned and canonicalised, and must stay inside the plugin folder
//!   (a symlink out of it is refused).
//! - HTML gets a CSP: no remote scripts, no frames, and fetches only to the
//!   view's own origin (the bridge) and its `network:` origins.
//! - `POST /__bridge` is the bridge (bridge.rs).

use std::path::{Component, Path, PathBuf};

use serde_json::json;
use tauri::http::{Method, Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder};

use super::{bridge, PluginUiState, SCHEME, VIEW_PREFIX};

/// The bridge's path on every plugin origin.
pub const BRIDGE_PATH: &str = "/__bridge";
/// The largest bridge call body (storage values included).
const MAX_CALL: usize = 2 * 1024 * 1024;

/// Answer one request to the scheme, off the webview's thread.
pub fn serve(ctx: UriSchemeContext<'_, tauri::Wry>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let label = ctx.webview_label().to_string();
    tauri::async_runtime::spawn(async move {
        let response = answer(&app, &label, request).await;
        responder.respond(response);
    });
}

/// The host key of a request: `evo-plugin://<key>/…`, or on Windows
/// `http://evo-plugin.<key>/…`.
pub fn key_of(host: &str) -> &str {
    host.strip_prefix(SCHEME).and_then(|rest| rest.strip_prefix('.')).unwrap_or(host)
}

async fn answer(app: &tauri::AppHandle, label: &str, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let key = key_of(request.uri().host().unwrap_or_default()).to_string();
    let path = request.uri().path().to_string();
    let state = app.state::<PluginUiState>();
    let Some(entry) = state.plugin(&key) else {
        return status(StatusCode::NOT_FOUND, "no such plugin, or it is off");
    };
    let from_view = label.starts_with(VIEW_PREFIX);
    if path == BRIDGE_PATH {
        if request.method() != Method::POST {
            return status(StatusCode::METHOD_NOT_ALLOWED, "the bridge takes POST");
        }
        if request.body().len() > MAX_CALL {
            return json_response(&bridge::refusal("too_large", "the call is larger than 2 MB"));
        }
        let reply = if from_view {
            bridge::handle(app.clone(), label.to_string(), key, request.body().clone()).await
        } else {
            bridge::refusal("not_a_view", "only plugin views call the bridge")
        };
        return json_response(&reply);
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return status(StatusCode::METHOD_NOT_ALLOWED, "only GET");
    }
    // A view loads its own plugin's files only; the app's windows nothing
    // (they get icons as data URLs from `plugins_state`).
    let relative = match decode(&path) {
        Some(relative) => relative,
        None => return status(StatusCode::BAD_REQUEST, "bad path"),
    };
    let own = state.views.lock().expect("views lock").get(label).map(|view| view.key.clone());
    if !from_view || own.as_deref() != Some(key.as_str()) {
        return status(StatusCode::FORBIDDEN, "only this plugin's views load its files");
    }
    let root = entry.root.clone();
    let network = entry.network();
    let read = tauri::async_runtime::spawn_blocking(move || {
        let file = resolve(&root, &relative)?;
        std::fs::read(&file).map(|bytes| (file, bytes)).map_err(|_| StatusCode::NOT_FOUND)
    })
    .await;
    match read {
        Ok(Ok((file, bytes))) => {
            let mime = mime(&file);
            let mut response = Response::builder()
                .header("Content-Type", mime)
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "no-cache");
            if mime.starts_with("text/html") {
                response = response.header("Content-Security-Policy", csp(&network));
            }
            let body = if request.method() == Method::HEAD { Vec::new() } else { bytes };
            response.body(body).unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR, "response"))
        }
        Ok(Err(code)) => status(code, "not found"),
        Err(_) => status(StatusCode::INTERNAL_SERVER_ERROR, "read failed"),
    }
}

fn status(code: StatusCode, text: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(text.as_bytes().to_vec())
        .expect("status response")
}

fn json_response(value: &serde_json::Value) -> Response<Vec<u8>> {
    Response::builder()
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-store")
        .body(value.to_string().into_bytes())
        .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR, &json!({"ok": false}).to_string()))
}

/// A URL path as a relative path with `/`: percent-decoded, `index.html`
/// for a folder. `None` when it is not valid UTF-8 or names a parent,
/// a drive, a backslash or a NUL.
pub fn decode(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let text = String::from_utf8(out).ok()?;
    if text.contains('\\') || text.contains('\0') || text.contains(':') {
        return None;
    }
    let parts: Vec<&str> = text.split('/').filter(|part| !part.is_empty() && *part != ".").collect();
    if parts.contains(&"..") {
        return None;
    }
    let mut relative = parts.join("/");
    if relative.is_empty() || text.ends_with('/') {
        relative = if relative.is_empty() { "index.html".into() } else { format!("{relative}/index.html") };
    }
    Some(relative)
}

/// The file `relative` names inside `root` (canonical), or why not: it must
/// exist, be a file, and stay inside `root` once links are followed.
pub fn resolve(root: &Path, relative: &str) -> Result<PathBuf, StatusCode> {
    if Path::new(relative).components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err(StatusCode::FORBIDDEN);
    }
    let candidate = root.join(relative);
    let file = std::fs::canonicalize(&candidate).map_err(|_| StatusCode::NOT_FOUND)?;
    if !file.starts_with(root) {
        return Err(StatusCode::FORBIDDEN);
    }
    if !file.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(file)
}

/// The content type of a plugin file, by extension.
pub fn mime(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "txt" | "md" => "text/plain; charset=utf-8",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// The CSP of a plugin page: its own files, inline styles, images from
/// itself or data/blob URLs; fetches to itself (the bridge) and to the
/// `network:` origins it was allowed; no frames, no remote scripts.
pub fn csp(network: &[String]) -> String {
    let connect = std::iter::once("'self'".to_string()).chain(network.iter().cloned()).collect::<Vec<_>>().join(" ");
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; \
         font-src 'self' data:; media-src 'self' data: blob:; connect-src {connect}; frame-src 'none'; \
         object-src 'none'; base-uri 'none'; form-action 'none'"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_from_both_url_forms() {
        assert_eq!(key_of("sticky-wall-1a2b3c4d"), "sticky-wall-1a2b3c4d");
        assert_eq!(key_of("evo-plugin.sticky-wall-1a2b3c4d"), "sticky-wall-1a2b3c4d");
    }

    #[test]
    fn paths_are_decoded_and_parents_refused() {
        assert_eq!(decode("/ui/wall.html").as_deref(), Some("ui/wall.html"));
        assert_eq!(decode("/ui/my%20notes.html").as_deref(), Some("ui/my notes.html"));
        assert_eq!(decode("/").as_deref(), Some("index.html"));
        assert_eq!(decode("/ui/").as_deref(), Some("ui/index.html"));
        assert_eq!(decode("/./ui//a.js").as_deref(), Some("ui/a.js"));
        for bad in ["/../secret", "/ui/%2e%2e/%2e%2e/x", "/ui/..%2fx", "/%5c..%5cx", "/C:/Windows/win.ini", "/a%00.html", "/%zz", "/%c3"] {
            assert_eq!(decode(bad), None, "{bad}");
        }
    }

    #[test]
    fn files_resolve_inside_the_plugin_only() {
        let base = std::env::temp_dir().join(format!("evo-plugin-scheme-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("plugin");
        std::fs::create_dir_all(root.join("ui")).unwrap();
        std::fs::write(root.join("ui/wall.html"), "x").unwrap();
        std::fs::write(base.join("secret.txt"), "s").unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        assert!(resolve(&root, "ui/wall.html").is_ok());
        assert_eq!(resolve(&root, "ui/missing.html"), Err(StatusCode::NOT_FOUND));
        assert_eq!(resolve(&root, "ui"), Err(StatusCode::NOT_FOUND), "a folder is not a file");
        assert!(resolve(&root, "../secret.txt").is_err());
        // A link out of the plugin is refused (where the OS lets a test make one).
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.join("secret.txt"), root.join("ui/link.txt")).unwrap();
            assert_eq!(resolve(&root, "ui/link.txt"), Err(StatusCode::FORBIDDEN));
        }
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(base.join("secret.txt"), root.join("ui/link.txt")).is_ok() {
            assert_eq!(resolve(&root, "ui/link.txt"), Err(StatusCode::FORBIDDEN));
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn types_and_policy() {
        assert_eq!(mime(Path::new("a/B.HTML")), "text/html; charset=utf-8");
        assert_eq!(mime(Path::new("x.mjs")), "text/javascript; charset=utf-8");
        assert_eq!(mime(Path::new("x.unknown")), "application/octet-stream");
        let policy = csp(&["https://api.example.com".into()]);
        assert!(policy.contains("connect-src 'self' https://api.example.com;"), "{policy}");
        assert!(policy.contains("script-src 'self';") && policy.contains("frame-src 'none'"), "{policy}");
        assert!(csp(&[]).contains("connect-src 'self';"));
    }
}
