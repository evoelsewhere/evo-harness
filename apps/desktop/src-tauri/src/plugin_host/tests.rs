use std::sync::Mutex;

use evo_plugins::accounts::parse_accounts;
use evo_plugins::settings::parse_settings;
use evo_plugins::store::AccountRecord;
use serde_json::json;

use super::*;

/// One plugin `p@m` with a token account, an OAuth account and a web login.
struct Fake {
    installed: Mutex<Installed>,
    plugin: Plugin,
    secrets: Map<String, Value>,
    logs: Mutex<Vec<String>>,
    signed_out: Mutex<Vec<String>>,
}

fn fake(approved: &[&str]) -> Fake {
    let (settings, problems) = parse_settings(
        Some(&json!({
            "apiKey": { "kind": "secret", "title": "API key" },
            "region": { "kind": "select", "options": ["eu", "us"], "default": "eu", "scope": "workspace" }
        })),
        None,
    );
    assert!(problems.is_empty(), "{problems:?}");
    let (accounts, problems) = parse_accounts(
        Some(&json!([
            { "id": "key", "service": "Tracker", "kind": "token", "setting": "apiKey" },
            { "id": "gh", "service": "GitHub", "kind": "oauth",
              "oauth": { "authorizeUrl": "https://github.com/login/oauth/authorize", "tokenUrl": "https://github.com/login/oauth/access_token", "clientId": "abc" } },
            { "id": "portal", "service": "Portal", "kind": "web",
              "web": { "loginUrl": "https://portal.example.com/login", "domains": ["portal.example.com"], "done": { "cookie": "SESSION" } } }
        ])),
        &settings,
    );
    assert!(problems.is_empty(), "{problems:?}");
    let plugin = Plugin { id: "p@m".into(), name: "p".into(), settings, accounts, ..Default::default() };
    let installed = Installed {
        id: "p@m".into(),
        enabled: true,
        approved_permissions: approved.iter().map(|s| s.to_string()).collect(),
        secrets_set: vec!["apiKey".into()],
        accounts: vec![
            AccountRecord { account: "gh".into(), id: "r1".into(), label: "ana".into(), state: "signedIn".into(), default: true, ..Default::default() },
            AccountRecord { account: "gh".into(), id: "r2".into(), label: "work".into(), state: "signedIn".into(), workspaces: vec!["ws".into()], ..Default::default() },
            AccountRecord { account: "portal".into(), id: "r3".into(), label: "ana".into(), state: "signedIn".into(), default: true, ..Default::default() },
        ],
        ..Default::default()
    };
    let mut secrets = Map::new();
    secrets.insert("setting:apiKey".into(), json!("key-personal-123"));
    secrets.insert("account:r1".into(), json!({ "accessToken": "gho_personal", "expiresAt": 99 }));
    secrets.insert("account:r2".into(), json!({ "accessToken": "gho_work" }));
    secrets.insert(
        "account:r3".into(),
        json!({ "cookies": [
            { "name": "SESSION", "value": "s3cr3t", "domain": ".portal.example.com", "path": "/" },
            { "name": "api", "value": "v", "domain": "api.portal.example.com", "path": "/v2" },
            { "name": "other", "value": "x", "domain": "elsewhere.com", "path": "/" }
        ] }),
    );
    Fake { installed: Mutex::new(installed), plugin, secrets, logs: Mutex::default(), signed_out: Mutex::default() }
}

impl Backing for Fake {
    fn plugin(&self, id: &str) -> Option<(Installed, Plugin)> {
        let installed = self.installed.lock().unwrap().clone();
        (id == "p@m" && installed.enabled).then(|| (installed, self.plugin.clone()))
    }
    fn secrets(&self, _: &str) -> Result<Map<String, Value>, String> {
        Ok(self.secrets.clone())
    }
    fn log(&self, plugin: &str, level: &str, message: &str) {
        self.logs.lock().unwrap().push(format!("{plugin} {level} {message}"));
    }
    fn signed_out(&self, id: &str, account: &Account, record: Option<&str>) {
        self.signed_out.lock().unwrap().push(format!("{id} {} {record:?}", account.id));
    }
}

fn get(host: &Host, backing: &Fake, key: Option<&str>, target: &str) -> Response {
    send(host, backing, key, "GET", target, b"")
}

fn send(host: &Host, backing: &Fake, key: Option<&str>, method: &str, target: &str, body: &[u8]) -> Response {
    let head = format!("{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}", body.len());
    let mut request = http::parse_head(head.as_bytes()).unwrap();
    request.key = key.map(str::to_string);
    request.body = body.to_vec();
    host.handle(backing, &request)
}

fn code(response: &Response) -> &str {
    response.body.as_ref().and_then(|b| b["code"].as_str()).unwrap_or_default()
}

#[test]
fn keys_name_one_plugin_and_part_and_stop_when_taken_back() {
    let host = Host::default();
    let backing = fake(&["runtime", "secrets"]);
    let backend = host.key("p@m", Caller::Backend).unwrap();
    assert_eq!(host.key("p@m", Caller::Backend).unwrap(), backend, "one key per part while the app runs");
    let hooks = host.key("p@m", Caller::Hooks).unwrap();
    assert_ne!(backend, hooks);

    assert_eq!(get(&host, &backing, None, "/v1/settings").status, 401);
    assert_eq!(get(&host, &backing, Some("made-up"), "/v1/settings").status, 401);
    assert_eq!(get(&host, &backing, Some(&backend), "/v1/settings").status, 200);
    // Hooks were not approved: their key gets nothing.
    let refused = get(&host, &backing, Some(&hooks), "/v1/settings");
    assert_eq!((refused.status, code(&refused)), (403, "no_permission"));

    // The plugin turned off: its keys stop at once, then are dropped.
    backing.installed.lock().unwrap().enabled = false;
    assert_eq!(get(&host, &backing, Some(&backend), "/v1/settings").status, 401);
    host.retain(|_, _| false);
    backing.installed.lock().unwrap().enabled = true;
    assert_eq!(get(&host, &backing, Some(&backend), "/v1/settings").status, 401, "a dropped key never comes back");
    assert_ne!(host.key("p@m", Caller::Backend).unwrap(), backend);
}

#[test]
fn tokens_need_secrets_and_follow_the_workspace() {
    let host = Host::default();
    let backing = fake(&["runtime"]);
    let key = host.key("p@m", Caller::Backend).unwrap();
    let refused = get(&host, &backing, Some(&key), "/v1/accounts/gh/token");
    assert_eq!((refused.status, code(&refused)), (403, "no_permission"));

    backing.installed.lock().unwrap().approved_permissions.push("secrets".into());
    let token = get(&host, &backing, Some(&key), "/v1/accounts/gh/token");
    assert_eq!(token.body.unwrap(), json!({ "accessToken": "gho_personal", "expiresAt": 99 }));
    let work = get(&host, &backing, Some(&key), "/v1/accounts/gh/token?workspace=ws");
    assert_eq!(work.body.unwrap()["accessToken"], "gho_work");
    let api_key = get(&host, &backing, Some(&key), "/v1/accounts/key/token");
    assert_eq!(api_key.body.unwrap()["accessToken"], "key-personal-123");

    let unknown = get(&host, &backing, Some(&key), "/v1/accounts/nope/token");
    assert_eq!((unknown.status, code(&unknown)), (404, "unknown_account"));
    let web = get(&host, &backing, Some(&key), "/v1/accounts/portal/token");
    assert_eq!(web.status, 400);

    // Refused by the service: marked, then no token until signed in again.
    let out = send(&host, &backing, Some(&key), "POST", "/v1/accounts/gh/signed-out", b"");
    assert_eq!(out.status, 204);
    assert_eq!(*backing.signed_out.lock().unwrap(), ["p@m gh Some(\"r1\")"]);
    backing.installed.lock().unwrap().accounts[0].state = "needsSignIn".into();
    let gone = get(&host, &backing, Some(&key), "/v1/accounts/gh/token");
    assert_eq!((gone.status, code(&gone)), (409, "not_signed_in"));
}

#[test]
fn cookies_only_for_declared_domains() {
    let host = Host::default();
    let backing = fake(&["runtime", "secrets"]);
    let key = host.key("p@m", Caller::Backend).unwrap();
    let url = |u: &str| format!("/v1/accounts/portal/cookies?url={}", u.replace(':', "%3A").replace('/', "%2F"));
    let header = |target: String| get(&host, &backing, Some(&key), &target).body.unwrap()["header"].as_str().unwrap().to_string();
    assert_eq!(header(url("https://portal.example.com/tickets")), "SESSION=s3cr3t");
    assert_eq!(header(url("https://api.portal.example.com/v2/x")), "SESSION=s3cr3t; api=v");
    let other = get(&host, &backing, Some(&key), &url("https://elsewhere.com/"));
    assert_eq!((other.status, code(&other)), (403, "no_permission"));
    let lookalike = get(&host, &backing, Some(&key), &url("https://evilportal.example.com.attacker.net/"));
    assert_eq!(lookalike.status, 403);
    assert_eq!(get(&host, &backing, Some(&key), &url("http://portal.example.com/")).status, 400, "https only");
    assert_eq!(get(&host, &backing, Some(&key), "/v1/accounts/gh/cookies?url=https%3A%2F%2Fgithub.com").status, 400);
}

#[test]
fn accounts_and_settings_carry_no_secret() {
    let host = Host::default();
    let backing = fake(&["hooks"]);
    let key = host.key("p@m", Caller::Hooks).unwrap();
    let accounts = get(&host, &backing, Some(&key), "/v1/accounts").body.unwrap();
    assert_eq!(accounts[0], json!({ "id": "key", "service": "Tracker", "state": "signedIn", "label": "" }));
    assert_eq!(accounts[1]["label"], "ana");
    {
        let mut installed = backing.installed.lock().unwrap();
        installed.workspace_config.insert("ws".into(), json!({ "region": "us" }).as_object().unwrap().clone());
    }
    let settings = get(&host, &backing, Some(&key), "/v1/settings").body.unwrap();
    assert_eq!(settings, json!({ "values": { "region": "eu" } }));
    let ws = get(&host, &backing, Some(&key), "/v1/settings?workspace=ws").body.unwrap();
    assert_eq!(ws["values"]["region"], "us");
    let text = format!("{accounts} {settings} {ws}");
    for secret in ["key-personal-123", "gho_", "s3cr3t"] {
        assert!(!text.contains(secret), "{secret} leaked");
    }
}

#[test]
fn log_lines_hide_secrets_and_calls_are_limited() {
    let host = Host::default();
    let backing = fake(&["runtime"]);
    let key = host.key("p@m", Caller::Backend).unwrap();
    let body = json!({ "level": "warn", "message": "token gho_personal and cookie s3cr3t refused" }).to_string();
    assert_eq!(send(&host, &backing, Some(&key), "POST", "/v1/log", body.as_bytes()).status, 204);
    assert_eq!(*backing.logs.lock().unwrap(), ["p warn token (hidden) and cookie (hidden) refused"]);
    assert_eq!(send(&host, &backing, Some(&key), "POST", "/v1/log", b"not json").status, 400);
    assert_eq!(get(&host, &backing, Some(&key), "/v1/nothing").status, 404);
    let mut limited = None;
    for _ in 0..RATE_LIMIT {
        let response = get(&host, &backing, Some(&key), "/v1/settings");
        if response.status == 429 {
            limited = Some(response);
            break;
        }
    }
    assert_eq!(limited.map(|r| r.status), Some(429));
}

#[test]
fn requests_parse_and_browser_pages_are_refused() {
    let head = b"GET /v1/accounts/gh/token?workspace=my%20ws&x=a+b HTTP/1.1\r\nAuthorization: Bearer k1\r\nContent-Length: 0";
    let request = http::parse_head(head).unwrap();
    assert_eq!(request.path, "/v1/accounts/gh/token");
    assert_eq!(request.query["workspace"], "my ws");
    assert_eq!(request.query["x"], "a b");
    assert_eq!(request.key.as_deref(), Some("k1"));
    assert_eq!(http::decode_for_test("%zz%4"), "%zz%4");
    assert_eq!(http::parse_head(b"GET / HTTP/1.1\r\nContent-Length: 999999").unwrap_err().status, 413);
    assert_eq!(http::parse_head(b"GET / HTTP/1.1\r\nTransfer-Encoding: chunked").unwrap_err().status, 411);

    let host = Host::default();
    let backing = fake(&["runtime"]);
    let key = host.key("p@m", Caller::Backend).unwrap();
    let mut request = http::parse_head(b"GET /v1/settings HTTP/1.1\r\nOrigin: https://evil.example").unwrap();
    request.key = Some(key);
    assert_eq!(host.handle(&backing, &request).status, 403);
    let written = String::from_utf8(http::write(&Response { status: 204, body: None })).unwrap();
    assert!(written.starts_with("HTTP/1.1 204 No Content\r\n") && written.ends_with("Content-Length: 0\r\n\r\n"));
}

#[test]
fn the_server_answers_over_loopback() {
    use std::io::{Read, Write};
    let host: &'static Host = Box::leak(Box::default());
    let backing: Arc<dyn Backing> = Arc::new(fake(&["runtime", "secrets"]));
    let key = host.key("p@m", Caller::Backend).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    tauri::async_runtime::spawn(async move {
        http::serve(tokio::net::TcpListener::from_std(listener).unwrap(), host, backing).await;
    });
    let ask = |raw: String| {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        let mut out = String::new();
        stream.read_to_string(&mut out).unwrap();
        out
    };
    let token = ask(format!("GET /v1/accounts/gh/token HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {key}\r\n\r\n"));
    assert!(token.starts_with("HTTP/1.1 200 OK\r\n"), "{token}");
    assert!(token.ends_with(r#"{"accessToken":"gho_personal","expiresAt":99}"#), "{token}");
    let body = r#"{"message":"hello"}"#;
    let log = ask(format!("POST /v1/log HTTP/1.1\r\nAuthorization: Bearer {key}\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
    assert!(log.starts_with("HTTP/1.1 204"), "{log}");
    let nokey = ask("GET /v1/settings HTTP/1.1\r\n\r\n".into());
    assert!(nokey.starts_with("HTTP/1.1 401") && nokey.contains("unauthorized"), "{nokey}");
}
