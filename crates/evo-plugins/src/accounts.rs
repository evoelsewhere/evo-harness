//! The accounts a plugin signs in to (`accounts` in its manifest;
//! docs/features/plugin-settings/spec.md, FR-20 – FR-24). This crate reads
//! and checks them; the shell signs in and keeps the sessions (ADR 0022).

use serde_json::Value;

use crate::settings::{Kind, Setting};

/// OAuth endpoints and client (also used by device-code sign-in).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth {
    /// An issuer to discover the endpoints from (OpenID Connect).
    pub issuer: Option<String>,
    pub authorize_url: Option<String>,
    pub token_url: Option<String>,
    pub device_url: Option<String>,
    pub revoke_url: Option<String>,
    /// Where the account's name is read after sign-in (OpenID `userinfo`, or
    /// a service's own, such as GitHub's `/user`).
    pub userinfo_url: Option<String>,
    /// The plugin's own client.
    pub client_id: Option<String>,
    /// Or settings the person fills in with their own app's client.
    pub client_id_setting: Option<String>,
    pub client_secret_setting: Option<String>,
    pub scopes: Vec<String>,
    /// A fixed loopback port, for services that need an exact redirect.
    pub redirect_port: Option<u16>,
}

/// A web login: where it starts, whose cookies are kept, and when it is done.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Web {
    pub login_url: String,
    pub domains: Vec<String>,
    pub done_cookie: Option<String>,
    pub done_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Method {
    Oauth(OAuth),
    Device(OAuth),
    /// An API key or token kept as a secret setting; `test` is a backend
    /// tool that checks it (FR-23).
    Token {
        setting: String,
        test: Option<String>,
    },
    Web(Web),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    /// The service's name, as the person reads it.
    pub service: String,
    pub method: Method,
    pub reason: Option<String>,
    /// The plugin works without it.
    pub optional: bool,
}

/// An https URL, or http on a loopback host (local services, tests).
pub fn acceptable_url(raw: &str) -> bool {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let name = if host.starts_with('[') {
        host.split(']').next().map(|h| &h[1..]).unwrap_or_default()
    } else {
        host.split(':').next().unwrap_or_default()
    };
    if name.is_empty() {
        return false;
    }
    match scheme {
        "https" => true,
        "http" => matches!(name, "127.0.0.1" | "localhost" | "::1"),
        _ => false,
    }
}

fn valid_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !domain.starts_with(['.', '-'])
        && !domain.ends_with(['.', '-'])
}

fn secret_setting(settings: &[Setting], key: &str) -> bool {
    settings
        .iter()
        .any(|s| s.key == key && s.kind == Kind::Secret)
}

fn parse_oauth(
    id: &str,
    value: Option<&Value>,
    settings: &[Setting],
    device: bool,
) -> Result<OAuth, String> {
    let object = value
        .and_then(Value::as_object)
        .ok_or_else(|| format!("accounts.{id}: needs an `oauth` object"))?;
    let text = |name: &str| {
        object
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let url = |name: &str| -> Result<Option<String>, String> {
        match text(name) {
            Some(raw) if !acceptable_url(&raw) => {
                Err(format!("accounts.{id}: `{name}` must be an https address"))
            }
            other => Ok(other),
        }
    };
    let oauth = OAuth {
        issuer: url("issuer")?,
        authorize_url: url("authorizeUrl")?,
        token_url: url("tokenUrl")?,
        device_url: url("deviceUrl")?,
        revoke_url: url("revokeUrl")?,
        userinfo_url: url("userinfoUrl")?,
        client_id: text("clientId"),
        client_id_setting: text("clientIdSetting"),
        client_secret_setting: text("clientSecretSetting"),
        scopes: object
            .get("scopes")
            .and_then(Value::as_array)
            .map(|s| {
                s.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        redirect_port: object
            .get("redirectPort")
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .filter(|p| *p >= 1024),
    };
    if oauth.client_id.is_none() && oauth.client_id_setting.is_none() {
        return Err(format!(
            "accounts.{id}: needs `clientId` or `clientIdSetting`"
        ));
    }
    for setting in [&oauth.client_id_setting, &oauth.client_secret_setting]
        .into_iter()
        .flatten()
    {
        if !settings.iter().any(|s| &s.key == setting) {
            return Err(format!(
                "accounts.{id}: the setting `{setting}` does not exist"
            ));
        }
    }
    if let Some(secret) = &oauth.client_secret_setting {
        if !secret_setting(settings, secret) {
            return Err(format!(
                "accounts.{id}: `clientSecretSetting` must name a secret setting"
            ));
        }
    }
    let endpoints = if device {
        oauth.issuer.is_some() || (oauth.device_url.is_some() && oauth.token_url.is_some())
    } else {
        oauth.issuer.is_some() || (oauth.authorize_url.is_some() && oauth.token_url.is_some())
    };
    if !endpoints {
        let needed = if device {
            "`deviceUrl` and `tokenUrl`"
        } else {
            "`authorizeUrl` and `tokenUrl`"
        };
        return Err(format!("accounts.{id}: needs `issuer`, or {needed}"));
    }
    Ok(oauth)
}

fn parse_one(item: &Value, settings: &[Setting], seen: &[Account]) -> Result<Account, String> {
    let object = item.as_object().ok_or("accounts: expected objects")?;
    let text = |name: &str| {
        object
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let id = text("id").ok_or("accounts: an account has no id")?;
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        || id.len() > 64
    {
        return Err(format!(
            "accounts: the id `{id}` may use only a-z, 0-9, - and _"
        ));
    }
    if seen.iter().any(|a| a.id == id) {
        return Err(format!("accounts: the id `{id}` is used twice"));
    }
    let service = text("service").ok_or_else(|| format!("accounts.{id}: no `service`"))?;
    let kind = text("kind").ok_or_else(|| format!("accounts.{id}: no `kind`"))?;
    let method = match kind.as_str() {
        "oauth" => Method::Oauth(parse_oauth(&id, object.get("oauth"), settings, false)?),
        "device" => Method::Device(parse_oauth(&id, object.get("oauth"), settings, true)?),
        "token" => {
            let setting = text("setting").ok_or_else(|| {
                format!("accounts.{id}: a token account names its secret `setting`")
            })?;
            if !secret_setting(settings, &setting) {
                return Err(format!(
                    "accounts.{id}: `setting` must name a secret setting"
                ));
            }
            Method::Token {
                setting,
                test: text("test"),
            }
        }
        "web" => {
            let web = object
                .get("web")
                .and_then(Value::as_object)
                .ok_or_else(|| format!("accounts.{id}: needs a `web` object"))?;
            let field = |name: &str| {
                web.get(name)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            let login_url = field("loginUrl")
                .filter(|u| acceptable_url(u))
                .ok_or_else(|| format!("accounts.{id}: `web.loginUrl` must be an https address"))?;
            let domains: Vec<String> = web
                .get("domains")
                .and_then(Value::as_array)
                .map(|d| {
                    d.iter()
                        .filter_map(Value::as_str)
                        .map(|s| s.trim().to_ascii_lowercase())
                        .collect()
                })
                .unwrap_or_default();
            if domains.is_empty() || !domains.iter().all(|d| valid_domain(d)) {
                return Err(format!(
                    "accounts.{id}: `web.domains` must list the site's domains"
                ));
            }
            let done = web.get("done").and_then(Value::as_object);
            let done_cookie = done
                .and_then(|d| d.get("cookie"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let done_url = done
                .and_then(|d| d.get("url"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if done_cookie.is_none() && done_url.is_none() {
                return Err(format!(
                    "accounts.{id}: `web.done` needs a `cookie` or a `url`"
                ));
            }
            Method::Web(Web {
                login_url,
                domains,
                done_cookie,
                done_url,
            })
        }
        other => {
            return Err(format!(
                "accounts.{id}: unknown kind `{other}` (oauth, device, token or web)"
            ))
        }
    };
    Ok(Account {
        id,
        service,
        method,
        reason: text("reason"),
        optional: object
            .get("optional")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Read `accounts`: each checked against the plugin's `settings`. A wrong
/// account is left out, with the reason.
pub fn parse_accounts(value: Option<&Value>, settings: &[Setting]) -> (Vec<Account>, Vec<String>) {
    let mut accounts = Vec::new();
    let mut problems = Vec::new();
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return (accounts, problems);
    };
    let Some(items) = value.as_array() else {
        problems.push("accounts: expected a list".into());
        return (accounts, problems);
    };
    for item in items {
        match parse_one(item, settings, &accounts) {
            Ok(account) => accounts.push(account),
            Err(problem) => problems.push(problem),
        }
    }
    (accounts, problems)
}

#[cfg(test)]
mod tests;
