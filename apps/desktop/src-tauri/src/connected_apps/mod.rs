//! Connected apps: hundreds of services through Composio with the person's
//! own project key (docs/features/connected-apps/spec.md; plan 3).
//!
//! * [`key`]: the key in the system keychain, the installation's Composio
//!   user and session ids in `app_settings` (FR-1, FR-18, NFR-1);
//! * [`composio`]: Composio's REST API;
//! * [`accounts`]: account status and label rules (FR-3 – FR-6);
//! * [`gate`]: the check every app call passes (FR-8 – FR-11, FR-13);
//! * [`files`]: local files handed to a tool (FR-14).
//!
//! [`Service`] holds the key, the one Tool Router session of the
//! installation and the connected accounts. With no key it makes no
//! request (NFR-4). With one, the session's MCP server joins the
//! connector pool as the app server `connected_apps`, its key in a header
//! that lives only in memory.

pub mod accounts;
pub mod composio;
#[cfg(test)]
pub mod fake;
pub mod files;
pub mod gate;
pub mod key;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use evo_mcp::{AppServer, ServerConfig};
use evo_vault::KeyStore;
use serde_json::{json, Value};

use accounts::Account;
use composio::{ApiError, Composio};
use key::Settings;

/// Accounts per service the session allows; Evo keeps to five (FR-5).
pub const SESSION_MAX_ACCOUNTS: usize = 10;

/// Whether the key is there and works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyState {
    None,
    Saved,
    Invalid,
}

/// Whether Composio can be used now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reach {
    /// No key: nothing to reach.
    Off,
    Ok,
    Unavailable,
}

/// The installation's Tool Router session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub user: String,
    pub mcp_url: String,
}

#[derive(Default)]
struct Inner {
    key: Option<String>,
    key_state: Option<KeyState>,
    reason: Option<String>,
    reach: Option<Reach>,
    session: Option<Session>,
    accounts: Vec<Account>,
}

/// The connected-apps service of the app.
pub struct Service {
    keys: Arc<dyn KeyStore>,
    settings: Arc<dyn Settings>,
    base: String,
    inner: Mutex<Inner>,
}

/// A new Composio user id for this installation.
fn new_user_id() -> String {
    use sha2::Digest;
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let seed = format!(
        "{}:{}:{}:{}",
        evo_vault::random_key().unwrap_or_default(),
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let digest = sha2::Sha256::digest(seed.as_bytes());
    format!(
        "evo_{}",
        digest
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

/// The session in a Composio answer.
fn session_of(answer: &Value, user: &str) -> Option<Session> {
    let id = answer
        .get("session_id")
        .or_else(|| answer.get("id"))
        .and_then(Value::as_str)?
        .to_string();
    let url = answer
        .pointer("/mcp/url")
        .and_then(Value::as_str)?
        .to_string();
    Some(Session {
        id,
        user: user.to_string(),
        mcp_url: url,
    })
}

impl Service {
    pub fn new(keys: Arc<dyn KeyStore>, settings: Arc<dyn Settings>, base: &str) -> Self {
        Self {
            keys,
            settings,
            base: base.to_string(),
            inner: Mutex::new(Inner::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn blocking<T: Send + 'static>(
        &self,
        act: impl FnOnce(&dyn KeyStore) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let keys = self.keys.clone();
        tokio::task::spawn_blocking(move || act(keys.as_ref()))
            .await
            .map_err(|e| e.to_string())?
    }

    /// The client for the saved key, if there is one.
    pub fn client(&self) -> Option<Composio> {
        let key = self.lock().key.clone()?;
        Composio::new(&self.base, &key).ok()
    }

    /// Read the key (no request when there is none: NFR-4) and set up the
    /// session. `Ok(None)`: no key.
    pub async fn start(&self) -> Result<Option<AppServer>, String> {
        let saved = self.blocking(key::load).await?;
        let Some(saved) = saved else {
            let mut inner = self.lock();
            *inner = Inner {
                key_state: Some(KeyState::None),
                reach: Some(Reach::Off),
                ..Inner::default()
            };
            return Ok(None);
        };
        self.lock().key = Some(saved.clone());
        self.connect(&saved).await.map(Some)
    }

    /// Check `key` with Composio, keep it in the keychain, and set up the
    /// session; a different key than before starts clean (FR-17).
    pub async fn set_key(&self, key: &str) -> Result<AppServer, String> {
        let key = key.trim().to_string();
        composio::check_format(&key)?;
        let client = Composio::new(&self.base, &key)?;
        client.check_key().await.map_err(|e| match e {
            ApiError::Refused(why) => format!("Composio refused this key ({why}). Check that it is a project key of a project you can use."),
            other => other.to_string(),
        })?;
        let saved = key.clone();
        self.blocking(move |keys| key::save(keys, &saved)).await?;
        self.lock().key = Some(key.clone());
        self.connect(&key).await
    }

    /// Forget the key and everything that came with it.
    pub async fn remove_key(&self) -> Result<(), String> {
        self.blocking(key::remove).await?;
        for name in [key::USER_ID, key::SESSION_ID, key::KEY_MARK] {
            self.settings.set(name, Value::Null);
        }
        let mut inner = self.lock();
        *inner = Inner {
            key_state: Some(KeyState::None),
            reach: Some(Reach::Off),
            ..Inner::default()
        };
        Ok(())
    }

    /// Get or make the session for `key`; the app server for the pool.
    async fn connect(&self, key: &str) -> Result<AppServer, String> {
        let mark = key::mark(key);
        if self.settings.as_ref().text(key::KEY_MARK).as_deref() != Some(mark.as_str()) {
            // Another project's key: its user and session are not ours.
            self.settings.set(key::USER_ID, Value::Null);
            self.settings.set(key::SESSION_ID, Value::Null);
            self.lock().accounts.clear();
        }
        let client = Composio::new(&self.base, key)?;
        let session = match self.session(&client).await {
            Ok(session) => session,
            Err(error) => {
                let mut inner = self.lock();
                let refused = matches!(error, ApiError::Refused(_));
                inner.key_state = Some(if refused {
                    KeyState::Invalid
                } else {
                    KeyState::Saved
                });
                inner.reach = Some(if refused {
                    Reach::Off
                } else {
                    Reach::Unavailable
                });
                inner.reason = Some(error.to_string());
                return Err(error.to_string());
            }
        };
        if !composio::mcp_url_ok(&session.mcp_url, &self.base) {
            return Err("Composio gave a tool address outside composio.dev; Evo does not send the key there.".into());
        }
        self.settings.set(key::KEY_MARK, json!(mark));
        self.settings.set(key::USER_ID, json!(session.user));
        self.settings.set(key::SESSION_ID, json!(session.id));
        let server = app_server(&session, key);
        let mut inner = self.lock();
        inner.key_state = Some(KeyState::Saved);
        inner.reach = Some(Reach::Ok);
        inner.reason = None;
        inner.session = Some(session);
        Ok(server)
    }

    /// The saved session if Composio still has it, else a new one for the
    /// saved user, the one earlier user of this project, or a new user.
    async fn session(&self, client: &Composio) -> Result<Session, ApiError> {
        let settings = self.settings.as_ref();
        let user = settings.text(key::USER_ID);
        if let (Some(id), Some(user)) = (settings.text(key::SESSION_ID), user.clone()) {
            match client.get_session(&id).await {
                Ok(answer) => {
                    let mut session = session_of(&answer, &user).unwrap_or(Session {
                        id: id.clone(),
                        user: user.clone(),
                        mcp_url: String::new(),
                    });
                    session.id = id;
                    if !session.mcp_url.is_empty() {
                        return Ok(session);
                    }
                }
                Err(ApiError::NotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let user = match user {
            Some(user) => user,
            None => earlier_user(&client.project_accounts().await?).unwrap_or_else(new_user_id),
        };
        let answer = client.create_session(&user, SESSION_MAX_ACCOUNTS).await?;
        session_of(&answer, &user).ok_or_else(|| ApiError::Failed {
            status: 200,
            message: "the new session has no tool address".into(),
        })
    }

    pub fn session_now(&self) -> Option<Session> {
        self.lock().session.clone()
    }

    pub fn key_state(&self) -> (KeyState, Reach, Option<String>) {
        let inner = self.lock();
        (
            inner.key_state.unwrap_or(KeyState::None),
            inner.reach.unwrap_or(Reach::Off),
            inner.reason.clone(),
        )
    }

    pub fn accounts_now(&self) -> Vec<Account> {
        self.lock().accounts.clone()
    }

    pub fn set_accounts(&self, accounts: Vec<Account>) {
        self.lock().accounts = accounts;
    }

    /// Mark Composio unreachable (or reachable again).
    pub fn set_reach(&self, reach: Reach, reason: Option<String>) {
        let mut inner = self.lock();
        inner.reach = Some(reach);
        inner.reason = reason;
    }
}

/// The user of a reinstall with the same key: the only `evo_` user of the
/// project with an active account (FR-18).
pub fn earlier_user(items: &[Value]) -> Option<String> {
    let mut users: Vec<String> = items
        .iter()
        .filter(|i| {
            i.get("status")
                .and_then(Value::as_str)
                .is_some_and(|s| s.eq_ignore_ascii_case("ACTIVE"))
        })
        .filter_map(|i| i.get("user_id").and_then(Value::as_str))
        .filter(|u| u.starts_with("evo_"))
        .map(str::to_string)
        .collect();
    users.sort();
    users.dedup();
    (users.len() == 1).then(|| users.remove(0))
}

/// The connector pool's server for `session`.
pub fn app_server(session: &Session, key: &str) -> AppServer {
    let mut headers = BTreeMap::new();
    headers.insert("x-api-key".to_string(), key.to_string());
    AppServer {
        name: gate::LABEL.into(),
        config: ServerConfig {
            kind: Some("http".into()),
            url: Some(session.mcp_url.clone()),
            headers,
            ..Default::default()
        },
        defaults: gate::default_permissions(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fake::Fake;
    use key::MemorySettings;

    const GOOD: &str = "ak_good_key_1";

    fn service(
        fake: &Fake,
        keys: &Arc<evo_vault::MemoryKeys>,
        settings: &Arc<MemorySettings>,
    ) -> Service {
        Service::new(keys.clone(), settings.clone(), &fake.base)
    }

    #[tokio::test]
    async fn no_key_no_request() {
        let fake = Fake::start(GOOD).await;
        let (keys, settings) = (
            Arc::new(evo_vault::MemoryKeys::default()),
            Arc::new(MemorySettings::default()),
        );
        let service = service(&fake, &keys, &settings);
        assert!(service.start().await.unwrap().is_none());
        assert_eq!(service.key_state(), (KeyState::None, Reach::Off, None));
        assert!(service.client().is_none());
        assert!(
            fake.seen().is_empty(),
            "no request without a key: {:?}",
            fake.seen()
        );
    }

    #[tokio::test]
    async fn a_refused_key_is_not_saved() {
        let fake = Fake::start(GOOD).await;
        let (keys, settings) = (
            Arc::new(evo_vault::MemoryKeys::default()),
            Arc::new(MemorySettings::default()),
        );
        let service = service(&fake, &keys, &settings);
        let error = service.set_key("ak_wrong_key").await.unwrap_err();
        assert!(error.contains("Composio refused this key"), "{error}");
        assert_eq!(key::load(keys.as_ref()).unwrap(), None);
        // Not a project key: refused before any request.
        let before = fake.seen().len();
        assert!(service
            .set_key("uak_user_key")
            .await
            .unwrap_err()
            .contains("not a Composio project key"));
        assert_eq!(fake.seen().len(), before);
        // A good one is kept, in the keychain only.
        let server = service.set_key(GOOD).await.unwrap();
        assert_eq!(key::load(keys.as_ref()).unwrap().as_deref(), Some(GOOD));
        assert_eq!(server.config.headers["x-api-key"], GOOD);
        assert!(server
            .config
            .url
            .as_deref()
            .unwrap()
            .ends_with("/tool_router/trs_fake/mcp"));
        let stored = serde_json::to_string(&*settings.0.lock().unwrap()).unwrap();
        assert!(!stored.contains(GOOD), "{stored}");
        assert!(fake.seen().iter().all(|s| s.key.as_deref() != Some("")));
    }

    #[tokio::test]
    async fn a_restart_keeps_the_session() {
        let fake = Fake::start(GOOD).await;
        let (keys, settings) = (
            Arc::new(evo_vault::MemoryKeys::default()),
            Arc::new(MemorySettings::default()),
        );
        let first = service(&fake, &keys, &settings)
            .set_key(GOOD)
            .await
            .unwrap();
        let creates = |f: &Fake| {
            f.seen()
                .iter()
                .filter(|s| s.method == "POST" && s.path == "/tool_router/session")
                .count()
        };
        assert_eq!(creates(&fake), 1);
        let user = settings.get(key::USER_ID).unwrap();
        let again = service(&fake, &keys, &settings)
            .start()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(creates(&fake), 1, "the saved session is reused");
        assert_eq!(settings.get(key::USER_ID).unwrap(), user);
        assert_eq!(again.config.headers, first.config.headers);
        // A session Composio forgot: a new one, same user.
        settings.set(key::SESSION_ID, json!("trs_gone"));
        service(&fake, &keys, &settings)
            .start()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(creates(&fake), 2);
        let body = fake
            .seen()
            .iter()
            .rev()
            .find(|s| s.method == "POST")
            .unwrap()
            .body
            .clone();
        assert_eq!(body["user_id"], user);
        assert_eq!(body["multi_account"]["require_explicit_selection"], true);
    }

    #[tokio::test]
    async fn a_new_key_starts_clean() {
        let fake = Fake::start(GOOD).await;
        let (keys, settings) = (
            Arc::new(evo_vault::MemoryKeys::default()),
            Arc::new(MemorySettings::default()),
        );
        let service = service(&fake, &keys, &settings);
        service.set_key(GOOD).await.unwrap();
        let first_user = settings.get(key::USER_ID).unwrap();
        service.set_accounts(vec![Account {
            id: "ca_1".into(),
            toolkit: "gmail".into(),
            label: "personal".into(),
            status: accounts::Status::Active,
            connected_at: String::new(),
        }]);
        *fake.key.lock().unwrap() = "ak_other_project".into();
        service.set_key("ak_other_project").await.unwrap();
        assert_ne!(
            settings.get(key::USER_ID).unwrap(),
            first_user,
            "another project's key gets a new user"
        );
        assert!(service.accounts_now().is_empty());
        // Removing the key forgets the ids too.
        service.remove_key().await.unwrap();
        assert!(settings.get(key::USER_ID).is_none() && settings.get(key::SESSION_ID).is_none());
        assert_eq!(key::load(keys.as_ref()).unwrap(), None);
        assert_eq!(service.key_state().0, KeyState::None);
    }

    #[tokio::test]
    async fn a_reinstall_finds_the_one_earlier_user() {
        let fake = Fake::start(GOOD).await;
        fake.route("GET", "/connected_accounts", |_| {
            (200, json!({"items": [
                {"id": "ca_1", "user_id": "evo_earlier", "status": "ACTIVE", "toolkit": {"slug": "gmail"}},
                {"id": "ca_2", "user_id": "someone-else", "status": "ACTIVE", "toolkit": {"slug": "gmail"}},
                {"id": "ca_3", "user_id": "evo_dead", "status": "EXPIRED", "toolkit": {"slug": "gmail"}}
            ], "next_cursor": null}))
        });
        let (keys, settings) = (
            Arc::new(evo_vault::MemoryKeys::default()),
            Arc::new(MemorySettings::default()),
        );
        service(&fake, &keys, &settings)
            .set_key(GOOD)
            .await
            .unwrap();
        assert_eq!(settings.get(key::USER_ID).unwrap(), "evo_earlier");
        assert_eq!(
            earlier_user(&[
                json!({"user_id": "evo_a", "status": "ACTIVE"}),
                json!({"user_id": "evo_b", "status": "ACTIVE"})
            ]),
            None,
            "two: neither"
        );
    }

    #[tokio::test]
    async fn composio_down_is_unavailable_not_an_error_state_of_the_key() {
        let (keys, settings) = (
            Arc::new(evo_vault::MemoryKeys::default()),
            Arc::new(MemorySettings::default()),
        );
        key::save(keys.as_ref(), GOOD).unwrap();
        let service = Service::new(
            keys.clone(),
            settings.clone(),
            "http://127.0.0.1:1/api/v3.1",
        );
        let error = service.start().await.unwrap_err();
        assert!(error.contains("could not be reached"), "{error}");
        let (state, reach, _) = service.key_state();
        assert_eq!((state, reach), (KeyState::Saved, Reach::Unavailable));
    }
}
