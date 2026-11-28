//! Memory writes through the journal, `until`, and the panel's conflict check
//! (docs/features/learning-layers/spec.md, FR-1 – FR-5, FR-9).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::config::test_support::AgentDirEnv;
use crate::memory::{self, Saved, Scope};

struct World {
    _env: AgentDirEnv,
    cwd: PathBuf,
}

fn world() -> World {
    let env = AgentDirEnv::new();
    std::fs::write(env.path().join("settings.json"), "{}").unwrap();
    let cwd = env.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    World { _env: env, cwd }
}

async fn agent(cwd: &Path, params: Value) -> Result<String, String> {
    let tool = crate::tools::memory::create_memory_tool(cwd.to_path_buf());
    tool.execute("t1", params, CancellationToken::new(), Arc::new(|_| {}))
        .await
        .map(|result| format!("{:?}", result.content))
        .map_err(|error| error.to_string())
}

fn journal(cwd: &Path) -> Vec<Value> {
    memory::changes(cwd, 50).unwrap()["changes"]
        .as_array()
        .unwrap()
        .clone()
}

fn topic_file(cwd: &Path, name: &str) -> String {
    let dir = memory::dir(Scope::Project, cwd).unwrap();
    std::fs::read_to_string(dir.join(format!("{name}.md"))).unwrap_or_default()
}

// FR-1, FR-2, FR-9: the agent, indexing, the person and an undo each leave a row.
#[tokio::test]
async fn every_memory_write_is_journaled() {
    let w = world();
    agent(
        &w.cwd,
        json!({ "action": "save", "name": "style", "type": "feedback",
                "content": "Small commits.\nmy key is sk-abcdef0123456789abcdef" }),
    )
    .await
    .unwrap();
    assert!(
        !topic_file(&w.cwd, "style").contains("sk-abcdef"),
        "secrets never reach disk"
    );
    assert!(topic_file(&w.cwd, "style").contains("«redacted "));

    let before = Some(topic_file(&w.cwd, "style"));
    agent(
        &w.cwd,
        json!({ "action": "update", "name": "style", "new_text": "No force pushes." }),
    )
    .await
    .unwrap();
    let after = Some(topic_file(&w.cwd, "style"));
    memory::record_change(Scope::Project, &w.cwd, "style", None, before, after).unwrap();

    let Saved::Done(_) = memory::save(
        &w.cwd,
        Scope::Project,
        "style",
        "Style",
        "feedback",
        "Small commits.\nRebase first.",
        None,
        None,
    )
    .unwrap() else {
        panic!("saved")
    };
    let rows = journal(&w.cwd);
    let authors: Vec<&str> = rows.iter().map(|r| r["by"].as_str().unwrap()).collect();
    assert_eq!(
        authors,
        ["person", "index", "agent", "agent"],
        "newest first"
    );
    assert_eq!(rows[3]["action"], "added");
    assert!(rows
        .iter()
        .all(|r| r["owner"] == format!("project:{}", memory::slug(&w.cwd))));

    // Undo the agent's update: the text before it is back, and the undo is a row.
    let agent_update = rows[2]["id"].as_i64().unwrap();
    // The person changed the topic since: refused.
    assert!(memory::undo_change(&w.cwd, agent_update)
        .unwrap_err()
        .contains("changed since"));
    let person = rows[0]["id"].as_i64().unwrap();
    memory::undo_change(&w.cwd, person).unwrap();
    assert!(topic_file(&w.cwd, "style").contains("No force pushes."));
    let rows = journal(&w.cwd);
    assert_eq!(
        (rows[0]["by"].as_str(), rows[0]["action"].as_str()),
        (Some("undo"), Some("undone"))
    );
    assert_eq!(rows[1]["undone"], true);
    assert!(memory::undo_change(&w.cwd, person)
        .unwrap_err()
        .contains("before"));

    // A person's delete is a row too.
    memory::delete(&w.cwd, Scope::Project, "style").unwrap();
    assert_eq!(journal(&w.cwd)[0]["action"], "deleted");
}

// FR-2: a row whose earlier text held a secret cannot be undone.
#[tokio::test]
async fn a_change_from_a_secret_cannot_be_undone() {
    let w = world();
    let id = memory::journal_row(
        "personal",
        "old",
        "index",
        None,
        Some("token=abc123def456ghi".into()),
        Some("clean".into()),
    )
    .unwrap()
    .unwrap();
    let row = &journal(&w.cwd)[0];
    assert_eq!(
        (row["id"].as_i64(), row["undoable"].as_bool()),
        (Some(id), Some(false))
    );
    assert!(memory::undo_change(&w.cwd, id)
        .unwrap_err()
        .contains("cannot be undone"));
}

// FR-4, FR-5: an entry that ended, or was struck, is not read.
#[tokio::test]
async fn an_expired_entry_is_not_read() {
    let w = world();
    agent(
        &w.cwd,
        json!({ "action": "save", "name": "travel", "type": "user",
                "content": "Likes window seats." }),
    )
    .await
    .unwrap();
    agent(
        &w.cwd,
        json!({ "action": "update", "name": "travel", "new_text": "The person is in Hanoi",
                "until": "2000-01-01" }),
    )
    .await
    .unwrap();
    agent(
        &w.cwd,
        json!({ "action": "update", "name": "travel", "new_text": "The person is in Da Nang",
                "until": "2999-01-01" }),
    )
    .await
    .unwrap();
    let file = topic_file(&w.cwd, "travel");
    assert!(
        file.contains("The person is in Hanoi · until 2000-01-01"),
        "{file}"
    );
    // A struck line, as the tidy-up leaves it.
    let dir = memory::dir(Scope::Project, &w.cwd).unwrap();
    std::fs::write(
        dir.join("travel.md"),
        file.replacen(
            "\n\nLikes window seats.",
            "\n\n- ~~2026-01-01 · Likes aisle seats~~ · superseded 2026-02-01\nLikes window seats.",
            1,
        ),
    )
    .unwrap();
    let read = agent(&w.cwd, json!({ "action": "read", "name": "travel" }))
        .await
        .unwrap();
    assert!(!read.contains("Hanoi"), "{read}");
    assert!(!read.contains("aisle"), "{read}");
    assert!(
        read.contains("Da Nang") && read.contains("window seats"),
        "{read}"
    );
    assert!(agent(
        &w.cwd,
        json!({ "action": "update", "name": "travel", "new_text": "x", "until": "soon" })
    )
    .await
    .unwrap_err()
    .contains("YYYY-MM-DD"));
}

// FR-3: a save from the panel is refused when the topic changed since it opened.
#[tokio::test]
async fn a_stale_panel_save_is_refused() {
    let w = world();
    agent(
        &w.cwd,
        json!({ "action": "save", "name": "plans", "type": "project", "content": "Ship in May." }),
    )
    .await
    .unwrap();
    let opened = memory::describe(&w.cwd, true).unwrap()["project"]["items"][0]["hash"]
        .as_str()
        .unwrap()
        .to_string();
    // The agent saves while the person edits.
    agent(
        &w.cwd,
        json!({ "action": "update", "name": "plans", "new_text": "Beta in April." }),
    )
    .await
    .unwrap();
    let mine = "Ship in June.";
    let refused = memory::save(
        &w.cwd,
        Scope::Project,
        "plans",
        "",
        "project",
        mine,
        None,
        Some(&opened),
    )
    .unwrap();
    let Saved::Conflict(current) = refused else {
        panic!("refused")
    };
    assert!(current["content"]
        .as_str()
        .unwrap()
        .contains("Beta in April."));
    assert!(
        !topic_file(&w.cwd, "plans").contains(mine),
        "nothing was written"
    );
    // With the new hash, or Overwrite with mine, it saves.
    let fresh = current["hash"].as_str().unwrap();
    assert!(matches!(
        memory::save(
            &w.cwd,
            Scope::Project,
            "plans",
            "",
            "project",
            mine,
            None,
            Some(fresh)
        )
        .unwrap(),
        Saved::Done(_)
    ));
    assert!(matches!(
        memory::save(
            &w.cwd,
            Scope::Project,
            "plans",
            "",
            "project",
            "Ship in July.",
            None,
            None
        )
        .unwrap(),
        Saved::Done(_)
    ));
    assert!(topic_file(&w.cwd, "plans").contains("Ship in July."));
}
