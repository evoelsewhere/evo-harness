//! Turn checkpoints: the working tree of a repository at a moment, kept as a
//! git commit under `refs/evo/checkpoints/<chat>/<seq>/<kind>`.
//!
//! A snapshot asks `git status` which files differ from `HEAD` (staged,
//! unstaged or new; ignored files are left out), reads `HEAD` into a
//! temporary index (`GIT_INDEX_FILE`, its cache tree complete), updates only
//! those files in it from the working tree, writes the tree and a commit whose
//! parent is `HEAD`, and points Evo's ref at it. Only changed files are hashed
//! and only their folders' trees rebuilt, so a 50,000-file repository takes
//! well under a second (NFR-1). The real index (`--no-optional-locks`), the
//! working tree, branches, tags and stashes are never touched. Objects are
//! shared with the repository, and deleting the refs frees them for git's
//! `gc`.
//!
//! Spec: `docs/features/source-control/spec.md` (FR-9, FR-11, FR-14, NFR-1,
//! NFR-3, NFR-4); decision: ADR 0024.

use std::ffi::OsStr;
use std::path::Path;

use serde::Serialize;

use super::{git, git_with, new_name, toplevel};

/// Snapshots go through the repository's own filters (line endings, LFS), as
/// `git add` and `git checkout` do: blobs are normalised as in any commit, so
/// a put-back file is what a checkout would write. Only the warning about
/// line-ending conversion is silenced.
const QUIET: [&str; 2] = ["-c", "core.safecrlf=false"];

/// `QUIET` followed by `args`.
fn quiet<'a>(args: &[&'a str]) -> Vec<&'a str> {
    QUIET.iter().copied().chain(args.iter().copied()).collect()
}

/// For commands on the temporary index: `QUIET`, and neither a checksum nor
/// an fsync of an index that is thrown away (git versions without these
/// settings ignore them).
fn on_temp_index<'a>(args: &[&'a str]) -> Vec<&'a str> {
    ["-c", "index.skipHash=true", "-c", "core.fsync=none"]
        .into_iter()
        .chain(quiet(args))
        .collect()
}

/// Where checkpoints live in a repository.
pub const REF_ROOT: &str = "refs/evo/checkpoints";
/// Turns kept per chat.
pub const KEEP_TURNS: usize = 50;

/// Which moment of a turn a snapshot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// When the turn started.
    Start,
    /// When it settled.
    End,
    /// Just before the person reverted it (undoing the revert goes back here).
    Undo,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Start => "start",
            Kind::End => "end",
            Kind::Undo => "undo",
        }
    }
}

/// One file that differs between two snapshots.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TurnFile {
    pub path: String,
    /// `added`, `modified`, `deleted` or `renamed`.
    pub status: &'static str,
    /// A renamed file's old path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Lines; `None` for binary files.
    pub added: Option<usize>,
    pub removed: Option<usize>,
}

/// What [`restore`] did.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Restored {
    /// Put back as they were in the source snapshot.
    pub reverted: Vec<String>,
    /// Changed since the turn ended: left as they are.
    pub skipped: Vec<String>,
}

/// A chat id is part of a ref name: letters, digits, `.`, `_` and `-` only.
fn checked(chat: &str) -> Result<&str, String> {
    let fine = !chat.is_empty()
        && chat.len() <= 128
        && !chat.starts_with('.')
        && !chat.contains("..")
        && !chat.ends_with(".lock")
        && chat
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if fine {
        Ok(chat)
    } else {
        Err(format!("not a usable chat id for a checkpoint: {chat:?}"))
    }
}

fn ref_name(chat: &str, seq: u64, kind: Kind) -> String {
    format!("{REF_ROOT}/{chat}/{seq}/{}", kind.name())
}

/// What `git status` says differs from `HEAD`: the paths (both sides of a
/// rename, conflicted and new files; NUL-terminated for `update-index -z`)
/// and `HEAD`'s commit (`None` before the first commit). Paths are relative
/// to the repository's top folder, wherever in it `dir` is.
fn changed_paths(dir: &Path) -> Result<(Vec<u8>, Option<String>), String> {
    let raw = super::git_bytes(
        dir,
        &[
            "--no-optional-locks",
            "-c",
            "status.relativePaths=false",
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=all",
        ],
    )?;
    let fields: Vec<&[u8]> = raw.split(|&b| b == 0).collect();
    let mut paths = Vec::new();
    let mut head = None;
    let mut push = |path: &[u8]| {
        if !path.is_empty() {
            paths.extend_from_slice(path);
            paths.push(0);
        }
    };
    let mut at = 0;
    while at < fields.len() {
        let entry = fields[at];
        at += 1;
        // The path is the last of a fixed number of space-separated fields.
        let last = |count: usize| entry.splitn(count, |&b| b == b' ').nth(count - 1);
        match entry.first() {
            Some(b'#') => {
                if let Some(oid) = entry.strip_prefix(b"# branch.oid ") {
                    head = Some(String::from_utf8_lossy(oid).to_string())
                        .filter(|oid| oid != "(initial)");
                }
            }
            Some(b'1') => push(last(9).unwrap_or_default()),
            Some(b'2') => {
                push(last(10).unwrap_or_default());
                push(fields.get(at).copied().unwrap_or_default());
                at += 1;
            }
            Some(b'u') => push(last(11).unwrap_or_default()),
            Some(b'?') => push(entry.get(2..).unwrap_or_default()),
            _ => {}
        }
    }
    Ok((paths, head))
}

/// Snapshot the working tree of `dir`'s repository and keep it as turn `seq`
/// of `chat` (`kind`). Returns the commit id.
///
/// Starting git costs tens of milliseconds on Windows: the top folder, the
/// status and reading `HEAD` into the temporary index run at the same time.
pub fn snapshot(dir: &Path, chat: &str, seq: u64, kind: Kind) -> Result<String, String> {
    let chat = checked(chat)?;
    let temp = std::env::temp_dir().join(format!("evo-checkpoint-{}.index", new_name()));
    let env: [(&str, &OsStr); 5] = [
        ("GIT_INDEX_FILE", temp.as_os_str()),
        // Checkpoints are Evo's, whoever the repository's user is (and if it has none).
        ("GIT_AUTHOR_NAME", OsStr::new("Evo")),
        ("GIT_AUTHOR_EMAIL", OsStr::new("evo@localhost")),
        ("GIT_COMMITTER_NAME", OsStr::new("Evo")),
        ("GIT_COMMITTER_EMAIL", OsStr::new("evo@localhost")),
    ];
    let result = (|| {
        let (root, status, read) = std::thread::scope(|scope| {
            let root = scope.spawn(|| toplevel(dir));
            let read =
                scope.spawn(|| git_with(dir, &on_temp_index(&["read-tree", "HEAD"]), &env, None));
            let status = changed_paths(dir);
            (
                root.join()
                    .unwrap_or_else(|_| Err("could not find the repository".into())),
                status,
                read.join()
                    .unwrap_or_else(|_| Err("could not read HEAD".into())),
            )
        });
        let root = root?;
        let (changed, parent) = status?;
        // Before the first commit there is no HEAD to read: the index starts empty.
        if parent.is_some() {
            read?;
        }
        if !changed.is_empty() {
            // A path still there is hashed from the working tree; a gone one is removed.
            git_with(
                &root,
                &on_temp_index(&["update-index", "--add", "--remove", "-z", "--stdin"]),
                &env,
                Some(&changed),
            )?;
        }
        let tree = text(git_with(
            &root,
            &on_temp_index(&["write-tree"]),
            &env,
            None,
        )?);
        let message = format!("evo checkpoint {chat} {seq} {}", kind.name());
        let mut args = vec!["commit-tree", tree.as_str(), "-m", message.as_str()];
        if let Some(parent) = parent.as_deref() {
            args.extend(["-p", parent]);
        }
        let commit = text(git_with(&root, &args, &env, None)?);
        git(&root, &["update-ref", &ref_name(chat, seq, kind), &commit])?;
        Ok(commit)
    })();
    let _ = std::fs::remove_file(&temp);
    result
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).trim().to_string()
}

/// The commit of turn `seq`'s `kind` snapshot, if it is kept.
pub fn find(dir: &Path, chat: &str, seq: u64, kind: Kind) -> Result<Option<String>, String> {
    let chat = checked(chat)?;
    let root = toplevel(dir)?;
    Ok(git(
        &root,
        &["rev-parse", "--verify", "-q", &ref_name(chat, seq, kind)],
    )
    .ok())
}

/// Every ref of `chat`, with its turn number.
fn refs(root: &Path, chat: &str) -> Result<Vec<(u64, String)>, String> {
    let prefix = format!("{REF_ROOT}/{chat}/");
    let listing = git(root, &["for-each-ref", "--format=%(refname)", &prefix])?;
    Ok(listing
        .lines()
        .filter_map(|name| {
            let seq = name
                .strip_prefix(&prefix)?
                .split('/')
                .next()?
                .parse()
                .ok()?;
            Some((seq, name.to_string()))
        })
        .collect())
}

/// The turns of `chat` with a checkpoint, oldest first.
pub fn turns(dir: &Path, chat: &str) -> Result<Vec<u64>, String> {
    let chat = checked(chat)?;
    let root = toplevel(dir)?;
    let mut seqs: Vec<u64> = refs(&root, chat)?.into_iter().map(|(seq, _)| seq).collect();
    seqs.sort_unstable();
    seqs.dedup();
    Ok(seqs)
}

fn delete_refs(root: &Path, names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Ok(());
    }
    let input: String = names
        .iter()
        .map(|name| format!("delete {name}\n"))
        .collect();
    git_with(
        root,
        &["update-ref", "--stdin"],
        &[],
        Some(input.as_bytes()),
    )
    .map(|_| ())
}

/// Keep the newest `keep` turns of `chat`; returns how many turns were removed.
pub fn prune(dir: &Path, chat: &str, keep: usize) -> Result<usize, String> {
    let chat = checked(chat)?;
    let root = toplevel(dir)?;
    let all = refs(&root, chat)?;
    let mut seqs: Vec<u64> = all.iter().map(|(seq, _)| *seq).collect();
    seqs.sort_unstable();
    seqs.dedup();
    let cut = seqs.len().saturating_sub(keep);
    let Some(&last_gone) = seqs.get(cut.wrapping_sub(1)).filter(|_| cut > 0) else {
        return Ok(0);
    };
    let gone: Vec<String> = all
        .into_iter()
        .filter(|(seq, _)| *seq <= last_gone)
        .map(|(_, name)| name)
        .collect();
    delete_refs(&root, &gone)?;
    Ok(cut)
}

/// Remove every checkpoint of `chat` (the chat was deleted).
pub fn drop_chat(dir: &Path, chat: &str) -> Result<(), String> {
    let chat = checked(chat)?;
    let root = toplevel(dir)?;
    let names: Vec<String> = refs(&root, chat)?
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    delete_refs(&root, &names)
}

/// NUL-separated output split into its fields.
fn fields(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|&b| b == 0)
        .filter(|field| !field.is_empty())
        .map(|field| String::from_utf8_lossy(field).to_string())
        .collect()
}

/// The files that differ between snapshots (or commits) `from` and `to`.
pub fn changes_between(dir: &Path, from: &str, to: &str) -> Result<Vec<TurnFile>, String> {
    let root = toplevel(dir)?;
    let diff = |extra: &str| {
        git_with(
            &root,
            &[
                "-c",
                "core.quotePath=false",
                "diff",
                extra,
                "-z",
                "-M",
                from,
                to,
            ],
            &[],
            None,
        )
    };
    let counts = super::scm::numstat_counts(&diff("--numstat")?);
    let names = fields(&diff("--name-status")?);
    let mut files = Vec::new();
    let mut at = 0;
    while at < names.len() {
        let code = names[at].chars().next().unwrap_or('M');
        let (status, from, path, step) = match code {
            'R' => ("renamed", names.get(at + 1).cloned(), names.get(at + 2), 3),
            'C' => ("added", None, names.get(at + 2), 3),
            'A' => ("added", None, names.get(at + 1), 2),
            'D' => ("deleted", None, names.get(at + 1), 2),
            _ => ("modified", None, names.get(at + 1), 2),
        };
        if let Some(path) = path {
            let (added, removed) = counts.get(path).copied().unwrap_or((None, None));
            files.push(TurnFile {
                path: path.clone(),
                status,
                from,
                added,
                removed,
            });
        }
        at += step;
    }
    Ok(files)
}

/// The blob `path` has in `commit`; `None` when it has none there.
fn blob_at(root: &Path, commit: &str, path: &str) -> Option<String> {
    git(
        root,
        &["rev-parse", "--verify", "-q", &format!("{commit}:{path}")],
    )
    .ok()
}

/// The blob `path` would get if it were added now (same filters as `add`);
/// `None` when no file is there.
fn blob_now(root: &Path, path: &str) -> Result<Option<String>, String> {
    let file = root.join(path);
    if !file.is_file() {
        return Ok(None);
    }
    let file = file.to_string_lossy().to_string();
    git(root, &quiet(&["hash-object", "--path", path, &file])).map(Some)
}

/// Put files back as they are in `source`. Without `paths`, every file that
/// differs between `source` and `end`. A file whose content is no longer what
/// `end` holds was changed since and is skipped, unless `force`. A file
/// `source` does not have is deleted.
pub fn restore(
    dir: &Path,
    source: &str,
    end: &str,
    paths: Option<&[String]>,
    force: bool,
) -> Result<Restored, String> {
    let root = toplevel(dir)?;
    let paths: Vec<String> = match paths {
        Some(paths) => paths.to_vec(),
        None => fields(&git_with(
            &root,
            &[
                "-c",
                "core.quotePath=false",
                "diff",
                "--name-only",
                "-z",
                "--no-renames",
                source,
                end,
            ],
            &[],
            None,
        )?),
    };
    let mut done = Restored::default();
    let mut from_source = Vec::new();
    for path in paths {
        if path.split('/').any(|part| part == "..") {
            return Err(format!("a path outside the repository: {path}"));
        }
        let changed_since = blob_at(&root, end, &path) != blob_now(&root, &path)?;
        if changed_since && !force {
            done.skipped.push(path);
            continue;
        }
        if blob_at(&root, source, &path).is_some() {
            from_source.push(path);
        } else {
            let file = root.join(&path);
            if file.is_file() {
                std::fs::remove_file(&file).map_err(|e| format!("could not remove {path}: {e}"))?;
            }
            done.reverted.push(path);
        }
    }
    if !from_source.is_empty() {
        let list: Vec<u8> = from_source
            .iter()
            .flat_map(|path| path.bytes().chain([0]))
            .collect();
        let source_arg = format!("--source={source}");
        git_with(
            &root,
            &quiet(&[
                "--literal-pathspecs",
                "restore",
                &source_arg,
                "--worktree",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ]),
            &[],
            Some(&list),
        )?;
        done.reverted.extend(from_source);
    }
    Ok(done)
}
