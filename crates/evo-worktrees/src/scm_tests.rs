//! The person's git operations against real temporary repositories
//! (docs/features/source-control/spec.md, FR-1, FR-4 – FR-6, edge cases).

use std::path::Path;

use super::git;
use super::scm::{self, Area, Target};
use super::tests;

/// A repository with one commit, no line-ending conversion; `None` without git.
fn repo() -> Option<(tempfile::TempDir, std::path::PathBuf)> {
    let (dir, root) = tests::repo()?;
    git(&root, &["config", "core.autocrlf", "false"]).unwrap();
    Some((dir, root))
}

fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn read(root: &Path, path: &str) -> Option<String> {
    std::fs::read_to_string(root.join(path)).ok()
}

/// 50 numbered lines, with `changes` (line number → new text) applied.
fn lines(changes: &[(usize, &str)]) -> String {
    (1..=50)
        .map(|n| {
            let line = changes
                .iter()
                .find(|(at, _)| *at == n)
                .map(|(_, text)| text.to_string())
                .unwrap_or_else(|| format!("line {n}"));
            format!("{line}\n")
        })
        .collect()
}

/// A committed 50-line file changed at lines 2, 25 and 48: three hunks.
fn three_hunks(root: &Path) {
    write(root, "src/long.txt", &lines(&[]));
    git(root, &["add", "."]).unwrap();
    git(root, &["commit", "-q", "-m", "long"]).unwrap();
    write(
        root,
        "src/long.txt",
        &lines(&[(2, "TWO"), (25, "TWENTY-FIVE"), (48, "FORTY-EIGHT")]),
    );
}

fn file<'a>(list: &'a [scm::ScmFile], path: &str) -> &'a scm::ScmFile {
    list.iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("{path} in {list:?}"))
}

#[test]
fn status_splits_staged_and_unstaged() {
    let Some((dir, root)) = repo() else { return };
    assert!(
        scm::status(dir.path()).unwrap().is_none(),
        "outside a repository"
    );
    write(&root, "src/old.rs", "old\n");
    write(&root, "src/gone.rs", "gone\n");
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "files"]).unwrap();

    write(&root, "src/main.rs", "fn main() {\n    staged();\n}\n");
    git(&root, &["add", "src/main.rs"]).unwrap();
    write(
        &root,
        "src/main.rs",
        "fn main() {\n    staged();\n    unstaged();\n}\n",
    );
    git(&root, &["mv", "src/old.rs", "src/renamed.rs"]).unwrap();
    git(&root, &["rm", "-q", "src/gone.rs"]).unwrap();
    write(&root, "notes/new.txt", "a\nb\n");

    let status = scm::status(&root.join("src")).unwrap().unwrap();
    assert_eq!(status.state, "normal");
    assert_eq!(file(&status.staged, "src/main.rs").status, "modified");
    assert_eq!(
        (
            file(&status.staged, "src/main.rs").added,
            file(&status.staged, "src/main.rs").removed
        ),
        (Some(3), Some(1))
    );
    assert_eq!(file(&status.unstaged, "src/main.rs").added, Some(1));
    assert_eq!(file(&status.staged, "src/renamed.rs").status, "renamed");
    assert_eq!(
        file(&status.staged, "src/renamed.rs").from.as_deref(),
        Some("src/old.rs")
    );
    assert_eq!(file(&status.staged, "src/gone.rs").status, "deleted");
    assert_eq!(file(&status.unstaged, "notes/new.txt").status, "untracked");
    assert_eq!(file(&status.unstaged, "notes/new.txt").added, Some(2));
    assert_eq!(status.staged.len(), 3, "{:?}", status.staged);
    assert_eq!(status.unstaged.len(), 2, "{:?}", status.unstaged);
}

#[test]
fn status_names_unusual_repository_states() {
    let dir = tempfile::tempdir().unwrap();
    let fresh = dir.path().join("fresh");
    std::fs::create_dir_all(&fresh).unwrap();
    if git(&fresh, &["init", "-q"]).is_err() {
        return;
    }
    write(&fresh, "a.txt", "a\n");
    git(&fresh, &["add", "."]).unwrap();
    let unborn = scm::status(&fresh).unwrap().unwrap();
    assert_eq!(unborn.head, None);
    assert_eq!(file(&unborn.staged, "a.txt").status, "added");

    let Some((_dir, root)) = repo() else { return };
    let base = git(&root, &["branch", "--show-current"]).unwrap();
    git(&root, &["checkout", "-q", "-b", "side"]).unwrap();
    write(&root, "src/main.rs", "fn main() { side(); }\n");
    git(&root, &["commit", "-q", "-am", "side"]).unwrap();
    git(&root, &["checkout", "-q", &base]).unwrap();
    write(&root, "src/main.rs", "fn main() { base(); }\n");
    git(&root, &["commit", "-q", "-am", "base"]).unwrap();
    assert!(git(&root, &["merge", "-q", "side"]).is_err(), "a conflict");
    let merging = scm::status(&root).unwrap().unwrap();
    assert_eq!(merging.state, "merge");
    assert_eq!(merging.conflicted, vec!["src/main.rs".to_string()]);

    git(&root, &["merge", "--abort"]).unwrap();
    git(&root, &["checkout", "-q", "--detach"]).unwrap();
    let detached = scm::status(&root).unwrap().unwrap();
    assert_eq!((detached.state, detached.branch), ("normal", None));
    assert!(detached.head.is_some());
}

#[test]
fn a_diff_has_stable_hunks_and_both_texts() {
    let Some((_dir, root)) = repo() else { return };
    three_hunks(&root);
    let diff = scm::diff(&root, "src/long.txt", Area::Unstaged).unwrap();
    assert_eq!(diff.hunks.len(), 3, "{:?}", diff.hunks);
    assert_eq!((diff.hunks[1].new_start, diff.hunks[1].new_lines), (22, 7));
    assert_eq!(diff.old.as_deref(), Some(lines(&[]).as_str()));
    assert!(diff.new.as_deref().unwrap().contains("TWENTY-FIVE"));
    assert!(!diff.binary && !diff.too_large && !diff.eol_only);

    let again = scm::diff(&root, "src/long.txt", Area::Unstaged).unwrap();
    assert_eq!(again.hash, diff.hash);
    let ids = |d: &scm::FileDiff| d.hunks.iter().map(|h| h.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&again), ids(&diff));

    write(
        &root,
        "src/long.txt",
        &lines(&[(2, "TWO"), (25, "TWENTY-FIVE")]),
    );
    let changed = scm::diff(&root, "src/long.txt", Area::Unstaged).unwrap();
    assert_ne!(changed.hash, diff.hash);
    assert_eq!(
        changed.hunks[0].id, diff.hunks[0].id,
        "an untouched hunk keeps its id"
    );

    let untracked = {
        write(&root, "src/new.txt", "x\ny\n");
        scm::diff(&root, "src/new.txt", Area::Unstaged).unwrap()
    };
    assert_eq!(untracked.old, None);
    assert_eq!(untracked.new.as_deref(), Some("x\ny\n"));
    assert_eq!(untracked.hunks.len(), 1);
}

#[test]
fn binary_large_and_line_ending_changes_have_no_hunks() {
    let Some((_dir, root)) = repo() else { return };
    std::fs::write(root.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
    write(&root, "crlf.txt", "a\nb\n");
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "bin"]).unwrap();

    std::fs::write(root.join("blob.bin"), [0u8, 9, 9]).unwrap();
    let binary = scm::diff(&root, "blob.bin", Area::Unstaged).unwrap();
    assert!(binary.binary && binary.hunks.is_empty() && binary.new.is_none());

    write(&root, "big.txt", &"x".repeat(1_100_000));
    let big = scm::diff(&root, "big.txt", Area::Unstaged).unwrap();
    assert!(big.too_large && big.hunks.is_empty() && big.new.is_none());

    write(&root, "crlf.txt", "a\r\nb\r\n");
    let eol = scm::diff(&root, "crlf.txt", Area::Unstaged).unwrap();
    assert!(eol.eol_only, "{eol:?}");
}

#[test]
fn one_hunk_is_staged_and_unstaged() {
    let Some((_dir, root)) = repo() else { return };
    three_hunks(&root);
    let diff = scm::diff(&root, "src/long.txt", Area::Unstaged).unwrap();
    let middle = Target::Hunk {
        path: "src/long.txt".into(),
        hunk: diff.hunks[1].id.clone(),
        expected: diff.hash.clone(),
    };
    scm::stage(&root, &middle).unwrap();
    let staged = scm::diff(&root, "src/long.txt", Area::Staged).unwrap();
    assert_eq!(staged.hunks.len(), 1);
    assert!(staged.new.as_deref().unwrap().contains("TWENTY-FIVE"));
    assert!(!staged.new.as_deref().unwrap().contains("TWO\n"));
    assert_eq!(
        scm::diff(&root, "src/long.txt", Area::Unstaged)
            .unwrap()
            .hunks
            .len(),
        2
    );

    let back = Target::Hunk {
        path: "src/long.txt".into(),
        hunk: staged.hunks[0].id.clone(),
        expected: staged.hash.clone(),
    };
    scm::unstage(&root, &back).unwrap();
    assert!(scm::diff(&root, "src/long.txt", Area::Staged)
        .unwrap()
        .hunks
        .is_empty());
    assert_eq!(
        scm::diff(&root, "src/long.txt", Area::Unstaged)
            .unwrap()
            .hunks
            .len(),
        3
    );
    assert_eq!(
        read(&root, "src/long.txt").unwrap(),
        lines(&[(2, "TWO"), (25, "TWENTY-FIVE"), (48, "FORTY-EIGHT")])
    );
}

#[test]
fn files_and_everything_are_staged_and_unstaged() {
    let Some((_dir, root)) = repo() else { return };
    write(&root, "src/main.rs", "fn main() { 1; }\n");
    write(&root, "src/new.rs", "new\n");
    scm::stage(
        &root,
        &Target::File {
            path: "src/new.rs".into(),
        },
    )
    .unwrap();
    let status = scm::status(&root).unwrap().unwrap();
    assert_eq!((status.staged.len(), status.unstaged.len()), (1, 1));

    scm::stage(&root, &Target::All).unwrap();
    assert_eq!(scm::status(&root).unwrap().unwrap().staged.len(), 2);
    scm::unstage(
        &root,
        &Target::File {
            path: "src/main.rs".into(),
        },
    )
    .unwrap();
    assert_eq!(scm::status(&root).unwrap().unwrap().staged.len(), 1);
    scm::unstage(&root, &Target::All).unwrap();
    let status = scm::status(&root).unwrap().unwrap();
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 2);
}

#[test]
fn a_stale_hunk_is_refused() {
    let Some((_dir, root)) = repo() else { return };
    three_hunks(&root);
    let diff = scm::diff(&root, "src/long.txt", Area::Unstaged).unwrap();
    write(
        &root,
        "src/long.txt",
        &lines(&[(2, "TWO!"), (25, "TWENTY-FIVE"), (48, "FORTY-EIGHT")]),
    );
    let stale = Target::Hunk {
        path: "src/long.txt".into(),
        hunk: diff.hunks[0].id.clone(),
        expected: diff.hash,
    };
    let error = scm::stage(&root, &stale).unwrap_err();
    assert!(error.contains(scm::CHANGED_SINCE_SHOWN), "{error}");
    assert!(scm::diff(&root, "src/long.txt", Area::Staged)
        .unwrap()
        .hunks
        .is_empty());
}

#[test]
fn discards_are_undone_from_what_was_kept() {
    let Some((_dir, root)) = repo() else { return };
    three_hunks(&root);
    let before = read(&root, "src/long.txt").unwrap();
    let diff = scm::diff(&root, "src/long.txt", Area::Unstaged).unwrap();
    let kept = scm::discard(
        &root,
        &Target::Hunk {
            path: "src/long.txt".into(),
            hunk: diff.hunks[0].id.clone(),
            expected: diff.hash,
        },
    )
    .unwrap();
    assert_eq!(
        read(&root, "src/long.txt").unwrap(),
        lines(&[(25, "TWENTY-FIVE"), (48, "FORTY-EIGHT")])
    );
    scm::restore_kept(&root, &kept).unwrap();
    assert_eq!(read(&root, "src/long.txt").unwrap(), before);

    write(&root, "src/new.rs", "new\n");
    let kept = scm::discard(
        &root,
        &Target::File {
            path: "src/new.rs".into(),
        },
    )
    .unwrap();
    assert_eq!(
        read(&root, "src/new.rs"),
        None,
        "an untracked file is removed"
    );
    scm::restore_kept(&root, &kept).unwrap();
    assert_eq!(read(&root, "src/new.rs").as_deref(), Some("new\n"));

    // Everything: staged changes stay staged, the rest goes.
    write(&root, "src/main.rs", "fn main() { staged(); }\n");
    git(&root, &["add", "src/main.rs"]).unwrap();
    let kept = scm::discard(&root, &Target::All).unwrap();
    let status = scm::status(&root).unwrap().unwrap();
    assert!(status.unstaged.is_empty(), "{:?}", status.unstaged);
    assert_eq!(status.staged.len(), 1);
    scm::restore_kept(&root, &kept).unwrap();
    assert_eq!(read(&root, "src/long.txt").unwrap(), before);
    assert_eq!(read(&root, "src/new.rs").as_deref(), Some("new\n"));
}

#[test]
fn commit_amend_and_recent_subjects() {
    let Some((_dir, root)) = repo() else { return };
    write(&root, "src/main.rs", "fn main() { 1; }\n");
    scm::stage(&root, &Target::All).unwrap();
    let made = scm::commit(&root, "feat(main): print one\n\nBody.", false).unwrap();
    assert!(made.branch.is_some());
    assert_eq!(
        git(&root, &["log", "-1", "--format=%s"]).unwrap(),
        "feat(main): print one"
    );
    assert!(scm::status(&root).unwrap().unwrap().staged.is_empty());

    let amended = scm::commit(&root, "feat(main): print the number one", true).unwrap();
    assert_ne!(amended.commit, made.commit);
    assert_eq!(
        scm::recent_subjects(&root, 5).unwrap(),
        vec![
            "feat(main): print the number one".to_string(),
            "init".to_string()
        ]
    );
}

#[test]
fn a_failing_hook_and_a_missing_identity_keep_the_files_staged() {
    let Some((_dir, root)) = repo() else { return };
    write(
        &root,
        ".git/hooks/pre-commit",
        "#!/bin/sh\necho \"lint failed: main.rs\"\nexit 1\n",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            root.join(".git/hooks/pre-commit"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    write(&root, "src/main.rs", "fn main() { 2; }\n");
    scm::stage(&root, &Target::All).unwrap();
    let error = scm::commit(&root, "fix: two", false).unwrap_err();
    assert!(error.contains("lint failed: main.rs"), "{error}");
    assert_eq!(scm::status(&root).unwrap().unwrap().staged.len(), 1);

    std::fs::remove_file(root.join(".git/hooks/pre-commit")).unwrap();
    git(&root, &["config", "user.name", ""]).unwrap();
    let error = scm::commit(&root, "fix: two", false).unwrap_err();
    assert!(error.contains("user.name"), "{error}");
    assert_eq!(scm::status(&root).unwrap().unwrap().staged.len(), 1);
}

#[test]
fn head_on_upstream_follows_pushes() {
    let Some((dir, root)) = repo() else { return };
    let clone = dir.path().join("clone");
    let source = root.to_string_lossy().to_string();
    let target = clone.to_string_lossy().to_string();
    git(dir.path(), &["clone", "-q", &source, &target]).unwrap();
    git(&clone, &["config", "core.autocrlf", "false"]).unwrap();
    git(&clone, &["config", "user.email", "evo@example.invalid"]).unwrap();
    git(&clone, &["config", "user.name", "evo"]).unwrap();
    assert!(scm::head_on_upstream(&clone));
    write(&clone, "src/main.rs", "fn main() { local(); }\n");
    scm::stage(&clone, &Target::All).unwrap();
    scm::commit(&clone, "local", false).unwrap();
    assert!(!scm::head_on_upstream(&clone));
    assert!(!scm::head_on_upstream(&root), "no upstream");
}

#[test]
fn paths_outside_the_repository_are_refused() {
    let Some((_dir, root)) = repo() else { return };
    for path in ["../x", "src/../../x", ""] {
        assert!(
            scm::stage(&root, &Target::File { path: path.into() }).is_err(),
            "{path:?}"
        );
        assert!(
            scm::discard(&root, &Target::File { path: path.into() }).is_err(),
            "{path:?}"
        );
        assert!(scm::diff(&root, path, Area::Unstaged).is_err(), "{path:?}");
    }
}

/// `git init` with one commit in `dir`; `false` without git.
fn init_repo(dir: &Path) -> bool {
    std::fs::create_dir_all(dir).unwrap();
    if git(dir, &["init", "-q"]).is_err() {
        return false;
    }
    for args in [
        &["config", "user.email", "evo@example.invalid"][..],
        &["config", "user.name", "evo"],
        &["config", "core.autocrlf", "false"],
    ] {
        git(dir, args).unwrap();
    }
    write(dir, "README.md", "x\n");
    git(dir, &["add", "."]).unwrap();
    git(dir, &["commit", "-q", "-m", "init"]).unwrap();
    true
}

#[test]
fn repositories_lists_the_folders_own_and_nested_ones() {
    let dir = tempfile::tempdir().unwrap();
    let top = dir.path().join("workspace");
    if !init_repo(&top) {
        return;
    }
    // As a real workspace keeps them: the nested repositories are ignored by the top one.
    write(
        &top,
        ".gitignore",
        "api/\nlibs/\nnode_modules/\ntarget/\n.cache/\na/\n",
    );
    git(&top, &["add", ".gitignore"]).unwrap();
    git(&top, &["commit", "-q", "-m", "ignore"]).unwrap();
    // Nested ones at depth 1 and 2, one inside another (not listed: found through its parent).
    assert!(init_repo(&top.join("api")));
    assert!(init_repo(&top.join("libs/ui")));
    assert!(init_repo(&top.join("libs/ui/inner")));
    // Skipped: dependency, build and hidden folders, and deeper than 4 levels.
    assert!(init_repo(&top.join("node_modules/pkg")));
    assert!(init_repo(&top.join("target/x")));
    assert!(init_repo(&top.join(".cache/repo")));
    assert!(init_repo(&top.join("a/b/c/d/e")));
    assert!(init_repo(&top.join("a/b/c/four")));
    write(&top.join("api"), "new.txt", "new\n");
    write(&top.join("api"), "README.md", "changed\n");

    let repos = scm::repositories(&top).unwrap();
    let paths: Vec<&str> = repos.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![".", "a/b/c/four", "api", "libs/ui"],
        "{repos:?}"
    );
    let api = repos.iter().find(|r| r.path == "api").unwrap();
    assert_eq!(api.changed, 2);
    assert!(api.branch.is_some());
    assert_eq!(
        repos[0].changed, 0,
        "the nested repositories are not changes of the top one: {repos:?}"
    );

    // From a subfolder of the top repository: the enclosing one is still `.`.
    let from_sub = scm::repositories(&top.join("libs")).unwrap();
    let paths: Vec<&str> = from_sub.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, vec![".", "ui"]);
    assert_eq!(from_sub[0].root, repos[0].root);
}

#[test]
fn a_plain_folder_lists_the_repositories_it_holds() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("projects");
    std::fs::create_dir_all(&plain).unwrap();
    if !init_repo(&plain.join("one")) {
        return;
    }
    assert!(init_repo(&plain.join("two")));
    let repos = scm::repositories(&plain).unwrap();
    let paths: Vec<&str> = repos.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, vec!["one", "two"]);
    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(scm::repositories(&empty).unwrap().is_empty());
}

#[test]
fn a_worktree_folder_counts_as_a_repository_and_the_scan_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("ws");
    if !init_repo(&plain.join("main")) {
        return;
    }
    let wt = plain.join("wt").to_string_lossy().to_string();
    git(&plain.join("main"), &["worktree", "add", "-q", &wt]).unwrap();
    assert!(
        plain.join("wt/.git").is_file(),
        "a worktree has a .git file"
    );
    let repos = scm::repositories(&plain).unwrap();
    let paths: Vec<&str> = repos.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, vec!["main", "wt"]);
    assert_eq!(
        scm::scan_repositories(&plain, 4, 1).unwrap().len(),
        1,
        "the limit holds"
    );
}

#[test]
fn discarding_a_folder_takes_every_unstaged_file_under_it_and_undoes() {
    let Some((_dir, root)) = repo() else { return };
    write(&root, "src/lib/a.rs", "a\n");
    write(&root, "src/lib/b.rs", "b\n");
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "lib"]).unwrap();
    write(&root, "src/lib/a.rs", "a changed\n");
    write(&root, "src/lib/new.rs", "new\n");
    write(&root, "src/lib/b.rs", "b staged\n");
    git(&root, &["add", "src/lib/b.rs"]).unwrap();
    write(&root, "src/main.rs", "fn main() { outside(); }\n");

    let kept = scm::discard(
        &root,
        &Target::File {
            path: "src/lib".into(),
        },
    )
    .unwrap();
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert_eq!(read(&root, "src/lib/a.rs").as_deref(), Some("a\n"));
    assert_eq!(read(&root, "src/lib/new.rs"), None);
    assert_eq!(
        read(&root, "src/lib/b.rs").as_deref(),
        Some("b staged\n"),
        "staged changes stay"
    );
    assert_eq!(
        read(&root, "src/main.rs").as_deref(),
        Some("fn main() { outside(); }\n"),
        "outside the folder stays"
    );

    scm::restore_kept(&root, &kept).unwrap();
    assert_eq!(read(&root, "src/lib/a.rs").as_deref(), Some("a changed\n"));
    assert_eq!(read(&root, "src/lib/new.rs").as_deref(), Some("new\n"));

    // A folder stages and unstages as a whole too.
    scm::stage(
        &root,
        &Target::File {
            path: "src/lib".into(),
        },
    )
    .unwrap();
    let staged: Vec<String> = scm::status(&root)
        .unwrap()
        .unwrap()
        .staged
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert_eq!(
        staged,
        vec!["src/lib/a.rs", "src/lib/b.rs", "src/lib/new.rs"]
    );
    scm::unstage(
        &root,
        &Target::File {
            path: "src/lib".into(),
        },
    )
    .unwrap();
    assert!(scm::status(&root).unwrap().unwrap().staged.is_empty());
}
