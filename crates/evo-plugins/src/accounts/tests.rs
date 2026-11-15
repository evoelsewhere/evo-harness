// docs/features/plugin-settings/spec.md (FR-20 – FR-24; T5).
use super::*;
use crate::settings::parse_settings;
use serde_json::json;

fn settings() -> Vec<Setting> {
    parse_settings(
        Some(&json!({
            "apiToken": { "kind": "secret" },
            "clientId": { "kind": "text" },
            "clientSecret": { "kind": "secret" },
            "plain": { "kind": "text" }
        })),
        None,
    )
    .0
}

#[test]
fn every_sign_in_kind_is_read() {
    let (accounts, problems) = parse_accounts(
        Some(&json!([
            { "id": "github", "service": "GitHub", "kind": "oauth", "reason": "Read your issues",
              "oauth": { "authorizeUrl": "https://github.com/login/oauth/authorize", "tokenUrl": "https://github.com/login/oauth/access_token",
                         "clientId": "Iv1.abc", "scopes": ["repo"], "redirectPort": 8765 } },
            { "id": "ms", "service": "Microsoft", "kind": "device", "optional": true,
              "oauth": { "issuer": "https://login.example.com/tenant/v2.0", "clientIdSetting": "clientId", "clientSecretSetting": "clientSecret" } },
            { "id": "key", "service": "Weather", "kind": "token", "setting": "apiToken", "test": "check_key" },
            { "id": "portal", "service": "Portal", "kind": "web",
              "web": { "loginUrl": "https://portal.example.com/login", "domains": ["Portal.Example.com"], "done": { "cookie": "SESSION" } } }
        ])),
        &settings(),
    );
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(accounts.len(), 4);
    match &accounts[0].method {
        Method::Oauth(o) => {
            assert_eq!(o.scopes, ["repo"]);
            assert_eq!(o.redirect_port, Some(8765));
        }
        other => panic!("{other:?}"),
    }
    assert!(accounts[1].optional);
    assert!(
        matches!(&accounts[2].method, Method::Token { setting, test } if setting == "apiToken" && test.as_deref() == Some("check_key"))
    );
    match &accounts[3].method {
        Method::Web(w) => assert_eq!(w.domains, ["portal.example.com"], "domains are lower-cased"),
        other => panic!("{other:?}"),
    }
    assert_eq!(accounts[0].reason.as_deref(), Some("Read your issues"));
}

#[test]
fn wrong_accounts_are_left_out_with_a_reason() {
    let (accounts, problems) = parse_accounts(
        Some(&json!([
            { "id": "a", "service": "A", "kind": "oauth", "oauth": { "authorizeUrl": "http://evil.example.com/a", "tokenUrl": "https://x/t", "clientId": "c" } },
            { "id": "b", "service": "B", "kind": "oauth", "oauth": { "tokenUrl": "https://x/t", "clientId": "c" } },
            { "id": "c", "service": "C", "kind": "oauth", "oauth": { "issuer": "https://x" } },
            { "id": "d", "service": "D", "kind": "token", "setting": "plain" },
            { "id": "e", "service": "E", "kind": "web", "web": { "loginUrl": "https://x/login", "domains": ["x"] } },
            { "id": "f", "service": "F", "kind": "device", "oauth": { "authorizeUrl": "https://x/a", "tokenUrl": "https://x/t", "clientId": "c" } },
            { "id": "g", "service": "G", "kind": "oauth", "oauth": { "issuer": "https://x", "clientIdSetting": "nope" } },
            { "id": "h", "service": "H", "kind": "oauth", "oauth": { "issuer": "https://x", "clientId": "c", "clientSecretSetting": "plain" } },
            { "id": "i", "service": "I", "kind": "magic" },
            { "id": "Bad", "service": "x", "kind": "token", "setting": "apiToken" },
            { "id": "k", "service": "K", "kind": "token", "setting": "apiToken" },
            { "id": "k", "service": "K2", "kind": "token", "setting": "apiToken" },
            { "id": "w", "service": "W", "kind": "web", "web": { "loginUrl": "https://x/login", "domains": ["../x"], "done": { "url": "https://x/home" } } }
        ])),
        &settings(),
    );
    assert_eq!(
        accounts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        ["k"]
    );
    let text = problems.join("\n");
    for expected in [
        "accounts.a: `authorizeUrl`",
        "accounts.b: needs `issuer`",
        "accounts.c: needs `clientId`",
        "accounts.d: `setting` must name a secret",
        "accounts.e: `web.done`",
        "accounts.f: needs `issuer`, or `deviceUrl`",
        "`nope` does not exist",
        "accounts.h: `clientSecretSetting`",
        "unknown kind `magic`",
        "`Bad`",
        "used twice",
        "accounts.w: `web.domains`",
    ] {
        assert!(
            text.contains(expected),
            "no problem mentions {expected:?}:\n{text}"
        );
    }
}

#[test]
fn only_https_or_loopback_http_addresses() {
    for ok in [
        "https://x.com/a",
        "http://127.0.0.1:8781/login",
        "http://localhost/cb",
        "http://[::1]:9/x",
    ] {
        assert!(acceptable_url(ok), "{ok}");
    }
    for bad in [
        "http://x.com/",
        "ftp://x",
        "javascript:alert(1)",
        "https://",
        "http://127.0.0.1.evil.com/",
        "http://user@evil.com/",
    ] {
        assert!(!acceptable_url(bad), "{bad}");
    }
}
