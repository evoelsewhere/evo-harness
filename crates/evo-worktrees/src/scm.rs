//! The person's git operations on a working folder: status split into staged
//! and unstaged changes, a file's diff with its hunks, stage / unstage /
//! discard by hunk, file or all (a discard keeps what it removes so it can be
//! undone), and commit / amend with the repository's own identity and hooks.
//!
//! Every write here is meant to follow a person's click (ADR 0024); nothing
//! in Evo's agent tools calls it. Paths are relative to the repository's top
//! folder and refused when they leave it.
//!
//! Spec: `docs/features/source-control/spec.md` (FR-1, FR-4 – FR-6, edge
//! cases).

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    count_lines, fnv, git, git_bytes, git_with, new_name, patch_hash, toplevel, UNTRACKED_MAX_FILES,
};

/// A hunk or file action whose diff is no longer the one the person saw.
pub const CHANGED_SINCE_SHOWN: &str = "the file changed since it was shown; look at it again";
/// Larger texts get no diff, only whole-file actions.
pub const DIFF_MAX_BYTES: usize = 1_000_000;
/// Git's first arguments for every read: paths as they are, never globs.
const READ: [&str; 3] = ["--literal-pathspecs", "-c", "core.quotePath=false"];

/// One changed file in a list.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScmFile {
    pub path: String,
    /// `added`, `modified`, `deleted`, `renamed` or `untracked`.
    pub status: &'static str,
    /// A renamed file's old path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Lines; `None` for binary files (and new files past the counting limit).
    pub added: Option<usize>,
    pub removed: Option<usize>,
}

/// Where the repository of a working folder stands.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScmStatus {
    /// The top folder; every path is relative to it.
    pub root: PathBuf,
    /// `None` on a detached `HEAD`.
    pub branch: Option<String>,
    /// Short commit id; `None` before the first commit.
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// `normal`, `merge`, `rebase`, `cherry-pick` or `revert`.
    pub state: &'static str,
    pub staged: Vec<ScmFile>,
    pub unstaged: Vec<ScmFile>,
    /// Files with merge conflicts (listed in neither list).
    pub conflicted: Vec<String>,
}

/// Which side of a file's changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Area {
    /// `HEAD` → index.
    Staged,
    /// Index → working tree (or a new file).
    Unstaged,
}

/// One hunk of a file diff; lines are 1-based.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    /// Stable while the hunk's text is the same.
    pub id: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
}

/// A file's diff for display and hunk actions.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    /// The text before and after; `None` when absent, binary or too large.
    pub old: Option<String>,
    pub new: Option<String>,
    pub hunks: Vec<Hunk>,
    /// Names exactly this diff: hunk actions pass it back.
    pub hash: String,
    pub binary: bool,
    pub too_large: bool,
    /// Only line endings differ.
    pub eol_only: bool,
}

/// What an action applies to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Target {
    All,
    File {
        path: String,
    },
    Hunk {
        path: String,
        hunk: String,
        expected: String,
    },
}

/// A file as it was before a discard: its bytes as a blob, or absent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    pub path: String,
    pub blob: Option<String>,
}

/// A commit just made.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Committed {
    /// Short id.
    pub commit: String,
    /// `None` on a detached `HEAD`.
    pub branch: Option<String>,
}

/// `READ` followed by `args`.
fn read<'a>(args: &[&'a str]) -> Vec<&'a str> {
    READ.iter().copied().chain(args.iter().copied()).collect()
}

/// `path` relative to the repository's top folder, forward slashes; refused
/// when empty, absolute or leaving the repository.
fn relative(path: &str) -> Result<String, String> {
    let path = path.replace('\\', "/");
    let path = path.trim_start_matches("./").trim_end_matches('/');
    let absolute = path.starts_with('/') || path.chars().nth(1) == Some(':');
    if path.is_empty() || absolute || path.split('/').any(|part| part == "..") {
        return Err(format!("not a path inside the repository: {path:?}"));
    }
    Ok(path.to_string())
}

/// NUL-separated output split into its fields.
fn fields(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|&b| b == 0)
        .filter(|field| !field.is_empty())
        .map(|field| String::from_utf8_lossy(field).to_string())
        .collect()
}

/// Line counts from `git diff --numstat -z` (renames under their new path).
pub(crate) fn numstat_counts(bytes: &[u8]) -> HashMap<String, (Option<usize>, Option<usize>)> {
    let fields = fields(bytes);
    let mut counts = HashMap::new();
    let mut at = 0;
    while at < fields.len() {
        let mut parts = fields[at].splitn(3, '\t');
        let added = parts.next().and_then(|n| n.parse::<usize>().ok());
        let removed = parts.next().and_then(|n| n.parse::<usize>().ok());
        let path = parts.next().unwrap_or_default();
        if path.is_empty() {
            // A rename: old and new path follow.
            if let Some(new) = fields.get(at + 2) {
                counts.insert(new.clone(), (added, removed));
            }
            at += 3;
        } else {
            counts.insert(path.to_string(), (added, removed));
            at += 1;
        }
    }
    counts
}

fn git_dir(root: &Path) -> Result<PathBuf, String> {
    let dir = PathBuf::from(git(root, &["rev-parse", "--git-dir"])?);
    Ok(if dir.is_absolute() {
        dir
    } else {
        root.join(dir)
    })
}

fn state(root: &Path) -> &'static str {
    let Ok(dir) = git_dir(root) else {
        return "normal";
    };
    if dir.join("rebase-merge").exists() || dir.join("rebase-apply").exists() {
        "rebase"
    } else if dir.join("MERGE_HEAD").exists() {
        "merge"
    } else if dir.join("CHERRY_PICK_HEAD").exists() {
        "cherry-pick"
    } else if dir.join("REVERT_HEAD").exists() {
        "revert"
    } else {
        "normal"
    }
}

/// The status of the repository `dir` is in; `None` outside a repository.
pub fn status(dir: &Path) -> Result<Option<ScmStatus>, String> {
    let Ok(root) = toplevel(dir) else {
        return Ok(None);
    };
    let raw = git_bytes(
        &root,
        &read(&[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
            "-z",
        ]),
    )?;
    let entries = fields(&raw);
    let (mut branch, mut head, mut upstream, mut ahead, mut behind) = (None, None, None, 0, 0);
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut conflicted = Vec::new();
    let mut at = 0;
    while at < entries.len() {
        let entry = &entries[at];
        at += 1;
        if let Some(header) = entry.strip_prefix("# ") {
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
        let (tag, rest) = entry.split_once(' ').unwrap_or((entry.as_str(), ""));
        let skip = match tag {
            "1" => 7,
            "2" => 8,
            "u" => 9,
            "?" => {
                unstaged.push(file(rest, "untracked", None));
                continue;
            }
            _ => continue,
        };
        let xy: Vec<char> = rest.chars().take(2).collect();
        let path = rest
            .splitn(skip + 1, ' ')
            .nth(skip)
            .unwrap_or_default()
            .to_string();
        if tag == "u" {
            conflicted.push(path);
            continue;
        }
        // A rename's old path is the next field.
        let from = if tag == "2" {
            at += 1;
            entries.get(at - 1).cloned()
        } else {
            None
        };
        let (x, y) = (
            xy.first().copied().unwrap_or('.'),
            xy.get(1).copied().unwrap_or('.'),
        );
        if x != '.' {
            let kind = match x {
                'A' | 'C' => "added",
                'D' => "deleted",
                'R' => "renamed",
                _ => "modified",
            };
            staged.push(file(&path, kind, if x == 'R' { from } else { None }));
        }
        if y != '.' {
            let kind = match y {
                'D' => "deleted",
                'A' => "added",
                _ => "modified",
            };
            unstaged.push(file(&path, kind, None));
        }
    }

    let staged_counts = numstat_counts(
        &git_bytes(&root, &read(&["diff", "--cached", "--numstat", "-z", "-M"]))
            .unwrap_or_default(),
    );
    let unstaged_counts =
        numstat_counts(&git_bytes(&root, &read(&["diff", "--numstat", "-z"])).unwrap_or_default());
    for entry in &mut staged {
        (entry.added, entry.removed) = staged_counts
            .get(&entry.path)
            .copied()
            .unwrap_or((None, None));
    }
    let mut measured = 0;
    for entry in &mut unstaged {
        (entry.added, entry.removed) = match unstaged_counts.get(&entry.path) {
            Some(&counts) => counts,
            None if entry.status == "untracked" && measured < UNTRACKED_MAX_FILES => {
                measured += 1;
                (count_lines(&root.join(&entry.path)), Some(0))
            }
            None => (None, None),
        };
    }

    Ok(Some(ScmStatus {
        state: state(&root),
        root,
        branch,
        head,
        upstream,
        ahead,
        behind,
        staged,
        unstaged,
        conflicted,
    }))
}

fn file(path: &str, status: &'static str, from: Option<String>) -> ScmFile {
    ScmFile {
        path: path.to_string(),
        status,
        from,
        added: None,
        removed: None,
    }
}

/// The blob `spec` names (`HEAD:path`, `:path`, `<commit>:path`), if any.
fn blob(root: &Path, spec: &str) -> Option<String> {
    git(root, &["rev-parse", "--verify", "-q", spec]).ok()
}

fn blob_bytes(root: &Path, oid: &str) -> Result<Vec<u8>, String> {
    git_bytes(root, &["cat-file", "blob", oid])
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8000)].contains(&0)
}

/// A diff's text split into its header and hunks, as bytes (a hunk is
/// applied exactly as git printed it).
struct Parsed {
    header: Vec<u8>,
    hunks: Vec<(Hunk, Vec<u8>)>,
}

fn parse(patch: &[u8]) -> Parsed {
    let mut header = Vec::new();
    let mut hunks: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for line in patch.split_inclusive(|&b| b == b'\n') {
        if line.starts_with(b"@@") {
            hunks.push((line.to_vec(), line.to_vec()));
        } else if let Some((_, text)) = hunks.last_mut() {
            text.extend_from_slice(line);
        } else {
            header.extend_from_slice(line);
        }
    }
    let mut seen: HashMap<u32, usize> = HashMap::new();
    let hunks = hunks
        .into_iter()
        .map(|(head, text)| {
            let ranges = String::from_utf8_lossy(&head);
            let mut numbers = ranges.split_whitespace().skip(1).take(2).map(|range| {
                let range = range.trim_start_matches(['-', '+']);
                let (start, lines) = range.split_once(',').unwrap_or((range, "1"));
                (start.parse().unwrap_or(0), lines.parse().unwrap_or(1))
            });
            let (old_start, old_lines) = numbers.next().unwrap_or((0, 0));
            let (new_start, new_lines) = numbers.next().unwrap_or((0, 0));
            // The id is the hunk's own text, so a hunk keeps it while others change.
            let body = String::from_utf8_lossy(&text[head.len()..]).to_string();
            let hash = fnv(&format!("{old_lines},{new_lines}\n{body}"));
            let nth = seen.entry(hash).or_insert(0);
            let id = format!("{hash:08x}-{nth}");
            *nth += 1;
            (
                Hunk {
                    id,
                    old_start,
                    old_lines,
                    new_start,
                    new_lines,
                },
                text,
            )
        })
        .collect();
    Parsed { header, hunks }
}

/// A new file's patch, made here (git would need `--no-index`).
fn new_file_patch(path: &str, bytes: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut patch = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
        lines.len()
    );
    for line in lines {
        patch.push('+');
        patch.push_str(line);
        if !line.ends_with('\n') {
            patch.push_str("\n\\ No newline at end of file\n");
        }
    }
    patch.into_bytes()
}

/// The diff of `path` in `area`, with what it was computed from.
struct Computed {
    diff: FileDiff,
    parsed: Parsed,
    /// A new file not in the index: its hunk is the whole file.
    untracked: bool,
}

fn compute(root: &Path, path: &str, area: Area) -> Result<Computed, String> {
    let path = relative(path)?;
    let index = blob(root, &format!(":{path}"));
    let worktree = root.join(&path);
    let (old, new, untracked) = match area {
        Area::Staged => (
            blob(root, &format!("HEAD:{path}"))
                .map(|oid| blob_bytes(root, &oid))
                .transpose()?,
            index
                .as_deref()
                .map(|oid| blob_bytes(root, oid))
                .transpose()?,
            false,
        ),
        Area::Unstaged => {
            let new = if worktree.is_file() {
                Some(std::fs::read(&worktree).map_err(|e| format!("could not read {path}: {e}"))?)
            } else {
                None
            };
            let old = index
                .as_deref()
                .map(|oid| blob_bytes(root, oid))
                .transpose()?;
            (old, new, index.is_none())
        }
    };
    let binary = [&old, &new]
        .iter()
        .any(|side| side.as_deref().is_some_and(looks_binary));
    let too_large = [&old, &new].iter().any(|side| {
        side.as_ref()
            .is_some_and(|bytes| bytes.len() > DIFF_MAX_BYTES)
    });
    let patch = if untracked {
        new.as_deref()
            .map(|bytes| new_file_patch(&path, bytes))
            .unwrap_or_default()
    } else {
        let mut args = vec!["diff", "--no-color", "--no-ext-diff", "-U3"];
        if area == Area::Staged {
            args.push("--cached");
        }
        args.extend(["--", path.as_str()]);
        git_bytes(root, &read(&args))?
    };
    let eol_only = !untracked && !patch.is_empty() && !binary && {
        let mut args = vec!["diff", "--no-color", "--no-ext-diff", "--ignore-cr-at-eol"];
        if area == Area::Staged {
            args.push("--cached");
        }
        args.extend(["--", path.as_str()]);
        git_bytes(root, &read(&args))?.is_empty()
    };
    let hash = patch_hash(&patch);
    let skip_text = binary || too_large;
    let parsed = if skip_text || eol_only {
        Parsed {
            header: Vec::new(),
            hunks: Vec::new(),
        }
    } else {
        parse(&patch)
    };
    let text = |side: Option<Vec<u8>>| {
        side.filter(|_| !skip_text)
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
    };
    Ok(Computed {
        diff: FileDiff {
            path,
            old: text(old),
            new: text(new),
            hunks: parsed.hunks.iter().map(|(hunk, _)| hunk.clone()).collect(),
            hash,
            binary,
            too_large,
            eol_only,
        },
        parsed,
        untracked,
    })
}

/// The diff of `path` in `area` of `dir`'s repository.
pub fn diff(dir: &Path, path: &str, area: Area) -> Result<FileDiff, String> {
    let root = toplevel(dir)?;
    compute(&root, path, area).map(|computed| computed.diff)
}

/// The diff of `path` between two commits (a turn's checkpoints).
pub fn diff_between(dir: &Path, from: &str, to: &str, path: &str) -> Result<FileDiff, String> {
    let root = toplevel(dir)?;
    let path = relative(path)?;
    let side = |commit: &str| {
        blob(&root, &format!("{commit}:{path}"))
            .map(|oid| blob_bytes(&root, &oid))
            .transpose()
    };
    let (old, new) = (side(from)?, side(to)?);
    let binary = [&old, &new]
        .iter()
        .any(|side| side.as_deref().is_some_and(looks_binary));
    let too_large = [&old, &new].iter().any(|side| {
        side.as_ref()
            .is_some_and(|bytes| bytes.len() > DIFF_MAX_BYTES)
    });
    let patch = git_bytes(
        &root,
        &read(&[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "-U3",
            from,
            to,
            "--",
            &path,
        ]),
    )?;
    let skip_text = binary || too_large;
    let text = |side: Option<Vec<u8>>| {
        side.filter(|_| !skip_text)
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
    };
    let hunks = if skip_text {
        Vec::new()
    } else {
        parse(&patch)
            .hunks
            .into_iter()
            .map(|(hunk, _)| hunk)
            .collect()
    };
    Ok(FileDiff {
        path,
        old: text(old),
        new: text(new),
        hunks,
        hash: patch_hash(&patch),
        binary,
        too_large,
        eol_only: false,
    })
}

/// The patch of one hunk of `area`'s diff, if the diff is still `expected`.
fn hunk_patch(
    root: &Path,
    path: &str,
    area: Area,
    hunk: &str,
    expected: &str,
) -> Result<(Computed, Vec<u8>), String> {
    let computed = compute(root, path, area)?;
    if computed.diff.hash != expected {
        return Err(CHANGED_SINCE_SHOWN.to_string());
    }
    let Some((_, text)) = computed.parsed.hunks.iter().find(|(h, _)| h.id == hunk) else {
        return Err(CHANGED_SINCE_SHOWN.to_string());
    };
    let mut patch = computed.parsed.header.clone();
    patch.extend_from_slice(text);
    Ok((computed, patch))
}

fn apply(root: &Path, patch: &[u8], flags: &[&str]) -> Result<(), String> {
    let mut args = vec!["apply", "--whitespace=nowarn"];
    args.extend_from_slice(flags);
    args.push("-");
    git_with(root, &args, &[], Some(patch)).map(|_| ())
}

fn has_head(root: &Path) -> bool {
    blob(root, "HEAD^{commit}").is_some()
}

/// Stage a hunk, a file or everything.
pub fn stage(dir: &Path, target: &Target) -> Result<(), String> {
    let root = toplevel(dir)?;
    match target {
        Target::All => git(&root, &["add", "-A"]).map(|_| ()),
        Target::File { path } => {
            let path = relative(path)?;
            git(&root, &read(&["add", "-A", "--", &path])).map(|_| ())
        }
        Target::Hunk {
            path,
            hunk,
            expected,
        } => {
            let (computed, patch) = hunk_patch(&root, path, Area::Unstaged, hunk, expected)?;
            if computed.untracked {
                return stage(
                    dir,
                    &Target::File {
                        path: computed.diff.path,
                    },
                );
            }
            apply(&root, &patch, &["--cached"])
        }
    }
}

/// Take a hunk, a file or everything out of the index (the working tree keeps it).
pub fn unstage(dir: &Path, target: &Target) -> Result<(), String> {
    let root = toplevel(dir)?;
    match target {
        Target::All if has_head(&root) => git(&root, &["reset", "-q"]).map(|_| ()),
        Target::All => git(&root, &["rm", "-q", "-r", "--cached", "--", "."]).map(|_| ()),
        Target::File { path } => {
            let path = relative(path)?;
            if has_head(&root) {
                git(&root, &read(&["restore", "--staged", "--", &path])).map(|_| ())
            } else {
                git(&root, &read(&["rm", "-q", "-r", "--cached", "--", &path])).map(|_| ())
            }
        }
        Target::Hunk {
            path,
            hunk,
            expected,
        } => {
            let (_, patch) = hunk_patch(&root, path, Area::Staged, hunk, expected)?;
            apply(&root, &patch, &["--cached", "-R"])
        }
    }
}

/// Keep `path` as it is now, to put it back after a discard.
fn keep(root: &Path, path: &str) -> Result<Kept, String> {
    let file = root.join(path);
    let blob = if file.is_file() {
        let file = file.to_string_lossy().to_string();
        Some(git(
            root,
            &["hash-object", "-w", "--no-filters", "--", &file],
        )?)
    } else {
        None
    };
    Ok(Kept {
        path: path.to_string(),
        blob,
    })
}

/// Throw away unstaged changes of a hunk, a file or everything (new files are
/// deleted). Staged changes stay. Returns what was there, for
/// [`restore_kept`].
pub fn discard(dir: &Path, target: &Target) -> Result<Vec<Kept>, String> {
    let root = toplevel(dir)?;
    match target {
        Target::All => {
            let Some(status) = status(&root)? else {
                return Ok(Vec::new());
            };
            let mut kept = Vec::new();
            for entry in status.unstaged {
                kept.extend(discard(dir, &Target::File { path: entry.path })?);
            }
            Ok(kept)
        }
        Target::File { path } if root.join(relative(path)?).is_dir() => {
            // A folder (the tree's folder rows, FR-18): every unstaged file under it.
            let folder = format!("{}/", relative(path)?);
            let Some(status) = status(&root)? else {
                return Ok(Vec::new());
            };
            let mut kept = Vec::new();
            for entry in status
                .unstaged
                .into_iter()
                .filter(|entry| entry.path.starts_with(&folder))
            {
                kept.extend(discard(dir, &Target::File { path: entry.path })?);
            }
            Ok(kept)
        }
        Target::File { path } => {
            let path = relative(path)?;
            let kept = keep(&root, &path)?;
            if blob(&root, &format!(":{path}")).is_some() {
                git(&root, &read(&["restore", "--worktree", "--", &path]))?;
            } else {
                let file = root.join(&path);
                if file.is_file() {
                    std::fs::remove_file(&file)
                        .map_err(|e| format!("could not delete {path}: {e}"))?;
                }
            }
            Ok(vec![kept])
        }
        Target::Hunk {
            path,
            hunk,
            expected,
        } => {
            let (computed, patch) = hunk_patch(&root, path, Area::Unstaged, hunk, expected)?;
            if computed.untracked {
                return discard(
                    dir,
                    &Target::File {
                        path: computed.diff.path,
                    },
                );
            }
            let kept = keep(&root, &computed.diff.path)?;
            apply(&root, &patch, &["-R"])?;
            Ok(vec![kept])
        }
    }
}

/// Put files back as [`discard`] kept them.
pub fn restore_kept(dir: &Path, kept: &[Kept]) -> Result<(), String> {
    let root = toplevel(dir)?;
    for entry in kept {
        let path = relative(&entry.path)?;
        let file = root.join(&path);
        match &entry.blob {
            Some(oid) => {
                let bytes = blob_bytes(&root, oid)?;
                if let Some(parent) = file.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("could not restore {path}: {e}"))?;
                }
                std::fs::write(&file, bytes)
                    .map_err(|e| format!("could not restore {path}: {e}"))?;
            }
            None if file.is_file() => {
                std::fs::remove_file(&file)
                    .map_err(|e| format!("could not restore {path}: {e}"))?;
            }
            None => {}
        }
    }
    Ok(())
}

/// Git does not know who commits.
const NO_IDENTITY: &str = "Git does not know who you are. Set the name and email for your commits, then commit again: git config --global user.name \"Your Name\" and git config --global user.email you@example.com (user.name, user.email).";

/// Commit what is staged (`amend`: replace the last commit). The
/// repository's identity, signing and hooks apply; a failing hook's output is
/// the error, and the index stays as it was.
pub fn commit(dir: &Path, message: &str, amend: bool) -> Result<Committed, String> {
    let root = toplevel(dir)?;
    let file = std::env::temp_dir().join(format!("evo-commit-{}.txt", new_name()));
    std::fs::write(&file, message)
        .map_err(|e| format!("could not write the commit message: {e}"))?;
    let file_arg = file.to_string_lossy().to_string();
    let mut args = vec!["commit", "-q", "-F", file_arg.as_str()];
    if amend {
        args.push("--amend");
    }
    let env: [(&str, &OsStr); 0] = [];
    let result = git_with(&root, &args, &env, None);
    let _ = std::fs::remove_file(&file);
    if let Err(error) = result {
        let identity = [
            "tell me who you are",
            "empty ident",
            "unable to auto-detect email",
        ];
        if identity.iter().any(|needle| error.contains(needle)) {
            return Err(NO_IDENTITY.to_string());
        }
        return Err(error);
    }
    let commit = git(&root, &["rev-parse", "--short", "HEAD"])?;
    let branch = git(&root, &["branch", "--show-current"])
        .ok()
        .filter(|name| !name.is_empty());
    Ok(Committed { commit, branch })
}

/// `HEAD` is already on its upstream branch (amending it rewrites shared history).
pub fn head_on_upstream(dir: &Path) -> bool {
    toplevel(dir)
        .and_then(|root| git(&root, &["merge-base", "--is-ancestor", "HEAD", "@{u}"]))
        .is_ok()
}

/// The subjects of the last `count` commits, newest first.
pub fn recent_subjects(dir: &Path, count: usize) -> Result<Vec<String>, String> {
    let root = toplevel(dir)?;
    if !has_head(&root) {
        return Ok(Vec::new());
    }
    let limit = format!("-n{count}");
    Ok(git(&root, &["log", &limit, "--format=%s"])?
        .lines()
        .map(str::to_string)
        .collect())
}

/// One Git repository found for a folder (FR-17).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    /// Its top folder.
    pub root: PathBuf,
    /// Where it is in the scanned folder, forward slashes: `.` for the
    /// repository the folder itself is in.
    pub path: String,
    /// `None` on a detached `HEAD`.
    pub branch: Option<String>,
    /// Changed, staged and new files (`git status`).
    pub changed: usize,
}

/// How deep below the folder nested repositories are looked for.
pub const REPOSITORY_DEPTH: usize = 4;
/// At most this many repositories are listed.
pub const REPOSITORY_LIMIT: usize = 50;
/// Folders never searched: dependencies and build output.
const NOT_SEARCHED: [&str; 6] = ["node_modules", "target", "dist", "build", ".venv", "vendor"];

/// The repository `dir` is in, then the repositories in its subfolders
/// (FR-17; plan 3.9).
pub fn repositories(dir: &Path) -> Result<Vec<Repository>, String> {
    scan_repositories(dir, REPOSITORY_DEPTH, REPOSITORY_LIMIT)
}

/// [`repositories`] with its bounds: `depth` levels below `dir`, `limit` entries.
pub fn scan_repositories(
    dir: &Path,
    depth: usize,
    limit: usize,
) -> Result<Vec<Repository>, String> {
    if !dir.is_dir() {
        return Err(format!("not a folder: {}", dir.display()));
    }
    let mut found: Vec<(PathBuf, String)> = Vec::new();
    if let Ok(root) = toplevel(dir) {
        found.push((root, ".".to_string()));
    }
    // Breadth first, never into a repository found inside `dir` (its own
    // nested ones belong to its listing) nor into skipped folders.
    let mut queue = std::collections::VecDeque::from([(dir.to_path_buf(), 0usize)]);
    let mut nested = Vec::new();
    while let Some((folder, level)) = queue.pop_front() {
        if level >= depth || found.len() + nested.len() >= limit {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .filter(|path| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                !name.starts_with('.') && !NOT_SEARCHED.contains(&name.as_str())
            })
            .collect();
        children.sort();
        for child in children {
            if child.join(".git").exists() {
                if found.len() + nested.len() < limit {
                    let relative = child
                        .strip_prefix(dir)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| child.to_string_lossy().to_string());
                    nested.push((child, relative));
                }
            } else {
                queue.push_back((child, level + 1));
            }
        }
    }
    nested.sort_by(|a, b| a.1.cmp(&b.1));
    found.extend(nested);
    // Branch and change count, a few repositories at a time.
    let mut repositories = Vec::with_capacity(found.len());
    for chunk in found.chunks(8) {
        let described: Vec<Repository> = std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|(root, path)| scope.spawn(move || describe(root, path)))
                .collect();
            handles
                .into_iter()
                .zip(chunk)
                .map(|(handle, (root, path))| {
                    handle.join().unwrap_or_else(|_| Repository {
                        root: root.clone(),
                        path: path.clone(),
                        branch: None,
                        changed: 0,
                    })
                })
                .collect()
        });
        repositories.extend(described);
    }
    Ok(repositories)
}

/// A found repository's branch and number of changes; a repository git
/// cannot read is still listed, with none.
fn describe(root: &Path, path: &str) -> Repository {
    let top = toplevel(root).unwrap_or_else(|_| root.to_path_buf());
    let raw = git_bytes(
        &top,
        &read(&[
            "--no-optional-locks",
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=normal",
        ]),
    )
    .unwrap_or_default();
    let entries = fields(&raw);
    let mut branch = None;
    let mut changed = 0;
    let mut skip_next = false;
    for entry in &entries {
        if skip_next {
            skip_next = false;
            continue;
        }
        if let Some(head) = entry.strip_prefix("# branch.head ") {
            branch = (head != "(detached)").then(|| head.to_string());
        } else if entry.starts_with("1 ") || entry.starts_with("u ") || entry.starts_with("? ") {
            changed += 1;
        } else if entry.starts_with("2 ") {
            changed += 1;
            // A rename's old path is the next field.
            skip_next = true;
        }
    }
    Repository {
        root: top,
        path: path.to_string(),
        branch,
        changed,
    }
}
