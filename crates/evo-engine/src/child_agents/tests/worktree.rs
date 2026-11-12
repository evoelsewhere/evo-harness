//! `"isolation": "worktree"`: the child's tools work in its own worktree; a
//! changed worktree is kept and reported, an unchanged one is removed.

use super::*;
use evo_ai::providers::faux::{FauxProvider, FauxScript};
use evo_harness::types::AgentHarnessOptions;

fn git(dir: &Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// A one-commit repository; `None` without git.
fn repo() -> Option<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    if !git(&root, &["init", "-q"]) {
        eprintln!("skipping: git is not available");
        return None;
    }
    std::fs::write(root.join("README.md"), "hello\n").unwrap();
    for args in [
        &["config", "user.email", "evo@example.invalid"][..],
        &["config", "user.name", "evo"],
        &["add", "."],
        &["commit", "-q", "-m", "init"],
    ] {
        assert!(git(&root, args));
    }
    Some((dir, root))
}

/// A parent whose built-in tools work in `cwd`, driven by `script`.
async fn parent_in(cwd: &Path, script: FauxScript) -> AgentHarness {
    let provider = FauxProvider::new(script);
    let tools = crate::tools::builtin_tools_in(cwd);
    let active_tool_names = tools
        .iter()
        .map(|tool| tool.tool.schema().name.clone())
        .collect();
    let session = Session::new(
        Arc::new(InMemorySessionStorage::new(
            SessionMetadata {
                id: "parent".into(),
                created_at: 0,
                parent_session_id: None,
            },
            Arc::new(SystemClock),
            Arc::new(DefaultIdGenerator::new()),
        )),
        None,
    );
    AgentHarness::create(AgentHarnessOptions {
        model: provider.default_model().clone(),
        models: vec![provider],
        tools,
        active_tool_names,
        session,
        ..Default::default()
    })
    .await
    .unwrap()
}

async fn finish(manager: &ChildAgents, parent: &AgentHarness, id: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let value = manager
                .handle(parent, 7, Path::new("."), json!({"op":"poll","childId":id}))
                .await
                .unwrap();
            if value["status"] != "running" {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child must finish")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_child_writes_in_its_own_worktree_which_is_kept() {
    // Managed worktrees go to the user data directory: use a private one.
    let _agent = crate::config::test_support::AgentDirEnv::new();
    let Some((_dir, root)) = repo() else { return };
    let script = FauxScript::new()
        .with_tool_call(
            "write",
            json!({"path":"from-child.txt","content":"made by the child\n"}),
        )
        .with_text("wrote the file");
    let parent = parent_in(&root, script).await;
    let manager = ChildAgents::default();
    let started = manager
        .handle(
            &parent,
            7,
            &root,
            json!({"op":"start","task":"write a file","isolation":"worktree"}),
        )
        .await
        .unwrap();
    let path = PathBuf::from(started["worktree"]["path"].as_str().unwrap());
    assert!(
        path.starts_with(_agent.path()),
        "worktrees go to the user data directory: {started}"
    );
    assert!(started["worktree"]["branch"]
        .as_str()
        .unwrap()
        .starts_with(crate::worktrees::BRANCH_PREFIX));

    let done = finish(&manager, &parent, started["childId"].as_str().unwrap()).await;
    assert_eq!(done["status"], "done", "{done}");
    let worktree = &done["result"]["worktree"];
    assert_eq!(
        (worktree["changed"].as_bool(), worktree["kept"].as_bool()),
        (Some(true), Some(true)),
        "{done}"
    );
    // The file is in the worktree, not in the parent's checkout.
    assert_eq!(
        std::fs::read_to_string(path.join("from-child.txt")).unwrap(),
        "made by the child\n"
    );
    assert!(!root.join("from-child.txt").exists());
    // Listed until removed.
    let worktrees = crate::worktrees::manager().unwrap();
    assert_eq!(worktrees.list(&root).unwrap().len(), 1);
    worktrees.remove(&root, &path).unwrap();
    assert!(!path.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unchanged_worktree_is_removed_when_the_child_finishes() {
    // Managed worktrees go to the user data directory: use a private one.
    let _agent = crate::config::test_support::AgentDirEnv::new();
    let Some((_dir, root)) = repo() else { return };
    let parent = parent_in(&root, FauxScript::new().with_text("nothing to change")).await;
    let manager = ChildAgents::default();
    let started = manager
        .handle(
            &parent,
            7,
            &root,
            json!({"op":"start","task":"look around","isolation":"worktree"}),
        )
        .await
        .unwrap();
    let path = PathBuf::from(started["worktree"]["path"].as_str().unwrap());
    assert!(path.exists());
    let done = finish(&manager, &parent, started["childId"].as_str().unwrap()).await;
    assert_eq!(done["result"]["worktree"]["kept"], json!(false), "{done}");
    assert!(!path.exists());
    assert!(crate::worktrees::manager()
        .unwrap()
        .list(&root)
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worktree_isolation_needs_a_repository_and_a_known_mode() {
    let dir = tempfile::tempdir().unwrap();
    let parent = parent_in(dir.path(), FauxScript::new().with_text("x")).await;
    let manager = ChildAgents::default();
    let error = manager
        .handle(
            &parent,
            7,
            dir.path(),
            json!({"op":"start","task":"t","isolation":"worktree"}),
        )
        .await
        .unwrap_err();
    assert!(error.contains("could not create a worktree"), "{error}");
    assert!(
        manager.jobs.lock().unwrap().is_empty(),
        "a refused start leaves no job"
    );
    let error = manager
        .handle(
            &parent,
            7,
            dir.path(),
            json!({"op":"start","task":"t","isolation":"container"}),
        )
        .await
        .unwrap_err();
    assert!(error.contains("unknown isolation"), "{error}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancelled_child_still_reports_its_kept_worktree() {
    let _agent = crate::config::test_support::AgentDirEnv::new();
    let Some((_dir, root)) = repo() else { return };
    // Writes a file, then talks for longer than the test waits.
    let script = FauxScript::new()
        .with_tool_call(
            "write",
            json!({"path":"from-child.txt","content":"made by the child\n"}),
        )
        .with_text("word ".repeat(20_000))
        .with_tokens_per_second(200.0);
    let parent = parent_in(&root, script).await;
    let manager = ChildAgents::default();
    let started = manager
        .handle(
            &parent,
            7,
            &root,
            json!({"op":"start","task":"write a file","isolation":"worktree"}),
        )
        .await
        .unwrap();
    let id = started["childId"].as_str().unwrap();
    let path = PathBuf::from(started["worktree"]["path"].as_str().unwrap());
    tokio::time::timeout(Duration::from_secs(20), async {
        while !path.join("from-child.txt").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the child writes its file");
    manager
        .handle(&parent, 7, &root, json!({"op":"cancel","childId":id}))
        .await
        .unwrap();

    let done = finish(&manager, &parent, id).await;
    assert_eq!(done["status"], "failed", "{done}");
    assert!(done["error"].as_str().unwrap().contains("cancel"), "{done}");
    // FR-13: a kept worktree's path and branch are reported, also on failure.
    let worktree = &done["worktree"];
    assert_eq!(
        (worktree["changed"].as_bool(), worktree["kept"].as_bool()),
        (Some(true), Some(true)),
        "{done}"
    );
    assert_eq!(worktree["path"], started["worktree"]["path"]);
    assert_eq!(worktree["branch"], started["worktree"]["branch"]);
    // The mesh report carries it too.
    let job = manager.jobs.lock().unwrap().get(id).cloned().unwrap();
    let report = ended(&job);
    assert_eq!(report.worktree.as_ref(), Some(&done["worktree"]));
    assert!(
        report.text.contains(path.to_string_lossy().as_ref())
            && report
                .text
                .contains(started["worktree"]["branch"].as_str().unwrap()),
        "{}",
        report.text
    );
    assert!(path.join("from-child.txt").is_file());
    crate::worktrees::manager()
        .unwrap()
        .remove(&root, &path)
        .unwrap();
}

// docs/features/session-roles-and-mail/spec.md (FR-9).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn named_worktree_is_on_its_evo_branch_and_a_taken_name_is_refused() {
    let Some((_dir, root)) = repo() else { return };
    let created = create_worktree(&root, Some("api-auth")).await.unwrap();
    assert_eq!(created.worktree.branch, "evo/api-auth");
    assert_eq!(worktree_json(&created, None)["branch"], "evo/api-auth");
    let error = create_worktree(&root, Some("api-auth")).await.unwrap_err();
    assert!(error.contains("already exists"), "{error}");
    let plain = create_worktree(&root, None).await.unwrap();
    assert!(plain.worktree.branch.starts_with("evo/worktree-"));
    // Removed under the root they were made in (other tests may point the
    // agent dir elsewhere meanwhile): `<data root>/<repo>-<hash>/<name>`.
    for made in [created, plain] {
        let root_dir = made.worktree.path.parent().and_then(Path::parent).unwrap();
        crate::worktrees::Worktrees::with_data_root(root_dir.to_path_buf())
            .remove(&root, &made.worktree.path)
            .unwrap();
    }
}
