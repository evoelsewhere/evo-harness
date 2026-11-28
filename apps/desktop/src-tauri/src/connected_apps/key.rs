//! Where the Composio project key and the installation's Composio ids live
//! (docs/features/connected-apps/spec.md, FR-1, FR-18, NFR-1; plan 3.4).
//!
//! The key is in the system keychain (`evo-vault`'s [`KeyStore`], account
//! [`KEY_ACCOUNT`]); the database holds only the non-secret user and
//! session ids (`app_settings`). Keychain calls may block: callers run
//! them off the async runtime.

use std::collections::HashMap;
use std::sync::Mutex;

use evo_vault::KeyStore;
use serde_json::Value;

/// The keychain account of the key.
pub const KEY_ACCOUNT: &str = "connected-apps";
/// `app_settings` keys.
pub const USER_ID: &str = "connectedApps.userId";
pub const SESSION_ID: &str = "connectedApps.sessionId";
/// A non-secret mark of the key the ids belong to (a hash prefix), so a
/// different key starts clean.
pub const KEY_MARK: &str = "connectedApps.keyMark";

pub fn load(keys: &dyn KeyStore) -> Result<Option<String>, String> {
    Ok(keys
        .get(KEY_ACCOUNT)?
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty()))
}

pub fn save(keys: &dyn KeyStore, key: &str) -> Result<(), String> {
    keys.set(KEY_ACCOUNT, key.trim())
}

pub fn remove(keys: &dyn KeyStore) -> Result<(), String> {
    keys.delete(KEY_ACCOUNT)
}

/// The mark of `key`: enough to tell two keys apart, nothing to use.
pub fn mark(key: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(key.trim().as_bytes());
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

/// The app settings the service reads and writes.
pub trait Settings: Send + Sync {
    fn get(&self, key: &str) -> Option<Value>;
    fn set(&self, key: &str, value: Value);
}

impl dyn Settings + '_ {
    pub fn text(&self, key: &str) -> Option<String> {
        self.get(key)
            .and_then(|v| v.as_str().map(str::to_string))
            .filter(|s| !s.is_empty())
    }
}

/// Settings in memory, for tests.
#[derive(Default)]
pub struct MemorySettings(pub Mutex<HashMap<String, Value>>);

impl Settings for MemorySettings {
    fn get(&self, key: &str) -> Option<Value> {
        self.0.lock().unwrap().get(key).cloned()
    }
    fn set(&self, key: &str, value: Value) {
        let mut all = self.0.lock().unwrap();
        if value.is_null() {
            all.remove(key);
        } else {
            all.insert(key.to_string(), value);
        }
    }
}

/// The app database's `app_settings`.
pub struct DbSettings(pub tauri::AppHandle);

impl Settings for DbSettings {
    fn get(&self, key: &str) -> Option<Value> {
        use tauri::Manager;
        let db = self.0.try_state::<crate::db::Db>()?;
        db.with(|conn| evo_store::settings::get(conn, key))
            .ok()
            .flatten()
    }
    fn set(&self, key: &str, value: Value) {
        use tauri::Manager;
        if let Some(db) = self.0.try_state::<crate::db::Db>() {
            let _ = db.with(|conn| evo_store::settings::set(conn, key, &value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_lives_in_the_keychain_account() {
        let keys = evo_vault::MemoryKeys::default();
        assert_eq!(load(&keys).unwrap(), None);
        save(&keys, " ak_test123 ").unwrap();
        assert_eq!(
            keys.get(KEY_ACCOUNT).unwrap().as_deref(),
            Some("ak_test123")
        );
        assert_eq!(load(&keys).unwrap().as_deref(), Some("ak_test123"));
        remove(&keys).unwrap();
        assert_eq!(load(&keys).unwrap(), None);
        assert_eq!(mark("ak_abcdef1234"), mark(" ak_abcdef1234"));
        assert_ne!(mark("ak_abcdef1234"), mark("ak_abcdef1235"));
        assert!(!mark("ak_abcdef1234").contains("1234"));
    }
}
