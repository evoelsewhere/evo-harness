//! Plugin secrets (docs/features/plugin-settings/spec.md, FR-5, NFR-1;
//! ADR 0022): one random 32-byte key per plugin in the operating system's
//! keychain (service [`SERVICE`], account = the plugin id) seals one file
//! per plugin with AES-256-GCM. The file holds a JSON object of secret
//! values; nothing in it is readable without the key, and the key never
//! leaves the keychain except into this process's memory.
//!
//! A keychain entry stays small (Windows allows 2,560 bytes), so tokens and
//! cookie jars of any size fit, and macOS asks at most once per plugin.
//!
//! Names inside a vault: [`setting_name`] for secret settings (personal or a
//! workspace's own), [`account_name`] for a signed-in account's session.
//!
//! Rules for callers:
//! - only the desktop shell opens vaults (the engine never does);
//! - call from a blocking thread (`spawn_blocking`): the keychain may block,
//!   and on Linux it runs its own small async runtime;
//! - one writer per plugin at a time (read, change, write is not atomic
//!   across callers).

use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Map, Value};

/// The keychain service every plugin key is filed under.
pub const SERVICE: &str = "evo.plugins";

/// Where a plugin's key lives. The real one is the keychain; tests use
/// [`MemoryKeys`].
pub trait KeyStore: Send + Sync {
    fn get(&self, plugin: &str) -> Result<Option<String>, String>;
    fn set(&self, plugin: &str, value: &str) -> Result<(), String>;
    fn delete(&self, plugin: &str) -> Result<(), String>;
}

/// The operating system's keychain.
pub struct Keychain;

impl KeyStore for Keychain {
    fn get(&self, plugin: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(SERVICE, plugin).map_err(describe)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(describe(error)),
        }
    }

    fn set(&self, plugin: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(SERVICE, plugin)
            .map_err(describe)?
            .set_password(value)
            .map_err(describe)
    }

    fn delete(&self, plugin: &str) -> Result<(), String> {
        match keyring::Entry::new(SERVICE, plugin)
            .map_err(describe)?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(describe(error)),
        }
    }
}

fn describe(error: keyring::Error) -> String {
    match error {
        keyring::Error::NoStorageAccess(cause) | keyring::Error::PlatformFailure(cause) => {
            format!("the system keychain is not available: {cause}")
        }
        other => format!("the system keychain refused: {other}"),
    }
}

/// Keys in memory, for tests.
#[derive(Default)]
pub struct MemoryKeys(std::sync::Mutex<std::collections::HashMap<String, String>>);

impl KeyStore for MemoryKeys {
    fn get(&self, plugin: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().get(plugin).cloned())
    }
    fn set(&self, plugin: &str, value: &str) -> Result<(), String> {
        self.0.lock().unwrap().insert(plugin.into(), value.into());
        Ok(())
    }
    fn delete(&self, plugin: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(plugin);
        Ok(())
    }
}

/// The vault name of secret setting `key`, or of a workspace's own value.
pub fn setting_name(key: &str, workspace: Option<&str>) -> String {
    match workspace {
        Some(workspace) => format!("setting:{key}@{workspace}"),
        None => format!("setting:{key}"),
    }
}

/// The vault name of a signed-in account's session (tokens or cookies).
pub fn account_name(record: &str) -> String {
    format!("account:{record}")
}

/// One plugin's sealed secrets.
pub struct Vault<'k> {
    keys: &'k dyn KeyStore,
    plugin: String,
    file: PathBuf,
}

/// A file name for a plugin id: a readable part and a hash of the whole id,
/// so ids that read alike (`a@x`, `a-x`) never share a file.
fn safe(id: &str) -> String {
    let readable: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(60)
        .collect();
    // FNV-1a, 32 bits: stable across runs and builds.
    let hash = id.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    format!("{readable}-{hash:08x}")
}

impl<'k> Vault<'k> {
    /// The vault of `plugin`, its file in `dir`.
    pub fn new(keys: &'k dyn KeyStore, dir: &Path, plugin: &str) -> Self {
        Self {
            keys,
            plugin: plugin.to_string(),
            file: dir.join(format!("{}.vault", safe(plugin))),
        }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    /// The key, made and filed on first use when `create`.
    fn key(&self, create: bool) -> Result<Option<[u8; 32]>, String> {
        if let Some(text) = self.keys.get(&self.plugin)? {
            let bytes = B64
                .decode(text.trim())
                .map_err(|_| "the vault key in the keychain is damaged".to_string())?;
            return bytes
                .try_into()
                .map(Some)
                .map_err(|_| "the vault key in the keychain is damaged".to_string());
        }
        if !create {
            return Ok(None);
        }
        let mut key = [0u8; 32];
        SystemRandom::new()
            .fill(&mut key)
            .map_err(|_| "no random source".to_string())?;
        self.keys.set(&self.plugin, &B64.encode(key))?;
        Ok(Some(key))
    }

    /// Every value (empty when nothing was ever stored).
    pub fn read(&self) -> Result<Map<String, Value>, String> {
        let Ok(sealed) = std::fs::read(&self.file) else {
            return Ok(Map::new());
        };
        let key = self
            .key(false)?
            .ok_or("the vault's key is gone from the keychain")?;
        let plain = open(&key, &sealed)?;
        serde_json::from_slice(&plain).map_err(|_| "the vault is damaged".to_string())
    }

    /// Replace every value.
    pub fn write(&self, values: &Map<String, Value>) -> Result<(), String> {
        let key = self.key(true)?.expect("created");
        let sealed = seal(
            &key,
            &serde_json::to_vec(values).map_err(|e| e.to_string())?,
        )?;
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let temporary = self.file.with_extension("vault.tmp");
        std::fs::write(&temporary, sealed).map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &self.file).map_err(|e| e.to_string())
    }

    pub fn get(&self, key: &str) -> Result<Option<Value>, String> {
        Ok(self.read()?.get(key).cloned())
    }

    pub fn set(&self, key: &str, value: Value) -> Result<(), String> {
        let mut all = self.read()?;
        all.insert(key.to_string(), value);
        self.write(&all)
    }

    pub fn remove(&self, key: &str) -> Result<(), String> {
        let mut all = self.read()?;
        if all.remove(key).is_some() {
            self.write(&all)?;
        }
        Ok(())
    }

    /// Delete the file and the key (uninstall).
    pub fn clear(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        self.keys.delete(&self.plugin)
    }
}

/// 32 random bytes as URL-safe base64: a bearer key for the plugin host API
/// (docs/features/plugin-settings/spec.md, FR-26).
pub fn random_key() -> Result<String, String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "no random source")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// `nonce ‖ ciphertext ‖ tag`.
pub fn seal(key: &[u8; 32], plain: &[u8]) -> Result<Vec<u8>, String> {
    let sealing = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).map_err(|_| "bad key")?);
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| "no random source")?;
    let mut data = plain.to_vec();
    sealing
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(b"evo-vault-1"),
            &mut data,
        )
        .map_err(|_| "could not seal")?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&data);
    Ok(out)
}

pub fn open(key: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>, String> {
    if sealed.len() < NONCE_LEN {
        return Err("the vault is damaged".into());
    }
    let opening = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).map_err(|_| "bad key")?);
    let (nonce, rest) = sealed.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| "the vault is damaged")?;
    let mut data = rest.to_vec();
    let plain = opening
        .open_in_place(nonce, Aad::from(b"evo-vault-1"), &mut data)
        .map_err(|_| "the vault does not open with its key")?;
    Ok(plain.to_vec())
}

#[cfg(test)]
mod tests;
