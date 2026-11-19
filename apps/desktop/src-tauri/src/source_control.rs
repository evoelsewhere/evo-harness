//! Source control from the app: the person's git operations on a chat's
//! folder (status, diffs, stage / unstage / discard with undo, commit and
//! amend, recent subjects) over `evo_worktrees::scm`, and dropping a deleted
//! chat's turn checkpoints (`evo_worktrees::checkpoint`). Every write here
//! follows a click in the Source control tool (ADR 0024); no agent tool
//! calls these commands.
//!
//! Spec: `docs/features/source-control/spec.md` (FR-1 – FR-6, FR-13, FR-14);
//! plan 3.3 "Shell commands", 3.5 "Commit", 3.7 "Paths".
//!
//! Paths from the window are relative to the repository's top folder (as
//! `scm_status` lists them) or absolute inside it; anything else is refused
//! before git runs. A discard keeps what it removed under an undo token for
//! this app run ([`Discards`], the last [`UNDO_KEPT`]). A commit asks first
//! (`needsConfirm`) while a run is active in the folder (any chat, from the
//! supervisor's run spans) or, for an amend, when `HEAD` is already on its
//! upstream.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use evo_store::logs::NewLog;
use evo_worktrees::scm::{self, Area, Committed, FileDiff, Kept, Repository, ScmStatus, Target};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::Db;
use crate::supervisor::{folder_key, EngineSupervisor};

/// Discards that can still be undone, at most (oldest forgotten first).
pub const UNDO_KEPT: usize = 50;
/// Recent commit subjects read at most.
const SUBJECTS_MAX: usize = 200;

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())?
}

/// What [`resolve`] refuses.
fn outside(path: &str) -> String {
    format!("not a path inside the repository: {path:?}")
}

fn is_absolute(path: &str) -> bool {
    let path = path.replace('\\', "/");
    path.starts_with('/') || path.chars().nth(1) == Some(':')
}

/// `path` relative to the repository's top folder with forward slashes, as
/// `evo_worktrees::scm` takes it. A relative path is taken as relative to the
/// top folder; an absolute one must be inside `root()` (compared without case
/// on Windows and macOS). Empty paths, `..` and paths that leave the
/// repository are refused.
fn resolve(path: &str, root: impl FnOnce() -> Option<PathBuf>) -> Result<String, String> {
    if path.contains('\0') {
        return Err(outside(path));
    }
    let normal = path.replace('\\', "/");
    let relative = if is_absolute(&normal) {
        let root = root().ok_or_else(|| "this folder is not in a Git repository".to_string())?;
        let root = root.to_string_lossy().replace('\\', "/");
        let root = root.strip_prefix("//?/").unwrap_or(&root).trim_end_matches('/').to_string();
        let normal = normal.strip_prefix("//?/").unwrap_or(&normal);
        let fits = normal.len() > root.len()
            && normal.is_char_boundary(root.len())
            && folder_key(Path::new(&normal[..root.len()])) == folder_key(Path::new(&root))
            && normal[root.len()..].starts_with('/');
        if !fits {
            return Err(outside(path));
        }
        normal[root.len() + 1..].to_string()
    } else {
        normal
    };
    let mut parts = Vec::new();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(outside(path)),
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(outside(path));
    }
    Ok(parts.join("/"))
}

/// [`resolve`] for a path of the repository `cwd` is in.
fn resolve_in(cwd: &Path, path: &str) -> Result<String, String> {
    resolve(path, || evo_worktrees::repo_root(cwd))
}

/// `target` with its path resolved inside the repository.
fn resolve_target(cwd: &Path, target: Target) -> Result<Target, String> {
    Ok(match target {
        Target::All => Target::All,
        Target::File { path } => Target::File { path: resolve_in(cwd, &path)? },
        Target::Hunk { path, hunk, expected } => Target::Hunk { path: resolve_in(cwd, &path)?, hunk, expected },
    })
}

/// A commit for `scm_diff_between`: an object id or a ref name (a turn's
/// checkpoint), never an option, a range or a revision expression.
fn revision(name: &str) -> Result<&str, String> {
    let fine = !name.is_empty()
        && name.len() <= 256
        && !name.starts_with('-')
        && !name.contains("..")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'));
    if fine {
        Ok(name)
    } else {
        Err(format!("not a commit: {name:?}"))
    }
}

/// One discard that can be undone.
struct Discarded {
    token: String,
    /// [`folder_key`] of the folder it was made in: undo writes there only.
    folder: String,
    kept: Vec<Kept>,
}

#[derive(Default)]
struct DiscardsInner {
    next: u64,
    recent: VecDeque<Discarded>,
}

/// What recent discards removed, by undo token, for this app run (FR-5).
/// Managed as Tauri state.
#[derive(Default)]
pub struct Discards(Mutex<DiscardsInner>);

impl Discards {
    fn inner(&self) -> std::sync::MutexGuard<'_, DiscardsInner> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Keep what a discard in `folder` removed; the token undoes it.
    pub fn keep(&self, folder: &Path, kept: Vec<Kept>) -> String {
        let mut inner = self.inner();
        inner.next += 1;
        let token = format!("discard-{}", inner.next);
        inner.recent.push_back(Discarded { token: token.clone(), folder: folder_key(folder), kept });
        while inner.recent.len() > UNDO_KEPT {
            inner.recent.pop_front();
        }
        token
    }

    /// What the discard `token` in `folder` removed; it is forgotten (an undo
    /// that fails puts it back with [`Self::put_back`]).
    pub fn take(&self, folder: &Path, token: &str) -> Result<Vec<Kept>, String> {
        let mut inner = self.inner();
        let Some(at) = inner.recent.iter().position(|discarded| discarded.token == token) else {
            return Err("this discard can no longer be undone".to_string());
        };
        if inner.recent[at].folder != folder_key(folder) {
            return Err("this discard was made in another folder".to_string());
        }
        Ok(inner.recent.remove(at).map(|discarded| discarded.kept).unwrap_or_default())
    }

    /// An undo of `token` failed: it can be tried again.
    fn put_back(&self, folder: &Path, token: String, kept: Vec<Kept>) {
        let mut inner = self.inner();
        inner.recent.push_back(Discarded { token, folder: folder_key(folder), kept });
        while inner.recent.len() > UNDO_KEPT {
            inner.recent.pop_front();
        }
    }
}

/// `scm_commit`'s options (`{ message, amend, confirmRunning }`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOptions {
    pub message: String,
    #[serde(default)]
    pub amend: bool,
    /// The person said yes to committing while a run is active, or to
    /// amending a commit that is on the upstream.
    #[serde(default)]
    pub confirm_running: bool,
}

/// Why a commit waits for the person's yes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsConfirm {
    /// Chats with a run active in this folder.
    pub running: Vec<String>,
    /// An amend would rewrite a commit already on the upstream.
    pub pushed: bool,
}

/// `scm_commit`'s answer: `{ commit, branch }` or `{ needsConfirm: { running, pushed } }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum CommitOutcome {
    Committed(Committed),
    #[serde(rename_all = "camelCase")]
    NeedsConfirm {
        needs_confirm: NeedsConfirm,
    },
}

/// Commit in `cwd` unless the person must say yes first (`running`: chats
/// with a run active in the folder).
fn commit_blocking(cwd: &Path, options: &CommitOptions, running: Vec<String>) -> Result<CommitOutcome, String> {
    if options.message.trim().is_empty() {
        return Err("write a commit message first".to_string());
    }
    if !options.confirm_running {
        let pushed = options.amend && scm::head_on_upstream(cwd);
        if !running.is_empty() || pushed {
            return Ok(CommitOutcome::NeedsConfirm { needs_confirm: NeedsConfirm { running, pushed } });
        }
    }
    scm::commit(cwd, &options.message, options.amend).map(CommitOutcome::Committed)
}

/// Discard in `cwd` and keep what went under a new undo token.
fn discard_blocking(discards: &Discards, cwd: &Path, target: Target) -> Result<String, String> {
    let target = resolve_target(cwd, target)?;
    let kept = scm::discard(cwd, &target)?;
    Ok(discards.keep(cwd, kept))
}

/// Put back what the discard `undo` removed.
fn undo_blocking(discards: &Discards, cwd: &Path, undo: &str) -> Result<(), String> {
    let kept = discards.take(cwd, undo)?;
    scm::restore_kept(cwd, &kept).inspect_err(|_| discards.put_back(cwd, undo.to_string(), kept.clone()))
}

/// The repository `cwd` is in, then the repositories in its subfolders, each
/// with its branch and number of changes (FR-17; plan 3.9). Empty when there is none.
#[tauri::command]
pub async fn scm_repositories(cwd: String) -> Result<Vec<Repository>, String> {
    blocking(move || scm::repositories(&PathBuf::from(cwd))).await
}

/// Where the repository `cwd` is in stands, with staged and unstaged changes
/// split; `null` outside a repository (FR-1).
#[tauri::command]
pub async fn scm_status(cwd: String) -> Result<Option<ScmStatus>, String> {
    blocking(move || scm::status(&PathBuf::from(cwd))).await
}

/// The diff of `path` in `area` (`staged` | `unstaged`), with its hunks (FR-2, FR-4).
#[tauri::command]
pub async fn scm_diff(cwd: String, path: String, area: Area) -> Result<FileDiff, String> {
    blocking(move || {
        let cwd = PathBuf::from(cwd);
        let path = resolve_in(&cwd, &path)?;
        scm::diff(&cwd, &path, area)
    })
    .await
}

/// The diff of `path` between two commits: a turn's checkpoints (FR-10).
#[tauri::command]
pub async fn scm_diff_between(cwd: String, from: String, to: String, path: String) -> Result<FileDiff, String> {
    blocking(move || {
        let cwd = PathBuf::from(cwd);
        let path = resolve_in(&cwd, &path)?;
        scm::diff_between(&cwd, revision(&from)?, revision(&to)?, &path)
    })
    .await
}

/// Stage everything, a file or a hunk (`{ kind: "all" | "file" | "hunk", … }`, FR-4).
#[tauri::command]
pub async fn scm_stage(cwd: String, target: Target) -> Result<(), String> {
    blocking(move || {
        let cwd = PathBuf::from(cwd);
        scm::stage(&cwd, &resolve_target(&cwd, target)?)
    })
    .await
}

/// Take everything, a file or a hunk out of the index (FR-4).
#[tauri::command]
pub async fn scm_unstage(cwd: String, target: Target) -> Result<(), String> {
    blocking(move || {
        let cwd = PathBuf::from(cwd);
        scm::unstage(&cwd, &resolve_target(&cwd, target)?)
    })
    .await
}

/// Throw away unstaged changes (FR-5); `{ undo }` is a token for
/// `scm_undo_discard`, valid for this app run and the last [`UNDO_KEPT`] discards.
#[tauri::command]
pub async fn scm_discard(app: tauri::AppHandle, cwd: String, target: Target) -> Result<serde_json::Value, String> {
    blocking(move || {
        use tauri::Manager;
        let undo = discard_blocking(&app.state::<Discards>(), &PathBuf::from(cwd), target)?;
        Ok(serde_json::json!({ "undo": undo }))
    })
    .await
}

/// Put back what the discard `undo` removed (FR-5). A token is used once.
#[tauri::command]
pub async fn scm_undo_discard(app: tauri::AppHandle, cwd: String, undo: String) -> Result<(), String> {
    blocking(move || {
        use tauri::Manager;
        undo_blocking(&app.state::<Discards>(), &PathBuf::from(cwd), &undo)
    })
    .await
}

/// Commit what is staged (`amend`: replace the last commit) with the
/// repository's identity, signing and hooks (FR-6). Unless `confirmRunning`,
/// answers `{ needsConfirm: { running, pushed } }` instead while a run is
/// active in the folder or when an amend would rewrite a commit on the
/// upstream (FR-13).
#[tauri::command]
pub async fn scm_commit(supervisor: State<'_, EngineSupervisor>, cwd: String, options: CommitOptions) -> Result<CommitOutcome, String> {
    let cwd = PathBuf::from(cwd);
    let running = supervisor.folder_running(&cwd);
    blocking(move || commit_blocking(&cwd, &options, running)).await
}

/// The subjects of the last `n` commits, newest first (for the draft, FR-7).
#[tauri::command]
pub async fn scm_recent_subjects(cwd: String, n: usize) -> Result<Vec<String>, String> {
    blocking(move || scm::recent_subjects(&PathBuf::from(cwd), n.min(SUBJECTS_MAX))).await
}

/// A chat was deleted: remove its turn checkpoints from its folder's
/// repository (FR-14). A folder that is gone, or no longer in a repository,
/// has none to remove (logged).
#[tauri::command]
pub async fn checkpoints_drop(db: State<'_, Db>, cwd: String, chat: String) -> Result<(), String> {
    let folder = PathBuf::from(&cwd);
    let (dropped, chat_id) = (chat.clone(), chat);
    let result = blocking(move || {
        if !folder.is_dir() || evo_worktrees::repo_root(&folder).is_none() {
            return Ok(false);
        }
        evo_worktrees::checkpoint::drop_chat(&folder, &dropped).map(|()| true)
    })
    .await;
    match result {
        Ok(true) => Ok(()),
        Ok(false) => {
            db.log(
                NewLog::new("info", "shell", format!("checkpoints of a deleted chat not dropped: {cwd} is gone or not in a repository")).chat(Some(&chat_id)),
            );
            Ok(())
        }
        Err(error) => {
            db.log(NewLog::new("warn", "shell", format!("could not drop the checkpoints of a deleted chat in {cwd}: {error}")).chat(Some(&chat_id)));
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A scratch repository with one commit, removed on drop.
    struct Repo(PathBuf);

    impl Repo {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!("evo-scm-shell-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            let repo = Self(root);
            repo.git(&["init", "-q", "-b", "main"]);
            for (key, value) in [
                ("user.name", "Evo Test"),
                ("user.email", "test@example.com"),
                ("commit.gpgsign", "false"),
                ("core.autocrlf", "false"),
                ("core.hooksPath", "no-hooks"),
            ] {
                repo.git(&["config", key, value]);
            }
            repo.write("a.txt", "one\n");
            repo.git(&["add", "-A"]);
            repo.git(&["commit", "-q", "-m", "first"]);
            repo
        }

        fn git(&self, args: &[&str]) -> String {
            let out = Command::new("git").args(args).current_dir(&self.0).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }

        fn write(&self, path: &str, text: &str) {
            std::fs::write(self.0.join(path), text).unwrap();
        }

        fn read(&self, path: &str) -> String {
            std::fs::read_to_string(self.0.join(path)).unwrap()
        }

        fn commits(&self) -> usize {
            self.git(&["rev-list", "--count", "HEAD"]).parse().unwrap()
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn options(message: &str, amend: bool, confirm_running: bool) -> CommitOptions {
        CommitOptions { message: message.into(), amend, confirm_running }
    }

    #[test]
    fn paths_resolve_inside_the_repository_and_are_refused_outside_it() {
        let root = || Some(PathBuf::from(if cfg!(windows) { "C:/work/repo" } else { "/work/repo" }));
        // Relative to the top folder, either slash, `.` parts dropped.
        assert_eq!(resolve("src/a.ts", root).unwrap(), "src/a.ts");
        assert_eq!(resolve("src\\b.ts", root).unwrap(), "src/b.ts");
        assert_eq!(resolve("./src/./c.ts", root).unwrap(), "src/c.ts");
        // Leaving the repository, empty, or a NUL.
        for bad in ["../x", "src/../../x", "a/..", "", ".", "./", "a\0b"] {
            assert!(resolve(bad, root).unwrap_err().contains("not a path inside the repository"), "{bad:?}");
        }
        // Absolute: inside the top folder only.
        if cfg!(windows) {
            assert_eq!(resolve("C:\\work\\repo\\src\\a.ts", root).unwrap(), "src/a.ts");
            // Case does not matter on Windows.
            assert_eq!(resolve("c:/WORK/repo/src/a.ts", root).unwrap(), "src/a.ts");
            assert_eq!(resolve("\\\\?\\C:\\work\\repo\\a.ts", root).unwrap(), "a.ts");
            for bad in ["C:/work/repo", "C:/work/repo/", "C:/work/repo2/a.ts", "C:/work/other/a.ts", "D:/work/repo/a.ts", "C:/work/repo/../x"] {
                assert!(resolve(bad, root).is_err(), "{bad:?}");
            }
        } else {
            assert_eq!(resolve("/work/repo/src/a.ts", root).unwrap(), "src/a.ts");
            for bad in ["/work/repo", "/work/repo/", "/work/repo2/a.ts", "/etc/passwd", "/work/repo/../x"] {
                assert!(resolve(bad, root).is_err(), "{bad:?}");
            }
        }
        // An absolute path outside any repository.
        assert!(resolve("/a/b", || None).unwrap_err().contains("not in a Git repository"));
        // A relative path needs no repository lookup.
        assert_eq!(resolve("a", || panic!("no lookup for a relative path")).unwrap(), "a");
    }

    #[test]
    fn a_target_outside_the_repository_is_refused_before_git_runs() {
        let repo = Repo::new();
        repo.write("a.txt", "changed\n");
        let outside = std::env::temp_dir().join("evo-outside.txt").to_string_lossy().to_string();
        for path in ["../escape.txt", outside.as_str()] {
            assert!(resolve_target(&repo.0, Target::File { path: path.into() }).is_err(), "{path}");
            let hunk = Target::Hunk { path: path.into(), hunk: "h".into(), expected: "x".into() };
            assert!(resolve_target(&repo.0, hunk).is_err(), "{path}");
            assert!(discard_blocking(&Discards::default(), &repo.0, Target::File { path: path.into() }).is_err());
        }
        // Nothing was discarded.
        assert_eq!(repo.read("a.txt"), "changed\n");
        // An absolute path inside the repository is taken.
        let inside = repo.0.join("a.txt").to_string_lossy().to_string();
        assert_eq!(resolve_target(&repo.0, Target::File { path: inside }).unwrap(), Target::File { path: "a.txt".into() });
    }

    #[test]
    fn a_turns_commits_must_be_ids_or_ref_names() {
        assert!(revision("3f2a9c1").is_ok());
        assert!(revision("refs/evo/checkpoints/chat-1/7/start").is_ok());
        for bad in ["", "--output=/tmp/x", "-p", "a..b", "HEAD@{1}", "HEAD~1", "a b", "x:y"] {
            assert!(revision(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn an_undo_token_brings_a_discard_back_once_in_its_own_folder() {
        let repo = Repo::new();
        let discards = Discards::default();
        repo.write("a.txt", "changed\n");
        repo.write("new.txt", "fresh\n");
        let token = discard_blocking(&discards, &repo.0, Target::All).unwrap();
        assert_eq!(repo.read("a.txt"), "one\n");
        assert!(!repo.0.join("new.txt").exists());
        // Another folder cannot use it, and the token stays.
        let other = Repo::new();
        assert!(undo_blocking(&discards, &other.0, &token).unwrap_err().contains("another folder"));
        // Undo puts both back.
        undo_blocking(&discards, &repo.0, &token).unwrap();
        assert_eq!(repo.read("a.txt"), "changed\n");
        assert_eq!(repo.read("new.txt"), "fresh\n");
        // A token works once; an unknown one never.
        assert!(undo_blocking(&discards, &repo.0, &token).unwrap_err().contains("can no longer be undone"));
        assert!(undo_blocking(&discards, &repo.0, "discard-999").is_err());
    }

    #[test]
    fn undo_tokens_are_fresh_and_only_the_last_fifty_are_kept() {
        let discards = Discards::default();
        let folder = Path::new("/work/repo");
        let tokens: Vec<String> = (0..UNDO_KEPT + 2).map(|i| discards.keep(folder, vec![Kept { path: format!("f{i}"), blob: None }])).collect();
        let unique: std::collections::HashSet<&String> = tokens.iter().collect();
        assert_eq!(unique.len(), tokens.len());
        // The two oldest are forgotten.
        assert!(discards.take(folder, &tokens[0]).is_err());
        assert!(discards.take(folder, &tokens[1]).is_err());
        assert_eq!(discards.take(folder, &tokens[2]).unwrap(), vec![Kept { path: "f2".into(), blob: None }]);
        assert!(discards.take(folder, tokens.last().unwrap()).is_ok());
        // Folders compare like the supervisor's (case and slashes on Windows and macOS).
        let token = discards.keep(Path::new("/Work/Repo/"), Vec::new());
        assert_eq!(discards.take(Path::new("/work/repo"), &token).is_ok(), cfg!(any(windows, target_os = "macos")));
    }

    #[test]
    fn a_commit_while_a_run_is_active_needs_the_persons_yes() {
        let repo = Repo::new();
        repo.write("a.txt", "two\n");
        repo.git(&["add", "-A"]);
        let running = vec!["chat-b".to_string()];
        let asked = commit_blocking(&repo.0, &options("second", false, false), running.clone()).unwrap();
        assert_eq!(asked, CommitOutcome::NeedsConfirm { needs_confirm: NeedsConfirm { running: running.clone(), pushed: false } });
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({ "needsConfirm": { "running": ["chat-b"], "pushed": false } }));
        assert_eq!(repo.commits(), 1, "nothing committed before the yes");
        // Yes: committed.
        let done = commit_blocking(&repo.0, &options("second", false, true), running).unwrap();
        let CommitOutcome::Committed(committed) = &done else { panic!("{done:?}") };
        assert_eq!(committed.branch.as_deref(), Some("main"));
        assert_eq!(serde_json::to_value(&done).unwrap(), serde_json::json!({ "commit": committed.commit, "branch": "main" }));
        assert_eq!(repo.commits(), 2);
        // No run: no question.
        repo.write("a.txt", "three\n");
        repo.git(&["add", "-A"]);
        assert!(matches!(commit_blocking(&repo.0, &options("third", false, false), Vec::new()).unwrap(), CommitOutcome::Committed(_)));
        assert_eq!(repo.commits(), 3);
        // An empty message is refused.
        assert!(commit_blocking(&repo.0, &options("  \n", false, true), Vec::new()).is_err());
    }

    #[test]
    fn amending_a_commit_on_the_upstream_needs_the_persons_yes() {
        let repo = Repo::new();
        // Not on an upstream: an amend goes straight through.
        assert!(matches!(commit_blocking(&repo.0, &options("first, again", true, false), Vec::new()).unwrap(), CommitOutcome::Committed(_)));
        // `HEAD` is on its upstream (a local branch stands in for the remote one).
        repo.git(&["branch", "shared"]);
        repo.git(&["branch", "--set-upstream-to=shared"]);
        let asked = commit_blocking(&repo.0, &options("rewritten", true, false), Vec::new()).unwrap();
        assert_eq!(asked, CommitOutcome::NeedsConfirm { needs_confirm: NeedsConfirm { running: Vec::new(), pushed: true } });
        assert_eq!(repo.git(&["log", "-1", "--format=%s"]), "first, again");
        // A plain commit on top is no rewrite.
        repo.write("a.txt", "two\n");
        repo.git(&["add", "-A"]);
        assert!(matches!(commit_blocking(&repo.0, &options("second", false, false), Vec::new()).unwrap(), CommitOutcome::Committed(_)));
        // Yes: amended.
        repo.git(&["branch", "-f", "shared", "HEAD"]);
        assert!(matches!(commit_blocking(&repo.0, &options("second, reworded", true, true), Vec::new()).unwrap(), CommitOutcome::Committed(_)));
        assert_eq!(repo.git(&["log", "-1", "--format=%s"]), "second, reworded");
    }

    #[test]
    fn commit_options_take_the_windows_camel_case() {
        let parsed: CommitOptions = serde_json::from_value(serde_json::json!({ "message": "m", "amend": true, "confirmRunning": true })).unwrap();
        assert!(parsed.amend && parsed.confirm_running);
        let parsed: CommitOptions = serde_json::from_value(serde_json::json!({ "message": "m" })).unwrap();
        assert!(!parsed.amend && !parsed.confirm_running);
    }
}
