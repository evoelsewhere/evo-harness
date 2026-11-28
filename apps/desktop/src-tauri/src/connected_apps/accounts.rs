//! Connected accounts: their status, and the rules for labels
//! (docs/features/connected-apps/spec.md, FR-3 – FR-6; plan 3.2).
//!
//! Every account of a service is its own row with a label the person
//! chose (Composio's `alias`). A label is unique within the service and
//! required once the service has an active account; the first is
//! "personal" when none is given. A label held only by accounts that are
//! not active can be taken again: those accounts are replaced. A service
//! holds at most [`MAX_ACCOUNTS`] usable (active or signing-in) accounts.

use serde::Serialize;
use serde_json::Value;

/// Usable accounts per service (spec A-1).
pub const MAX_ACCOUNTS: usize = 5;
/// Longest label.
pub const MAX_LABEL: usize = 32;
/// The first account's label when none is given.
pub const FIRST_LABEL: &str = "personal";

/// An account's state as the page shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Active,
    /// Signing in: a link was made and not finished yet.
    Pending,
    /// Not usable until the person signs in again.
    NeedsSignIn,
    Expired,
    Failed,
}

impl Status {
    /// Composio's `status` of a connected account.
    pub fn from_composio(status: &str) -> Self {
        match status.to_ascii_uppercase().as_str() {
            "ACTIVE" => Status::Active,
            "INITIATED" | "INITIALIZING" => Status::Pending,
            "EXPIRED" => Status::Expired,
            "FAILED" => Status::Failed,
            _ => Status::NeedsSignIn,
        }
    }

    /// Active or signing in: counts against [`MAX_ACCOUNTS`].
    pub fn usable(self) -> bool {
        matches!(self, Status::Active | Status::Pending)
    }
}

/// One connected account.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// Composio's connected account id (`ca_…`).
    pub id: String,
    /// The service (Composio's toolkit slug, lower case: `gmail`).
    pub toolkit: String,
    pub label: String,
    pub status: Status,
    /// When it was made, as Composio gives it (ISO 8601).
    pub connected_at: String,
}

impl Account {
    /// One item of Composio's `GET /connected_accounts` list; `None` when it
    /// has no id or toolkit.
    pub fn from_composio(item: &Value) -> Option<Self> {
        let text = |pointer: &str| {
            item.pointer(pointer)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let id = text("/id").or_else(|| text("/nanoid"))?;
        let toolkit = text("/toolkit/slug")
            .or_else(|| text("/toolkit_slug"))?
            .to_ascii_lowercase();
        let label = text("/alias")
            .or_else(|| text("/data/alias"))
            .filter(|l| !l.trim().is_empty())
            .unwrap_or_else(|| FIRST_LABEL.to_string());
        Some(Self {
            id,
            toolkit,
            label,
            status: Status::from_composio(&text("/status").unwrap_or_default()),
            connected_at: text("/created_at").unwrap_or_default(),
        })
    }
}

/// What a Connect with a label does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelPlan {
    /// The label to sign in under.
    pub label: String,
    /// Accounts that hold the label and are not active: removed first.
    pub replaces: Vec<String>,
}

fn same_label(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Check a new account of `toolkit` labelled `label` against `accounts`
/// (every account, of any service). `Err`: why not, in the person's words.
pub fn plan_label(
    accounts: &[Account],
    toolkit: &str,
    label: Option<&str>,
    service: &str,
) -> Result<LabelPlan, String> {
    let mine: Vec<&Account> = accounts
        .iter()
        .filter(|a| a.toolkit.eq_ignore_ascii_case(toolkit))
        .collect();
    let any_active = mine.iter().any(|a| a.status == Status::Active);
    let label = label.map(str::trim).filter(|l| !l.is_empty());
    let label = match label {
        Some(label) => label.to_string(),
        None if any_active => {
            return Err(format!(
                "{service} already has an account: give this one a label, such as \"work\"."
            ))
        }
        None => FIRST_LABEL.to_string(),
    };
    if label.chars().count() > MAX_LABEL {
        return Err(format!("A label has at most {MAX_LABEL} characters."));
    }
    if label.chars().any(char::is_control) {
        return Err("A label cannot hold control characters.".into());
    }
    let holders: Vec<&&Account> = mine
        .iter()
        .filter(|a| same_label(&a.label, &label))
        .collect();
    if holders.iter().any(|a| a.status == Status::Active) {
        return Err(format!("{service} already has an account labelled \"{label}\": choose another label, or Reconnect that one."));
    }
    let replaces: Vec<String> = holders.iter().map(|a| a.id.clone()).collect();
    let usable = mine
        .iter()
        .filter(|a| a.status.usable() && !replaces.contains(&a.id))
        .count();
    if usable >= MAX_ACCOUNTS {
        return Err(format!(
            "{service} already has {MAX_ACCOUNTS} accounts: disconnect one first."
        ));
    }
    Ok(LabelPlan { label, replaces })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn account(id: &str, toolkit: &str, label: &str, status: Status) -> Account {
        Account {
            id: id.into(),
            toolkit: toolkit.into(),
            label: label.into(),
            status,
            connected_at: String::new(),
        }
    }

    #[test]
    fn label_rules_first_account_is_personal_and_later_ones_need_a_label() {
        let plan = plan_label(&[], "gmail", None, "Gmail").unwrap();
        assert_eq!(
            plan,
            LabelPlan {
                label: "personal".into(),
                replaces: vec![]
            }
        );
        let accounts = [account("ca_1", "gmail", "personal", Status::Active)];
        let error = plan_label(&accounts, "gmail", None, "Gmail").unwrap_err();
        assert!(error.contains("give this one a label"), "{error}");
        assert_eq!(
            plan_label(&accounts, "gmail", Some(" work "), "Gmail")
                .unwrap()
                .label,
            "work"
        );
        // Another service's accounts do not count.
        assert_eq!(
            plan_label(&accounts, "slack", None, "Slack").unwrap().label,
            "personal"
        );
    }

    #[test]
    fn label_rules_labels_are_unique_within_a_service() {
        let accounts = [account("ca_1", "gmail", "Work", Status::Active)];
        let error = plan_label(&accounts, "gmail", Some("work"), "Gmail").unwrap_err();
        assert!(
            error.contains("already has an account labelled \"work\""),
            "{error}"
        );
        assert!(plan_label(&accounts, "slack", Some("work"), "Slack").is_ok());
        assert!(plan_label(&accounts, "gmail", Some(&"x".repeat(33)), "Gmail").is_err());
    }

    #[test]
    fn label_rules_a_dead_label_is_reused_by_replacing_its_account() {
        let accounts = [
            account("ca_1", "gmail", "personal", Status::Active),
            account("ca_2", "gmail", "work", Status::Expired),
            account("ca_3", "gmail", "WORK", Status::Pending),
        ];
        let plan = plan_label(&accounts, "gmail", Some("work"), "Gmail").unwrap();
        assert_eq!(
            plan,
            LabelPlan {
                label: "work".into(),
                replaces: vec!["ca_2".into(), "ca_3".into()]
            }
        );
    }

    #[test]
    fn label_rules_five_usable_accounts_at_most() {
        let mut accounts: Vec<Account> = (0..5)
            .map(|n| {
                account(
                    &format!("ca_{n}"),
                    "gmail",
                    &format!("a{n}"),
                    Status::Active,
                )
            })
            .collect();
        let error = plan_label(&accounts, "gmail", Some("sixth"), "Gmail").unwrap_err();
        assert!(error.contains("already has 5 accounts"), "{error}");
        // A failed one does not count; a pending one does.
        accounts[4].status = Status::Failed;
        assert!(plan_label(&accounts, "gmail", Some("sixth"), "Gmail").is_ok());
        accounts[4].status = Status::Pending;
        assert!(plan_label(&accounts, "gmail", Some("sixth"), "Gmail").is_err());
        // Replacing the pending one under its own label frees its place.
        assert_eq!(
            plan_label(&accounts, "gmail", Some("a4"), "Gmail")
                .unwrap()
                .replaces,
            ["ca_4"]
        );
    }

    #[test]
    fn accounts_read_from_composio_items() {
        let item = json!({"id": "ca_x", "toolkit": {"slug": "GMAIL"}, "alias": "work", "status": "EXPIRED", "created_at": "2026-11-28T10:00:00Z"});
        let account = Account::from_composio(&item).unwrap();
        assert_eq!(account.toolkit, "gmail");
        assert_eq!(account.label, "work");
        assert_eq!(account.status, Status::Expired);
        assert_eq!(Status::from_composio("INITIATED"), Status::Pending);
        assert_eq!(Status::from_composio("INACTIVE"), Status::NeedsSignIn);
        assert!(Account::from_composio(&json!({"status": "ACTIVE"})).is_none());
    }
}
