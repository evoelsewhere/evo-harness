//! Developer mode: a plugin's open views reload when its files change
//! (docs/features/plugin-ui/spec.md, FR-14).
//!
//! The folder of each plugin in Developer mode with an open view is checked
//! every [`TICK`]: a change in its files (count, sizes, times) reloads its
//! views. Polling a few hundred files costs little and needs no watcher
//! crate; nothing is read while no such view is open.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

use super::PluginUiState;

const TICK: Duration = Duration::from_millis(500);
/// A plugin folder is looked at to this depth and this many files at most.
const DEPTH: usize = 8;
const MAX_FILES: usize = 5_000;
/// Folders a plugin's build leaves that are not the view's files.
const SKIPPED: &[&str] = &["node_modules", ".git", "target"];

/// A summary of a folder's files that changes when any of them does.
pub fn fingerprint(root: &Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |value: u64| {
        for byte in value.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut files = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let Ok(meta) = entry.metadata() else { continue };
            let name = entry.file_name();
            if meta.is_dir() {
                if depth < DEPTH && !SKIPPED.iter().any(|skip| name == *skip) {
                    stack.push((entry.path(), depth + 1));
                }
                continue;
            }
            files += 1;
            if files > MAX_FILES {
                return hash;
            }
            for byte in name.to_string_lossy().bytes() {
                mix(u64::from(byte));
            }
            mix(meta.len());
            let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64);
            mix(modified);
        }
    }
    mix(files as u64);
    hash
}

/// Check the folders of plugins in Developer mode with open views, for as
/// long as the app runs.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut seen: HashMap<String, u64> = HashMap::new();
        loop {
            tokio::time::sleep(TICK).await;
            let state = app.state::<PluginUiState>();
            // Plugins in Developer mode, with the labels of their open views.
            let mut watched: HashMap<String, (std::path::PathBuf, Vec<String>)> = HashMap::new();
            {
                let views = state.views.lock().expect("views lock");
                for live in views.values() {
                    let Some(entry) = state.plugin(&live.key).filter(|entry| entry.developer_mode) else { continue };
                    watched.entry(entry.id.clone()).or_insert_with(|| (entry.root.clone(), Vec::new())).1.push(live.label.clone());
                }
            }
            seen.retain(|id, _| watched.contains_key(id));
            for (id, (root, labels)) in watched {
                let now = tauri::async_runtime::spawn_blocking(move || fingerprint(&root)).await.unwrap_or_default();
                match seen.insert(id, now) {
                    Some(before) if before != now => {
                        for label in labels {
                            if let Some(webview) = app.get_webview(&label) {
                                let _ = webview.reload();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_to_any_file_changes_the_fingerprint() {
        let root = std::env::temp_dir().join(format!("evo-plugin-watch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("ui")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
        std::fs::write(root.join("ui/wall.html"), "a").unwrap();
        let first = fingerprint(&root);
        assert_eq!(fingerprint(&root), first, "stable while nothing changes");
        std::fs::write(root.join("ui/wall.html"), "ab").unwrap();
        let edited = fingerprint(&root);
        assert_ne!(edited, first, "an edit");
        std::fs::write(root.join("ui/new.js"), "").unwrap();
        let added = fingerprint(&root);
        assert_ne!(added, edited, "a new file");
        std::fs::write(root.join("node_modules/x/dep.js"), "x").unwrap();
        assert_eq!(fingerprint(&root), added, "node_modules is not the view's files");
        std::fs::remove_file(root.join("ui/new.js")).unwrap();
        assert_ne!(fingerprint(&root), added, "a removed file");
        let _ = std::fs::remove_dir_all(&root);
    }
}
