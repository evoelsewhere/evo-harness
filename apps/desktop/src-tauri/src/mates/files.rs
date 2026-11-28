//! Evo Mates' Files page (docs/features/evo-mates/spec.md, FR-84; plan
//! 3.12): the files in every mate's own folder, newest first. The disk is
//! read on each opening; nothing is indexed.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::db::Db;

/// How deep a mate's folder is walked.
pub const DEPTH: usize = 4;
/// At most this many files are listed.
pub const MAX: usize = 500;

/// Folders that hold dependencies or build output, never listed.
const SKIPPED: [&str; 7] = [
    "node_modules",
    "target",
    "dist",
    "build",
    ".git",
    "__pycache__",
    ".venv",
];

/// A file on the Files page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MateFile {
    pub mate_id: String,
    pub path: String,
    pub name: String,
    pub size: u64,
    /// ms since the epoch.
    pub modified: i64,
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64)
}

/// The files under `dir` to `depth`, with no hidden entries or skipped folders, newest first, at most `max`.
pub fn collect(dir: &Path, depth: usize, max: usize) -> Vec<(PathBuf, u64, i64)> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((folder, level)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || SKIPPED.contains(&name.as_str()) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                if level + 1 < depth {
                    stack.push((entry.path(), level + 1));
                }
            } else if meta.is_file() {
                out.push((entry.path(), meta.len(), modified_ms(&meta)));
            }
        }
    }
    out.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    out.truncate(max);
    out
}

/// Every mate's files, newest first (FR-84).
#[tauri::command]
pub async fn mates_files(app: AppHandle) -> Result<Vec<MateFile>, String> {
    let mates = app
        .state::<Db>()
        .with(|conn| evo_store::mates::list_mates(conn))?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut all: Vec<MateFile> = Vec::new();
        for mate in mates {
            let home = super::mate_dir(&mate.id);
            for dir in [home.join("work"), home.join("knowledge")] {
                for (path, size, modified) in collect(&dir, DEPTH, MAX) {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    all.push(MateFile {
                        mate_id: mate.id.clone(),
                        path: path.display().to_string(),
                        name,
                        size,
                        modified,
                    });
                }
            }
        }
        all.sort_by(|a, b| b.modified.cmp(&a.modified));
        all.truncate(MAX);
        all
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, age_secs: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }

    #[test]
    fn files_are_listed_newest_first_without_hidden_or_build_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("old.md"), 300);
        write(&root.join("charts/new.png"), 10);
        write(&root.join(".secret"), 1);
        write(&root.join("node_modules/pkg/index.js"), 1);
        write(&root.join("a/b/c/d/too-deep.txt"), 1);
        let names: Vec<String> = collect(root, DEPTH, MAX)
            .into_iter()
            .map(|(p, _, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["new.png", "old.md"]);
        assert_eq!(collect(root, DEPTH, 1).len(), 1, "at most N");
        assert!(collect(&root.join("missing"), DEPTH, MAX).is_empty());
    }
}
