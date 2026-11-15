//! Deno and uv for plugin backends: pinned versions, the SHA-256 their
//! projects publish for each archive, downloaded into `<agent dir>/runtimes`
//! on first need (docs/features/plugin-settings/spec.md, FR-18; spike S2).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::json;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

/// The largest archive accepted.
const MAX_ARCHIVE: u64 = 300 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Deno,
    Uv,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Deno => "Deno",
            Tool::Uv => "uv",
        }
    }

    pub fn version(self) -> &'static str {
        match self {
            Tool::Deno => "2.9.8",
            Tool::Uv => "0.11.23",
        }
    }

    fn binary_name(self) -> &'static str {
        match (self, cfg!(windows)) {
            (Tool::Deno, true) => "deno.exe",
            (Tool::Deno, false) => "deno",
            (Tool::Uv, true) => "uv.exe",
            (Tool::Uv, false) => "uv",
        }
    }
}

/// One downloadable archive.
#[derive(Debug, PartialEq)]
pub struct Asset {
    pub url: String,
    pub sha256: &'static str,
    pub zip: bool,
}

/// The target this build runs on, as the projects name their archives.
fn target() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        _ => return None,
    })
}

/// The archive of `tool` for `target`, with its published checksum.
pub fn asset(tool: Tool, target: &str) -> Option<Asset> {
    let sha256 = match (tool, target) {
        (Tool::Deno, "x86_64-pc-windows-msvc") => "cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd84fa1e959927edd93bf",
        (Tool::Deno, "aarch64-apple-darwin") => "9f0bf660393da2eaff044cdc7444cb08c73f235849b61d224d825a194ce7c98e",
        (Tool::Deno, "x86_64-apple-darwin") => "4b2f5edd2639b7470f034238b321d7002d8dfb4726d8558f52c17afac370eb2c",
        (Tool::Deno, "x86_64-unknown-linux-gnu") => "e890975bc2549807bbcc0b584ffafda72cc7c93349dd99c80483dd784f6c4609",
        (Tool::Deno, "aarch64-unknown-linux-gnu") => "d49596c20b9788cf8f5d8ddf644103d4bde56ed239753f0bdb92ba9361d51905",
        (Tool::Uv, "x86_64-pc-windows-msvc") => "02ad29f07e674d68726ba3bb1ff25b335d83515756e2b1a194bb56c3cc30e07c",
        (Tool::Uv, "aarch64-apple-darwin") => "71ef9de85db820749b3b12b7585624ee279e9c5afcbc6f8236bc3d628c4305b0",
        (Tool::Uv, "x86_64-apple-darwin") => "7a88155033cc469bba5bd5a24212e355eb92e3e2a276320b669ec576296c1e25",
        (Tool::Uv, "x86_64-unknown-linux-gnu") => "e12c4cda2fe8c305510a78380a88f2c32a27e90cdcd123cefd2873388f0ebb5f",
        (Tool::Uv, "aarch64-unknown-linux-gnu") => "1873a77350f6621279ae1a0d2227f2bd8b67131598f14a7eb0ba2215d3da2c98",
        _ => return None,
    };
    let version = tool.version();
    Some(match tool {
        Tool::Deno => Asset { url: format!("https://github.com/denoland/deno/releases/download/v{version}/deno-{target}.zip"), sha256, zip: true },
        Tool::Uv => {
            let zip = target.contains("windows");
            let ext = if zip { "zip" } else { "tar.gz" };
            Asset { url: format!("https://github.com/astral-sh/uv/releases/download/{version}/uv-{target}.{ext}"), sha256, zip }
        }
    })
}

/// Where `tool`'s binary lives once installed.
pub fn binary(tool: Tool) -> PathBuf {
    super::runtimes_dir().join(format!("{}-{}", tool.name().to_lowercase(), tool.version())).join(tool.binary_name())
}

/// The installed binary, when it is here.
pub fn installed(tool: Tool) -> Option<PathBuf> {
    let path = binary(tool);
    path.is_file().then_some(path)
}

/// Download, check and unpack `tool` (one download per tool at a time);
/// progress goes to the windows as `plugins://runtime-download`.
pub async fn ensure(app: &AppHandle, tool: Tool) -> Result<PathBuf, String> {
    let state = app.state::<super::RuntimeState>();
    let _one = state.downloads.lock().await;
    if let Some(path) = installed(tool) {
        return Ok(path);
    }
    let target = target().ok_or("this computer's system is not supported")?;
    let asset = asset(tool, target).ok_or("this computer's system is not supported")?;
    let client = reqwest::Client::builder()
        .user_agent("evo")
        .http1_only()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client.get(&asset.url).send().await.map_err(|e| format!("download failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("download failed with HTTP {}", response.status().as_u16()));
    }
    let total = response.content_length();
    let mut bytes: Vec<u8> = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut last = 0u64;
    while let Some(chunk) = response.chunk().await.map_err(|e| format!("download failed: {e}"))? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > MAX_ARCHIVE {
            return Err("the archive is larger than expected".into());
        }
        let received = bytes.len() as u64;
        if received - last > 512 * 1024 || Some(received) == total {
            last = received;
            let _ = app.emit("plugins://runtime-download", json!({ "runtime": tool.name(), "received": received, "total": total }));
        }
    }
    let dest = binary(tool);
    tauri::async_runtime::spawn_blocking(move || unpack(&bytes, &asset, tool, &dest))
        .await
        .map_err(|e| e.to_string())??;
    installed(tool).ok_or_else(|| format!("{} did not unpack", tool.name()))
}

/// Check `bytes` against the pinned checksum and put the tool's binary at
/// `dest` (written to a temporary name, then renamed).
pub fn unpack(bytes: &[u8], asset: &Asset, tool: Tool, dest: &Path) -> Result<(), String> {
    let digest = format!("{:x}", Sha256::digest(bytes));
    if digest != asset.sha256 {
        return Err(format!("the download does not match its published checksum ({digest})"));
    }
    let wanted = tool.binary_name();
    let mut found: Option<Vec<u8>> = None;
    if asset.zip {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).map_err(|e| e.to_string())?;
            let name = file.name().map(|n| n.to_string()).unwrap_or_default();
            if Path::new(&name).file_name().is_some_and(|n| n == wanted) {
                let mut out = Vec::new();
                file.read_to_end(&mut out).map_err(|e| e.to_string())?;
                found = Some(out);
                break;
            }
        }
    } else {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let is_it = entry.path().ok().and_then(|p| p.file_name().map(|n| n == wanted)).unwrap_or(false);
            if is_it {
                let mut out = Vec::new();
                entry.read_to_end(&mut out).map_err(|e| e.to_string())?;
                found = Some(out);
                break;
            }
        }
    }
    let binary = found.ok_or_else(|| format!("the archive holds no `{wanted}`"))?;
    let dir = dest.parent().ok_or("no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let temporary = dest.with_extension("partial");
    {
        let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&binary).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&temporary, dest).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_target_has_both_runtimes_pinned() {
        for target in ["x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
            for tool in [Tool::Deno, Tool::Uv] {
                let asset = asset(tool, target).unwrap_or_else(|| panic!("{tool:?} {target}"));
                assert_eq!(asset.sha256.len(), 64);
                assert!(asset.url.starts_with("https://github.com/") && asset.url.contains(tool.version()), "{}", asset.url);
            }
        }
        assert!(asset(Tool::Uv, "x86_64-pc-windows-msvc").unwrap().zip);
        assert!(!asset(Tool::Uv, "x86_64-unknown-linux-gnu").unwrap().zip);
        assert!(asset(Tool::Deno, "riscv64-unknown-linux-gnu").is_none());
    }

    fn zip_with(name: &str, body: &[u8]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut out);
            writer.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(body).unwrap();
            writer.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn an_archive_must_match_its_checksum_and_hold_the_binary() {
        let dir = std::env::temp_dir().join(format!("evo-runtime-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let name = Tool::Deno.binary_name();
        let bytes = zip_with(name, b"binary");
        let leaked: &'static str = Box::leak(format!("{:x}", Sha256::digest(&bytes)).into_boxed_str());
        let good = Asset { url: String::new(), sha256: leaked, zip: true };
        let dest = dir.join(name);
        unpack(&bytes, &good, Tool::Deno, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"binary");
        let wrong = Asset { url: String::new(), sha256: "00", zip: true };
        assert!(unpack(&bytes, &wrong, Tool::Deno, &dir.join("x")).unwrap_err().contains("checksum"));
        let other = zip_with("readme.txt", b"x");
        let other_sum: &'static str = Box::leak(format!("{:x}", Sha256::digest(&other)).into_boxed_str());
        let other_asset = Asset { url: String::new(), sha256: other_sum, zip: true };
        assert!(unpack(&other, &other_asset, Tool::Deno, &dir.join("y")).unwrap_err().contains("holds no"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
