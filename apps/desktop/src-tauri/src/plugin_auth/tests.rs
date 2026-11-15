//! Sign-in against a local OAuth test server (plugin-settings T18, T19).

use std::sync::{Arc, Mutex};

use evo_plugins::accounts::parse_accounts;
use evo_plugins::settings::parse_settings;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::oauth::{self, Client, Failure};
use super::*;

type Handler = Arc<dyn Fn(&str, &str, &HashMap<String, String>) -> (u16, String) + Send + Sync>;

/// A test service on loopback: `handler(method, path, form)`; the requests seen.
async fn service(handler: Handler) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { continue };
            let (handler, log) = (handler.clone(), log.clone());
            tokio::spawn(async move {
                let mut data = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = stream.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&data).to_string();
                    if let Some(at) = text.find("\r\n\r\n") {
                        let length = text[..at]
                            .lines()
                            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                            .unwrap_or(0);
                        if data.len() >= at + 4 + length {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&data).to_string();
                let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
                let mut parts = head.split_whitespace();
                let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
                let path = target.split('?').next().unwrap_or("").to_string();
                let form = oauth::parse_query(body);
                log.lock().unwrap().push(format!("{method} {path} {body}"));
                let (status, reply) = handler(&method, &path, &form);
                let out = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
                let _ = stream.write_all(out.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (base, seen)
}

fn client() -> Client {
    Client { id: "client-1".into(), secret: None }
}

#[test]
fn pkce_follows_rfc_7636() {
    assert_eq!(oauth::challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    assert_ne!(oauth::random(), oauth::random());
    let url = oauth::authorize_url("https://id.example.com/auth?x=1", "c 1", "http://127.0.0.1:5/callback", &["read".into(), "write".into()], "st", "ch").unwrap();
    let query = oauth::parse_query(url.split_once('?').unwrap().1);
    assert_eq!(query["x"], "1");
    assert_eq!(query["client_id"], "c 1");
    assert_eq!(query["scope"], "read write");
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["state"], "st");
}

#[tokio::test]
async fn the_return_takes_only_its_own_state() {
    let back = oauth::Return::bind(None).await.unwrap();
    let redirect = back.redirect.clone();
    let cancel = CancellationToken::new();
    let waiting = tokio::spawn(async move { back.code("right", &cancel).await });
    let http = reqwest::Client::new();
    let forged = http.get(format!("{redirect}?code=stolen&state=wrong")).send().await.unwrap();
    assert_eq!(forged.status().as_u16(), 400, "a forged return is refused");
    let no_state = http.get(format!("{redirect}?code=stolen")).send().await.unwrap();
    assert_eq!(no_state.status().as_u16(), 400);
    let ours = http.get(format!("{redirect}?code=good&state=right")).send().await.unwrap();
    assert_eq!(ours.status().as_u16(), 200);
    assert_eq!(waiting.await.unwrap().unwrap(), "good");

    // The service's error ends the sign-in with it; cancel ends it too.
    let back = oauth::Return::bind(None).await.unwrap();
    let redirect = back.redirect.clone();
    let cancel = CancellationToken::new();
    let waiting = tokio::spawn(async move { back.code("s", &cancel).await });
    http.get(format!("{redirect}?error=access_denied&state=s")).send().await.unwrap();
    assert!(waiting.await.unwrap().unwrap_err().contains("access_denied"));
    let back = oauth::Return::bind(None).await.unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(back.code("s", &cancel).await.unwrap_err(), "cancelled");
}

#[tokio::test]
async fn code_exchange_checks_pkce_then_renewal_keeps_the_refresh_token() {
    let verifier = oauth::random();
    let challenge = oauth::challenge(&verifier);
    let (base, seen) = service(Arc::new(move |_, path, form| match (path, form.get("grant_type").map(String::as_str)) {
        ("/token", Some("authorization_code")) => {
            if form.get("code").map(String::as_str) != Some("good") || oauth::challenge(form.get("code_verifier").map(String::as_str).unwrap_or("")) != challenge {
                return (400, json!({ "error": "invalid_grant", "error_description": "bad verifier" }).to_string());
            }
            (200, json!({ "access_token": "a-1", "refresh_token": "r-1", "expires_in": 3600, "scope": "read write", "token_type": "bearer" }).to_string())
        }
        ("/token", Some("refresh_token")) => match form.get("refresh_token").map(String::as_str) {
            Some("r-1") => (200, json!({ "access_token": "a-2", "expires_in": "60" }).to_string()),
            _ => (400, json!({ "error": "invalid_grant" }).to_string()),
        },
        _ => (503, "{}".into()),
    }))
    .await;
    let http = oauth::client().unwrap();
    let token = format!("{base}/token");
    let wrong = oauth::exchange(&http, &token, &client(), "good", "http://127.0.0.1:1/callback", "not-the-verifier").await;
    assert!(matches!(wrong, Err(Failure::Refused(m)) if m.contains("invalid_grant")));
    let answer = oauth::exchange(&http, &token, &client(), "good", "http://127.0.0.1:1/callback", &verifier).await.unwrap();
    let session = oauth::session_of(&answer, &["read".into()], None, 1_000).unwrap();
    assert_eq!(session["accessToken"], "a-1");
    assert_eq!(session["expiresAt"], 1_000 + 3_600_000);
    assert_eq!(session["scopes"], json!(["read", "write"]));
    assert!(seen.lock().unwrap().iter().any(|r| r.contains("client_id=client-1")));

    assert!(!oauth::due(&session, 1_000));
    assert!(oauth::due(&session, 1_000 + 3_600_000 - 60_000), "under 2 minutes left");
    let renewed = oauth::refresh(&http, &token, &client(), &session, 5_000).await.unwrap();
    assert_eq!(renewed["accessToken"], "a-2");
    assert_eq!(renewed["refreshToken"], "r-1", "kept when the service sends none");
    assert_eq!(renewed["expiresAt"], 65_000);
    assert_eq!(renewed["scopes"], json!(["read", "write"]));

    let mut revoked = session.clone();
    revoked.insert("refreshToken".into(), json!("r-gone"));
    assert!(matches!(oauth::refresh(&http, &token, &client(), &revoked, 0).await, Err(Failure::Refused(_))));
    let down = oauth::refresh(&http, &format!("{base}/down"), &client(), &session, 0).await;
    assert!(matches!(down, Err(Failure::Offline(_))), "a failing service keeps the session");
    let unreachable = oauth::refresh(&http, "http://127.0.0.1:9/token", &client(), &session, 0).await;
    assert!(matches!(unreachable, Err(Failure::Offline(_))));
}

#[tokio::test]
async fn device_code_waits_for_the_person_and_can_be_cancelled() {
    let polls = Arc::new(Mutex::new(0));
    let count = polls.clone();
    let (base, _) = service(Arc::new(move |_, path, form| match path {
        "/device" => (200, json!({ "device_code": "dc", "user_code": "ABCD-1234", "verification_uri": "https://id.example.com/device", "interval": 1, "expires_in": 30 }).to_string()),
        "/token" if form.get("device_code").map(String::as_str) == Some("dc") => {
            let mut n = count.lock().unwrap();
            *n += 1;
            match *n {
                1 => (400, json!({ "error": "authorization_pending" }).to_string()),
                2 => (400, json!({ "error": "slow_down" }).to_string()),
                _ => (200, json!({ "access_token": "a-dev", "expires_in": 600 }).to_string()),
            }
        }
        _ => (400, json!({ "error": "expired_token" }).to_string()),
    }))
    .await;
    let http = oauth::client().unwrap();
    let mut device = oauth::device_start(&http, &format!("{base}/device"), &client(), &["read".into()]).await.unwrap();
    assert_eq!((device.user_code.as_str(), device.interval), ("ABCD-1234", 1));
    device.interval = 1;
    let cancel = CancellationToken::new();
    let started = std::time::Instant::now();
    let answer = oauth::device_poll(&http, &format!("{base}/token"), &client(), &device, &cancel).await.unwrap();
    assert_eq!(answer["access_token"], "a-dev");
    assert_eq!(*polls.lock().unwrap(), 3);
    assert!(started.elapsed() >= std::time::Duration::from_secs(7), "slow_down adds 5 seconds");

    let mut other = device.clone();
    other.device_code = "unknown".into();
    assert!(oauth::device_poll(&http, &format!("{base}/token"), &client(), &other, &cancel).await.unwrap_err().contains("expired_token"));
    cancel.cancel();
    assert_eq!(oauth::device_poll(&http, &format!("{base}/token"), &client(), &device, &cancel).await.unwrap_err(), "cancelled");
}

#[tokio::test]
async fn endpoints_come_from_the_issuer_unless_declared() {
    let holder: Arc<Mutex<String>> = Arc::default();
    let base_for = holder.clone();
    let (base, _) = service(Arc::new(move |_, path, _| {
        let base = base_for.lock().unwrap().clone();
        match path {
            "/.well-known/openid-configuration" => (
                200,
                json!({ "authorization_endpoint": format!("{base}/auth"), "token_endpoint": format!("{base}/token"),
                        "device_authorization_endpoint": format!("{base}/device"), "revocation_endpoint": format!("{base}/revoke"),
                        "userinfo_endpoint": format!("{base}/me") })
                .to_string(),
            ),
            "/me" => (200, json!({ "sub": "1", "email": "ana@example.com" }).to_string()),
            _ => (404, "{}".into()),
        }
    }))
    .await;
    *holder.lock().unwrap() = base.clone();
    let http = oauth::client().unwrap();
    let declared = OAuth { issuer: Some(base.clone()), token_url: Some("https://own.example.com/token".into()), ..Default::default() };
    let found = oauth::endpoints(&http, &declared).await.unwrap();
    assert_eq!(found.authorize.as_deref(), Some(format!("{base}/auth").as_str()));
    assert_eq!(found.token.as_deref(), Some("https://own.example.com/token"), "declared wins");
    assert_eq!(found.revoke.as_deref(), Some(format!("{base}/revoke").as_str()));
    assert_eq!(oauth::label_of_userinfo(&http, found.userinfo.as_deref().unwrap(), "t").await.as_deref(), Some("ana@example.com"));
    let missing = OAuth { issuer: Some("http://127.0.0.1:9".into()), ..Default::default() };
    assert!(oauth::endpoints(&http, &missing).await.is_err());
}

#[test]
fn names_come_from_id_tokens_and_answers_from_forms_too() {
    use base64::Engine as _;
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"sub":"1","preferred_username":"ana"}"#);
    assert_eq!(oauth::label_of_id_token(&format!("h.{claims}.s")).as_deref(), Some("ana"));
    assert_eq!(oauth::label_of_id_token("not-a-jwt"), None);
    let session = oauth::session_of(&oauth::parse_query("access_token=x&scope=repo%2Cuser").into_iter().map(|(k, v)| (k, json!(v))).collect(), &[], None, 0).unwrap();
    assert_eq!(session["scopes"], json!(["repo", "user"]));
    assert!(session.get("expiresAt").is_none(), "no expiry: never due");
    assert!(!oauth::due(&session, u64::MAX / 2));
}

fn plugin_with_accounts() -> (Installed, Plugin) {
    let (settings, _) = parse_settings(Some(&json!({ "key": { "kind": "secret" } })), None);
    let (accounts, problems) = parse_accounts(
        Some(&json!([
            { "id": "gh", "service": "GitHub", "kind": "oauth", "oauth": { "authorizeUrl": "https://github.com/a", "tokenUrl": "https://github.com/t", "clientId": "c", "scopes": ["repo", "user"] } },
            { "id": "key", "service": "Tracker", "kind": "token", "setting": "key", "optional": true },
            { "id": "portal", "service": "Portal", "kind": "web", "web": { "loginUrl": "https://p.example.com", "domains": ["p.example.com"], "done": { "cookie": "S" } } }
        ])),
        &settings,
    );
    assert!(problems.is_empty(), "{problems:?}");
    let plugin = Plugin { id: "p@m".into(), name: "p".into(), settings, accounts, ..Default::default() };
    let installed = Installed {
        id: "p@m".into(),
        accounts: vec![
            AccountRecord { account: "gh".into(), id: "r1".into(), label: "ana".into(), state: "signedIn".into(), default: true, scopes: vec!["repo".into()], ..Default::default() },
            AccountRecord { account: "portal".into(), id: "r2".into(), label: "p.example.com".into(), state: "needsSignIn".into(), default: true, ..Default::default() },
        ],
        ..Default::default()
    };
    (installed, plugin)
}

#[test]
fn status_shows_states_and_access_an_update_asks_for() {
    let (installed, plugin) = plugin_with_accounts();
    let status = status(&installed, &plugin);
    assert_eq!(status[0]["state"], "signedIn");
    assert_eq!(status[0]["signIns"][0]["missingScopes"], json!(["user"]), "sign in again to grant user");
    assert_eq!(status[1]["state"], "signedOut");
    assert_eq!(status[2]["state"], "needsSignIn");
    let text = serde_json::to_string(&status).unwrap();
    assert!(!text.contains("accessToken") && !text.contains("cookies"));
    let public = crate::plugin_ui::bridge::public_account(status[0].clone());
    assert_eq!(public, json!({ "id": "gh", "service": "GitHub", "kind": "oauth", "state": "signedIn", "signIns": [{ "label": "ana", "state": "signedIn", "default": true }] }));
}

#[test]
fn a_tool_asks_only_for_needed_accounts_not_signed_in() {
    let (installed, plugin) = plugin_with_accounts();
    let missing: Vec<&str> = plugin.accounts.iter().filter(|a| !a.optional && !signed_in(&installed, a)).map(|a| a.id.as_str()).collect();
    assert_eq!(missing, ["portal"], "the optional key is not asked for; GitHub is signed in");
    let mut with_key = installed.clone();
    with_key.secrets_set.push("key".into());
    assert!(signed_in(&with_key, &plugin.accounts[1]));
}

#[test]
fn a_web_login_ends_on_its_declared_address() {
    let web = evo_plugins::accounts::Web { done_url: Some("https://p.example.com/home".into()), ..Default::default() };
    assert!(web::done_by_url(&web, "https://p.example.com/home?x=1"));
    assert!(!web::done_by_url(&web, "https://sso.example.com/login"));
}
