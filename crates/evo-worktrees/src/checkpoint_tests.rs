//! Turn checkpoints against real temporary repositories
//! (docs/features/source-control/spec.md, FR-9, FR-11, FR-14, NFR-3, NFR-4).

use std::path::Path;

use super::checkpoint::{self, Kind};
use super::git;
use super::tests;

/// A repository with one commit that writes files as they are (no line-ending
/// conversion), so the tests can compare bytes; `None` without git.
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

/// What a checkpoint must never change: status, index, branches, tags, stashes, `HEAD`.
fn untouched(root: &Path) -> Vec<String> {
    let index = std::fs::read(root.join(".git/index")).unwrap_or_default();
    vec![
        git(
            root,
            &[
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=all",
            ],
        )
        .unwrap(),
        format!("{:?}", index),
        git(root, &["branch", "-a"]).unwrap(),
        git(root, &["tag"]).unwrap(),
        git(root, &["stash", "list"]).unwrap(),
        git(root, &["rev-parse", "HEAD"]).unwrap_or_default(),
    ]
}

#[test]
fn a_snapshot_keeps_the_working_tree_and_changes_nothing() {
    let Some((_dir, root)) = repo() else { return };
    write(&root, ".gitignore", "build/\n");
    git(&root, &["add", ".gitignore"]).unwrap();
    git(&root, &["commit", "-q", "-m", "ignore"]).unwrap();
    write(&root, "src/main.rs", "fn main() { changed(); }\n");
    write(&root, "src/staged.rs", "staged\n");
    git(&root, &["add", "src/staged.rs"]).unwrap();
    write(&root, "notes/new file.txt", "untracked\n");
    write(&root, "build/out.bin", "ignored\n");
    let before = untouched(&root);

    let commit = checkpoint::snapshot(&root.join("src"), "chat-1", 1, Kind::Start).unwrap();

    assert_eq!(
        untouched(&root),
        before,
        "status, index, refs and HEAD stay as they were"
    );
    let at = |path: &str| git(&root, &["show", &format!("{commit}:{path}")]).ok();
    assert_eq!(
        at("src/main.rs").as_deref(),
        Some("fn main() { changed(); }")
    );
    assert_eq!(at("src/staged.rs").as_deref(), Some("staged"));
    assert_eq!(at("notes/new file.txt").as_deref(), Some("untracked"));
    assert_eq!(at("build/out.bin"), None, "ignored files are left out");
    assert_eq!(
        checkpoint::find(&root, "chat-1", 1, Kind::Start)
            .unwrap()
            .as_deref(),
        Some(commit.as_str())
    );
    assert_eq!(
        checkpoint::find(&root, "chat-1", 1, Kind::End).unwrap(),
        None
    );
}

#[test]
fn a_snapshot_works_before_the_first_commit_and_without_an_identity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("fresh");
    std::fs::create_dir_all(&root).unwrap();
    if git(&root, &["init", "-q"]).is_err() {
        return;
    }
    write(&root, "a.txt", "one\n");
    let start = checkpoint::snapshot(&root, "c", 1, Kind::Start).unwrap();
    write(&root, "a.txt", "one\ntwo\n");
    let end = checkpoint::snapshot(&root, "c", 1, Kind::End).unwrap();
    let files = checkpoint::changes_between(&root, &start, &end).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(
        (files[0].path.as_str(), files[0].status),
        ("a.txt", "modified")
    );
    assert_eq!((files[0].added, files[0].removed), (Some(1), Some(0)));
}

#[test]
fn changes_between_lists_every_kind_with_line_counts() {
    let Some((_dir, root)) = repo() else { return };
    let long: String = (1..=40).map(|n| format!("line {n}\n")).collect();
    write(&root, "src/old.rs", &long);
    write(&root, "src/gone.rs", "bye\n");
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "more"]).unwrap();

    let start = checkpoint::snapshot(&root, "c", 7, Kind::Start).unwrap();
    write(&root, "src/main.rs", "fn main() {\n    run();\n}\n");
    write(&root, "src/added.rs", "a\nb\n");
    std::fs::remove_file(root.join("src/gone.rs")).unwrap();
    std::fs::rename(root.join("src/old.rs"), root.join("src/new.rs")).unwrap();
    let end = checkpoint::snapshot(&root, "c", 7, Kind::End).unwrap();

    let files = checkpoint::changes_between(&root, &start, &end).unwrap();
    let file = |p: &str| {
        files
            .iter()
            .find(|f| f.path == p)
            .unwrap_or_else(|| panic!("{p} in {files:?}"))
            .clone()
    };
    assert_eq!(file("src/main.rs").status, "modified");
    assert_eq!(
        (file("src/main.rs").added, file("src/main.rs").removed),
        (Some(3), Some(1))
    );
    assert_eq!(file("src/added.rs").status, "added");
    assert_eq!(file("src/gone.rs").status, "deleted");
    assert_eq!(file("src/new.rs").status, "renamed");
    assert_eq!(file("src/new.rs").from.as_deref(), Some("src/old.rs"));
    assert_eq!(files.len(), 4, "{files:?}");
}

#[test]
fn restore_puts_a_turn_back_and_skips_files_changed_since() {
    let Some((_dir, root)) = repo() else { return };
    write(&root, "src/kept.rs", "kept\n");
    write(&root, "src/b.rs", "b\n");
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "files"]).unwrap();

    let start = checkpoint::snapshot(&root, "c", 1, Kind::Start).unwrap();
    // The turn: edits main, adds new, deletes b.
    write(&root, "src/main.rs", "fn main() { turn(); }\n");
    write(&root, "src/new.rs", "new\n");
    std::fs::remove_file(root.join("src/b.rs")).unwrap();
    let end = checkpoint::snapshot(&root, "c", 1, Kind::End).unwrap();
    // After the turn the person changes main again, and an unrelated file.
    write(&root, "src/main.rs", "fn main() { later(); }\n");
    write(&root, "src/kept.rs", "kept, edited later\n");

    let restored = checkpoint::restore(&root, &start, &end, None, false).unwrap();
    assert_eq!(restored.skipped, vec!["src/main.rs".to_string()]);
    let mut reverted = restored.reverted.clone();
    reverted.sort();
    assert_eq!(
        reverted,
        vec!["src/b.rs".to_string(), "src/new.rs".to_string()]
    );
    assert_eq!(read(&root, "src/b.rs").as_deref(), Some("b\n"));
    assert_eq!(read(&root, "src/new.rs"), None);
    assert_eq!(
        read(&root, "src/main.rs").as_deref(),
        Some("fn main() { later(); }\n")
    );
    assert_eq!(
        read(&root, "src/kept.rs").as_deref(),
        Some("kept, edited later\n"),
        "not the turn's file"
    );

    let forced = checkpoint::restore(
        &root,
        &start,
        &end,
        Some(&["src/main.rs".to_string()]),
        true,
    )
    .unwrap();
    assert_eq!(forced.reverted, vec!["src/main.rs".to_string()]);
    assert_eq!(
        read(&root, "src/main.rs").as_deref(),
        Some("fn main() {}\n")
    );
}

#[test]
fn a_revert_is_undone_from_the_snapshot_taken_before_it() {
    let Some((_dir, root)) = repo() else { return };
    let start = checkpoint::snapshot(&root, "c", 2, Kind::Start).unwrap();
    write(&root, "src/main.rs", "fn main() { turn(); }\n");
    write(&root, "src/new.rs", "new\n");
    let end = checkpoint::snapshot(&root, "c", 2, Kind::End).unwrap();

    let undo = checkpoint::snapshot(&root, "c", 2, Kind::Undo).unwrap();
    let reverted = checkpoint::restore(&root, &start, &end, None, false)
        .unwrap()
        .reverted;
    assert_eq!(read(&root, "src/new.rs"), None);

    let back = checkpoint::restore(&root, &undo, &start, Some(&reverted), false).unwrap();
    assert!(back.skipped.is_empty(), "{back:?}");
    assert_eq!(
        read(&root, "src/main.rs").as_deref(),
        Some("fn main() { turn(); }\n")
    );
    assert_eq!(read(&root, "src/new.rs").as_deref(), Some("new\n"));
}

#[test]
fn prune_keeps_the_newest_turns_and_drop_removes_a_chat() {
    let Some((_dir, root)) = repo() else { return };
    for seq in 1..=5 {
        write(&root, "src/main.rs", &format!("fn main() {{ {seq}; }}\n"));
        checkpoint::snapshot(&root, "a", seq, Kind::Start).unwrap();
        checkpoint::snapshot(&root, "a", seq, Kind::End).unwrap();
    }
    checkpoint::snapshot(&root, "b", 1, Kind::Start).unwrap();
    assert_eq!(checkpoint::turns(&root, "a").unwrap(), vec![1, 2, 3, 4, 5]);

    assert_eq!(checkpoint::prune(&root, "a", 2).unwrap(), 3);
    assert_eq!(checkpoint::turns(&root, "a").unwrap(), vec![4, 5]);
    assert!(checkpoint::find(&root, "a", 5, Kind::End)
        .unwrap()
        .is_some());

    checkpoint::drop_chat(&root, "a").unwrap();
    assert!(checkpoint::turns(&root, "a").unwrap().is_empty());
    assert_eq!(
        checkpoint::turns(&root, "b").unwrap(),
        vec![1],
        "other chats keep theirs"
    );
    let refs = git(&root, &["for-each-ref", "--format=%(refname)"]).unwrap();
    assert!(!refs.contains("refs/evo/checkpoints/a/"), "{refs}");
    for listing in [&["branch", "-a"][..], &["tag"], &["stash", "list"]] {
        assert!(!git(&root, listing).unwrap().contains("checkpoint"));
    }
}

#[test]
fn a_bad_chat_id_and_a_plain_folder_are_refused() {
    let Some((dir, root)) = repo() else { return };
    for bad in ["", "a/b", "..", "a b", "x:y"] {
        assert!(
            checkpoint::snapshot(&root, bad, 1, Kind::Start).is_err(),
            "{bad:?}"
        );
    }
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    assert!(checkpoint::snapshot(&plain, "c", 1, Kind::Start).is_err());
}

#[test]
fn a_reverted_file_is_written_as_a_checkout_would_with_autocrlf() {
    let Some((_dir, root)) = repo() else { return };
    git(&root, &["config", "core.autocrlf", "true"]).unwrap();
    write(&root, "src/crlf.txt", "one\ntwo\n");
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "crlf"]).unwrap();
    // As a checkout on Windows leaves it: CRLF, with a clean status.
    std::fs::remove_file(root.join("src/crlf.txt")).unwrap();
    git(&root, &["checkout", "--", "src/crlf.txt"]).unwrap();
    let checked_out = std::fs::read(root.join("src/crlf.txt")).unwrap();
    assert_eq!(checked_out, b"one\r\ntwo\r\n");

    let start = checkpoint::snapshot(&root, "c", 1, Kind::Start).unwrap();
    write(&root, "src/crlf.txt", "one\nTWO\n");
    let end = checkpoint::snapshot(&root, "c", 1, Kind::End).unwrap();
    let files = checkpoint::changes_between(&root, &start, &end).unwrap();
    assert_eq!(
        (files[0].added, files[0].removed),
        (Some(1), Some(1)),
        "line endings are not a change"
    );

    let restored = checkpoint::restore(&root, &start, &end, None, false).unwrap();
    assert_eq!(restored.reverted, vec!["src/crlf.txt".to_string()]);
    assert_eq!(
        std::fs::read(root.join("src/crlf.txt")).unwrap(),
        checked_out
    );
}

/// Bytes git keeps in `.git/objects` (loose and packed).
fn objects_size(root: &Path) -> u64 {
    let counts = git(root, &["count-objects", "-v"]).unwrap();
    counts
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(": ")?;
            matches!(key, "size" | "size-pack").then(|| value.parse::<u64>().ok())?
        })
        .sum::<u64>()
        * 1024
}

/// NFR-1 and NFR-4 on a large repository: run with
/// `cargo test -p evo-worktrees checkpoint_large -- --ignored --nocapture`.
#[test]
#[ignore = "builds a 50,000-file repository; slow"]
fn checkpoint_large_repository_is_fast_and_small() {
    // `EVO_BIG_REPO` keeps the repository between runs (it takes minutes to build).
    let kept = std::env::var_os("EVO_BIG_REPO").map(std::path::PathBuf::from);
    let Some((_dir, fresh)) = repo() else { return };
    let root = kept.clone().unwrap_or(fresh);
    let path = |n: usize| format!("tree/d{:03}/f{:03}.txt", n / 100, n % 100);
    if !root.join("tree/d499/f099.txt").exists() {
        if kept.is_some() {
            std::fs::create_dir_all(&root).unwrap();
            git(&root, &["init", "-q"]).unwrap();
            git(&root, &["config", "core.autocrlf", "false"]).unwrap();
            git(&root, &["config", "user.email", "evo@example.invalid"]).unwrap();
            git(&root, &["config", "user.name", "evo"]).unwrap();
        }
        for n in 0..50_000 {
            write(&root, &path(n), &format!("file {n}\nsome text\n"));
        }
        git(&root, &["add", "-A"]).unwrap();
        git(&root, &["commit", "-q", "-m", "large"]).unwrap();
        git(&root, &["gc", "-q"]).unwrap();
    }
    git(&root, &["checkout", "-q", "--", "."]).unwrap();
    // Refresh the index's stat cache, as a status in the app would.
    git(&root, &["status", "--porcelain"]).unwrap();

    for n in (0..50_000).step_by(250) {
        write(&root, &path(n), &format!("file {n}\nchanged\n"));
    }
    // The first snapshot after the files changed reads them cold; later ones
    // (every turn after the first) find git's data in the OS cache.
    let timed = |kind: Kind| {
        let started = std::time::Instant::now();
        let commit = checkpoint::snapshot(&root, "big", 1, kind).unwrap();
        (commit, started.elapsed())
    };
    let (start, cold) = timed(Kind::Start);
    let (end, warm) = timed(Kind::End);
    println!("checkpoint of 50,000 files with 200 changed: first {cold:?}, then {warm:?}");
    assert!(checkpoint::changes_between(&root, &start, &end)
        .unwrap()
        .is_empty());

    let before = objects_size(&root);
    for turn in 2..=51 {
        checkpoint::snapshot(&root, "big", turn, Kind::Start).unwrap();
        write(&root, &path(turn as usize * 7), &format!("turn {turn}\n"));
        checkpoint::snapshot(&root, "big", turn, Kind::End).unwrap();
    }
    let growth = objects_size(&root) - before;
    println!(
        "50 turns of one small edit grew the objects by {} KiB (repository: {} KiB)",
        growth / 1024,
        before / 1024
    );
    assert!(growth < before / 2, "NFR-4: {growth} bytes on {before}");
    assert!(warm < std::time::Duration::from_secs(1), "NFR-1: {warm:?}");
}
