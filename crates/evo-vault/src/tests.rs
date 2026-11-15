// docs/features/plugin-settings/spec.md (FR-5, NFR-1; T1, T8).
use super::*;
use serde_json::json;

#[test]
fn sealed_values_round_trip_and_never_show_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let keys = MemoryKeys::default();
    let vault = Vault::new(&keys, dir.path(), "notes@acme");
    assert!(vault.read().unwrap().is_empty(), "nothing stored yet");
    vault.set("apiToken", json!("tok_SECRET_123")).unwrap();
    vault
        .set(
            "cookies",
            json!("SESSION=".to_string() + &"x".repeat(64 * 1024)),
        )
        .unwrap();
    assert_eq!(
        vault.get("apiToken").unwrap(),
        Some(json!("tok_SECRET_123"))
    );
    let raw = std::fs::read(vault.file()).unwrap();
    assert!(!String::from_utf8_lossy(&raw).contains("tok_SECRET_123"));
    assert!(
        keys.get("notes@acme").unwrap().unwrap().len() < 100,
        "the keychain entry stays small"
    );
    vault.remove("apiToken").unwrap();
    assert_eq!(vault.get("apiToken").unwrap(), None);
}

#[test]
fn a_vault_does_not_open_with_another_key_or_when_damaged() {
    let dir = tempfile::tempdir().unwrap();
    let keys = MemoryKeys::default();
    let vault = Vault::new(&keys, dir.path(), "a@x");
    vault.set("k", json!(1)).unwrap();
    // Another plugin's key cannot open it.
    let other = MemoryKeys::default();
    let stolen = Vault::new(&other, dir.path(), "a@x");
    other.set("a@x", &B64.encode([7u8; 32])).unwrap();
    assert!(stolen.read().is_err());
    // A flipped byte is refused.
    let mut raw = std::fs::read(vault.file()).unwrap();
    let last = raw.len() - 1;
    raw[last] ^= 1;
    std::fs::write(vault.file(), raw).unwrap();
    assert!(vault.read().is_err());
}

#[test]
fn a_vault_whose_key_is_gone_says_so_and_clear_removes_both() {
    let dir = tempfile::tempdir().unwrap();
    let keys = MemoryKeys::default();
    let vault = Vault::new(&keys, dir.path(), "a@x");
    vault.set("k", json!(1)).unwrap();
    keys.delete("a@x").unwrap();
    assert!(vault.read().unwrap_err().contains("gone"));
    vault.set("k", json!(2)).ok();
    vault.clear().unwrap();
    assert!(!vault.file().exists());
    assert_eq!(keys.get("a@x").unwrap(), None);
}

/// Against the real keychain: `cargo test -p evo-vault -- --ignored --nocapture`.
#[test]
#[ignore = "touches the system keychain"]
fn the_system_keychain_holds_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = format!("evo-vault-spike-{}@local", std::process::id());
    let vault = Vault::new(&Keychain, dir.path(), &plugin);
    let started = std::time::Instant::now();
    vault.set("apiToken", json!("tok_SECRET_123")).unwrap();
    vault
        .set(
            "cookies",
            json!("SESSION=".to_string() + &"y".repeat(64 * 1024)),
        )
        .unwrap();
    assert_eq!(
        vault.get("apiToken").unwrap(),
        Some(json!("tok_SECRET_123"))
    );
    let key = Keychain
        .get(&plugin)
        .unwrap()
        .expect("a key in the keychain");
    println!(
        "key entry: {} chars; three writes and a read took {:?}",
        key.len(),
        started.elapsed()
    );
    // A big value directly in the keychain (why the vault exists).
    let big = format!("{plugin}-big");
    let direct = Keychain.set(&big, &"z".repeat(4096));
    println!("a 4096-byte keychain entry: {direct:?}");
    let _ = Keychain.delete(&big);
    vault.clear().unwrap();
    assert_eq!(Keychain.get(&plugin).unwrap(), None, "the key is removed");
}

#[test]
fn names_keep_settings_workspaces_and_accounts_apart() {
    assert_eq!(setting_name("token", None), "setting:token");
    assert_eq!(setting_name("token", Some("ws-1")), "setting:token@ws-1");
    assert_eq!(account_name("a1"), "account:a1");
    let dir = tempfile::tempdir().unwrap();
    let keys = MemoryKeys::default();
    let vault = Vault::new(&keys, dir.path(), "p@x");
    vault
        .set(&setting_name("token", None), json!("mine"))
        .unwrap();
    vault
        .set(&setting_name("token", Some("ws-1")), json!("team"))
        .unwrap();
    assert_eq!(
        vault.get(&setting_name("token", Some("ws-1"))).unwrap(),
        Some(json!("team"))
    );
    assert_eq!(
        vault.get(&setting_name("token", None)).unwrap(),
        Some(json!("mine"))
    );
}

#[test]
fn two_plugins_never_share_a_file_or_a_key() {
    let dir = tempfile::tempdir().unwrap();
    let keys = MemoryKeys::default();
    let a = Vault::new(&keys, dir.path(), "a@x");
    let b = Vault::new(&keys, dir.path(), "a-x");
    assert_ne!(a.file(), b.file());
    a.set("k", json!("from a")).unwrap();
    assert_eq!(b.get("k").unwrap(), None);
    assert_ne!(keys.get("a@x").unwrap(), keys.get("a-x").unwrap());
}

#[test]
fn random_keys_are_long_and_different() {
    let a = crate::random_key().unwrap();
    let b = crate::random_key().unwrap();
    assert_eq!(a.len(), 43);
    assert_ne!(a, b);
    assert!(a
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
}
