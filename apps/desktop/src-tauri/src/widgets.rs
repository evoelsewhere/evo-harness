//! `show_widget` documents, served from the `evo-widget` scheme
//! (docs/features/office-v3-parity/spec.md FR-13, NFR-2, NFR-3; ADR 0027).
//!
//! A widget is model-written HTML with script. A `data:` frame inherits the
//! app's CSP, under which none of its scripts run, so the window hands the
//! wrapped document to the shell ([`widget_open`]) and its sandboxed frame
//! loads `evo-widget://localhost/<id>` (`http://evo-widget.localhost/<id>` on
//! Windows), which answers with the widget CSP as the header.
//!
//! - A document is served only to the window that opened it; plugin views
//!   get nothing.
//! - Documents live in memory, at most [`MAX_DOCS`] and [`MAX_BYTES`] in all,
//!   the oldest dropped first; [`widget_close`] drops one when its frame goes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::http::{Method, Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder};

/// The scheme widget documents are served from.
pub const SCHEME: &str = "evo-widget";
/// Documents kept at once, over every window.
pub const MAX_DOCS: usize = 64;
/// Their size in all.
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
/// One document's size, at most.
pub const MAX_DOC: usize = 16 * 1024 * 1024;

/// The CDNs a widget may load scripts from (v3; `widget-document.ts` keeps
/// the same list for its `<meta>` policy).
pub const ALLOWED_CDNS: [&str; 4] = [
    "cdnjs.cloudflare.com",
    "cdn.jsdelivr.net",
    "unpkg.com",
    "esm.sh",
];

/// The widget policy (v3), sent as the document's header.
pub fn csp() -> String {
    let scripts = ["'self'", "blob:", "data:"]
        .into_iter()
        .chain(ALLOWED_CDNS)
        .chain(["'unsafe-inline'", "'unsafe-eval'"])
        .collect::<Vec<_>>()
        .join(" ");
    [
        "default-src 'none'".to_string(),
        format!("script-src {scripts}"),
        "style-src 'self' blob: data: 'unsafe-inline'".into(),
        "img-src 'self' blob: data:".into(),
        "font-src 'self' blob: data:".into(),
        "connect-src 'self'".into(),
        "frame-src 'self'".into(),
        "object-src 'none'".into(),
        "base-uri 'self'".into(),
        "form-action 'none'".into(),
    ]
    .join("; ")
}

/// A random id in the shape of the app's (`crypto.randomUUID()`).
fn new_id(seq: u64) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        hasher.write_u64(seq);
        hasher.write_usize(i);
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct Doc {
    label: String,
    html: Arc<str>,
    /// Order of opening, for dropping the oldest.
    seq: u64,
}

/// The documents the windows opened.
#[derive(Default)]
pub struct WidgetDocs {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    docs: HashMap<String, Doc>,
    bytes: usize,
    seq: u64,
}

impl Inner {
    fn remove(&mut self, id: &str) {
        if let Some(doc) = self.docs.remove(id) {
            self.bytes -= doc.html.len();
        }
    }
}

impl WidgetDocs {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Keep `html` for window `label`; its id. Too large: why not.
    pub fn open(&self, label: &str, html: String) -> Result<String, String> {
        if html.len() > MAX_DOC {
            return Err(format!(
                "the widget is larger than {} MB",
                MAX_DOC / (1024 * 1024)
            ));
        }
        let mut inner = self.lock();
        // Room for it: the oldest documents go first.
        while !inner.docs.is_empty()
            && (inner.docs.len() >= MAX_DOCS || inner.bytes + html.len() > MAX_BYTES)
        {
            let oldest = inner
                .docs
                .iter()
                .min_by_key(|(_, doc)| doc.seq)
                .map(|(id, _)| id.clone());
            match oldest {
                Some(id) => inner.remove(&id),
                None => break,
            }
        }
        inner.seq += 1;
        let seq = inner.seq;
        let id = new_id(seq);
        inner.bytes += html.len();
        inner.docs.insert(
            id.clone(),
            Doc {
                label: label.to_string(),
                html: Arc::from(html),
                seq,
            },
        );
        Ok(id)
    }

    /// The document `id`, when window `label` opened it.
    pub fn get(&self, label: &str, id: &str) -> Option<Arc<str>> {
        self.lock()
            .docs
            .get(id)
            .filter(|doc| doc.label == label)
            .map(|doc| doc.html.clone())
    }

    /// Drop document `id` of window `label`.
    pub fn close(&self, label: &str, id: &str) {
        let mut inner = self.lock();
        if inner.docs.get(id).is_some_and(|doc| doc.label == label) {
            inner.remove(id);
        }
    }

    /// Drop every document of window `label` (it closed).
    pub fn window_gone(&self, label: &str) {
        let mut inner = self.lock();
        let ids: Vec<String> = inner
            .docs
            .iter()
            .filter(|(_, doc)| doc.label == label)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            inner.remove(&id);
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().expect("widgets lock").docs.len()
    }
}

/// The answer to `method path` from webview `label`.
pub fn respond(docs: &WidgetDocs, label: &str, method: &Method, path: &str) -> Response<Vec<u8>> {
    // Plugin views have their own scheme and reach nothing of the app's.
    if label.starts_with(crate::plugin_ui::VIEW_PREFIX) {
        return status(StatusCode::FORBIDDEN, "plugin views load no widgets");
    }
    if method != Method::GET && method != Method::HEAD {
        return status(StatusCode::METHOD_NOT_ALLOWED, "only GET");
    }
    let id = path.trim_start_matches('/');
    let Some(html) = docs.get(label, id) else {
        return status(StatusCode::NOT_FOUND, "no such widget in this window");
    };
    let body = if method == Method::HEAD {
        Vec::new()
    } else {
        html.as_bytes().to_vec()
    };
    Response::builder()
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Content-Security-Policy", csp())
        .header("X-Content-Type-Options", "nosniff")
        .header("Referrer-Policy", "no-referrer")
        .header("Cache-Control", "no-store")
        .body(body)
        .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR, "response"))
}

fn status(code: StatusCode, text: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(text.as_bytes().to_vec())
        .expect("status response")
}

/// Answer one request to the scheme.
pub fn serve(
    ctx: UriSchemeContext<'_, tauri::Wry>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let label = ctx.webview_label().to_string();
    let app = ctx.app_handle().clone();
    let response = respond(
        &app.state::<WidgetDocs>(),
        &label,
        request.method(),
        request.uri().path(),
    );
    responder.respond(response);
}

/// Keep a widget document for the calling window; the frame loads it by id.
#[tauri::command]
pub fn widget_open(
    webview: tauri::Webview,
    docs: tauri::State<'_, WidgetDocs>,
    html: String,
) -> Result<String, String> {
    docs.open(webview.label(), html)
}

/// The frame showing widget `id` is gone.
#[tauri::command]
pub fn widget_close(webview: tauri::Webview, docs: tauri::State<'_, WidgetDocs>, id: String) {
    docs.close(webview.label(), &id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header<'a>(response: &'a Response<Vec<u8>>, name: &str) -> Option<&'a str> {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
    }

    #[test]
    fn a_document_is_served_with_the_widget_policy_to_its_window_only() {
        let docs = WidgetDocs::default();
        let id = docs.open("main", "<p>hi</p>".into()).unwrap();
        let ok = respond(&docs, "main", &Method::GET, &format!("/{id}"));
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.body(), b"<p>hi</p>");
        assert_eq!(
            header(&ok, "Content-Type"),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(header(&ok, "Content-Security-Policy"), Some(csp().as_str()));
        assert_eq!(header(&ok, "Cache-Control"), Some("no-store"));
        // Another window, a plugin view, an unknown id, a wrong method.
        assert_eq!(
            respond(&docs, "chat-x", &Method::GET, &format!("/{id}")).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            respond(&docs, "plugin-1", &Method::GET, &format!("/{id}")).status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            respond(&docs, "main", &Method::GET, "/nope").status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            respond(&docs, "main", &Method::POST, &format!("/{id}")).status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        // HEAD answers with the headers and no body.
        let head = respond(&docs, "main", &Method::HEAD, &format!("/{id}"));
        assert_eq!(head.status(), StatusCode::OK);
        assert!(head.body().is_empty());
    }

    #[test]
    fn the_policy_lets_scripts_from_the_allowlist_run_and_nothing_else_reach_out() {
        let policy = csp();
        assert!(policy.starts_with("default-src 'none';"), "{policy}");
        assert!(
            policy.contains("script-src 'self' blob: data: cdnjs.cloudflare.com cdn.jsdelivr.net unpkg.com esm.sh 'unsafe-inline' 'unsafe-eval';"),
            "{policy}"
        );
        assert!(policy.contains("connect-src 'self';"), "{policy}");
        assert!(policy.contains("object-src 'none'"), "{policy}");
    }

    #[test]
    fn closing_and_a_closed_window_drop_documents() {
        let docs = WidgetDocs::default();
        let a = docs.open("main", "a".into()).unwrap();
        let b = docs.open("main", "b".into()).unwrap();
        let c = docs.open("chat-1", "c".into()).unwrap();
        assert_ne!(a, b);
        // Another window cannot close it.
        docs.close("chat-1", &a);
        assert!(docs.get("main", &a).is_some());
        docs.close("main", &a);
        assert!(docs.get("main", &a).is_none());
        docs.window_gone("main");
        assert!(docs.get("main", &b).is_none());
        assert_eq!(docs.get("chat-1", &c).as_deref(), Some("c"));
    }

    #[test]
    fn documents_are_capped_and_the_oldest_go_first() {
        let docs = WidgetDocs::default();
        let first = docs.open("main", "first".into()).unwrap();
        let ids: Vec<String> = (0..MAX_DOCS)
            .map(|i| docs.open("main", format!("doc {i}")).unwrap())
            .collect();
        assert_eq!(docs.len(), MAX_DOCS);
        assert!(docs.get("main", &first).is_none(), "the oldest went");
        assert!(docs.get("main", &ids[MAX_DOCS - 1]).is_some());
        // Too large for one document.
        assert!(docs.open("main", "x".repeat(MAX_DOC + 1)).is_err());
        // Bytes: big documents push older ones out.
        let big = "y".repeat(MAX_DOC);
        let bigs: Vec<String> = (0..(MAX_BYTES / MAX_DOC) + 1)
            .map(|_| docs.open("main", big.clone()).unwrap())
            .collect();
        assert!(
            docs.get("main", &bigs[0]).is_none(),
            "over the byte cap the oldest went"
        );
        assert!(docs.get("main", bigs.last().unwrap()).is_some());
    }
}
