//! Official MCP servers and Zapier beside Composio (docs/features/
//! connected-apps/spec.md, FR-22 – FR-24, NFR-1, NFR-5; plan 3.9).
//!
//! Which are connected is kept in `app_settings` (`connectedApps.official`,
//! a list of ids; `connectedApps.zapier`, a flag). A pasted token lives only
//! in the keychain (`official:<id>`, `zapier`) and in the server's header
//! while the app runs. OAuth sign-ins are the pool's (`Pool::sign_in` by the
//! server's key). Every app server — Composio's session, the official
//! servers, Zapier — reaches the pool together ([`Service::app_servers`]).

use std::collections::BTreeMap;

use evo_mcp::{AppServer, ServerConfig};
use serde_json::{json, Value};

use super::official::{self, Auth, Official, ZAPIER};
use super::Service;

/// `app_settings` keys.
pub const OFFICIAL: &str = "connectedApps.official";
pub const ZAPIER_ON: &str = "connectedApps.zapier";

fn token_account(id: &str) -> String {
    format!("official:{id}")
}
const ZAPIER_ACCOUNT: &str = "zapier";

/// An official server's app server: its listed address only (NFR-5), and
/// its token as a header when it signs in by token.
pub fn official_server(o: &Official, token: Option<&str>) -> AppServer {
    let mut headers = BTreeMap::new();
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        let header = o
            .token_header
            .clone()
            .unwrap_or_else(|| "Authorization".into());
        let prefix = o.token_prefix.clone().unwrap_or_else(|| "Bearer ".into());
        headers.insert(header, format!("{prefix}{token}"));
    }
    AppServer {
        name: o.server_name(),
        config: ServerConfig {
            kind: Some("http".into()),
            url: Some(o.url.clone()),
            headers,
            ..Default::default()
        },
        defaults: BTreeMap::new(),
    }
}

/// Zapier's app server, with its connection token when there is one.
pub fn zapier_server(token: Option<&str>) -> AppServer {
    let mut headers = BTreeMap::new();
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        headers.insert("Authorization".to_string(), format!("Bearer {token}"));
    }
    AppServer {
        name: ZAPIER.into(),
        config: ServerConfig {
            kind: Some("http".into()),
            url: Some(official::zapier().url.clone()),
            headers,
            ..Default::default()
        },
        defaults: BTreeMap::new(),
    }
}

impl Service {
    /// The ids of the connected official servers.
    pub fn official_ids(&self) -> Vec<String> {
        self.settings
            .get(OFFICIAL)
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_str)
            .filter(|id| official::find(id).is_some())
            .map(str::to_string)
            .collect()
    }

    pub fn zapier_on(&self) -> bool {
        self.settings
            .get(ZAPIER_ON)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Connect official server `id`: a token sign-in needs `token`, kept in
    /// the keychain; connecting again replaces the earlier one (FR-23).
    pub async fn connect_official(
        &self,
        id: &str,
        token: Option<String>,
    ) -> Result<&'static Official, String> {
        let o = official::find(id).ok_or_else(|| format!("No official server \"{id}\"."))?;
        let token = token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        match (o.auth, token) {
            (Auth::Token, None) => {
                return Err(format!(
                    "{} signs in with a token: {}",
                    o.name,
                    o.token_hint.clone().unwrap_or_default()
                ))
            }
            (_, Some(token)) => {
                let account = token_account(id);
                self.blocking(move |keys| keys.set(&account, &token))
                    .await?;
            }
            (Auth::Oauth, None) => {}
        }
        let mut ids = self.official_ids();
        if !ids.iter().any(|x| x == id) {
            ids.push(id.to_string());
        }
        self.settings.set(OFFICIAL, json!(ids));
        Ok(o)
    }

    /// Forget official server `id` and its token.
    pub async fn disconnect_official(&self, id: &str) -> Result<(), String> {
        let ids: Vec<String> = self
            .official_ids()
            .into_iter()
            .filter(|x| x != id)
            .collect();
        self.settings.set(OFFICIAL, json!(ids));
        let account = token_account(id);
        self.blocking(move |keys| keys.delete(&account)).await
    }

    /// Connect Zapier, with a connection token when the person pastes one.
    pub async fn connect_zapier(&self, token: Option<String>) -> Result<(), String> {
        if let Some(token) = token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
        {
            self.blocking(move |keys| keys.set(ZAPIER_ACCOUNT, &token))
                .await?;
        }
        self.settings.set(ZAPIER_ON, json!(true));
        Ok(())
    }

    pub async fn disconnect_zapier(&self) -> Result<(), String> {
        self.settings.set(ZAPIER_ON, Value::Null);
        self.blocking(|keys| keys.delete(ZAPIER_ACCOUNT)).await
    }

    /// Every app server the pool should have now: Composio's session (with
    /// a key and a session), the connected official servers and Zapier.
    pub async fn app_servers(&self) -> Result<Vec<AppServer>, String> {
        let mut out = Vec::new();
        if let (Some(session), Some(key)) = (self.session_now(), self.lock().key.clone()) {
            out.push(super::app_server(&session, &key));
        }
        for id in self.official_ids() {
            let o = official::find(&id).expect("listed");
            let account = token_account(&id);
            let token = if o.auth == Auth::Token {
                self.blocking(move |keys| keys.get(&account)).await?
            } else {
                None
            };
            out.push(official_server(o, token.as_deref()));
        }
        if self.zapier_on() {
            let token = self.blocking(|keys| keys.get(ZAPIER_ACCOUNT)).await?;
            out.push(zapier_server(token.as_deref()));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::key::MemorySettings;
    use super::*;
    use evo_vault::KeyStore;
    use std::sync::Arc;

    fn service() -> (Service, Arc<evo_vault::MemoryKeys>, Arc<MemorySettings>) {
        let keys = Arc::new(evo_vault::MemoryKeys::default());
        let settings = Arc::new(MemorySettings::default());
        (
            Service::new(
                keys.clone(),
                settings.clone(),
                "http://127.0.0.1:1/api/v3.1",
            ),
            keys,
            settings,
        )
    }

    #[tokio::test]
    async fn an_official_token_is_only_a_header() {
        let (service, keys, settings) = service();
        let error = service.connect_official("github", None).await.unwrap_err();
        assert!(error.contains("personal access token"), "{error}");
        service
            .connect_official("github", Some(" ghp_SECRET_1 ".into()))
            .await
            .unwrap();
        service.connect_official("notion", None).await.unwrap();
        assert_eq!(
            keys.get("official:github").unwrap().as_deref(),
            Some("ghp_SECRET_1")
        );
        let stored = serde_json::to_string(&*settings.0.lock().unwrap()).unwrap();
        assert!(!stored.contains("ghp_SECRET_1"), "{stored}");
        let servers = service.app_servers().await.unwrap();
        assert_eq!(
            servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["official_github", "official_notion"],
            "no Composio without a key"
        );
        let github = &servers[0].config;
        assert_eq!(
            github.url.as_deref(),
            Some("https://api.githubcopilot.com/mcp/"),
            "only the listed address (NFR-5)"
        );
        assert_eq!(github.headers.len(), 1);
        assert_eq!(github.headers["Authorization"], "Bearer ghp_SECRET_1");
        assert!(
            servers[1].config.headers.is_empty(),
            "an OAuth server's token is the pool's sign-in, not a header"
        );
        // Zapier with a token; disconnecting forgets it.
        service
            .connect_zapier(Some("zap_TOKEN".into()))
            .await
            .unwrap();
        let servers = service.app_servers().await.unwrap();
        assert_eq!(
            servers.last().unwrap().config.headers["Authorization"],
            "Bearer zap_TOKEN"
        );
        service.disconnect_zapier().await.unwrap();
        service.disconnect_official("github").await.unwrap();
        assert_eq!(keys.get("official:github").unwrap(), None);
        assert_eq!(keys.get("zapier").unwrap(), None);
        assert_eq!(
            service
                .app_servers()
                .await
                .unwrap()
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["official_notion"]
        );
    }

    #[tokio::test]
    async fn connecting_again_replaces_the_sign_in() {
        let (service, keys, _settings) = service();
        service.connect_official("linear", None).await.unwrap();
        service.connect_official("linear", None).await.unwrap();
        assert_eq!(
            service.official_ids(),
            ["linear"],
            "one Linear server remains"
        );
        service
            .connect_official("github", Some("ghp_old".into()))
            .await
            .unwrap();
        service
            .connect_official("github", Some("ghp_new".into()))
            .await
            .unwrap();
        assert_eq!(
            keys.get("official:github").unwrap().as_deref(),
            Some("ghp_new")
        );
        let servers = service.app_servers().await.unwrap();
        assert_eq!(
            servers
                .iter()
                .filter(|s| s.name == "official_github")
                .count(),
            1
        );
        assert!(
            service.connect_official("asana", None).await.is_err(),
            "not on the list"
        );
    }
}
