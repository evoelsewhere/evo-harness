//! Record Composio's real answers as redacted fixtures (docs/features/
//! connected-apps/tasks.md, T1). Not part of the normal run:
//!
//! `EVO_RECORD_KEYCHAIN=connected-apps@<identifier> bun run test:shell record_composio_shapes -- --ignored`
//!
//! The key is read from that keychain entry (the feature's own path) and only
//! sent to Composio. A throwaway Composio user is used; the sign-in link it
//! makes is removed again. Everything written is redacted: no key, user id,
//! email, account, session or auth config id.

use std::path::Path;

use evo_vault::KeyStore;
use serde_json::{json, Map, Value};

use super::composio::{Composio, BASE};

fn redact(value: &Value, key: &str) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                let lower = k.to_ascii_lowercase();
                let secret_key = lower == "id"
                    || lower == "nanoid"
                    || lower == "uuid"
                    || lower.ends_with("_id")
                    || lower.ends_with("id")
                        && lower.len() <= 12
                        && v.is_string()
                        && !lower.contains("valid")
                    || lower.contains("email")
                    || lower.contains("token")
                    || lower.contains("secret")
                    || lower == "user"
                    || lower == "alias";
                let v = if secret_key && v.is_string() {
                    json!("REDACTED")
                } else {
                    redact(v, key)
                };
                out.insert(k.clone(), v);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(|i| redact(i, key)).collect()),
        Value::String(s) => {
            let mut s = s.replace(key, "REDACTED_KEY");
            // Ids inside URLs and text: Composio's prefixes.
            for prefix in ["trs_", "ca_", "ac_", "evo_", "lk_", "ln_", "usr_", "u_"] {
                while let Some(at) = s.find(prefix) {
                    let end = s[at..]
                        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                        .map(|e| at + e)
                        .unwrap_or(s.len());
                    if end - at <= prefix.len() + 2 {
                        break;
                    }
                    s.replace_range(at..end, "REDACTED");
                }
            }
            // Email addresses.
            let words: Vec<String> = s
                .split_whitespace()
                .map(|w| {
                    if w.contains('@') && w.contains('.') {
                        "REDACTED@example.com".to_string()
                    } else {
                        w.to_string()
                    }
                })
                .collect();
            if s.contains('@') {
                s = words.join(" ");
            }
            Value::String(s)
        }
        other => other.clone(),
    }
}

/// Text parts of an MCP result parsed as JSON where they are JSON.
fn readable(result: &Value) -> Value {
    let mut out = result.clone();
    if let Some(parts) = out.get_mut("content").and_then(Value::as_array_mut) {
        for part in parts {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                    part["text"] = parsed;
                }
            }
        }
    }
    out
}

fn write(dir: &Path, name: &str, value: &Value, key: &str) {
    let text = serde_json::to_string_pretty(&redact(value, key)).unwrap();
    assert!(!text.contains(key), "{name} still holds the key");
    std::fs::write(dir.join(name), text + "\n").unwrap();
    eprintln!("wrote {name}");
}

/// Cut long lists to a few items, so fixtures stay small.
fn few(mut value: Value, n: usize) -> Value {
    for field in ["items", "tools"] {
        if let Some(list) = value.get_mut(field).and_then(Value::as_array_mut) {
            list.truncate(n);
        }
    }
    value
}

#[tokio::test]
#[ignore = "calls Composio with the person's key"]
async fn record_composio_shapes() {
    let account =
        std::env::var("EVO_RECORD_KEYCHAIN").expect("EVO_RECORD_KEYCHAIN names the keychain entry");
    let key = evo_vault::Keychain
        .get(&account)
        .unwrap()
        .expect("a key in that keychain entry");
    let client = Composio::new(BASE, &key).unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/connected_apps/testdata/live");
    std::fs::create_dir_all(&dir).unwrap();
    let user = format!("evo_record_{}", std::process::id());

    let session = client.create_session(&user, 10).await.expect("session");
    write(&dir, "session_create.json", &session, &key);
    let id = session["session_id"]
        .as_str()
        .or(session["id"].as_str())
        .expect("session id")
        .to_string();
    let url = session
        .pointer("/mcp/url")
        .and_then(Value::as_str)
        .expect("mcp url")
        .to_string();
    if let Ok(got) = client.get_session(&id).await {
        write(&dir, "session_get.json", &got, &key);
    }
    let toolkits = client
        .request(
            reqwest::Method::GET,
            &format!("/tool_router/session/{id}/toolkits?limit=3"),
            None,
        )
        .await;
    write(
        &dir,
        "session_toolkits.json",
        &toolkits.unwrap_or_else(|e| json!({"error": e.to_string()})),
        &key,
    );
    write(
        &dir,
        "toolkits.json",
        &few(
            client
                .toolkits(None)
                .await
                .map(|v| few(v, 3))
                .unwrap_or_else(|e| json!({"error": e.to_string()})),
            3,
        ),
        &key,
    );
    let accounts = client
        .request(reqwest::Method::GET, "/connected_accounts?limit=3", None)
        .await;
    write(
        &dir,
        "connected_accounts.json",
        &accounts.unwrap_or_else(|e| json!({"error": e.to_string()})),
        &key,
    );
    let configs = client
        .request(reqwest::Method::GET, "/auth_configs?limit=3", None)
        .await;
    write(
        &dir,
        "auth_configs.json",
        &configs.unwrap_or_else(|e| json!({"error": e.to_string()})),
        &key,
    );
    let tools = client
        .request(
            reqwest::Method::GET,
            "/tools?toolkit_slug=gmail&limit=3",
            None,
        )
        .await;
    write(
        &dir,
        "tools_gmail.json",
        &tools.unwrap_or_else(|e| json!({"error": e.to_string()})),
        &key,
    );

    // A sign-in link, removed again.
    match client.link(&id, "gmail", "record").await {
        Ok(link) => {
            write(&dir, "link.json", &link, &key);
            if let Some(account) = link.get("connected_account_id").and_then(Value::as_str) {
                let _ = client.delete_account(account).await;
            }
        }
        Err(e) => write(&dir, "link.json", &json!({"error": e.to_string()}), &key),
    }

    // The session's MCP server: its tools and two read-only calls.
    let mut headers = std::collections::BTreeMap::new();
    headers.insert("x-api-key".to_string(), key.clone());
    let transport = evo_mcp::config::Transport::Http { url, headers };
    let conn = evo_mcp::client::connect(&transport, None, std::sync::Arc::new(|_| {}), None)
        .await
        .expect("mcp connects");
    let list = conn.tools().await.expect("tools/list");
    write(&dir, "mcp_tools_list.json", &json!({ "tools": list }), &key);
    let mut search = Map::new();
    search.insert(
        "queries".into(),
        json!([{ "use_case": "fetch my recent emails" }]),
    );
    search.insert("session".into(), json!({ "generate_id": true }));
    let found = conn.call_tool("COMPOSIO_SEARCH_TOOLS", Some(search)).await;
    write(
        &dir,
        "mcp_search_tools.json",
        &found
            .map(|r| readable(&r))
            .unwrap_or_else(|e| json!({"error": e})),
        &key,
    );
    let mut schemas = Map::new();
    schemas.insert(
        "tool_slugs".into(),
        json!(["GMAIL_SEND_EMAIL", "GMAIL_FETCH_EMAILS"]),
    );
    let got = conn
        .call_tool("COMPOSIO_GET_TOOL_SCHEMAS", Some(schemas))
        .await;
    write(
        &dir,
        "mcp_get_tool_schemas.json",
        &got.map(|r| readable(&r))
            .unwrap_or_else(|e| json!({"error": e})),
        &key,
    );
    conn.close().await;

    for entry in std::fs::read_dir(&dir).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(!text.contains(&key));
        assert!(!text.contains(&user), "the user id is redacted");
    }
}

#[test]
fn redaction_leaves_no_ids_or_emails() {
    let value = json!({
        "id": "ca_abc123", "user_id": "evo_record_1", "alias": "work",
        "mcp": {"url": "https://backend.composio.dev/tool_router/trs_Xyz123abc/mcp"},
        "data": {"email": "me@example.org", "note": "sent by me@example.org with ak_SECRETKEY"},
        "status": "ACTIVE"
    });
    let out = redact(&value, "ak_SECRETKEY").to_string();
    for bad in [
        "ca_abc123",
        "evo_record_1",
        "trs_Xyz123abc",
        "me@example.org",
        "ak_SECRETKEY",
        "work",
    ] {
        assert!(!out.contains(bad), "{bad} in {out}");
    }
    assert!(out.contains("ACTIVE"));
}
