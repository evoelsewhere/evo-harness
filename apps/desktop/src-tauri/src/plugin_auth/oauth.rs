//! OAuth for plugin accounts (docs/features/plugin-settings/spec.md, FR-21,
//! FR-22, FR-25, FR-30): endpoints (declared, or discovered from an
//! issuer), PKCE with a state check, the one-time loopback return, the token
//! exchange, renewal and revocation (RFC 6749, 7636, 8252, 7009, 8414).

use std::collections::HashMap;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use evo_plugins::accounts::OAuth;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// How a token request failed.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// The service refused it for good (revoked, password changed): sign in again.
    Refused(String),
    /// The service could not be reached, or failed for now: try again later.
    Offline(String),
}

impl Failure {
    pub fn message(&self) -> &str {
        match self {
            Failure::Refused(m) | Failure::Offline(m) => m,
        }
    }
}

/// Where the flows go.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Endpoints {
    pub authorize: Option<String>,
    pub token: Option<String>,
    pub device: Option<String>,
    pub revoke: Option<String>,
    pub userinfo: Option<String>,
}

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("evo")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}

/// The account's endpoints: declared ones, the rest from the issuer's
/// metadata (OpenID Connect, then OAuth server metadata).
pub async fn endpoints(http: &reqwest::Client, oauth: &OAuth) -> Result<Endpoints, String> {
    let mut found = Endpoints {
        authorize: oauth.authorize_url.clone(),
        token: oauth.token_url.clone(),
        device: oauth.device_url.clone(),
        revoke: oauth.revoke_url.clone(),
        userinfo: oauth.userinfo_url.clone(),
    };
    let Some(issuer) = oauth.issuer.as_deref() else {
        return Ok(found);
    };
    let base = issuer.trim_end_matches('/');
    let mut metadata = None;
    for path in ["/.well-known/openid-configuration", "/.well-known/oauth-authorization-server"] {
        if let Ok(value) = get_json(http, &format!("{base}{path}")).await {
            metadata = Some(value);
            break;
        }
    }
    let metadata = metadata.ok_or_else(|| format!("could not read the sign-in settings of {issuer}"))?;
    let field = |key: &str| metadata.get(key).and_then(Value::as_str).filter(|u| evo_plugins::accounts::acceptable_url(u)).map(str::to_string);
    found.authorize = found.authorize.or_else(|| field("authorization_endpoint"));
    found.token = found.token.or_else(|| field("token_endpoint"));
    found.device = found.device.or_else(|| field("device_authorization_endpoint"));
    found.revoke = found.revoke.or_else(|| field("revocation_endpoint"));
    found.userinfo = found.userinfo.or_else(|| field("userinfo_endpoint"));
    Ok(found)
}

async fn get_json(http: &reqwest::Client, url: &str) -> Result<Value, String> {
    let response = http.get(url).header("Accept", "application/json").send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let text = response.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// 32 random bytes, URL-safe: a PKCE verifier or a state.
pub fn random() -> String {
    evo_vault::random_key().unwrap_or_else(|_| format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()))
}

/// The S256 challenge of a PKCE `verifier`.
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// `pairs` as `application/x-www-form-urlencoded`.
pub fn form(pairs: &[(&str, &str)]) -> String {
    let mut url = tauri::Url::parse("http://x/").expect("a fixed URL");
    url.query_pairs_mut().extend_pairs(pairs.iter().copied());
    url.query().unwrap_or_default().to_string()
}

/// The browser address that starts a sign-in.
pub fn authorize_url(authorize: &str, client_id: &str, redirect: &str, scopes: &[String], state: &str, challenge: &str) -> Result<String, String> {
    let mut url = tauri::Url::parse(authorize).map_err(|e| format!("bad authorize address: {e}"))?;
    let scope = scopes.join(" ");
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect)
            .append_pair("state", state)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        if !scope.is_empty() {
            query.append_pair("scope", &scope);
        }
    }
    Ok(url.to_string())
}

/// A one-time loopback listener for the browser's return.
pub struct Return {
    listener: TcpListener,
    pub redirect: String,
}

impl Return {
    /// On `port` (the plugin's declared one), or a free one.
    pub async fn bind(port: Option<u16>) -> Result<Return, String> {
        let listener = TcpListener::bind(("127.0.0.1", port.unwrap_or(0)))
            .await
            .map_err(|e| format!("could not listen for the sign-in's return: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        Ok(Return { listener, redirect: format!("http://127.0.0.1:{port}/callback") })
    }

    /// Wait for the return with `state`; requests without it (a forged or
    /// replayed return, a favicon) are answered and ignored. The code, or the
    /// service's error.
    pub async fn code(self, state: &str, cancel: &CancellationToken) -> Result<String, String> {
        loop {
            let accepted = tokio::select! {
                accepted = self.listener.accept() => accepted,
                _ = cancel.cancelled() => return Err("cancelled".into()),
            };
            let Ok((mut stream, _)) = accepted else { continue };
            let mut buffer = vec![0u8; 8192];
            let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer)).await.ok().and_then(Result::ok).unwrap_or(0);
            let head = String::from_utf8_lossy(&buffer[..n]).to_string();
            let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
            let query = parse_query(target.split_once('?').map(|(_, q)| q).unwrap_or(""));
            let ours = target.starts_with("/callback") && query.get("state").map(String::as_str) == Some(state);
            let (status, body) = if ours {
                if query.contains_key("error") {
                    ("200 OK", "Sign-in did not finish. You can close this tab.")
                } else {
                    ("200 OK", "Signed in to Evo. You can close this tab.")
                }
            } else {
                ("400 Bad Request", "This is not an Evo sign-in.")
            };
            let page = format!("<!doctype html><meta charset=utf-8><title>Evo</title><p style=\"font:16px system-ui;margin:3em\">{body}</p>");
            let reply = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
            let _ = stream.write_all(reply.as_bytes()).await;
            let _ = stream.shutdown().await;
            if !ours {
                continue;
            }
            if let Some(error) = query.get("error") {
                let detail = query.get("error_description").map(|d| format!(": {d}")).unwrap_or_default();
                return Err(format!("the service said {error}{detail}"));
            }
            return query.get("code").cloned().ok_or_else(|| "the service sent no code".into());
        }
    }
}

pub fn parse_query(query: &str) -> HashMap<String, String> {
    tauri::Url::parse(&format!("http://x/?{query}"))
        .map(|u| u.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect())
        .unwrap_or_default()
}

/// POST a form to `url` and read a token answer (JSON, or a form as some
/// services send). Errors say whether to sign in again or try later.
pub async fn token_request(http: &reqwest::Client, url: &str, pairs: &[(&str, &str)]) -> Result<Map<String, Value>, Failure> {
    let response = http
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(form(pairs))
        .send()
        .await
        .map_err(|e| Failure::Offline(format!("could not reach {}: {e}", host_of(url))))?;
    let status = response.status().as_u16();
    let text = response.text().await.map_err(|e| Failure::Offline(e.to_string()))?;
    let body: Map<String, Value> = serde_json::from_str(&text).unwrap_or_else(|_| parse_query(&text).into_iter().map(|(k, v)| (k, Value::String(v))).collect());
    if let Some(error) = body.get("error").and_then(Value::as_str) {
        let detail = body.get("error_description").and_then(Value::as_str).map(|d| format!(": {d}")).unwrap_or_default();
        let message = format!("{error}{detail}");
        return Err(match error {
            "authorization_pending" | "slow_down" | "temporarily_unavailable" => Failure::Offline(message),
            _ => Failure::Refused(message),
        });
    }
    if status >= 500 || status == 429 {
        return Err(Failure::Offline(format!("{} answered HTTP {status}", host_of(url))));
    }
    if status >= 400 {
        return Err(Failure::Refused(format!("{} answered HTTP {status}", host_of(url))));
    }
    Ok(body)
}

fn host_of(url: &str) -> String {
    tauri::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_else(|| url.to_string())
}

/// What evo keeps of a token answer: `{accessToken, refreshToken,
/// expiresAt (ms), scopes, idToken?}`. `previous` keeps a refresh token the
/// service did not send again.
pub fn session_of(answer: &Map<String, Value>, asked: &[String], previous: Option<&Map<String, Value>>, now_ms: u64) -> Result<Map<String, Value>, Failure> {
    let access = answer.get("access_token").and_then(Value::as_str).ok_or_else(|| Failure::Refused("the service sent no access token".into()))?;
    let mut session = Map::new();
    session.insert("accessToken".into(), json!(access));
    let refresh = answer
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| previous.and_then(|p| p.get("refreshToken")).and_then(Value::as_str).map(str::to_string));
    if let Some(refresh) = refresh {
        session.insert("refreshToken".into(), json!(refresh));
    }
    let seconds = answer.get("expires_in").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
    if let Some(seconds) = seconds {
        session.insert("expiresAt".into(), json!(now_ms + seconds * 1000));
    }
    let scopes: Vec<String> = match answer.get("scope").and_then(Value::as_str) {
        Some(granted) => granted.split([' ', ',']).filter(|s| !s.is_empty()).map(str::to_string).collect(),
        None => previous
            .and_then(|p| p.get("scopes"))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_else(|| asked.to_vec()),
    };
    session.insert("scopes".into(), json!(scopes));
    if let Some(id_token) = answer.get("id_token").and_then(Value::as_str) {
        session.insert("idToken".into(), json!(id_token));
    }
    Ok(session)
}

/// The name an ID token gives (email, preferred name, name), unverified:
/// it is only shown.
pub fn label_of_id_token(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    label_of_claims(&claims)
}

pub fn label_of_claims(claims: &Value) -> Option<String> {
    ["email", "preferred_username", "login", "name", "username"]
        .iter()
        .find_map(|key| claims.get(*key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string))
}

/// The account's name from the userinfo endpoint, if the service has one.
pub async fn label_of_userinfo(http: &reqwest::Client, url: &str, token: &str) -> Option<String> {
    let response = http.get(url).bearer_auth(token).header("Accept", "application/json").send().await.ok()?;
    let value: Value = serde_json::from_str(&response.text().await.ok()?).ok()?;
    label_of_claims(&value)
}

/// Ask the service to revoke `token` (RFC 7009); best effort.
pub async fn revoke(http: &reqwest::Client, url: &str, client_id: &str, token: &str, hint: &str) -> Result<(), String> {
    let response = http
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form(&[("token", token), ("token_type_hint", hint), ("client_id", client_id)]))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("HTTP {}", response.status().as_u16()))
    }
}

/// Whether `session` should be renewed now: under 2 minutes left.
pub fn due(session: &Map<String, Value>, now_ms: u64) -> bool {
    session.get("expiresAt").and_then(Value::as_u64).is_some_and(|at| at <= now_ms + 120_000)
}

/// Whether `session` can no longer be used.
pub fn expired(session: &Map<String, Value>, now_ms: u64) -> bool {
    session.get("expiresAt").and_then(Value::as_u64).is_some_and(|at| at <= now_ms)
}

/// The app's own client: its id and, when the service needs one, its secret.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Client {
    pub id: String,
    pub secret: Option<String>,
}

impl Client {
    fn pairs<'a>(&'a self, pairs: &mut Vec<(&'a str, &'a str)>) {
        pairs.push(("client_id", &self.id));
        if let Some(secret) = &self.secret {
            pairs.push(("client_secret", secret));
        }
    }
}

/// Exchange a code from the browser's return (with the PKCE verifier).
pub async fn exchange(http: &reqwest::Client, token: &str, client: &Client, code: &str, redirect: &str, verifier: &str) -> Result<Map<String, Value>, Failure> {
    let mut pairs = vec![("grant_type", "authorization_code"), ("code", code), ("redirect_uri", redirect), ("code_verifier", verifier)];
    client.pairs(&mut pairs);
    token_request(http, token, &pairs).await
}

/// Renew `session` with its refresh token.
pub async fn refresh(http: &reqwest::Client, token: &str, client: &Client, session: &Map<String, Value>, now_ms: u64) -> Result<Map<String, Value>, Failure> {
    let refresh = session.get("refreshToken").and_then(Value::as_str).ok_or_else(|| Failure::Refused("the session cannot be renewed".into()))?;
    let mut pairs = vec![("grant_type", "refresh_token"), ("refresh_token", refresh)];
    client.pairs(&mut pairs);
    let answer = token_request(http, token, &pairs).await?;
    session_of(&answer, &[], Some(session), now_ms)
}

/// A device-code sign-in as started (RFC 8628).
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub device_code: String,
    pub user_code: String,
    pub uri: String,
    pub complete: Option<String>,
    pub interval: u64,
    pub expires_in: u64,
}

/// Ask the service for a device code.
pub async fn device_start(http: &reqwest::Client, url: &str, client: &Client, scopes: &[String]) -> Result<Device, String> {
    let scope = scopes.join(" ");
    let mut pairs = vec![("scope", scope.as_str())];
    client.pairs(&mut pairs);
    let answer = token_request(http, url, &pairs).await.map_err(|f| f.message().to_string())?;
    let text = |key: &str| answer.get(key).and_then(Value::as_str).map(str::to_string);
    let number = |key: &str, default: u64| answer.get(key).and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(default);
    Ok(Device {
        device_code: text("device_code").ok_or("the service sent no device code")?,
        user_code: text("user_code").ok_or("the service sent no code to show")?,
        uri: text("verification_uri").or_else(|| text("verification_url")).ok_or("the service sent no address")?,
        complete: text("verification_uri_complete"),
        interval: number("interval", 5).max(1),
        expires_in: number("expires_in", 900),
    })
}

/// Poll until the person confirms, the code expires, or `cancel`.
pub async fn device_poll(http: &reqwest::Client, token: &str, client: &Client, device: &Device, cancel: &CancellationToken) -> Result<Map<String, Value>, String> {
    let mut interval = device.interval;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(device.expires_in);
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(interval)) => {}
            _ = cancel.cancelled() => return Err("cancelled".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("the code expired".into());
        }
        let mut pairs = vec![("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("device_code", device.device_code.as_str())];
        client.pairs(&mut pairs);
        match token_request(http, token, &pairs).await {
            Ok(answer) => return Ok(answer),
            Err(Failure::Offline(message)) if message.starts_with("slow_down") => interval += 5,
            Err(Failure::Offline(_)) => {}
            Err(Failure::Refused(message)) => return Err(message),
        }
    }
}
