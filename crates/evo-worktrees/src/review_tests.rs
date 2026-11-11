//! Review, apply and removal in the awkward cases: a workspace that is itself
//! a worktree, a moved `HEAD`, text that is not UTF-8, a failure part-way, a
//! folder still in use, and leftovers.

use super::tests::repo;
use super::*;

fn manager(dir: &tempfile::TempDir) -> Worktrees {
    Worktrees::with_data_root(dir.path().join("data"))
}

/// Every file under `root` (not `.git`) with its bytes.
fn tree(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if name == ".git" || name == ".evo" {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(name, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn branches(root: &Path) -> String {
    git(root, &["branch", "--list", &format!("{BRANCH_PREFIX}*")]).unwrap()
}

#[test]
fn apply_lands_in_the_checkout_the_worktree_was_cut_from() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    // The workspace is the person's own linked worktree, one commit ahead.
    let linked = dir.path().join("linked");
    git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            &linked.to_string_lossy(),
        ],
    )
    .unwrap();
    std::fs::write(linked.join("feature.txt"), "feature\n").unwrap();
    git(&linked, &["add", "."]).unwrap();
    git(&linked, &["commit", "-q", "-m", "feature"]).unwrap();
    let feature_head = git(&linked, &["rev-parse", "HEAD"]).unwrap();

    let created = worktrees.create(&linked, Location::UserData).unwrap();
    assert_eq!(created.base, feature_head);
    let wt = &created.worktree.path;
    std::fs::write(wt.join("feature.txt"), "feature\nmore\n").unwrap();
    std::fs::write(wt.join("child.txt"), "child\n").unwrap();

    let changes = worktrees.changes(&linked, wt).unwrap();
    assert_eq!(changes.base, feature_head);
    let mut paths: Vec<_> = changes.files.iter().map(|f| f.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, ["child.txt", "feature.txt"]);

    worktrees.apply(&linked, wt, None).unwrap();
    assert!(
        linked.join("child.txt").is_file(),
        "applied where it was cut"
    );
    assert!(!root.join("child.txt").exists(), "not in the main checkout");
    assert_eq!(git(&root, &["status", "--porcelain"]).unwrap(), "");
}

#[test]
fn the_fork_point_is_the_one_recorded_at_creation() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    std::fs::write(root.join("second.txt"), "second\n").unwrap();
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "second"]).unwrap();

    let created = worktrees.create(&root, Location::UserData).unwrap();
    let wt = &created.worktree.path;
    std::fs::write(wt.join("child.txt"), "child\n").unwrap();
    // The person moves to an older branch meanwhile.
    git(&root, &["checkout", "-q", "-b", "older", "HEAD~1"]).unwrap();

    let changes = worktrees.changes(&root, wt).unwrap();
    assert_eq!(changes.base, created.base);
    let paths: Vec<_> = changes.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["child.txt"], "only the child's work");

    // A worktree made before the fork was recorded: the merge base, as before.
    std::fs::remove_file(git_dir(wt).unwrap().join(BASE_FILE)).unwrap();
    let older = worktrees.changes(&root, wt).unwrap();
    assert_eq!(older.base, git(&root, &["rev-parse", "HEAD"]).unwrap());
}

#[test]
fn apply_keeps_text_that_is_not_utf8() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    git(&root, &["config", "core.autocrlf", "false"]).unwrap();
    std::fs::write(root.join("latin1.txt"), b"caf\xe9\nline\n").unwrap();
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "latin1"]).unwrap();

    let created = worktrees.create(&root, Location::UserData).unwrap();
    let wt = &created.worktree.path;
    let latin1 = b"caf\xe9 cr\xe8me\nline\n";
    let shift_jis = b"\x93\xfa\x96\x7b\x8c\xea\n";
    std::fs::write(wt.join("latin1.txt"), latin1).unwrap();
    std::fs::write(wt.join("sjis.txt"), shift_jis).unwrap();

    worktrees.apply(&root, wt, None).unwrap();
    assert_eq!(std::fs::read(root.join("latin1.txt")).unwrap(), latin1);
    assert_eq!(std::fs::read(root.join("sjis.txt")).unwrap(), shift_jis);
}

#[test]
fn a_failure_part_way_puts_the_workspace_back() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    std::fs::write(root.join("old.txt"), "to delete\n").unwrap();
    std::fs::write(root.join("notes.txt"), "one\ntwo\nthree\nfour\nfive\nsix\n").unwrap();
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "more"]).unwrap();

    let created = worktrees.create(&root, Location::UserData).unwrap();
    let wt = &created.worktree.path;
    std::fs::write(wt.join("src/main.rs"), "fn main() { child(); }\n").unwrap();
    std::fs::write(
        wt.join("notes.txt"),
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\n",
    )
    .unwrap();
    std::fs::remove_file(wt.join("old.txt")).unwrap();
    std::fs::create_dir_all(wt.join("deep/er")).unwrap();
    std::fs::write(wt.join("deep/er/new.txt"), "new\n").unwrap();

    // The person's own uncommitted work: an edit to a patched file, and a new file.
    std::fs::write(root.join("notes.txt"), "ONE\ntwo\nthree\nfour\nfive\nsix\n").unwrap();
    std::fs::write(root.join("untracked.txt"), "mine\n").unwrap();
    let before = tree(&root);
    let status = git(&root, &["status", "--porcelain"]).unwrap();

    FAIL_AFTER_APPLY.with(|fail| fail.set(true));
    let error = worktrees.apply(&root, wt, None).unwrap_err();
    FAIL_AFTER_APPLY.with(|fail| fail.set(false));
    assert!(error.contains("put back"), "{error}");
    assert_eq!(tree(&root), before, "byte-identical after the failure");
    assert!(!root.join("deep").exists(), "new folders are removed again");
    assert_eq!(git(&root, &["status", "--porcelain"]).unwrap(), status);
    assert!(wt.exists(), "the worktree is kept to try again");

    // The same apply goes through once nothing fails.
    let applied = worktrees.apply(&root, wt, None).unwrap();
    assert!(applied.cleaned);
    assert_eq!(
        std::fs::read_to_string(root.join("deep/er/new.txt"))
            .unwrap()
            .replace("\r\n", "\n"),
        "new\n"
    );
    assert!(!root.join("old.txt").exists());
    let notes = std::fs::read_to_string(root.join("notes.txt")).unwrap();
    assert!(
        notes.starts_with("ONE") && notes.contains("seven"),
        "{notes}"
    );
}

#[cfg(windows)]
#[test]
fn a_locked_file_does_not_leave_the_workspace_half_changed() {
    use std::os::windows::fs::OpenOptionsExt;
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    std::fs::write(root.join("b.txt"), "b1\nb2\nb3\nb4\nb5\nb6\n").unwrap();
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "more"]).unwrap();

    let created = worktrees.create(&root, Location::UserData).unwrap();
    let wt = &created.worktree.path;
    std::fs::write(wt.join("a.txt"), "a changed\n").unwrap();
    std::fs::write(wt.join("b.txt"), "b1\nb2\nb3\nb4\nb5\nb6\nb7\n").unwrap();

    // The person's uncommitted edit to another patched file.
    std::fs::write(root.join("b.txt"), "B1\nb2\nb3\nb4\nb5\nb6\n").unwrap();
    let before = tree(&root);
    // Another program has `a.txt` open: readable, but it cannot be replaced.
    const FILE_SHARE_READ: u32 = 1;
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(root.join("a.txt"))
        .unwrap();
    let error = worktrees.apply(&root, wt, None).unwrap_err();
    drop(held);
    assert!(error.contains("put back"), "{error}");
    assert_eq!(tree(&root), before, "byte-identical after the failure");
    assert!(wt.exists(), "the worktree is kept to try again");
    assert!(worktrees.apply(&root, wt, None).unwrap().cleaned);
}

#[cfg(windows)]
#[test]
fn a_folder_in_use_is_left_and_cleaned_up_later() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    let created = worktrees.create(&root, Location::UserData).unwrap();
    let wt = &created.worktree.path;
    std::fs::write(wt.join("child.txt"), "child\n").unwrap();
    // A process still works in the worktree (a nested session's engine).
    let mut held = Command::new("ping")
        .args(["-n", "120", "127.0.0.1"])
        .current_dir(wt.join("src"))
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let applied = worktrees.apply(&root, wt, None).unwrap();
    assert!(!applied.cleaned, "applied, the folder is still in use");
    assert!(root.join("child.txt").is_file());
    assert!(wt.exists());
    assert!(branches(&root).trim().is_empty(), "the branch is gone");
    assert!(worktrees.list(&root).unwrap().is_empty());
    // Not changed any more: nothing to keep.
    assert!(!Worktrees::changed(wt, Some(&created.base)));

    held.kill().unwrap();
    held.wait().unwrap();
    let removed = worktrees.remove(&root, wt).unwrap();
    assert!(removed.cleaned);
    assert!(!wt.exists());
}

#[test]
fn a_leftover_folder_under_a_managed_location_is_removed() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    for location in [Location::UserData, Location::Repository] {
        let created = worktrees.create(&root, location).unwrap();
        let wt = &created.worktree.path;
        // What a failed removal leaves: no registration, part of the folder, the branch.
        git(
            &root,
            &["worktree", "remove", "--force", &wt.to_string_lossy()],
        )
        .unwrap();
        std::fs::create_dir_all(wt.join("left/over")).unwrap();
        std::fs::write(wt.join("left/over/file"), "x").unwrap();
        assert!(!branches(&root).trim().is_empty());

        let removed = worktrees.remove(&root, wt).unwrap();
        assert!(removed.cleaned);
        assert!(!wt.exists());
        assert!(branches(&root).trim().is_empty());
        // Gone already: removing it again is not an error.
        assert!(worktrees.remove(&root, wt).unwrap().cleaned);
    }

    // Anything else stays refused: other names there, and folders elsewhere.
    let managed = root.join(".evo/worktrees");
    let own = managed.join("my-notes");
    std::fs::create_dir_all(&own).unwrap();
    assert!(worktrees.remove(&root, &own).is_err());
    assert!(own.exists());
    let nested = managed.join("abc123").join("inner");
    std::fs::create_dir_all(&nested).unwrap();
    assert!(worktrees.remove(&root, &nested).is_err());
    let outside = dir.path().join("abc123");
    std::fs::create_dir_all(&outside).unwrap();
    assert!(worktrees.remove(&root, &outside).is_err());
    assert!(worktrees
        .remove(
            &root,
            &managed
                .join("abc123")
                .join("..")
                .join("..")
                .join("..")
                .join("src")
        )
        .is_err());
    assert!(nested.exists() && outside.exists() && root.join("src/main.rs").exists());
}

#[test]
fn apply_refuses_changes_that_were_not_reviewed() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    let created = worktrees.create(&root, Location::UserData).unwrap();
    let wt = &created.worktree.path;
    std::fs::write(wt.join("child.txt"), "reviewed\n").unwrap();
    let shown = worktrees.changes(&root, wt).unwrap();
    assert_eq!(shown.hash, worktrees.changes(&root, wt).unwrap().hash);

    // The child keeps working after the dialog opened.
    std::fs::write(wt.join("later.txt"), "not reviewed\n").unwrap();
    let error = worktrees.apply(&root, wt, Some(&shown.hash)).unwrap_err();
    assert!(error.contains("changed since you opened this"), "{error}");
    assert!(!root.join("child.txt").exists() && !root.join("later.txt").exists());
    assert!(wt.exists());

    let now = worktrees.changes(&root, wt).unwrap();
    assert_ne!(now.hash, shown.hash);
    worktrees.apply(&root, wt, Some(&now.hash)).unwrap();
    assert!(root.join("child.txt").is_file() && root.join("later.txt").is_file());
}

#[test]
fn a_failed_create_leaves_no_branch_or_folder() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    // `git worktree add` fails after it made the branch and the folder.
    let hook = root.join(".git/hooks/post-checkout");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    for location in [Location::UserData, Location::Repository] {
        assert!(worktrees.create(&root, location).is_err());
    }
    assert!(branches(&root).trim().is_empty(), "{}", branches(&root));
    for location in [Location::UserData, Location::Repository] {
        let left: Vec<_> = std::fs::read_dir(worktrees.dir(&root, location))
            .map(|entries| entries.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "{left:?}");
    }
    let registered = git(&root, &["worktree", "list", "--porcelain"]).unwrap();
    assert_eq!(registered.matches("worktree ").count(), 1, "{registered}");
}

#[test]
fn removing_deletes_the_worktree_branch_by_name() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    let created = worktrees.create(&root, Location::UserData).unwrap();
    // The child switched to a branch of its own.
    git(&created.worktree.path, &["checkout", "-q", "-b", "side"]).unwrap();
    assert!(
        worktrees
            .remove(&root, &created.worktree.path)
            .unwrap()
            .cleaned
    );
    assert!(branches(&root).trim().is_empty(), "{}", branches(&root));
}

#[test]
fn a_missing_worktree_is_not_changed() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    assert!(!Worktrees::changed(&dir.path().join("gone"), Some("abc")));
    let created = worktrees.create(&root, Location::UserData).unwrap();
    std::fs::write(created.worktree.path.join("x.txt"), "x").unwrap();
    assert!(Worktrees::changed(
        &created.worktree.path,
        Some(&created.base)
    ));
    worktrees.remove(&root, &created.worktree.path).unwrap();
    assert!(!Worktrees::changed(
        &created.worktree.path,
        Some(&created.base)
    ));
}

/// `link` pointing at the folder `real`: a symlink, or a junction on Windows.
fn link_dir(real: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(real, link).is_ok()
    }
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(real)
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }
}

#[test]
fn user_data_worktrees_behind_a_link_are_listed_and_removable() {
    let Some((dir, root)) = repo() else { return };
    let real = dir.path().join("real-data");
    std::fs::create_dir_all(&real).unwrap();
    let link = dir.path().join("data-link");
    if !link_dir(&real, &link) {
        eprintln!("skipping: cannot link folders here");
        return;
    }
    let worktrees = Worktrees::with_data_root(link);
    let created = worktrees.create(&root, Location::UserData).unwrap();
    let listed = worktrees.list(&root).unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].location, "userData");
    // By the path it was created with, and by the one git reports.
    worktrees.changes(&root, &created.worktree.path).unwrap();
    worktrees.changes(&root, &listed[0].path).unwrap();
    assert!(
        worktrees
            .remove(&root, &created.worktree.path)
            .unwrap()
            .cleaned
    );
    assert!(worktrees.list(&root).unwrap().is_empty());
}

#[test]
fn file_names_outside_ascii_are_listed_as_they_are() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    let created = worktrees.create(&root, Location::UserData).unwrap();
    let wt = &created.worktree.path;
    std::fs::write(wt.join("naïve café.txt"), "x\n").unwrap();
    std::fs::write(wt.join("tab\there.txt"), "y\n").ok();
    let changes = worktrees.changes(&root, wt).unwrap();
    assert!(
        changes
            .files
            .iter()
            .any(|f| f.path == "naïve café.txt" && f.added == Some(1)),
        "{:?}",
        changes.files
    );
    assert!(
        changes.patch.contains("naïve café.txt"),
        "{}",
        changes.patch
    );
    worktrees.apply(&root, wt, None).unwrap();
    assert!(root.join("naïve café.txt").is_file());
}

#[test]
fn an_apply_error_says_what_git_said() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    let created = worktrees.create(&root, Location::UserData).unwrap();
    let wt = &created.worktree.path;
    std::fs::write(wt.join("new.txt"), "from the child\n").unwrap();
    // The same new file exists in the workspace: not a conflicting hunk.
    std::fs::write(root.join("new.txt"), "mine\n").unwrap();
    let error = worktrees.apply(&root, wt, None).unwrap_err();
    assert!(error.contains("already exists"), "{error}");
    assert_eq!(
        std::fs::read_to_string(root.join("new.txt")).unwrap(),
        "mine\n"
    );
}

#[test]
fn renames_binaries_deletions_and_crlf_round_trip() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    git(&root, &["config", "core.autocrlf", "true"]).unwrap();
    let binary: Vec<u8> = (0..=255u8).cycle().take(3000).collect();
    std::fs::write(root.join("crlf.txt"), "a\r\nb\r\n").unwrap();
    std::fs::write(root.join("bin.dat"), &binary).unwrap();
    std::fs::write(root.join("old-name.txt"), "1\n2\n3\n4\n5\n").unwrap();
    std::fs::write(root.join("gone.txt"), "gone\n").unwrap();
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-q", "-m", "files"]).unwrap();

    let created = worktrees.create(&root, Location::UserData).unwrap();
    let wt = &created.worktree.path;
    let changed: Vec<u8> = binary.iter().rev().copied().collect();
    std::fs::write(wt.join("crlf.txt"), "a\r\nb\r\nc\r\n").unwrap();
    std::fs::write(wt.join("bin.dat"), &changed).unwrap();
    std::fs::rename(wt.join("old-name.txt"), wt.join("new-name.txt")).unwrap();
    std::fs::remove_file(wt.join("gone.txt")).unwrap();
    std::fs::create_dir_all(wt.join("assets/img")).unwrap();
    std::fs::write(wt.join("assets/img/new.bin"), &binary).unwrap();

    let changes = worktrees.changes(&root, wt).unwrap();
    let binary_file = changes.files.iter().find(|f| f.path == "bin.dat").unwrap();
    assert_eq!((binary_file.added, binary_file.removed), (None, None));

    assert!(
        worktrees
            .apply(&root, wt, Some(&changes.hash))
            .unwrap()
            .cleaned
    );
    assert_eq!(
        std::fs::read(root.join("crlf.txt")).unwrap(),
        b"a\r\nb\r\nc\r\n"
    );
    assert_eq!(std::fs::read(root.join("bin.dat")).unwrap(), changed);
    assert_eq!(
        std::fs::read(root.join("assets/img/new.bin")).unwrap(),
        binary
    );
    assert!(!root.join("old-name.txt").exists() && !root.join("gone.txt").exists());
    assert!(root.join("new-name.txt").is_file());
}

#[test]
fn worktrees_are_created_side_by_side() {
    let Some((dir, root)) = repo() else { return };
    let worktrees = manager(&dir);
    let created: Vec<_> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| worktrees.create(&root, Location::UserData)))
            .collect();
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    for one in &created {
        let one = one.as_ref().unwrap();
        assert_eq!(
            recorded_base(&one.worktree.path).as_deref(),
            Some(one.base.as_str())
        );
    }
    assert_eq!(worktrees.list(&root).unwrap().len(), 8);
}
