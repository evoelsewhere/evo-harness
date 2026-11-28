//! The official MCP servers Evo offers, and Zapier's (docs/features/
//! connected-apps/spec.md, FR-20, FR-21, FR-24, A-8; plan 3.9).
//!
//! `official.json` ships with Evo: for each service its MCP address and how
//! it signs in, checked against the service's own documentation (`docs`).
//! An official server talks only to that address (NFR-5).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// How an official server signs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    /// The service's own OAuth in the browser.
    Oauth,
    /// A key or token the service issues, pasted by the person.
    Token,
}

/// One official server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Official {
    pub id: String,
    pub name: String,
    pub category: String,
    pub url: String,
    pub auth: Auth,
    /// The header a token goes in (`Authorization`).
    #[serde(default)]
    pub token_header: Option<String>,
    /// What goes before the token in it (`Bearer `).
    #[serde(default)]
    pub token_prefix: Option<String>,
    /// Where to get the token, for the token dialog.
    #[serde(default)]
    pub token_hint: Option<String>,
    pub docs: String,
    #[serde(default)]
    pub preview: bool,
    /// The Composio toolkits of the same service.
    #[serde(default)]
    pub composio: Vec<String>,
}

impl Official {
    /// The app server's name (`official_github`) and pool key.
    pub fn server_name(&self) -> String {
        format!("official_{}", self.id)
    }
}

/// Zapier's MCP server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Zapier {
    pub url: String,
    pub docs: String,
    /// Where the person chooses the actions their server offers.
    pub actions: String,
    /// Zapier tasks one successful call uses.
    pub tasks_per_call: u32,
}

#[derive(Debug, Deserialize)]
struct List {
    servers: Vec<Official>,
    zapier: Zapier,
}

fn list() -> &'static List {
    static LIST: OnceLock<List> = OnceLock::new();
    LIST.get_or_init(|| serde_json::from_str(include_str!("official.json")).expect("official.json is valid"))
}

/// Every official server, in the list's order.
pub fn all() -> &'static [Official] {
    &list().servers
}

pub fn find(id: &str) -> Option<&'static Official> {
    all().iter().find(|o| o.id == id)
}

/// The official server for a Composio toolkit, if the service has one.
pub fn for_toolkit(toolkit: &str) -> Option<&'static Official> {
    all().iter().find(|o| o.composio.iter().any(|t| t.eq_ignore_ascii_case(toolkit)))
}

pub fn zapier() -> &'static Zapier {
    &list().zapier
}

/// The Zapier app server's name.
pub const ZAPIER: &str = "zapier";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_servers_are_https_and_unique() {
        let mut ids = std::collections::HashSet::new();
        for o in all() {
            assert!(ids.insert(&o.id), "duplicate id {}", o.id);
            assert!(o.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()), "id {}", o.id);
            for url in [&o.url, &o.docs] {
                let parsed = reqwest::Url::parse(url).unwrap_or_else(|e| panic!("{}: {url}: {e}", o.id));
                assert_eq!(parsed.scheme(), "https", "{}: {url}", o.id);
            }
            if o.auth == Auth::Token {
                assert!(o.token_header.is_some() && o.token_hint.is_some(), "{} needs a header and a hint", o.id);
            }
        }
        assert!(all().len() >= 6);
        for id in ["github", "notion", "linear", "atlassian", "sentry", "stripe"] {
            assert!(find(id).is_some(), "{id}");
        }
        assert!(zapier().url.starts_with("https://mcp.zapier.com/"));
        assert_eq!(for_toolkit("jira").map(|o| o.id.as_str()), Some("atlassian"));
        assert_eq!(for_toolkit("gmail"), None);
        assert_eq!(find("github").unwrap().server_name(), "official_github");
    }
}
