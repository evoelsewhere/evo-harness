//! A mate's secrets (docs/features/evo-mates/spec.md, FR-43): values the
//! person types into a mate's secure card, kept in a vault of the mate's own
//! (`evo-vault`: a key in the OS keychain sealing one file in the mate's
//! folder). The value never enters the transcript or a provider request:
//!
//! - the card's value goes from the window straight here
//!   ([`mates_secret_submit`]), not through the engine;
//! - the mate writes `{{secret:NAME}}` in a connector tool's arguments and
//!   the shell puts the value in only inside that call ([`resolve`]);
//! - a value a tool echoes back is replaced by `[secret NAME]` ([`mask`]).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};

use crate::db::Db;

const OPEN: &str = "{{secret:";
const CLOSE: &str = "}}";

/// A secret's name: letters, digits, `_` and `-`, upper case, at most 64.
pub fn clean_name(name: &str) -> Option<String> {
    let name: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_matches('_').to_string();
    (!name.is_empty() && name.len() <= 64).then_some(name)
}

/// The names `{{secret:NAME}}` refers to in `text`.
pub fn references(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(OPEN) {
        let after = &rest[at + OPEN.len()..];
        match after.find(CLOSE) {
            Some(end) => {
                if let Some(name) = clean_name(&after[..end]) {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
                rest = &after[end + CLOSE.len()..];
            }
            None => break,
        }
    }
    names
}

/// `args` with every `{{secret:NAME}}` in its strings replaced by the value
/// `lookup` gives. `Err`: the names it does not have.
pub fn resolve(
    args: &Value,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Value, Vec<String>> {
    let mut missing = Vec::new();
    let resolved = walk(args, &mut |text| {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find(OPEN) {
            out.push_str(&rest[..at]);
            let after = &rest[at + OPEN.len()..];
            let Some(end) = after.find(CLOSE) else {
                out.push_str(&rest[at..]);
                rest = "";
                break;
            };
            let name = clean_name(&after[..end]).unwrap_or_default();
            match lookup(&name) {
                Some(value) => out.push_str(&value),
                None => {
                    if !missing.contains(&name) {
                        missing.push(name);
                    }
                }
            }
            rest = &after[end + CLOSE.len()..];
        }
        out.push_str(rest);
        out
    });
    if missing.is_empty() {
        Ok(resolved)
    } else {
        Err(missing)
    }
}

fn walk(value: &Value, change: &mut dyn FnMut(&str) -> String) -> Value {
    match value {
        Value::String(text) if text.contains(OPEN) => Value::String(change(text)),
        Value::Array(items) => Value::Array(items.iter().map(|item| walk(item, change)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), walk(v, change)))
                .collect::<Map<_, _>>(),
        ),
        other => other.clone(),
    }
}

/// `text` with each of `secrets`' values (name, value) shown as `[secret NAME]`.
/// Values shorter than four characters are left: they would mask ordinary text.
pub fn mask(text: &str, secrets: &[(String, String)]) -> String {
    let mut out = text.to_string();
    let mut sorted: Vec<&(String, String)> = secrets
        .iter()
        .filter(|(_, v)| v.chars().count() >= 4)
        .collect();
    // Longer first: a value inside another is not cut in half.
    sorted.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    for (name, value) in sorted {
        out = out.replace(value.as_str(), &format!("[secret {name}]"));
    }
    out
}

// ── The vault ───────────────────────────────────────────────────────────────

/// The vault's keychain account: the mate's id (already `mate-…`).
fn vault_id(mate: &str) -> String {
    if mate.starts_with("mate-") {
        mate.to_string()
    } else {
        format!("mate-{mate}")
    }
}

fn with_vault<T>(
    mate: &str,
    act: impl FnOnce(&evo_vault::Vault) -> Result<T, String>,
) -> Result<T, String> {
    let dir = super::mate_dir(mate);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let vault = evo_vault::Vault::new(&evo_vault::Keychain, &dir, &vault_id(mate));
    act(&vault)
}

/// Every secret of `mate`, by name. Call off the async runtime (the keychain may block).
pub fn read_all(mate: &str) -> Result<Vec<(String, String)>, String> {
    with_vault(mate, |vault| {
        Ok(vault
            .read()?
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|v| (k, v.to_string())))
            .collect())
    })
}

pub fn save(mate: &str, name: &str, value: &str) -> Result<(), String> {
    with_vault(mate, |vault| {
        vault.set(name, Value::String(value.to_string()))
    })
}

/// Remove a mate's whole vault (the mate was deleted).
pub fn forget_all(mate: &str) -> Result<(), String> {
    with_vault(mate, |vault| vault.clear())
}

// ── The card's value, on its way to the tool ────────────────────────────────

fn staged() -> &'static Mutex<HashMap<(String, String), String>> {
    static STAGED: OnceLock<Mutex<HashMap<(String, String), String>>> = OnceLock::new();
    STAGED.get_or_init(Default::default)
}

/// The value the person typed into `chat`'s card for secret `name`, taken once.
pub fn take_staged(chat: &str, name: &str) -> Option<String> {
    staged()
        .lock()
        .expect("secrets lock")
        .remove(&(chat.to_string(), name.to_string()))
}

/// The secure card's value: kept for the waiting `request_secret` call, which
/// saves it once the card is answered. Never logged.
#[tauri::command]
pub fn mates_secret_submit(chat: String, name: String, value: String) -> Result<(), String> {
    let name = clean_name(&name).ok_or("That name cannot be a secret's name.")?;
    if value.is_empty() {
        return Err("Type the value first.".into());
    }
    staged()
        .lock()
        .expect("secrets lock")
        .insert((chat, name), value);
    Ok(())
}

fn mate_id_of(app: &AppHandle, id: &str) -> Result<String, String> {
    app.state::<Db>()
        .with(|conn| evo_store::mates::get_mate(conn, id))?
        .map(|mate| mate.id)
        .ok_or_else(|| "No such mate.".to_string())
}

/// The names of a mate's secrets, never their values.
#[tauri::command]
pub async fn mates_secrets(app: AppHandle, id: String) -> Result<Vec<String>, String> {
    let mate = mate_id_of(&app, &id)?;
    let mut names: Vec<String> = tauri::async_runtime::spawn_blocking(move || read_all(&mate))
        .await
        .map_err(|e| e.to_string())??
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    names.sort();
    Ok(names)
}

#[tauri::command]
pub async fn mates_delete_secret(app: AppHandle, id: String, name: String) -> Result<(), String> {
    let mate = mate_id_of(&app, &id)?;
    tauri::async_runtime::spawn_blocking(move || with_vault(&mate, |vault| vault.remove(&name)))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_are_plain() {
        assert_eq!(
            clean_name(" github token ").as_deref(),
            Some("GITHUB_TOKEN")
        );
        assert_eq!(clean_name("api-key").as_deref(), Some("API-KEY"));
        assert_eq!(clean_name("  "), None);
    }

    #[test]
    fn references_resolve_only_inside_the_call() {
        let args = json!({ "auth": "Bearer {{secret:token}}", "list": ["{{secret:TOKEN}}", 3], "plain": "no secret" });
        assert_eq!(references(&args.to_string()), vec!["TOKEN".to_string()]);
        let lookup = |name: &str| (name == "TOKEN").then(|| "s3cr3t-value".to_string());
        let resolved = resolve(&args, &lookup).unwrap();
        assert_eq!(
            resolved,
            json!({ "auth": "Bearer s3cr3t-value", "list": ["s3cr3t-value", 3], "plain": "no secret" })
        );
        // The arguments the transcript keeps still hold the reference only.
        assert!(!args.to_string().contains("s3cr3t-value"));
        assert_eq!(
            resolve(&json!({ "a": "{{secret:OTHER}}" }), &lookup),
            Err(vec!["OTHER".to_string()])
        );
    }

    #[test]
    fn echoed_values_are_masked() {
        let secrets = vec![
            ("TOKEN".to_string(), "s3cr3t-value".to_string()),
            ("PIN".to_string(), "12".to_string()),
        ];
        assert_eq!(
            mask("you sent s3cr3t-value; 12 items", &secrets),
            "you sent [secret TOKEN]; 12 items"
        );
    }
}
