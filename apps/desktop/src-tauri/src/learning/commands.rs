//! What the windows ask of the learning layers
//! (docs/features/learning-layers/spec.md, FR-6, FR-14, FR-15, FR-18, FR-19;
//! plan 3.3).

use serde_json::{json, Value};
use tauri::{AppHandle, State};

use super::jobs::Learning;

/// The owner ids a window may name.
fn valid_owner(owner: &str) -> bool {
    let safe = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') && part != "..";
    owner == "personal" || owner.strip_prefix("project:").is_some_and(safe) || owner.strip_prefix("mate:").is_some_and(safe)
}

/// **Tidy now** for each of `owners`: the results, in order. Without a usable
/// model only expiry and duplicates run, and a result says so (FR-24).
#[tauri::command]
pub async fn learning_tidy_now(app: AppHandle, owners: Vec<String>) -> Result<Vec<Value>, String> {
    let mut results = Vec::new();
    for owner in owners {
        if !valid_owner(&owner) {
            return Err(format!("unknown memory owner `{owner}`"));
        }
        let result = super::jobs::run_step(&app, "tidy", &owner, Vec::new()).await;
        results.push(match result {
            Ok(result) => result,
            Err(error) => json!({ "owner": owner, "error": error }),
        });
    }
    Ok(results)
}

/// The dream's ideas still waiting for an answer, for these owners (FR-14).
#[tauri::command]
pub fn learning_suggestions(db: State<'_, crate::db::Db>, owners: Vec<String>) -> Result<Vec<evo_store::learning::Suggestion>, String> {
    let owners: Vec<&str> = owners.iter().map(String::as_str).filter(|owner| valid_owner(owner)).collect();
    db.with(|conn| evo_store::learning::pending(conn, &owners))
}

/// **Set it up** (accepted) or **No thanks**: answered once; a declined idea
/// is never offered again (FR-14).
#[tauri::command]
pub fn learning_suggestion_answer(app: AppHandle, db: State<'_, crate::db::Db>, id: i64, accepted: bool) -> Result<bool, String> {
    use tauri::Emitter;
    let answered = db.with(|conn| evo_store::learning::answer(conn, id, accepted))?;
    let _ = app.emit("learning://changed", json!({ "suggestion": id }));
    Ok(answered)
}

/// A learning setting changed: look again now.
#[tauri::command]
pub fn learning_poke(learning: State<'_, Learning>) {
    learning.wake.notify_one();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_owner_ids_are_accepted() {
        for owner in ["personal", "project:C--work-app", "mate:m_1"] {
            assert!(valid_owner(owner), "{owner}");
        }
        for owner in ["", "project:", "mate:../x", "mate:a/b", "other", "mate:a b"] {
            assert!(!valid_owner(owner), "{owner}");
        }
    }
}
