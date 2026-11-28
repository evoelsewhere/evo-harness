//! A mate's Knowledge (docs/features/evo-mates/spec.md, FR-77; plan 3.10):
//! files the person adds, copied into `<mate>/knowledge/`, that every turn
//! of the mate names and tells it to read when they bear on the request.
//! They stay on the device and never go into a team file (the file writes
//! the mate's stored instructions, which never hold the list).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::db::Db;

/// At most this many files per mate.
pub const MAX_FILES: usize = 50;
/// At most this many bytes per file.
pub const MAX_BYTES: u64 = 25 * 1024 * 1024;

/// `<agent dir>/mates/<id>/knowledge`.
pub fn dir_of(id: &str) -> PathBuf {
    super::mate_dir(id).join("knowledge")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeFile {
    pub name: String,
    pub bytes: u64,
}

/// What adding files did.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Added {
    pub added: Vec<String>,
    /// `(file name, why)` for each file left out.
    pub refused: Vec<(String, String)>,
}

/// The files in `dir`, by name; none when it does not exist.
pub fn list_in(dir: &Path) -> Vec<KnowledgeFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<KnowledgeFile> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            meta.is_file().then(|| KnowledgeFile {
                name: entry.file_name().to_string_lossy().into_owned(),
                bytes: meta.len(),
            })
        })
        .collect();
    files.sort_by_key(|f| f.name.to_lowercase());
    files
}

/// `name`, or `stem (2).ext` and so on when `dir` has it already.
fn free_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !dir.join(candidate).exists())
        .expect("a free name")
}

/// Copy `sources` into `dir`: refuses folders, links, hidden files, files
/// over [`MAX_BYTES`] and files past [`MAX_FILES`], each with the reason.
pub fn add_into(dir: &Path, sources: &[PathBuf]) -> Result<Added, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let mut count = list_in(dir).len();
    let mut out = Added::default();
    for source in sources {
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let refuse = |why: &str| {
            (
                if name.is_empty() {
                    source.display().to_string()
                } else {
                    name.clone()
                },
                why.to_string(),
            )
        };
        let meta = match std::fs::symlink_metadata(source) {
            Ok(meta) => meta,
            Err(_) => {
                out.refused.push(refuse("it cannot be read"));
                continue;
            }
        };
        let why = if meta.file_type().is_symlink() {
            Some("links are not added")
        } else if meta.is_dir() {
            Some("folders are not added; add the files in it")
        } else if name.is_empty() || name.starts_with('.') {
            Some("hidden files are not added")
        } else if meta.len() > MAX_BYTES {
            Some("it is over 25 MB")
        } else if count >= MAX_FILES {
            Some("a mate keeps at most 50 knowledge files")
        } else {
            None
        };
        if let Some(why) = why {
            out.refused.push(refuse(why));
            continue;
        }
        let target = free_name(dir, &name);
        std::fs::copy(source, dir.join(&target))
            .map_err(|e| format!("Could not copy {name}: {e}"))?;
        count += 1;
        out.added.push(target);
    }
    Ok(out)
}

/// Remove `name` from `dir`; a name with a path in it is refused.
pub fn remove_in(dir: &Path, name: &str) -> Result<bool, String> {
    if name.is_empty() || name.contains(['/', '\\']) || name == ".." || name == "." {
        return Err("That is not a knowledge file's name.".into());
    }
    let path = dir.join(name);
    if !path.is_file() {
        return Ok(false);
    }
    std::fs::remove_file(&path).map_err(|e| format!("Could not remove {name}: {e}"))?;
    Ok(true)
}

/// The lines a mate's role ends with when it has knowledge files.
pub fn block(dir: &Path, files: &[KnowledgeFile]) -> Option<String> {
    if files.is_empty() {
        return None;
    }
    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    Some(format!(
        "Your knowledge files, in {}: {}. Read the ones that bear on a request before you answer it, and say which you used.",
        dir.display(),
        names.join(", ")
    ))
}

/// Rewrite the mate's roles so its next turns name its files (as a profile change does).
async fn reread(app: &AppHandle, id: &str) -> Result<(), String> {
    let db = app.state::<Db>();
    let mate = db
        .with(|conn| evo_store::mates::get_mate(conn, id))?
        .ok_or("No such mate.")?;
    super::write_role(&db, &mate)?;
    super::restart_when_idle(app, &mate.chat_id).await;
    super::channels::mate_changed(app, &mate).await;
    super::changed(app);
    Ok(())
}

#[tauri::command]
pub fn mates_knowledge(id: String) -> Vec<KnowledgeFile> {
    list_in(&dir_of(&id))
}

#[tauri::command]
pub async fn mates_knowledge_add(
    app: AppHandle,
    id: String,
    paths: Vec<String>,
) -> Result<Added, String> {
    let sources: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let added = add_into(&dir_of(&id), &sources)?;
    if !added.added.is_empty() {
        reread(&app, &id).await?;
    }
    Ok(added)
}

#[tauri::command]
pub async fn mates_knowledge_remove(
    app: AppHandle,
    id: String,
    name: String,
) -> Result<(), String> {
    if remove_in(&dir_of(&id), &name)? {
        reread(&app, &id).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(dir: &Path, name: &str, bytes: usize) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, vec![b'x'; bytes]).unwrap();
        path
    }

    #[test]
    fn adding_copies_files_and_refuses_folders_hidden_files_and_big_ones() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        let knowledge = to.path().join("knowledge");
        let notes = file(from.path(), "notes.md", 10);
        let hidden = file(from.path(), ".env", 10);
        let folder = from.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        let big = from.path().join("big.bin");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        let added = add_into(
            &knowledge,
            &[
                notes.clone(),
                hidden,
                folder,
                big,
                from.path().join("missing.txt"),
            ],
        )
        .unwrap();
        assert_eq!(added.added, ["notes.md"]);
        let refused: Vec<&str> = added
            .refused
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(refused, [".env", "folder", "big.bin", "missing.txt"]);
        // The same name again is kept apart.
        assert_eq!(
            add_into(&knowledge, &[notes]).unwrap().added,
            ["notes (2).md"]
        );
        assert_eq!(
            list_in(&knowledge)
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["notes (2).md", "notes.md"]
        );
    }

    #[test]
    fn a_mate_keeps_at_most_fifty_files() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        let sources: Vec<PathBuf> = (0..MAX_FILES + 1)
            .map(|i| file(from.path(), &format!("f{i}.txt"), 1))
            .collect();
        let added = add_into(to.path(), &sources).unwrap();
        assert_eq!(added.added.len(), MAX_FILES);
        assert_eq!(added.refused.len(), 1);
    }

    #[test]
    fn removing_takes_a_name_only() {
        let to = tempfile::tempdir().unwrap();
        file(to.path(), "a.txt", 1);
        assert!(remove_in(to.path(), "../a.txt").is_err());
        assert!(remove_in(to.path(), "a.txt").unwrap());
        assert!(!remove_in(to.path(), "a.txt").unwrap());
    }

    #[test]
    fn the_role_names_the_files_and_tells_the_mate_to_read_them() {
        let to = tempfile::tempdir().unwrap();
        assert_eq!(block(to.path(), &[]), None);
        file(to.path(), "prices.csv", 3);
        let text = block(to.path(), &list_in(to.path())).unwrap();
        assert!(
            text.contains("prices.csv") && text.contains("Read the ones that bear on a request"),
            "{text}"
        );
        let mate: evo_store::mates::Mate = serde_json::from_value(serde_json::json!({
            "id": "m", "name": "Ada", "avatar": {}, "role": "Keeps the books", "instructions": "Be exact.",
            "provider": null, "model": null, "thinking": null, "tools": null, "connectors": null,
            "skills": null, "apps": null, "approvalMode": "ask", "sending": "ask",
            "notifications": true, "initiative": "medium", "unanswered": 0, "paused": false,
            "folder": "/m", "projectFolder": null, "chatId": "c", "template": null,
            "pinned": false, "hidden": false, "position": 0, "createdAt": 0, "updatedAt": 0
        }))
        .unwrap();
        let role = super::super::instructions_with(&mate, Some(text.clone()));
        assert!(role.ends_with(&format!("Be exact.\n\n{text}")), "{role}");
        assert!(
            !mate.instructions.contains("prices.csv"),
            "the stored instructions, which a team file writes, never hold it"
        );
    }
}
