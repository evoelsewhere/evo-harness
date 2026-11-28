//! Local files handed to a connected app's tool
//! (docs/features/connected-apps/spec.md, FR-14; plan 3.2, 3.7).
//!
//! A tool argument Composio marks as taking a file (`file_uploadable` in
//! its schema) may hold a full local path. It is uploaded only when the
//! file lies in one of the allowed places — the chat's or mate's folder,
//! or the folder of files attached to the conversation — and no part of
//! its path below that place is hidden (starts with `.`). Evo's own data
//! is refused, except a mate's own folder, minus its `memory/` and its
//! vaults. At most [`MAX_FILES`] files a call. Checking needs no network:
//! uploading happens only after the approval card, which lists each file.
//! Anything that is not a local path is sent unchanged.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

/// Files a call may upload.
pub const MAX_FILES: usize = 10;

/// Where files may come from.
#[derive(Debug, Clone, Default)]
pub struct Places {
    /// The chat's or mate's folder, and the attachments folder.
    pub roots: Vec<PathBuf>,
    /// Evo's own data (the agent dir, the app's data and cache dirs).
    pub evo_data: Vec<PathBuf>,
    /// The mate's own folder (`<agent dir>/mates/<id>`), when a mate calls.
    pub mate_dir: Option<PathBuf>,
}

/// A file that passed the checks.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalFile {
    /// Where it sits in the arguments (a JSON pointer).
    pub pointer: String,
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
}

/// The argument names whose schema marks them as taking a file.
fn file_properties(schema: &Value) -> Vec<String> {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|props| {
            props
                .iter()
                .filter(|(_, p)| {
                    p.get("file_uploadable").and_then(Value::as_bool) == Some(true)
                        || p.get("format").and_then(Value::as_str) == Some("path")
                })
                .map(|(name, _)| name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `text` reads as a full local path (not a URL, not an uploaded
/// file's key).
pub fn looks_local(text: &str) -> bool {
    let text = text.trim();
    if text.contains("://") {
        return false;
    }
    Path::new(text).is_absolute()
        || text.starts_with(['/', '\\', '~'])
        || (text.len() > 2
            && text.as_bytes()[1] == b':'
            && text.as_bytes()[0].is_ascii_alphabetic())
}

/// The file arguments of a call that hold local paths: `(pointer, path)`;
/// a list of paths gives one item each.
pub fn local_paths(schema: &Value, args: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for name in file_properties(schema) {
        let escaped = name.replace('~', "~0").replace('/', "~1");
        match args.get(&name) {
            Some(Value::String(text)) if looks_local(text) => {
                out.push((format!("/{escaped}"), text.trim().to_string()))
            }
            Some(Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    if let Some(text) = item.as_str().filter(|t| looks_local(t)) {
                        out.push((format!("/{escaped}/{i}"), text.trim().to_string()));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn expand(raw: &str) -> PathBuf {
    match raw.strip_prefix('~') {
        Some(rest) => home()
            .unwrap_or_default()
            .join(rest.trim_start_matches(['/', '\\'])),
        None => PathBuf::from(raw),
    }
}

fn hidden_below(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .map(|rest| {
            rest.components().any(
                |c| matches!(c, Component::Normal(name) if name.to_string_lossy().starts_with('.')),
            )
        })
        .unwrap_or(true)
}

/// Check one path. `Err`: why it may not be uploaded.
pub fn check(pointer: &str, raw: &str, places: &Places) -> Result<LocalFile, String> {
    let wanted = expand(raw);
    if !wanted.is_absolute() {
        return Err(format!("{raw} is not a full path."));
    }
    let real = std::fs::canonicalize(&wanted).map_err(|_| format!("{raw} does not exist."))?;
    let meta = std::fs::metadata(&real).map_err(|e| format!("{raw} cannot be read: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{raw} is not a file."));
    }
    let canonical = |dirs: &[PathBuf]| -> Vec<PathBuf> {
        dirs.iter()
            .filter_map(|d| std::fs::canonicalize(d).ok())
            .collect()
    };
    let mate = places
        .mate_dir
        .as_ref()
        .and_then(|d| std::fs::canonicalize(d).ok());
    let in_mate = mate.as_ref().is_some_and(|m| real.starts_with(m));
    if in_mate {
        let mate = mate.as_ref().expect("checked");
        let rest = real.strip_prefix(mate).unwrap_or(Path::new(""));
        if rest.starts_with("memory")
            || real
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("vault"))
        {
            return Err(format!(
                "{raw} is part of the mate's memory or secrets, which are never uploaded."
            ));
        }
    } else if canonical(&places.evo_data)
        .iter()
        .any(|d| real.starts_with(d))
    {
        return Err(format!("{raw} is Evo's own data, which is never uploaded."));
    }
    let roots = canonical(&places.roots);
    let Some(root) = roots
        .iter()
        .filter(|r| real.starts_with(r))
        .max_by_key(|r| r.components().count())
    else {
        return Err(format!("{raw} is outside the folders files may be sent from (the working folder and files attached to this conversation). Copy it into the working folder first."));
    };
    if hidden_below(&real, root) {
        return Err(format!(
            "{raw} is a hidden file or inside a hidden folder, which is never uploaded."
        ));
    }
    Ok(LocalFile {
        pointer: pointer.to_string(),
        name: real
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: meta.len(),
        path: real,
    })
}

/// Check every local path of a call (`(pointer, path)`). `Err` names the
/// first that may not be uploaded.
pub fn check_all(found: &[(String, String)], places: &Places) -> Result<Vec<LocalFile>, String> {
    if found.len() > MAX_FILES {
        return Err(format!(
            "A call may send at most {MAX_FILES} files, not {}.",
            found.len()
        ));
    }
    found
        .iter()
        .map(|(pointer, raw)| check(pointer, raw, places))
        .collect()
}

/// One line per file for the approval card: name, path, size.
pub fn card_lines(files: &[LocalFile]) -> String {
    files
        .iter()
        .map(|f| {
            format!(
                "{} — {} ({})",
                f.name,
                crate::agent_tools::browser::upload::os_path(&f.path),
                size_text(f.size)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn size_text(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.1} KB", b as f64 / (1u64 << 10) as f64),
        b => format!("{b} bytes"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tree() -> (tempfile::TempDir, Places) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (path, text) in [
            ("work/report.pdf", "pdf"),
            ("work/.secret/key.txt", "k"),
            ("work/.env", "E=1"),
            ("home/.ssh/id_rsa", "private"),
            ("evo/agent/auth.json", "{}"),
            ("evo/agent/mates/m1/work/notes.md", "n"),
            ("evo/agent/mates/m1/memory/me.md", "m"),
            ("evo/agent/mates/m1/mate-m1-0000.vault", "v"),
            ("elsewhere/x.txt", "x"),
        ] {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        let places = Places {
            roots: vec![
                root.join("work"),
                root.join("evo/agent/mates/m1"),
                root.join("home"),
            ],
            evo_data: vec![root.join("evo")],
            mate_dir: Some(root.join("evo/agent/mates/m1")),
        };
        (dir, places)
    }

    fn path(dir: &tempfile::TempDir, rel: &str) -> String {
        dir.path().join(rel).to_string_lossy().into_owned()
    }

    #[test]
    fn a_hidden_file_is_never_uploaded() {
        let (dir, places) = tree();
        for rel in ["work/.env", "work/.secret/key.txt", "home/.ssh/id_rsa"] {
            let error = check("/file", &path(&dir, rel), &places).unwrap_err();
            assert!(error.contains("hidden"), "{rel}: {error}");
        }
        let ok = check("/file", &path(&dir, "work/report.pdf"), &places).unwrap();
        assert_eq!((ok.name.as_str(), ok.size), ("report.pdf", 3));
    }

    #[test]
    fn evo_data_memory_and_vaults_stay_home() {
        let (dir, places) = tree();
        assert!(check("/f", &path(&dir, "evo/agent/auth.json"), &places)
            .unwrap_err()
            .contains("Evo's own data"));
        assert!(check(
            "/f",
            &path(&dir, "evo/agent/mates/m1/memory/me.md"),
            &places
        )
        .unwrap_err()
        .contains("memory"));
        assert!(check(
            "/f",
            &path(&dir, "evo/agent/mates/m1/mate-m1-0000.vault"),
            &places
        )
        .unwrap_err()
        .contains("secrets"));
        assert!(check(
            "/f",
            &path(&dir, "evo/agent/mates/m1/work/notes.md"),
            &places
        )
        .is_ok());
        let error = check("/f", &path(&dir, "elsewhere/x.txt"), &places).unwrap_err();
        assert!(error.contains("outside the folders"), "{error}");
        assert!(check("/f", "relative/x.txt", &places).is_err());
        let error = check("/f", &path(&dir, "work/../elsewhere/x.txt"), &places).unwrap_err();
        assert!(error.contains("outside"), "`..` is resolved first: {error}");
    }

    #[test]
    fn only_file_arguments_holding_local_paths_are_files() {
        let schema = json!({"properties": {
            "attachment": {"type": "string", "file_uploadable": true},
            "files": {"type": "array", "file_uploadable": true},
            "body": {"type": "string"}
        }});
        let args = json!({
            "attachment": "C:\\work\\a.pdf",
            "files": ["/tmp/b.txt", "https://example.com/c.txt"],
            "body": "/not/a/file/argument"
        });
        let mut found = local_paths(&schema, &args);
        found.sort();
        assert_eq!(
            found,
            [
                ("/attachment".to_string(), "C:\\work\\a.pdf".to_string()),
                ("/files/0".to_string(), "/tmp/b.txt".to_string())
            ]
        );
        assert!(
            local_paths(&schema, &json!({"attachment": {"s3key": "k"}})).is_empty(),
            "an uploaded file is sent unchanged"
        );
    }

    #[test]
    fn at_most_ten_files_a_call() {
        let (dir, places) = tree();
        let one = ("/f".to_string(), path(&dir, "work/report.pdf"));
        assert!(check_all(&vec![one.clone(); 10], &places).is_ok());
        assert!(check_all(&vec![one; 11], &places)
            .unwrap_err()
            .contains("at most 10"));
    }
}
