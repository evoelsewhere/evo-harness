//! Registering the host with Chrome and Edge for the current user
//! (docs/features/browser-bridge/spec.md FR-2, NFR-4; plan 3.3 Host manifest).
//!
//! Windows: the host manifest lives in Evo's agent folder and a per-user
//! registry key points each browser at it. macOS and Linux: the manifest file
//! itself goes into each browser's per-user `NativeMessagingHosts` folder.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use evo_bridge_host::HOST_NAME;

/// The browsers Webbridge supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Browser {
    Chrome,
    Edge,
}

impl Browser {
    pub const ALL: [Browser; 2] = [Browser::Chrome, Browser::Edge];

    pub fn parse(name: &str) -> Option<Browser> {
        match name {
            "chrome" => Some(Browser::Chrome),
            "edge" => Some(Browser::Edge),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Browser::Chrome => "chrome",
            Browser::Edge => "edge",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Browser::Chrome => "Chrome",
            Browser::Edge => "Edge",
        }
    }

    /// Where the extension is installed from, for the person to open.
    pub fn extensions_page(self) -> &'static str {
        match self {
            Browser::Chrome => "chrome://extensions",
            Browser::Edge => "edge://extensions",
        }
    }

    /// The per-user registry key (under `HKEY_CURRENT_USER`) that names the host manifest.
    pub fn registry_key(self) -> String {
        let vendor = match self {
            Browser::Chrome => r"Software\Google\Chrome",
            Browser::Edge => r"Software\Microsoft\Edge",
        };
        format!(r"{vendor}\NativeMessagingHosts\{HOST_NAME}")
    }

    /// The per-user folders (macOS, Linux) where the browser looks for host manifests.
    #[allow(dead_code)]
    pub fn manifest_dirs(self, home: &Path) -> Vec<PathBuf> {
        if cfg!(target_os = "macos") {
            let support = home.join("Library").join("Application Support");
            match self {
                Browser::Chrome => vec![
                    support
                        .join("Google")
                        .join("Chrome")
                        .join("NativeMessagingHosts"),
                    support.join("Chromium").join("NativeMessagingHosts"),
                ],
                Browser::Edge => vec![support.join("Microsoft Edge").join("NativeMessagingHosts")],
            }
        } else {
            let config = home.join(".config");
            match self {
                Browser::Chrome => vec![
                    config.join("google-chrome").join("NativeMessagingHosts"),
                    config.join("chromium").join("NativeMessagingHosts"),
                ],
                Browser::Edge => vec![config.join("microsoft-edge").join("NativeMessagingHosts")],
            }
        }
    }
}

/// The host manifest the browsers read.
pub fn manifest(host: &Path) -> Value {
    json!({
        "name": HOST_NAME,
        "description": "Evo Webbridge",
        "path": host.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": super::ids::allowed_origins(),
    })
}

/// The host executable shipped beside the app.
pub fn host_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let host = exe
        .parent()
        .ok_or("the app has no folder")?
        .join(evo_bridge_host::HOST_EXECUTABLE);
    if !host.is_file() {
        return Err(format!("the Webbridge host is missing: {}", host.display()));
    }
    Ok(host)
}

/// Evo's copy of the manifest (Windows reads it through the registry).
pub fn manifest_file() -> PathBuf {
    super::dir().join(format!("{HOST_NAME}.json"))
}

/// Register the host with `browser`.
pub fn register(browser: Browser) -> Result<(), String> {
    let host = host_path()?;
    let text = serde_json::to_string_pretty(&manifest(&host)).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        let file = manifest_file();
        std::fs::create_dir_all(file.parent().expect("the manifest has a folder"))
            .map_err(|e| e.to_string())?;
        std::fs::write(&file, text).map_err(|e| e.to_string())?;
        registry::set_default(&browser.registry_key(), &file.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
        for dir in browser.manifest_dirs(&home) {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            std::fs::write(dir.join(format!("{HOST_NAME}.json")), &text)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// Remove the host's registration with `browser`.
pub fn unregister(browser: Browser) -> Result<(), String> {
    #[cfg(windows)]
    {
        registry::delete(&browser.registry_key())
    }
    #[cfg(not(windows))]
    {
        let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
        for dir in browser.manifest_dirs(&home) {
            let file = dir.join(format!("{HOST_NAME}.json"));
            if file.exists() {
                std::fs::remove_file(file).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

/// Whether `browser` would start this app's host.
pub fn is_registered(browser: Browser) -> bool {
    #[cfg(windows)]
    {
        let Some(file) = registry::get_default(&browser.registry_key()) else {
            return false;
        };
        points_at_host(Path::new(&file))
    }
    #[cfg(not(windows))]
    {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return false;
        };
        browser
            .manifest_dirs(&home)
            .iter()
            .any(|dir| points_at_host(&dir.join(format!("{HOST_NAME}.json"))))
    }
}

/// Whether the manifest at `file` names this app's host.
fn points_at_host(file: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(file) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let Ok(host) = host_path() else {
        return false;
    };
    manifest["path"].as_str().is_some_and(|path| {
        evo_bridge_host::peer::same_folder(Path::new(path), &host)
            && Path::new(path).file_name() == host.file_name()
    })
}

#[cfg(windows)]
mod registry {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    };

    pub fn set_default(key: &str, value: &str) -> Result<(), String> {
        unsafe {
            let mut handle = HKEY::default();
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(key),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut handle,
                None,
            )
            .ok()
            .map_err(|e| e.to_string())?;
            let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            let result = RegSetValueExW(handle, None, None, REG_SZ, Some(bytes))
                .ok()
                .map_err(|e| e.to_string());
            let _ = RegCloseKey(handle);
            result
        }
    }

    pub fn get_default(key: &str) -> Option<String> {
        unsafe {
            let mut size = 0u32;
            RegGetValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(key),
                None,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size),
            )
            .ok()
            .ok()?;
            let mut buf = vec![0u16; (size as usize).div_ceil(2)];
            RegGetValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(key),
                None,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut size),
            )
            .ok()
            .ok()?;
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..len]))
        }
    }

    pub fn delete(key: &str) -> Result<(), String> {
        const ERROR_FILE_NOT_FOUND: u32 = 2;
        let result = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key)) };
        if result.is_ok() || result.0 == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(result.to_hresult().message())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_names_the_host_and_only_evo_extensions() {
        let manifest = manifest(Path::new(
            r"C:\Program Files\Evo Harness\evo-bridge-host.exe",
        ));
        assert_eq!(manifest["name"], HOST_NAME);
        assert_eq!(manifest["type"], "stdio");
        assert_eq!(
            manifest["path"],
            r"C:\Program Files\Evo Harness\evo-bridge-host.exe"
        );
        let origins = manifest["allowed_origins"].as_array().unwrap();
        assert_eq!(
            origins[0],
            format!("chrome-extension://{}/", super::super::ids::UNPACKED_ID)
        );
    }

    #[test]
    fn each_browser_has_its_own_registry_key() {
        assert_eq!(
            Browser::Chrome.registry_key(),
            r"Software\Google\Chrome\NativeMessagingHosts\dev.evo.webbridge"
        );
        assert_eq!(
            Browser::Edge.registry_key(),
            r"Software\Microsoft\Edge\NativeMessagingHosts\dev.evo.webbridge"
        );
    }

    #[test]
    fn browsers_parse_by_id() {
        for browser in Browser::ALL {
            assert_eq!(Browser::parse(browser.id()), Some(browser));
        }
        assert_eq!(Browser::parse("firefox"), None);
    }

    #[cfg(windows)]
    #[test]
    fn registry_values_round_trip_under_a_scratch_key() {
        let key = r"Software\EvoTests\webbridge-setup-test";
        registry::set_default(key, r"C:\x\manifest.json").unwrap();
        assert_eq!(
            registry::get_default(key).as_deref(),
            Some(r"C:\x\manifest.json")
        );
        registry::delete(key).unwrap();
        assert_eq!(registry::get_default(key), None);
        registry::delete(key).unwrap();
        let _ = registry::delete(r"Software\EvoTests");
    }
}
