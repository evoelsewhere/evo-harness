//! What a patch is about to overwrite, kept so a failed apply can be undone:
//! `git apply` deletes the old files before it writes the new ones, and a file
//! another program holds stops it in between.

use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// How one path was before the patch.
enum Before {
    Absent,
    /// A copy of the file, in the snapshot's folder.
    File(PathBuf),
    /// Where the link pointed.
    Link(PathBuf),
    /// A folder: a patch never replaces one.
    Folder,
}

pub(crate) struct Snapshot {
    root: PathBuf,
    entries: Vec<(PathBuf, Before)>,
    /// Folders the patch may create, deepest first.
    new_folders: Vec<PathBuf>,
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    let size = |path: &Path| std::fs::metadata(path).map(|meta| meta.len()).ok();
    size(a).is_some()
        && size(a) == size(b)
        && matches!((std::fs::read(a), std::fs::read(b)), (Ok(a), Ok(b)) if a == b)
}

fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, at)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, at)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, at);
        Err(ErrorKind::Unsupported.into())
    }
}

impl Snapshot {
    /// Save what is at each of `paths` (relative to `root`) into `store`.
    pub(crate) fn take(root: &Path, paths: &[PathBuf], store: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(store).map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        let mut new_folders = Vec::new();
        for (index, relative) in paths.iter().enumerate() {
            if !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            {
                return Err(format!("{} is not inside the folder", relative.display()));
            }
            let full = root.join(relative);
            let before = match full.symlink_metadata() {
                Ok(meta) if meta.file_type().is_symlink() => Before::Link(
                    std::fs::read_link(&full)
                        .map_err(|e| format!("{}: {e}", relative.display()))?,
                ),
                Ok(meta) if meta.is_dir() => Before::Folder,
                Ok(_) => {
                    let copy = store.join(index.to_string());
                    std::fs::copy(&full, &copy)
                        .map_err(|e| format!("{}: {e}", relative.display()))?;
                    Before::File(copy)
                }
                // Not there, or under something that is not a folder.
                Err(error)
                    if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) =>
                {
                    Before::Absent
                }
                Err(error) => return Err(format!("{}: {error}", relative.display())),
            };
            let mut folder = relative.parent();
            while let Some(parent) = folder.filter(|p| !p.as_os_str().is_empty()) {
                if root.join(parent).symlink_metadata().is_err()
                    && !new_folders.iter().any(|known| known == parent)
                {
                    new_folders.push(parent.to_path_buf());
                }
                folder = parent.parent();
            }
            entries.push((relative.clone(), before));
        }
        new_folders.sort_by_key(|folder| std::cmp::Reverse(folder.components().count()));
        Ok(Self {
            root: root.to_path_buf(),
            entries,
            new_folders,
        })
    }

    /// Put every path back as it was. `Err` names the ones that could not be.
    pub(crate) fn restore(&self) -> Result<(), String> {
        let mut failed = Vec::new();
        for (relative, before) in &self.entries {
            let full = self.root.join(relative);
            let now = full.symlink_metadata().ok();
            let is_link = now
                .as_ref()
                .is_some_and(|meta| meta.file_type().is_symlink());
            let done = match before {
                Before::Folder => Ok(()),
                Before::Absent => match &now {
                    Some(meta) if !meta.is_dir() => std::fs::remove_file(&full),
                    _ => Ok(()),
                },
                // Untouched (a file git could not replace is still what it was).
                Before::File(copy) if now.is_some() && !is_link && same_bytes(copy, &full) => {
                    Ok(())
                }
                Before::Link(target)
                    if is_link && std::fs::read_link(&full).is_ok_and(|to| to == *target) =>
                {
                    Ok(())
                }
                Before::File(_) | Before::Link(_) => (|| {
                    if let Some(parent) = full.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    if now.is_some() {
                        std::fs::remove_file(&full)?;
                    }
                    match before {
                        Before::File(copy) => std::fs::copy(copy, &full).map(|_| ()),
                        Before::Link(target) => link(target, &full),
                        _ => Ok(()),
                    }
                })(),
            };
            if let Err(error) = done {
                failed.push(format!("{}: {error}", relative.display()));
            }
        }
        // Only empty ones go: `remove_dir` refuses the rest.
        for folder in &self.new_folders {
            let _ = std::fs::remove_dir(self.root.join(folder));
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(failed.join("; "))
        }
    }
}
