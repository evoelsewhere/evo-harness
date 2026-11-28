//! Connecting, reconnecting and disconnecting accounts
//! (docs/features/connected-apps/spec.md, FR-4 – FR-6, FR-16; plan 3.5).
//!
//! Connect checks the label ([`accounts::plan_label`]), removes the dead
//! accounts that held it, asks Composio for a sign-in link under that
//! label and keeps it as a pending sign-in: the page can open it again
//! without making another account while it lives ([`LINK_LIFE`]); after
//! that, Open makes a new link. The app polls the accounts until the new
//! one is active. A service Composio has no managed sign-in for uses the
//! person's newest enabled auth config in their project, else the steps
//! to make one.

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

use super::accounts::{self, Account, Status};
use super::composio::{ApiError, Composio};
use super::{Service, Session};

/// How long a sign-in link lives (spec A-1).
pub const LINK_LIFE: Duration = Duration::from_secs(10 * 60);
/// How often a pending sign-in is checked.
pub const POLL_EVERY: Duration = Duration::from_secs(2);

/// A sign-in under way.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub id: String,
    pub toolkit: String,
    pub label: String,
    #[serde(skip)]
    pub url: String,
    /// The account Composio made for it, when it said.
    pub account: Option<String>,
    #[serde(skip)]
    pub made: Option<Instant>,
}

impl Pending {
    pub fn alive(&self) -> bool {
        self.made.is_some_and(|m| m.elapsed() < LINK_LIFE)
    }
}

/// The sign-in URL in a link answer.
fn url_of(answer: &Value) -> Option<String> {
    [
        "/redirect_url",
        "/redirectUrl",
        "/link_url",
        "/url",
        "/data/redirect_url",
    ]
    .iter()
    .find_map(|p| answer.pointer(p).and_then(Value::as_str))
    .map(str::to_string)
}

/// The steps for a service with no sign-in Composio provides (FR-16).
pub fn own_auth_steps(service: &str, toolkit: &str) -> String {
    format!(
        "{service} has no sign-in that Composio provides: it needs your own OAuth app. In Composio, open your project → Auth Configs → Create auth config → {service} ({toolkit}), enter your app's client ID and secret, and turn it on for Tool Router. Then Connect again."
    )
}

/// Whether a link refusal means the service needs the person's own auth config.
fn needs_own_auth(error: &ApiError) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("auth config")
        || text.contains("auth_config")
        || text.contains("managed auth")
        || text.contains("no default")
}

/// The id of the newest enabled auth config among Composio's `items`.
fn newest_enabled(items: &[Value]) -> Option<String> {
    items
        .iter()
        .filter(|c| {
            c.get("status")
                .and_then(Value::as_str)
                .is_none_or(|s| s.eq_ignore_ascii_case("ENABLED"))
        })
        .max_by_key(|c| {
            c.get("created_at")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
        .and_then(|c| c.get("id").and_then(Value::as_str))
        .map(str::to_string)
}

impl Service {
    fn ready(&self) -> Result<(Composio, Session), String> {
        let client = self.client().ok_or("No Composio key is saved.")?;
        let session = self
            .session_now()
            .ok_or("Connected apps is not ready: Composio could not be reached.")?;
        Ok((client, session))
    }

    /// Read the installation's accounts from Composio.
    pub async fn refresh_accounts(&self) -> Result<Vec<Account>, String> {
        let (client, session) = self.ready()?;
        let items = client
            .accounts(&session.user)
            .await
            .map_err(|e| e.to_string())?;
        let mut accounts: Vec<Account> = items.iter().filter_map(Account::from_composio).collect();
        accounts.sort_by(|a, b| {
            a.toolkit
                .cmp(&b.toolkit)
                .then(a.connected_at.cmp(&b.connected_at))
        });
        self.set_accounts(accounts.clone());
        Ok(accounts)
    }

    /// Start a sign-in to `toolkit` under `label` (`reconnect`: the
    /// account to sign in again, under its own label). `managed`: Composio
    /// provides the service's sign-in.
    pub async fn begin_connect(
        &self,
        toolkit: &str,
        label: Option<&str>,
        service: &str,
        managed: bool,
        reconnect: Option<&str>,
    ) -> Result<Pending, String> {
        let toolkit = toolkit.trim().to_ascii_lowercase();
        let (client, session) = self.ready()?;
        let accounts = self.refresh_accounts().await?;
        let plan = match reconnect {
            Some(id) => {
                let old = accounts
                    .iter()
                    .find(|a| a.id == id)
                    .ok_or("That account is no longer connected.")?;
                let others: Vec<Account> =
                    accounts.iter().filter(|a| a.id != id).cloned().collect();
                let mut plan =
                    accounts::plan_label(&others, &old.toolkit, Some(&old.label), service)?;
                plan.replaces.insert(0, id.to_string());
                plan
            }
            None => accounts::plan_label(&accounts, &toolkit, label, service)?,
        };
        for id in &plan.replaces {
            client.delete_account(id).await.map_err(|e| e.to_string())?;
        }
        if !managed {
            self.use_own_auth(&client, &session, &toolkit, service)
                .await?;
        }
        let answer = match client.link(&session.id, &toolkit, &plan.label).await {
            Ok(answer) => answer,
            Err(error) if managed && needs_own_auth(&error) => {
                self.use_own_auth(&client, &session, &toolkit, service)
                    .await?;
                client
                    .link(&session.id, &toolkit, &plan.label)
                    .await
                    .map_err(|e| e.to_string())?
            }
            Err(error) => return Err(error.to_string()),
        };
        let url = url_of(&answer).ok_or("Composio gave no sign-in address.")?;
        if !url.starts_with("https://") {
            return Err("Composio gave a sign-in address that is not HTTPS.".into());
        }
        let pending = Pending {
            id: super::super::mates::new_id("signin"),
            toolkit,
            label: plan.label,
            url,
            account: answer
                .get("connected_account_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            made: Some(Instant::now()),
        };
        self.lock()
            .pending
            .insert(pending.id.clone(), pending.clone());
        if !plan.replaces.is_empty() {
            let _ = self.refresh_accounts().await;
        }
        Ok(pending)
    }

    /// Use the person's newest enabled auth config for `toolkit` in the
    /// session (FR-16); `Err` holds the steps when there is none.
    async fn use_own_auth(
        &self,
        client: &Composio,
        session: &Session,
        toolkit: &str,
        service: &str,
    ) -> Result<(), String> {
        let configs = client
            .auth_configs(toolkit)
            .await
            .map_err(|e| e.to_string())?;
        let id = newest_enabled(&configs).ok_or_else(|| own_auth_steps(service, toolkit))?;
        client
            .patch_session(&session.id, json!({ "auth_configs": { toolkit: id } }))
            .await
            .map_err(|e| {
                format!(
                    "{service}: your auth config could not be used ({e}). {}",
                    own_auth_steps(service, toolkit)
                )
            })?;
        Ok(())
    }

    /// The pending sign-in's link to open again: the same while it lives,
    /// else a new one under the same label.
    pub async fn reopen(
        &self,
        pending: &str,
        service: &str,
        managed: bool,
    ) -> Result<Pending, String> {
        let found = self
            .lock()
            .pending
            .get(pending)
            .cloned()
            .ok_or("That sign-in is no longer pending: Connect again.")?;
        if found.alive() {
            return Ok(found);
        }
        self.lock().pending.remove(pending);
        let fresh = self
            .begin_connect(&found.toolkit, Some(&found.label), service, managed, None)
            .await?;
        Ok(fresh)
    }

    pub fn pending_now(&self) -> Vec<Pending> {
        self.lock().pending.values().cloned().collect()
    }

    /// Check a pending sign-in once: its account when it is active (and
    /// the pending sign-in is done), `None` while it waits. A link past its
    /// life is dropped.
    pub async fn poll(&self, pending: &str) -> Result<Option<Account>, String> {
        let Some(found) = self.lock().pending.get(pending).cloned() else {
            return Ok(None);
        };
        let accounts = self.refresh_accounts().await?;
        let done = accounts.into_iter().find(|a| {
            a.status == Status::Active
                && (found.account.as_deref() == Some(a.id.as_str())
                    || (a.toolkit == found.toolkit && a.label.eq_ignore_ascii_case(&found.label)))
        });
        if done.is_some() || !found.alive() {
            self.lock().pending.remove(pending);
        }
        Ok(done)
    }

    /// Disconnect one account and revoke it at the service (FR-6).
    pub async fn disconnect(&self, account: &str) -> Result<(), String> {
        let (client, _) = self.ready()?;
        client
            .delete_account(account)
            .await
            .map_err(|e| e.to_string())?;
        self.refresh_accounts().await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::super::fake::Fake;
    use super::super::key::MemorySettings;
    use super::*;
    use std::sync::{Arc, Mutex};

    const KEY: &str = "ak_good_key_1";

    /// A fake whose accounts list is `accounts` and whose links make a
    /// pending account there.
    async fn world() -> (Fake, Service, Arc<Mutex<Vec<Value>>>) {
        let fake = Fake::start(KEY).await;
        let accounts: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let list = accounts.clone();
        fake.route("GET", "/connected_accounts", move |_| {
            (
                200,
                json!({"items": list.lock().unwrap().clone(), "next_cursor": null}),
            )
        });
        let made = accounts.clone();
        fake.route("POST", "/tool_router/session/trs_fake/link", move |seen| {
            let mut all = made.lock().unwrap();
            let n = all.len() + 1;
            let id = format!("ca_{n}");
            all.push(json!({"id": id, "user_id": "u", "status": "INITIATED", "alias": seen.body["alias"], "toolkit": {"slug": seen.body["toolkit"]}, "created_at": format!("2026-11-28T10:0{n}:00Z")}));
            (201, json!({"redirect_url": format!("https://connect.composio.dev/link/lk_{id}"), "connected_account_id": id}))
        });
        let gone = accounts.clone();
        fake.route("DELETE", "/connected_accounts/", move |seen| {
            let id = seen
                .path
                .trim_start_matches("/connected_accounts/")
                .split('?')
                .next()
                .unwrap()
                .to_string();
            gone.lock().unwrap().retain(|a| a["id"] != id);
            (200, json!({"success": true}))
        });
        let service = Service::new(
            Arc::new(evo_vault::MemoryKeys::default()),
            Arc::new(MemorySettings::default()),
            &fake.base,
        );
        service.set_key(KEY).await.unwrap();
        (fake, service, accounts)
    }

    fn activate(accounts: &Arc<Mutex<Vec<Value>>>, id: &str) {
        for a in accounts.lock().unwrap().iter_mut() {
            if a["id"] == id {
                a["status"] = json!("ACTIVE");
            }
        }
    }

    #[tokio::test]
    async fn two_labelled_gmail_accounts_are_two_rows() {
        let (_fake, service, accounts) = world().await;
        let first = service
            .begin_connect("gmail", None, "Gmail", true, None)
            .await
            .unwrap();
        assert_eq!(first.label, "personal");
        assert!(first.url.starts_with("https://connect.composio.dev/"));
        assert_eq!(
            service.poll(&first.id).await.unwrap(),
            None,
            "waits while signing in"
        );
        activate(&accounts, "ca_1");
        let done = service.poll(&first.id).await.unwrap().unwrap();
        assert_eq!(
            (done.label.as_str(), done.status),
            ("personal", Status::Active)
        );
        assert!(service.pending_now().is_empty());
        let error = service
            .begin_connect("gmail", None, "Gmail", true, None)
            .await
            .unwrap_err();
        assert!(error.contains("give this one a label"), "{error}");
        let work = service
            .begin_connect("gmail", Some("work"), "Gmail", true, None)
            .await
            .unwrap();
        activate(&accounts, "ca_2");
        service.poll(&work.id).await.unwrap().unwrap();
        let rows = service.accounts_now();
        assert_eq!(
            rows.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(),
            ["personal", "work"]
        );
    }

    #[tokio::test]
    async fn open_again_reuses_the_pending_sign_in() {
        let (fake, service, _accounts) = world().await;
        let pending = service
            .begin_connect("gmail", None, "Gmail", true, None)
            .await
            .unwrap();
        let links = |f: &Fake| {
            f.seen()
                .iter()
                .filter(|s| s.path.ends_with("/link"))
                .count()
        };
        let again = service.reopen(&pending.id, "Gmail", true).await.unwrap();
        assert_eq!(
            (again.id.as_str(), again.url.as_str()),
            (pending.id.as_str(), pending.url.as_str())
        );
        assert_eq!(links(&fake), 1, "no second account while the link lives");
        // Past its life: a new link under the same label.
        service.lock().pending.get_mut(&pending.id).unwrap().made =
            Some(Instant::now() - LINK_LIFE - Duration::from_secs(1));
        let fresh = service.reopen(&pending.id, "Gmail", true).await.unwrap();
        assert_eq!(fresh.label, "personal");
        assert_ne!(fresh.id, pending.id);
        assert_eq!(links(&fake), 2);
    }

    #[tokio::test]
    async fn disconnect_revokes_one_and_reconnect_keeps_the_label() {
        let (fake, service, accounts) = world().await;
        let p = service
            .begin_connect("gmail", None, "Gmail", true, None)
            .await
            .unwrap();
        activate(&accounts, "ca_1");
        service.poll(&p.id).await.unwrap();
        let w = service
            .begin_connect("gmail", Some("work"), "Gmail", true, None)
            .await
            .unwrap();
        activate(&accounts, "ca_2");
        service.poll(&w.id).await.unwrap();
        service.disconnect("ca_1").await.unwrap();
        let delete = fake
            .seen()
            .into_iter()
            .find(|s| s.method == "DELETE")
            .unwrap();
        assert_eq!(
            delete.path,
            "/connected_accounts/ca_1?revoke_on_delete=true"
        );
        assert_eq!(
            service
                .accounts_now()
                .iter()
                .map(|a| a.label.as_str())
                .collect::<Vec<_>>(),
            ["work"],
            "the other account stays"
        );
        // Expired "work": Reconnect signs in again under "work", one row.
        accounts.lock().unwrap()[0]["status"] = json!("EXPIRED");
        let again = service
            .begin_connect("gmail", None, "Gmail", true, Some("ca_2"))
            .await
            .unwrap();
        assert_eq!(again.label, "work");
        let labels: Vec<String> = service
            .accounts_now()
            .iter()
            .map(|a| a.label.clone())
            .collect();
        assert_eq!(labels, ["work"], "the expired one was replaced");
        // An expired label connected again: one row.
        accounts
            .lock()
            .unwrap()
            .iter_mut()
            .for_each(|a| a["status"] = json!("EXPIRED"));
        service
            .begin_connect("gmail", Some("work"), "Gmail", true, None)
            .await
            .unwrap();
        assert_eq!(
            service
                .accounts_now()
                .iter()
                .filter(|a| a.label == "work")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn a_toolkit_without_managed_auth_uses_the_persons_config() {
        let (fake, service, _accounts) = world().await;
        fake.route("GET", "/auth_configs", |seen| {
            if seen.path.contains("toolkit_slug=acme") {
                (
                    200,
                    json!({"items": [
                    {"id": "ac_old", "status": "ENABLED", "created_at": "2026-01-01T00:00:00Z"},
                    {"id": "ac_new", "status": "ENABLED", "created_at": "2026-06-01T00:00:00Z"},
                    {"id": "ac_off", "status": "DISABLED", "created_at": "2026-09-01T00:00:00Z"}
                ], "next_cursor": null}),
                )
            } else {
                (200, json!({"items": [], "next_cursor": null}))
            }
        });
        fake.route("PATCH", "/tool_router/session/trs_fake", |_| {
            (200, json!({}))
        });
        service
            .begin_connect("acme", None, "Acme", false, None)
            .await
            .unwrap();
        let patch = fake
            .seen()
            .into_iter()
            .find(|s| s.method == "PATCH")
            .unwrap();
        assert_eq!(patch.body, json!({"auth_configs": {"acme": "ac_new"}}));
        // None of the person's: the steps.
        let error = service
            .begin_connect("other", None, "Other", false, None)
            .await
            .unwrap_err();
        assert!(
            error.contains("needs your own OAuth app") && error.contains("Auth Configs"),
            "{error}"
        );
    }
}
