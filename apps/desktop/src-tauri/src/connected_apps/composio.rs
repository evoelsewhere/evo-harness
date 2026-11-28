//! Composio's REST API, with the person's project key
//! (docs/features/connected-apps/plan.md, section 1 and 3.2).
//!
//! Every request carries the key as `x-api-key` to [`BASE`] only; errors
//! come back as text the person can act on. The calls and shapes are those
//! recorded in `testdata/` (T1).

use std::time::Duration;

use reqwest::Method;
use serde_json::{json, Value};

/// Composio's API.
pub const BASE: &str = "https://backend.composio.dev/api/v3.1";
/// A project key starts so; a user or org key does not.
pub const KEY_PREFIX: &str = "ak_";

/// A failed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// No answer: no network, or Composio is down.
    Unreachable(String),
    /// 401 / 403: the key was refused.
    Refused(String),
    /// 404.
    NotFound(String),
    /// Any other answer that is not a success.
    Failed { status: u16, message: String },
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Unreachable(e) => write!(f, "Composio could not be reached: {e}"),
            ApiError::Refused(e) => write!(f, "Composio refused the key: {e}"),
            ApiError::NotFound(e) => write!(f, "Composio does not know it: {e}"),
            ApiError::Failed { status, message } => {
                write!(f, "Composio answered {status}: {message}")
            }
        }
    }
}

/// Composio's message in an error body (`{error: {message}}`, `{message}`).
fn message_of(body: &Value, fallback: &str) -> String {
    body.pointer("/error/message")
        .or_else(|| body.get("message"))
        .or_else(|| body.get("error").filter(|e| e.is_string()))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| fallback.chars().take(300).collect())
}

/// Whether `key` looks like a project key (checked before any request).
pub fn check_format(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Paste your Composio project key.".into());
    }
    if !key.starts_with(KEY_PREFIX) || key.len() < 8 || key.chars().any(char::is_whitespace) {
        return Err(format!("This is not a Composio project key: a project key starts with \"{KEY_PREFIX}\" (Composio → your project → Settings → API keys)."));
    }
    Ok(())
}

/// A client for one key.
#[derive(Clone)]
pub struct Composio {
    http: reqwest::Client,
    base: String,
    key: String,
}

impl Composio {
    pub fn new(base: &str, key: &str) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .user_agent("evo")
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            key: key.trim().to_string(),
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// The key, for the session's MCP header only.
    pub fn key(&self) -> &str {
        &self.key
    }

    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, ApiError> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.base))
            .header("x-api-key", &self.key);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Unreachable(without_key(&e.to_string(), &self.key)))?;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        match status {
            200..=299 => Ok(if body.is_null() { json!({}) } else { body }),
            401 | 403 => Err(ApiError::Refused(message_of(&body, &text))),
            404 => Err(ApiError::NotFound(message_of(&body, &text))),
            _ => Err(ApiError::Failed {
                status,
                message: message_of(&body, &text),
            }),
        }
    }

    /// Check the key: one cheap authenticated read.
    pub async fn check_key(&self) -> Result<(), ApiError> {
        self.request(Method::GET, "/connected_accounts?limit=1", None)
            .await
            .map(|_| ())
    }

    /// A Tool Router session for `user_id`: several accounts per service,
    /// each call naming its account, connections managed by the agent.
    pub async fn create_session(
        &self,
        user_id: &str,
        max_accounts: usize,
    ) -> Result<Value, ApiError> {
        let body = json!({
            "user_id": user_id,
            "manage_connections": { "enable": true },
            "multi_account": { "enable": true, "max_accounts_per_toolkit": max_accounts, "require_explicit_selection": true },
        });
        self.request(Method::POST, "/tool_router/session", Some(body))
            .await
    }

    pub async fn get_session(&self, session: &str) -> Result<Value, ApiError> {
        self.request(
            Method::GET,
            &format!("/tool_router/session/{}", enc(session)),
            None,
        )
        .await
    }

    pub async fn patch_session(&self, session: &str, body: Value) -> Result<Value, ApiError> {
        self.request(
            Method::PATCH,
            &format!("/tool_router/session/{}", enc(session)),
            Some(body),
        )
        .await
    }

    /// One page of the session's toolkits.
    pub async fn session_toolkits(
        &self,
        session: &str,
        cursor: Option<&str>,
    ) -> Result<Value, ApiError> {
        let mut path = format!("/tool_router/session/{}/toolkits?limit=1000", enc(session));
        if let Some(cursor) = cursor {
            path.push_str(&format!("&cursor={}", enc(cursor)));
        }
        self.request(Method::GET, &path, None).await
    }

    /// One page of Composio's catalogue (names, logos, categories).
    pub async fn toolkits(&self, cursor: Option<&str>) -> Result<Value, ApiError> {
        let mut path = "/toolkits?limit=1000".to_string();
        if let Some(cursor) = cursor {
            path.push_str(&format!("&cursor={}", enc(cursor)));
        }
        self.request(Method::GET, &path, None).await
    }

    /// A sign-in link for `toolkit` under `alias`.
    pub async fn link(&self, session: &str, toolkit: &str, alias: &str) -> Result<Value, ApiError> {
        let body = json!({ "toolkit": toolkit, "alias": alias });
        self.request(
            Method::POST,
            &format!("/tool_router/session/{}/link", enc(session)),
            Some(body),
        )
        .await
    }

    /// Every connected account of `user_id` (all pages).
    pub async fn accounts(&self, user_id: &str) -> Result<Vec<Value>, ApiError> {
        self.all_pages(&format!(
            "/connected_accounts?user_ids={}&limit=100",
            enc(user_id)
        ))
        .await
    }

    /// Every connected account of the project (all users), to find an
    /// earlier user (FR-18).
    pub async fn project_accounts(&self) -> Result<Vec<Value>, ApiError> {
        self.all_pages("/connected_accounts?limit=100").await
    }

    async fn all_pages(&self, first: &str) -> Result<Vec<Value>, ApiError> {
        let mut out = Vec::new();
        let mut path = first.to_string();
        for _ in 0..50 {
            let page = self.request(Method::GET, &path, None).await?;
            out.extend(
                page.get("items")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
            match page
                .get("next_cursor")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
            {
                Some(cursor) => path = format!("{first}&cursor={}", enc(cursor)),
                None => break,
            }
        }
        Ok(out)
    }

    /// Disconnect `account` and revoke its access at the service.
    pub async fn delete_account(&self, account: &str) -> Result<(), ApiError> {
        match self
            .request(
                Method::DELETE,
                &format!("/connected_accounts/{}?revoke_on_delete=true", enc(account)),
                None,
            )
            .await
        {
            Ok(_) | Err(ApiError::NotFound(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// The project's auth configs for `toolkit`.
    pub async fn auth_configs(&self, toolkit: &str) -> Result<Vec<Value>, ApiError> {
        self.all_pages(&format!(
            "/auth_configs?toolkit_slug={}&limit=100",
            enc(toolkit)
        ))
        .await
    }

    /// A presigned place to put a file for `tool` (`{key, new_presigned_url}`).
    pub async fn upload_request(
        &self,
        toolkit: &str,
        tool: &str,
        name: &str,
        mimetype: &str,
        md5: &str,
    ) -> Result<Value, ApiError> {
        let body = json!({ "toolkit_slug": toolkit, "tool_slug": tool, "filename": name, "mimetype": mimetype, "md5": md5 });
        self.request(Method::POST, "/files/upload/request", Some(body))
            .await
    }

    /// Put `bytes` at a presigned URL (HTTPS on Composio's storage only; no
    /// key, no redirects).
    pub async fn put_file(&self, url: &str, bytes: Vec<u8>, mimetype: &str) -> Result<(), String> {
        let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
        if !upload_url_ok(&parsed, &self.base) {
            return Err(format!(
                "Composio gave an upload address Evo does not trust ({})",
                parsed.host_str().unwrap_or("?")
            ));
        }
        let response = self
            .http
            .put(parsed)
            .header("content-type", mimetype)
            .body(bytes)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("the upload was refused ({})", response.status()))
        }
    }
}

/// Percent-encode a path or query part.
pub fn enc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn without_key(text: &str, key: &str) -> String {
    if key.len() >= 4 {
        text.replace(key, "(key)")
    } else {
        text.to_string()
    }
}

fn same_origin(url: &reqwest::Url, base: &str) -> bool {
    reqwest::Url::parse(base).is_ok_and(|b| {
        b.scheme() == url.scheme()
            && b.host_str() == url.host_str()
            && b.port_or_known_default() == url.port_or_known_default()
    })
}

fn composio_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "composio.dev" || host.ends_with(".composio.dev")
}

/// The session's MCP URL must be HTTPS on `composio.dev` or a subdomain
/// (or the API's own origin, as in tests): the key goes there.
pub fn mcp_url_ok(url: &str, base: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    same_origin(&parsed, base)
        || (parsed.scheme() == "https" && parsed.host_str().is_some_and(composio_host))
}

/// Where files may be uploaded: HTTPS on Composio's storage, which is an
/// S3 bucket or a `composio.dev` host (or the API's own origin in tests).
pub fn upload_url_ok(url: &reqwest::Url, base: &str) -> bool {
    if same_origin(url, base) {
        return true;
    }
    url.scheme() == "https"
        && url.host_str().is_some_and(|h| {
            let h = h.to_ascii_lowercase();
            composio_host(&h)
                || (h.ends_with(".amazonaws.com") && h.contains("composio"))
                || h.ends_with(".r2.cloudflarestorage.com")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_project_keys_pass_the_format_check() {
        assert!(check_format("ak_abc123xyz").is_ok());
        assert!(check_format("  ").unwrap_err().contains("Paste"));
        assert!(check_format("uak_abc123xyz")
            .unwrap_err()
            .contains("not a Composio project key"));
        assert!(check_format("ak_ab c123").is_err());
    }

    #[test]
    fn the_key_goes_only_to_composio() {
        assert!(mcp_url_ok(
            "https://backend.composio.dev/tool_router/trs_1/mcp",
            BASE
        ));
        assert!(mcp_url_ok("https://mcp.composio.dev/x", BASE));
        assert!(!mcp_url_ok("http://backend.composio.dev/x", BASE));
        assert!(!mcp_url_ok("https://composio.dev.evil.example/x", BASE));
        assert!(!mcp_url_ok("https://evilcomposio.dev/x", BASE));
        assert!(mcp_url_ok(
            "http://127.0.0.1:4000/mcp",
            "http://127.0.0.1:4000/api/v3.1"
        ));
        assert!(!mcp_url_ok(
            "http://127.0.0.1:4001/mcp",
            "http://127.0.0.1:4000/api/v3.1"
        ));
        assert_eq!(enc("a b/c"), "a%20b%2Fc");
        assert_eq!(
            without_key("bad ak_SECRET there", "ak_SECRET"),
            "bad (key) there"
        );
    }
}
