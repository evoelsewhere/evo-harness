//! Managed Git worktrees: isolated working copies for delegated agents and
//! nested sessions.
//!
//! A worktree of the project's repository, on a branch of its own cut from the
//! current `HEAD`. Where new ones go is a [`Location`] (Settings → Sandbox):
//!
//! * inside the repository: `<repo>/.evo/worktrees/<name>`, with
//!   `/.evo/worktrees/` added to the repository-local `.git/info/exclude`
//!   (never `.gitignore`);
//! * the user data directory: `~/.evo/agent/worktrees/<repo>-<hash>/<name>`.
//!
//! Worktrees in either place are listed and removable whichever is selected.
//! Branches are named `evo/worktree-<name>`; removing a worktree deletes its
//! branch too.
//!
//! What a worktree changed ([`Worktrees::changes`]) is measured from the commit
//! it was cut from (recorded when it is made), commits and new files included;
//! [`Worktrees::apply`] brings those changes into the working folder it was cut
//! from (uncommitted), all of them or none, and removes the worktree. Used by
//! the engine (child agents) and by the desktop shell (review, apply, discard,
//! worktree sessions); it only shells out to `git`.
//!
//! Spec: `docs/features/subagents-and-nested-sessions/spec.md` (FR-13, FR-21,
//! FR-22, NFR-4).

mod snapshot;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Where new managed worktrees go. Existing ones are found in either place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Location {
    /// `<repository>/.evo/worktrees`, kept out of Git by `.git/info/exclude`.
    Repository,
    /// The user data directory (`~/.evo/agent/worktrees`). The default: the
    /// engine's working data stays out of the user's project.
    #[default]
    UserData,
}

use Location as WorktreeLocation;

/// Branches of managed worktrees.
pub const BRANCH_PREFIX: &str = "evo/worktree-";
/// Prefix of a branch the caller named (`evo/<name>`;
/// docs/features/session-roles-and-mail/spec.md, FR-9).
pub const NAMED_BRANCH_PREFIX: &str = "evo/";
/// Inside the repository, relative to its root.
const REPO_DIR: &str = ".evo/worktrees";
/// The line that keeps them out of `git status`.
const EXCLUDE_LINE: &str = "/.evo/worktrees/";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
    /// `repository` or `userData`.
    pub location: &'static str,
    /// Uncommitted changes, or commits beyond where it started.
    pub changed: bool,
}

/// A worktree just made for a child, and the folder the child works in (the
/// same subfolder of the worktree as the parent's is of the repository).
#[derive(Debug, Clone)]
pub struct Created {
    pub worktree: Worktree,
    pub base: String,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Worktrees {
    /// `~/.evo/agent/worktrees` (tests use a temporary folder).
    data_root: PathBuf,
}

/// `git` output byte for byte: a patch must keep its trailing newline and
/// text that is not UTF-8.
fn git_bytes(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The engine has no console; git must not open one.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("git {} failed: {}", args.join(" "), stderr.trim()))
    }
}

/// `git` output as text, for reading and display only.
fn git_raw(dir: &Path, args: &[&str]) -> Result<String, String> {
    git_bytes(dir, args).map(|out| String::from_utf8_lossy(&out).to_string())
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git_raw(dir, args).map(|out| out.trim_end().to_string())
}

/// Paths as one comparable string: forward slashes, no trailing slash, and
/// case-folded where the file system ignores case.
fn key(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.trim_end_matches('/').to_string();
    if cfg!(any(windows, target_os = "macos")) {
        text.to_lowercase()
    } else {
        text
    }
}

/// Without the `\\?\` prefix Windows puts on resolved paths.
fn plain(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(rest.to_string());
        }
    }
    path
}

/// `path` with links and junctions resolved, as far as it exists: git records
/// a worktree by its resolved path.
fn resolved(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        if let Ok(real) = std::fs::canonicalize(current) {
            let mut real = plain(real);
            real.extend(missing.iter().rev());
            return real;
        }
        match (current.parent(), current.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name);
                current = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

fn same(a: &Path, b: &Path) -> bool {
    key(&resolved(a)) == key(&resolved(b))
}

fn is_under(path: &Path, dir: &Path) -> bool {
    let (path, dir) = (key(&resolved(path)), key(&resolved(dir)));
    path.len() > dir.len() && path.starts_with(&dir) && path.as_bytes()[dir.len()] == b'/'
}

/// FNV-1a: a stable name for a repository's folder in the data directory.
fn fnv(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5u32, |hash, byte| {
        (hash ^ byte as u32).wrapping_mul(0x0100_0193)
    })
}

/// What the review dialog showed, to apply exactly that.
fn patch_hash(patch: &[u8]) -> String {
    let hash = patch.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ *byte as u64).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}{:x}", patch.len())
}

/// A path as git printed it (`-z`: not quoted).
fn path_of(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).to_string())
    }
}

/// In a worktree's own git folder: the commit it was cut from.
const BASE_FILE: &str = "evo-base";
/// In a worktree's own git folder: the branch evo made for it, when the
/// caller named it (removal deletes that branch, and only an `evo/` one).
const BRANCH_FILE: &str = "evo-branch";

fn record_branch(worktree: &Path, branch: &str) -> Result<(), String> {
    let file = git_dir(worktree)?.join(BRANCH_FILE);
    std::fs::write(&file, format!("{branch}\n"))
        .map_err(|e| format!("could not write {}: {e}", file.display()))
}

/// The branch [`record_branch`] wrote, if it is one evo names.
fn recorded_branch(worktree: &Path) -> Option<String> {
    let text = std::fs::read_to_string(git_dir(worktree).ok()?.join(BRANCH_FILE)).ok()?;
    let branch = text.trim();
    branch
        .strip_prefix(NAMED_BRANCH_PREFIX)
        .is_some_and(|rest| !rest.is_empty())
        .then(|| branch.to_string())
}

/// `evo/<name>` for a branch the caller named, if Git takes it and no branch
/// of that name exists yet.
fn named_branch(main: &Path, name: &str) -> Result<String, String> {
    let name = name.trim().trim_start_matches(NAMED_BRANCH_PREFIX);
    if name.is_empty() {
        return Err("the branch name is empty".into());
    }
    let branch = format!("{NAMED_BRANCH_PREFIX}{name}");
    if git(main, &["check-ref-format", "--branch", &branch]).is_err() {
        return Err(format!("`{branch}` is not a valid branch name"));
    }
    let exists = git(
        main,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    );
    if exists.is_ok() {
        return Err(format!(
            "a branch `{branch}` already exists; choose another name"
        ));
    }
    Ok(branch)
}

fn git_dir(worktree: &Path) -> Result<PathBuf, String> {
    git(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-dir"],
    )
    .map(PathBuf::from)
}

fn record_base(worktree: &Path, base: &str) -> Result<(), String> {
    let file = git_dir(worktree)?.join(BASE_FILE);
    std::fs::write(&file, format!("{base}\n"))
        .map_err(|e| format!("could not write {}: {e}", file.display()))
}

/// `None` for a worktree made before the fork was recorded.
fn recorded_base(worktree: &Path) -> Option<String> {
    let text = std::fs::read_to_string(git_dir(worktree).ok()?.join(BASE_FILE)).ok()?;
    let base = text.trim();
    (base.len() >= 40 && base.bytes().all(|b| b.is_ascii_hexdigit())).then(|| base.to_string())
}

/// The top folder of the working tree `dir` is in.
fn toplevel(dir: &Path) -> Result<PathBuf, String> {
    git(dir, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .map_err(|_| "this folder is not in a Git repository".to_string())
}

/// The top folder of the working tree `dir` is in; `None` outside a repository.
pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    toplevel(dir).ok()
}

/// Unique per process and moment: time, process id and a counter.
fn new_name() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let salt = fnv(&format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )) & 0xffff;
    format!("{millis:x}{salt:04x}")
}

impl Worktrees {
    /// `data_root` holds the user-data-directory worktrees (`<agent dir>/worktrees`).
    pub fn with_data_root(data_root: PathBuf) -> Self {
        Self { data_root }
    }

    /// The repository's main working folder, also when `cwd` is inside one of
    /// its worktrees, so a worktree is never nested in another. Only for where
    /// worktrees live: changes are applied to `cwd`'s own working tree.
    fn main_root(cwd: &Path) -> Result<PathBuf, String> {
        let top = toplevel(cwd)?;
        let common = PathBuf::from(git(
            cwd,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?);
        // `<main>/.git` for ordinary repositories; a bare common dir has no main folder.
        match (common.file_name().and_then(|n| n.to_str()), common.parent()) {
            (Some(".git"), Some(main)) => Ok(main.to_path_buf()),
            _ => Ok(top),
        }
    }

    fn user_data_dir(&self, main: &Path) -> PathBuf {
        let name = main
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "repo".into());
        self.data_root
            .join(format!("{name}-{:08x}", fnv(&key(main))))
    }

    fn dir(&self, main: &Path, location: WorktreeLocation) -> PathBuf {
        match location {
            WorktreeLocation::Repository => main.join(REPO_DIR),
            WorktreeLocation::UserData => self.user_data_dir(main),
        }
    }

    /// Keep `<repo>/.evo/worktrees` out of `git status` without touching
    /// `.gitignore`.
    fn exclude(main: &Path) -> Result<(), String> {
        let common = PathBuf::from(git(
            main,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?);
        let file = common.join("info").join("exclude");
        let current = std::fs::read_to_string(&file).unwrap_or_default();
        if current.lines().any(|line| line.trim() == EXCLUDE_LINE) {
            return Ok(());
        }
        std::fs::create_dir_all(file.parent().expect("exclude has a folder"))
            .map_err(|e| e.to_string())?;
        let separator = if current.is_empty() || current.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        std::fs::write(
            &file,
            format!("{current}{separator}# evo managed worktrees\n{EXCLUDE_LINE}\n"),
        )
        .map_err(|e| format!("could not update {}: {e}", file.display()))
    }

    /// A new worktree of `cwd`'s repository, on a new branch at `HEAD`.
    pub fn create(&self, cwd: &Path, location: WorktreeLocation) -> Result<Created, String> {
        self.create_named(cwd, location, None)
    }

    /// [`create`](Self::create), on the branch `evo/<branch>` when the caller
    /// names one. A name Git refuses, or one a branch already has, is refused
    /// before anything is made.
    pub fn create_named(
        &self,
        cwd: &Path,
        location: WorktreeLocation,
        branch: Option<&str>,
    ) -> Result<Created, String> {
        let main = Self::main_root(cwd)?;
        let named = branch.map(|name| named_branch(&main, name)).transpose()?;
        // Where `cwd` sits in its working tree (`src/`), to start the child there too.
        let prefix = git(cwd, &["rev-parse", "--show-prefix"])?;
        let base = git(cwd, &["rev-parse", "HEAD"])
            .map_err(|_| "the repository has no commits yet".to_string())?;
        if location == WorktreeLocation::Repository {
            Self::exclude(&main)?;
        }
        let name = new_name();
        let dir = self.dir(&main, location);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        let path = dir.join(&name);
        let branch = named
            .clone()
            .unwrap_or_else(|| format!("{BRANCH_PREFIX}{name}"));
        let folder = path.to_string_lossy().to_string();
        let added = git(&main, &["worktree", "add", "-b", &branch, &folder, &base])
            .and_then(|_| record_base(&path, &base))
            .and_then(|_| match &named {
                Some(branch) => record_branch(&path, branch),
                None => Ok(()),
            });
        if let Err(error) = added {
            // Leave nothing behind: git may have made the branch and the folder.
            let _ = git(
                &main,
                &["worktree", "remove", "--force", "--force", &folder],
            );
            if path.symlink_metadata().is_ok() {
                let _ = std::fs::remove_dir_all(&path);
            }
            let _ = git(&main, &["worktree", "prune"]);
            let _ = git(&main, &["branch", "-D", &branch]);
            return Err(error);
        }
        let child_cwd = path.join(prefix.trim_end_matches('/'));
        let cwd = if child_cwd.is_dir() {
            child_cwd
        } else {
            path.clone()
        };
        Ok(Created {
            worktree: Worktree {
                path,
                branch,
                location: location_name(location),
                changed: false,
            },
            base,
            cwd,
        })
    }

    /// Whether `path` has uncommitted changes or commits beyond `base`. A
    /// folder that is gone, or is no longer a worktree, has nothing to keep.
    pub fn changed(path: &Path, base: Option<&str>) -> bool {
        if path.join(".git").symlink_metadata().is_err() {
            return false;
        }
        let dirty = git(path, &["status", "--porcelain"])
            .map(|out| !out.trim().is_empty())
            .unwrap_or(true);
        let moved = match base {
            Some(base) => git(path, &["rev-parse", "HEAD"])
                .map(|head| head != base)
                .unwrap_or(true),
            None => false,
        };
        dirty || moved
    }

    /// Managed worktrees of `cwd`'s repository, in either location.
    pub fn list(&self, cwd: &Path) -> Result<Vec<Worktree>, String> {
        let main = Self::main_root(cwd)?;
        let places = [
            (
                self.dir(&main, WorktreeLocation::Repository),
                WorktreeLocation::Repository,
            ),
            // Anything of this repository under the data root, however the
            // repository's own path was spelled when it was made.
            (self.data_root.clone(), WorktreeLocation::UserData),
        ];
        let porcelain = git(&main, &["worktree", "list", "--porcelain"])?;
        let mut found = Vec::new();
        for block in porcelain.split("\n\n") {
            let mut path = None;
            let mut branch = String::new();
            for line in block.lines() {
                if let Some(p) = line.strip_prefix("worktree ") {
                    path = Some(PathBuf::from(p));
                } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                    branch = b.to_string();
                }
            }
            let Some(path) = path else { continue };
            if let Some((_, location)) = places.iter().find(|(dir, _)| is_under(&path, dir)) {
                let changed = match recorded_base(&path) {
                    Some(base) => Self::changed(&path, Some(&base)),
                    // Made before the fork was recorded: commits on the branch
                    // beyond where `main` is now count as changes.
                    None => {
                        let ahead =
                            git(&main, &["rev-list", "--count", &format!("HEAD..{branch}")])
                                .map(|n| n.trim() != "0")
                                .unwrap_or(false);
                        ahead || Self::changed(&path, None)
                    }
                };
                found.push(Worktree {
                    changed,
                    path,
                    branch,
                    location: location_name(*location),
                });
            }
        }
        Ok(found)
    }

    /// The managed worktree of `cwd`'s repository that git has at `path`.
    fn registered(&self, cwd: &Path, path: &Path) -> Result<Option<Worktree>, String> {
        Ok(self.list(cwd)?.into_iter().find(|w| same(&w.path, path)))
    }

    /// A managed worktree of `cwd`'s repository at `path`, or an error.
    fn managed(&self, cwd: &Path, path: &Path) -> Result<Worktree, String> {
        self.registered(cwd, path)?.ok_or_else(|| not_managed(path))
    }

    /// A folder directly under a managed location of this repository that git
    /// no longer has as a worktree: what a removal leaves when something was
    /// still using the folder. Names are the ones [`new_name`] makes.
    fn leftover(&self, main: &Path, path: &Path) -> Result<PathBuf, String> {
        let named = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_hexdigit()));
        let placed = path.parent().is_some_and(|parent| {
            [WorktreeLocation::Repository, WorktreeLocation::UserData]
                .iter()
                .any(|location| same(parent, &self.dir(main, *location)))
        });
        if named && placed {
            Ok(path.to_path_buf())
        } else {
            Err(not_managed(path))
        }
    }

    /// What the worktree at `path` changed, against the working tree `cwd` is in.
    fn diff(&self, cwd: &Path, path: &Path) -> Result<Diff, String> {
        // Where it was cut from: `cwd`'s own working tree, which is not the
        // repository's main one when `cwd` is in a worktree itself.
        let target = toplevel(cwd)?;
        let worktree = self.managed(cwd, path)?.path;
        if same(&target, &worktree) {
            return Err(
                "a worktree's changes go to the folder it was cut from, not to itself".into(),
            );
        }
        let base = match recorded_base(&worktree) {
            Some(base) => base,
            // Made before the fork was recorded: where its history left the folder's.
            None => {
                let head = git(&target, &["rev-parse", "HEAD"])?;
                git(&worktree, &["merge-base", "HEAD", &head])?
            }
        };
        // Stage everything in the worktree's own index so new files count; the
        // repository's index is untouched.
        git(&worktree, &["add", "-A"])?;
        let patch = git_bytes(
            &worktree,
            &[
                "-c",
                "core.quotePath=false",
                "diff",
                "--cached",
                "--binary",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                &base,
            ],
        )?;
        // `-z`: names as they are, not quoted.
        let numstat = git_bytes(
            &worktree,
            &["diff", "--cached", "--numstat", "--no-renames", "-z", &base],
        )?;
        let statuses = git_bytes(
            &worktree,
            &[
                "diff",
                "--cached",
                "--name-status",
                "--no-renames",
                "-z",
                &base,
            ],
        )?;
        let mut counts = std::collections::HashMap::new();
        for record in numstat.split(|b| *b == 0) {
            let mut parts = record.splitn(3, |b| *b == b'\t');
            let mut number = || {
                let text = std::str::from_utf8(parts.next()?).ok()?;
                text.parse::<usize>().ok()
            };
            let (added, removed) = (number(), number());
            if let Some(name) = parts.next() {
                counts.insert(name, (added, removed));
            }
        }
        let (mut files, mut touched) = (Vec::new(), Vec::new());
        let mut fields = statuses.split(|b| *b == 0);
        while let (Some(status), Some(name)) = (fields.next(), fields.next()) {
            let status = match status.first() {
                Some(b'A') => "added",
                Some(b'D') => "deleted",
                _ => "modified",
            };
            let (added, removed) = counts.get(name).copied().unwrap_or((None, None));
            files.push(FileChange {
                path: String::from_utf8_lossy(name).to_string(),
                status,
                added,
                removed,
            });
            touched.push(path_of(name));
        }
        Ok(Diff {
            changes: Changes {
                added: files.iter().filter_map(|f| f.added).sum(),
                removed: files.iter().filter_map(|f| f.removed).sum(),
                base,
                files,
                hash: patch_hash(&patch),
                patch: String::from_utf8_lossy(&patch).to_string(),
            },
            patch,
            target,
            touched,
        })
    }

    /// Everything the worktree at `path` changed since it was cut from the
    /// working tree `cwd` is in: its commits, uncommitted edits and new files,
    /// as one patch.
    pub fn changes(&self, cwd: &Path, path: &Path) -> Result<Changes, String> {
        self.diff(cwd, path).map(|diff| diff.changes)
    }

    /// Bring the worktree's changes into the working tree `cwd` is in
    /// (uncommitted, nothing staged), then remove the worktree and its branch.
    /// All of the patch lands or nothing is changed. `expected` is the
    /// [`Changes::hash`] that was reviewed: other changes are refused.
    pub fn apply(
        &self,
        cwd: &Path,
        path: &Path,
        expected: Option<&str>,
    ) -> Result<Applied, String> {
        let diff = self.diff(cwd, path)?;
        if expected.is_some_and(|hash| hash != diff.changes.hash) {
            return Err(CHANGED_SINCE_REVIEW.into());
        }
        if !diff.patch.iter().all(u8::is_ascii_whitespace) {
            apply_patch(&diff.target, &diff.patch, &diff.touched)?;
        }
        // The changes are in: a folder that cannot be removed yet is not a failure.
        let cleaned = self.remove(cwd, path).is_ok_and(|removed| removed.cleaned);
        Ok(Applied {
            changes: diff.changes,
            cleaned,
        })
    }

    /// Remove a managed worktree and its branch. Refuses anything else.
    ///
    /// A folder something still uses (a process working in it, on Windows) is
    /// left: its branch and git's record of it go, `cleaned` is `false`, and
    /// removing the same path again later deletes what is left.
    pub fn remove(&self, cwd: &Path, path: &Path) -> Result<Removed, String> {
        let main = Self::main_root(cwd)?;
        let (folder, registered) = match self.registered(cwd, path)? {
            Some(worktree) => (worktree.path, true),
            None => (self.leftover(&main, path)?, false),
        };
        // A branch the caller named, read before the worktree's git folder goes.
        let named = registered.then(|| recorded_branch(&folder)).flatten();
        let mut detail = None;
        if registered {
            let folder = folder.to_string_lossy();
            // Twice: a locked worktree too.
            if let Err(error) = git(
                &main,
                &["worktree", "remove", "--force", "--force", &folder],
            ) {
                detail = Some(error);
            }
        }
        // Whatever git left, or a folder it no longer knows.
        if folder.symlink_metadata().is_ok() {
            if let Err(error) = std::fs::remove_dir_all(&folder) {
                detail.get_or_insert(format!("could not delete {}: {error}", folder.display()));
            }
        }
        let _ = git(&main, &["worktree", "prune"]);
        // By name: the child may have switched to another branch.
        let made = named.or_else(|| {
            folder
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("{BRANCH_PREFIX}{name}"))
        });
        if let Some(branch) = made {
            let exists = git(
                &main,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{branch}"),
                ],
            );
            if exists.is_ok() {
                if let Err(error) = git(&main, &["branch", "-D", &branch]) {
                    detail.get_or_insert(error);
                }
            }
        }
        let cleaned = folder.symlink_metadata().is_err();
        Ok(Removed {
            detail: if cleaned { None } else { detail },
            cleaned,
        })
    }
}

fn not_managed(path: &Path) -> String {
    format!(
        "{} is not a managed worktree of this repository",
        path.display()
    )
}

/// Said when the worktree is not what the review showed any more.
pub const CHANGED_SINCE_REVIEW: &str =
    "the worktree changed since you opened this review: look at the changes again, then apply them";

/// A worktree's changes and what applying them needs.
struct Diff {
    changes: Changes,
    /// As git wrote it: text that is not UTF-8 stays what it is.
    patch: Vec<u8>,
    /// The working tree it applies to.
    target: PathBuf,
    /// Every path the patch writes or deletes, relative to `target`.
    touched: Vec<PathBuf>,
}

/// How an apply ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub changes: Changes,
    /// The worktree's folder is gone. `false`: the changes are applied but
    /// something still uses the folder; see [`Worktrees::remove`].
    pub cleaned: bool,
}

/// How a removal ended.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Removed {
    /// The folder is gone.
    pub cleaned: bool,
    /// Why it is still there.
    pub detail: Option<String>,
}

#[cfg(test)]
thread_local! {
    /// Tests: the apply fails after git wrote the files.
    static FAIL_AFTER_APPLY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Apply `patch` to the working tree `target`: all of it, or nothing.
fn apply_patch(target: &Path, patch: &[u8], touched: &[PathBuf]) -> Result<(), String> {
    let work = std::env::temp_dir().join(format!("evo-worktree-{}", new_name()));
    std::fs::create_dir_all(&work).map_err(|e| format!("could not write the patch: {e}"))?;
    let file = work.join("changes.patch");
    let file_arg = file.to_string_lossy().to_string();
    let mut keep = false;
    let result = (|| {
        std::fs::write(&file, patch).map_err(|e| format!("could not write the patch: {e}"))?;
        git(
            target,
            &["apply", "--check", "--whitespace=nowarn", &file_arg],
        )
        .map_err(|error| {
            // What git says when the folder differs where the patch changes it.
            if error.contains("patch does not apply") || error.contains("patch failed") {
                format!("the changes do not apply cleanly to your working folder (it changed in the same places): {error}")
            } else {
                format!("the changes cannot be applied to your working folder: {error}")
            }
        })?;
        // The check does not see a file another program holds: keep what is
        // there to put it back if git stops part-way.
        let saved = snapshot::Snapshot::take(target, touched, &work.join("saved"))
            .map_err(|e| format!("could not save your working folder before applying: {e}"))?;
        let applied = git(target, &["apply", "--whitespace=nowarn", &file_arg]);
        #[cfg(test)]
        let applied = applied.and_then(|out| {
            if FAIL_AFTER_APPLY.with(|fail| fail.get()) {
                Err("stopped by the test".into())
            } else {
                Ok(out)
            }
        });
        let Err(error) = applied else { return Ok(()) };
        match saved.restore() {
            Ok(()) => Err(format!(
                "applying the changes failed and your working folder was put back as it was: {error}"
            )),
            Err(left) => {
                keep = true;
                Err(format!(
                    "applying the changes failed ({error}) and some files could not be put back: {left}. What they held before is saved in {}",
                    work.join("saved").display()
                ))
            }
        }
    })();
    if !keep {
        let _ = std::fs::remove_dir_all(&work);
    }
    result
}

/// Git's empty tree: the base of a repository with no commit yet.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// New files are only measured up to this size and this many.
const UNTRACKED_MAX_BYTES: u64 = 2 * 1024 * 1024;
const UNTRACKED_MAX_FILES: usize = 500;

/// Where a working folder's repository stands: its branch, how far it is from
/// its upstream, and what is changed against `HEAD` (staged, unstaged and new
/// files together). Read-only: nothing is staged or written.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    /// The repository's top folder; `files` paths are relative to it.
    pub root: PathBuf,
    /// `None` on a detached `HEAD`.
    pub branch: Option<String>,
    /// Short commit id; `None` before the first commit.
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<FileChange>,
    pub added: usize,
    pub removed: usize,
}

/// Lines of a new file, `None` when it looks binary or is too big to read.
fn count_lines(path: &Path) -> Option<usize> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > UNTRACKED_MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return None;
    }
    let newlines = bytes.iter().filter(|&&b| b == b'\n').count();
    Some(if bytes.last().is_some_and(|&b| b != b'\n') {
        newlines + 1
    } else {
        newlines
    })
}

/// The status of the repository `dir` is in; `None` outside a repository.
pub fn repo_status(dir: &Path) -> Result<Option<RepoStatus>, String> {
    let Ok(root) = git(dir, &["rev-parse", "--show-toplevel"]) else {
        return Ok(None);
    };
    let root = PathBuf::from(root);
    let status = git(
        &root,
        &[
            "-c",
            "core.quotePath=false",
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ],
    )?;

    let (mut branch, mut head, mut upstream, mut ahead, mut behind) = (None, None, None, 0, 0);
    // Path → `added`, `modified` or `deleted`, in git's order.
    let mut changed: Vec<(String, &'static str)> = Vec::new();
    let kind = |xy: &str| {
        if xy.contains('D') {
            "deleted"
        } else if xy.contains('A') {
            "added"
        } else {
            "modified"
        }
    };
    for line in status.lines() {
        if let Some(header) = line.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" if value != "(initial)" => {
                    head = Some(value.chars().take(7).collect())
                }
                "branch.head" if value != "(detached)" => branch = Some(value.to_string()),
                "branch.upstream" => upstream = Some(value.to_string()),
                "branch.ab" => {
                    for part in value.split(' ') {
                        if let Some(n) = part.strip_prefix('+') {
                            ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let mut fields = line.splitn(2, ' ');
        let (tag, rest) = (
            fields.next().unwrap_or_default(),
            fields.next().unwrap_or_default(),
        );
        // Ordinary (1), renamed (2) and unmerged (u) entries end with the path.
        let skip = match tag {
            "1" => 7,
            "2" => 8,
            "u" => 9,
            "?" => {
                changed.push((rest.to_string(), "added"));
                continue;
            }
            _ => continue,
        };
        let xy = rest.split(' ').next().unwrap_or_default();
        let path = rest.splitn(skip + 1, ' ').nth(skip).unwrap_or_default();
        // A rename's path is `new\told`.
        let path = path.split('\t').next().unwrap_or_default().to_string();
        changed.push((path, kind(xy)));
    }

    // Line counts of tracked changes against HEAD, without touching the index.
    let base = if head.is_some() { "HEAD" } else { EMPTY_TREE };
    let numstat = git(
        &root,
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            base,
            "--numstat",
            "--no-renames",
        ],
    )
    .unwrap_or_default();
    let mut counts = std::collections::HashMap::new();
    for line in numstat.lines() {
        let mut parts = line.splitn(3, '\t');
        let added = parts.next().and_then(|n| n.parse::<usize>().ok());
        let removed = parts.next().and_then(|n| n.parse::<usize>().ok());
        if let Some(path) = parts.next() {
            counts.insert(path.to_string(), (added, removed));
        }
    }

    let mut measured = 0;
    let files: Vec<FileChange> = changed
        .into_iter()
        .map(|(path, status)| {
            let (added, removed) = match counts.get(&path) {
                Some(&count) => count,
                // Untracked: every line is new.
                None if status == "added" && measured < UNTRACKED_MAX_FILES => {
                    measured += 1;
                    (count_lines(&root.join(&path)), Some(0))
                }
                None => (None, None),
            };
            FileChange {
                path,
                status,
                added,
                removed,
            }
        })
        .collect();

    Ok(Some(RepoStatus {
        added: files.iter().filter_map(|f| f.added).sum(),
        removed: files.iter().filter_map(|f| f.removed).sum(),
        root,
        branch,
        head,
        upstream,
        ahead,
        behind,
        files,
    }))
}

/// One file a worktree changed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    /// `added`, `modified` or `deleted`.
    pub status: &'static str,
    /// Lines; `None` for binary files.
    pub added: Option<usize>,
    pub removed: Option<usize>,
}

/// What a worktree changed since it was cut.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    /// The commit it was cut from.
    pub base: String,
    pub files: Vec<FileChange>,
    pub added: usize,
    pub removed: usize,
    /// The `git apply` patch as text, for display: bytes that are not UTF-8
    /// show as U+FFFD here, and are applied as they are.
    pub patch: String,
    /// Names exactly this patch: pass it to [`Worktrees::apply`] to apply
    /// what was reviewed and nothing newer.
    pub hash: String,
}

/// Largest file whose `HEAD` text is read for line markers.
const HEAD_TEXT_MAX: usize = 1_000_000;

/// Paths under `dir`'s repository that git ignores, as absolute paths with
/// forward slashes (a folder ends with `/`). Read-only; `None` outside a
/// repository.
pub fn ignored_paths(dir: &Path) -> Result<Option<Vec<String>>, String> {
    let Ok(root) = git(dir, &["rev-parse", "--show-toplevel"]) else {
        return Ok(None);
    };
    let root = root.replace('\\', "/");
    let list = git_raw(
        Path::new(&root),
        &[
            "-c",
            "core.quotePath=false",
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ],
    )?;
    let top = root.trim_end_matches('/');
    Ok(Some(
        list.split('\0')
            .filter(|p| !p.is_empty())
            .map(|p| format!("{top}/{p}"))
            .collect(),
    ))
}

/// The text of `path` at `HEAD`. `None` when the file is untracked, binary,
/// too big, outside the repository, or there is no repository or commit yet.
pub fn file_at_head(dir: &Path, path: &Path) -> Result<Option<String>, String> {
    let Ok(root) = git(dir, &["rev-parse", "--show-toplevel"]) else {
        return Ok(None);
    };
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        dir.join(path)
    };
    let absolute = absolute.to_string_lossy().replace('\\', "/");
    let fold = |s: &str| {
        if cfg!(any(windows, target_os = "macos")) {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    let prefix = format!("{}/", fold(root));
    if !fold(&absolute).starts_with(&prefix)
        || absolute.len() <= prefix.len()
        || !absolute.is_char_boundary(prefix.len())
    {
        return Ok(None);
    }
    let relative = &absolute[prefix.len()..];
    if relative.split('/').any(|part| part == "..") {
        return Ok(None);
    }
    let Ok(text) = git_raw(
        Path::new(root),
        &[
            "-c",
            "core.fsmonitor=false",
            "show",
            &format!("HEAD:{relative}"),
        ],
    ) else {
        return Ok(None);
    };
    if text.len() > HEAD_TEXT_MAX || text.contains('\0') {
        return Ok(None);
    }
    Ok(Some(text))
}

fn location_name(location: WorktreeLocation) -> &'static str {
    match location {
        WorktreeLocation::Repository => "repository",
        WorktreeLocation::UserData => "userData",
    }
}

#[cfg(test)]
mod review_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// A repository with one commit, in a temporary folder; `None` without git.
    pub(super) fn repo() -> Option<(tempfile::TempDir, PathBuf)> {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir_all(root.join("src")).unwrap();
        if git(&root, &["init", "-q"]).is_err() {
            eprintln!("skipping: git is not available");
            return None;
        }
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        for args in [
            &["config", "user.email", "evo@example.invalid"][..],
            &["config", "user.name", "evo"],
            &["add", "."],
            &["commit", "-q", "-m", "init"],
        ] {
            git(&root, args).unwrap();
        }
        Some((dir, root))
    }

    #[test]
    fn repo_status_counts_tracked_and_new_files() {
        let Some((dir, root)) = repo() else { return };
        assert_eq!(
            repo_status(dir.path()).unwrap(),
            None,
            "outside a repository"
        );

        let clean = repo_status(&root.join("src")).unwrap().unwrap();
        assert!(clean.files.is_empty());
        assert!(clean.branch.is_some() && clean.head.is_some());

        std::fs::write(root.join("src/main.rs"), "fn main() {\n    run();\n}\n").unwrap();
        std::fs::write(root.join("src/new file.rs"), "a\nb\nc").unwrap();
        let status = repo_status(&root).unwrap().unwrap();
        let file = |p: &str| status.files.iter().find(|f| f.path == p).unwrap().clone();
        assert_eq!(file("src/main.rs").status, "modified");
        assert_eq!(
            (file("src/main.rs").added, file("src/main.rs").removed),
            (Some(3), Some(1))
        );
        assert_eq!(file("src/new file.rs").status, "added");
        assert_eq!(file("src/new file.rs").added, Some(3));
        assert_eq!((status.added, status.removed), (6, 1));
    }

    #[test]
    fn head_text_and_ignored_paths() {
        let Some((dir, root)) = repo() else { return };
        assert_eq!(
            file_at_head(dir.path(), &dir.path().join("x")).unwrap(),
            None,
            "outside a repository"
        );

        std::fs::write(root.join("src/main.rs"), "changed\n").unwrap();
        let head = file_at_head(&root, &root.join("src/main.rs")).unwrap();
        assert_eq!(head.as_deref(), Some("fn main() {}\n"));
        assert_eq!(
            file_at_head(&root.join("src"), Path::new("main.rs"))
                .unwrap()
                .as_deref(),
            Some("fn main() {}\n")
        );
        std::fs::write(root.join("src/new.rs"), "x").unwrap();
        assert_eq!(
            file_at_head(&root, &root.join("src/new.rs")).unwrap(),
            None,
            "untracked"
        );
        assert_eq!(
            file_at_head(&root, &dir.path().join("elsewhere.txt")).unwrap(),
            None,
            "outside the root"
        );
        assert_eq!(
            file_at_head(&root, &root.join("src/../src/main.rs")).unwrap(),
            None,
            "parent segments"
        );

        std::fs::write(root.join(".gitignore"), "target/\n*.log\n").unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("target/a"), "1").unwrap();
        std::fs::write(root.join("a.log"), "1").unwrap();
        let ignored = ignored_paths(&root).unwrap().unwrap();
        assert!(
            ignored.iter().any(|p| p.ends_with("/target/")),
            "{ignored:?}"
        );
        assert!(ignored.iter().any(|p| p.ends_with("/a.log")), "{ignored:?}");
        assert_eq!(ignored_paths(dir.path()).unwrap(), None);
    }

    // docs/features/session-roles-and-mail/spec.md (FR-9).
    #[test]
    fn named_branch_is_made_refused_when_taken_or_invalid_and_removed() {
        let Some((dir, root)) = repo() else { return };
        let worktrees = Worktrees::with_data_root(dir.path().join("data"));
        let created = worktrees
            .create_named(&root, WorktreeLocation::UserData, Some("api-auth"))
            .unwrap();
        assert_eq!(created.worktree.branch, "evo/api-auth");
        let listed = worktrees.list(&root).unwrap();
        assert!(
            listed.iter().any(|w| w.branch == "evo/api-auth"),
            "{listed:?}"
        );
        // Taken or invalid: refused, and nothing new is made.
        let before = worktrees.list(&root).unwrap().len();
        let error = worktrees
            .create_named(&root, WorktreeLocation::UserData, Some("api-auth"))
            .unwrap_err();
        assert!(error.contains("already exists"), "{error}");
        let error = worktrees
            .create_named(&root, WorktreeLocation::UserData, Some("bad name..x"))
            .unwrap_err();
        assert!(error.contains("not a valid branch name"), "{error}");
        assert_eq!(worktrees.list(&root).unwrap().len(), before);
        // A person's own branch of another name is never touched by removal.
        git(&root, &["branch", "evo/keep-me"]).unwrap();
        worktrees.remove(&root, &created.worktree.path).unwrap();
        let branches = git(&root, &["branch", "--list", "evo/*"]).unwrap();
        assert!(!branches.contains("evo/api-auth"), "{branches}");
        assert!(branches.contains("evo/keep-me"), "{branches}");
    }

    #[test]
    fn inside_the_repository_and_excluded_from_git() {
        let Some((dir, root)) = repo() else { return };
        let worktrees = Worktrees::with_data_root(dir.path().join("data"));
        let created = worktrees
            .create(&root.join("src"), WorktreeLocation::Repository)
            .unwrap();
        assert!(is_under(
            &created.worktree.path,
            &root.join(".evo/worktrees")
        ));
        assert!(created.worktree.branch.starts_with(BRANCH_PREFIX));
        // The child works in the same subfolder as the parent.
        assert!(created.cwd.ends_with("src") && created.cwd.join("main.rs").is_file());
        // `.gitignore` untouched, the folder excluded locally, the main tree clean.
        assert!(!root.join(".gitignore").exists());
        let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).unwrap();
        assert!(exclude.contains(EXCLUDE_LINE));
        assert_eq!(git(&root, &["status", "--porcelain"]).unwrap(), "");
        // A second worktree adds no second exclude line.
        worktrees
            .create(&root, WorktreeLocation::Repository)
            .unwrap();
        let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.matches(EXCLUDE_LINE).count(), 1);
    }

    #[test]
    fn both_locations_are_listed_and_removable() {
        let Some((dir, root)) = repo() else { return };
        let worktrees = Worktrees::with_data_root(dir.path().join("data"));
        let inside = worktrees
            .create(&root, WorktreeLocation::Repository)
            .unwrap();
        let outside = worktrees.create(&root, WorktreeLocation::UserData).unwrap();
        assert!(is_under(&outside.worktree.path, &dir.path().join("data")));

        let listed = worktrees.list(&root).unwrap();
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert!(listed.iter().all(|w| !w.changed));

        // A change in one shows up, from the worktree and from the list.
        std::fs::write(outside.worktree.path.join("new.txt"), "x").unwrap();
        assert!(Worktrees::changed(
            &outside.worktree.path,
            Some(&outside.base)
        ));
        assert!(!Worktrees::changed(
            &inside.worktree.path,
            Some(&inside.base)
        ));
        let changed: Vec<_> = worktrees
            .list(&root)
            .unwrap()
            .into_iter()
            .filter(|w| w.changed)
            .collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(key(&changed[0].path), key(&outside.worktree.path));

        // Listed from inside a worktree too, and removed with their branches.
        assert_eq!(worktrees.list(&inside.worktree.path).unwrap().len(), 2);
        worktrees.remove(&root, &inside.worktree.path).unwrap();
        worktrees.remove(&root, &outside.worktree.path).unwrap();
        assert!(worktrees.list(&root).unwrap().is_empty());
        assert!(!inside.worktree.path.exists() && !outside.worktree.path.exists());
        let branches = git(&root, &["branch", "--list", &format!("{BRANCH_PREFIX}*")]).unwrap();
        assert!(branches.trim().is_empty(), "branches left: {branches}");
    }

    #[test]
    fn only_managed_worktrees_are_removed() {
        let Some((dir, root)) = repo() else { return };
        let worktrees = Worktrees::with_data_root(dir.path().join("data"));
        let elsewhere = dir.path().join("mine");
        git(
            &root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "mine",
                &elsewhere.to_string_lossy(),
            ],
        )
        .unwrap();
        assert!(worktrees.list(&root).unwrap().is_empty());
        assert!(worktrees.remove(&root, &elsewhere).is_err());
        assert!(elsewhere.exists());
    }

    #[test]
    fn changes_are_measured_from_the_fork_and_applied_to_the_working_folder() {
        let Some((dir, root)) = repo() else { return };
        let worktrees = Worktrees::with_data_root(dir.path().join("data"));
        let created = worktrees.create(&root, WorktreeLocation::UserData).unwrap();
        let wt = &created.worktree.path;
        // An edit, a new file, a deleted file, and a commit in between.
        std::fs::write(wt.join("src/main.rs"), "fn main() { println!(\"hi\"); }\n").unwrap();
        git(wt, &["commit", "-qam", "say hi"]).unwrap();
        std::fs::write(wt.join("NOTES.md"), "new\nfile\n").unwrap();
        std::fs::create_dir_all(wt.join("docs")).unwrap();
        std::fs::write(wt.join("docs/a.md"), "a\n").unwrap();

        let changes = worktrees.changes(&root, wt).unwrap();
        assert_eq!(changes.base, created.base);
        let mut paths: Vec<_> = changes
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.status))
            .collect();
        paths.sort();
        assert_eq!(
            paths,
            [
                ("NOTES.md", "added"),
                ("docs/a.md", "added"),
                ("src/main.rs", "modified")
            ]
        );
        assert_eq!((changes.added, changes.removed), (4, 1));
        assert!(changes.patch.contains("+fn main() { println!(\"hi\"); }"));
        // The repository is untouched until applied.
        assert_eq!(git(&root, &["status", "--porcelain"]).unwrap(), "");

        worktrees.apply(&root, wt, None).unwrap();
        // Line endings follow the repository's own settings (core.autocrlf).
        let read = |p: &str| {
            std::fs::read_to_string(root.join(p))
                .unwrap()
                .replace("\r\n", "\n")
        };
        assert_eq!(read("NOTES.md"), "new\nfile\n");
        assert!(std::fs::read_to_string(root.join("src/main.rs"))
            .unwrap()
            .contains("hi"));
        // Uncommitted, unstaged, history unchanged; the worktree and branch are gone.
        let status = git(&root, &["status", "--porcelain"]).unwrap();
        assert!(
            status.contains(" M src/main.rs") && status.contains("?? NOTES.md"),
            "{status}"
        );
        assert_eq!(git(&root, &["rev-parse", "HEAD"]).unwrap(), created.base);
        assert!(!wt.exists());
        assert!(worktrees.list(&root).unwrap().is_empty());
    }

    #[test]
    fn a_conflicting_apply_changes_nothing() {
        let Some((dir, root)) = repo() else { return };
        let worktrees = Worktrees::with_data_root(dir.path().join("data"));
        let created = worktrees
            .create(&root, WorktreeLocation::Repository)
            .unwrap();
        std::fs::write(
            created.worktree.path.join("src/main.rs"),
            "fn main() { worktree(); }\n",
        )
        .unwrap();
        // The working folder changed the same line meanwhile.
        std::fs::write(root.join("src/main.rs"), "fn main() { mine(); }\n").unwrap();
        let error = worktrees
            .apply(&root, &created.worktree.path, None)
            .unwrap_err();
        assert!(error.contains("do not apply cleanly"), "{error}");
        assert_eq!(
            std::fs::read_to_string(root.join("src/main.rs"))
                .unwrap()
                .replace("\r\n", "\n"),
            "fn main() { mine(); }\n"
        );
        assert!(
            created.worktree.path.exists(),
            "the worktree is kept to try again"
        );
    }

    #[test]
    fn outside_a_repository_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let worktrees = Worktrees::with_data_root(dir.path().join("data"));
        if git(dir.path(), &["--version"]).is_err() {
            return;
        }
        let error = worktrees
            .create(dir.path(), WorktreeLocation::Repository)
            .unwrap_err();
        assert!(error.contains("not in a Git repository"), "{error}");
    }
}
