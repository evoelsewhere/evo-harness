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

/// A step for `owner` in the test's agent folder, on a scripted model.
fn step<'a>(
    task: crate::learning::run::Task,
    owner: &str,
    args: &'a crate::args::Args,
    answers: Vec<&str>,
) -> crate::learning::run::Step<'a> {
    use evo_ai::providers::faux::{FauxProvider, FauxScript, FauxStep};
    let agent = crate::config::agent_dir().unwrap();
    let script = FauxScript::new();
    script.set_responses(answers.into_iter().map(FauxStep::text).collect());
    let provider = FauxProvider::new(script);
    let model = provider.default_model().clone();
    let provider: Arc<dyn evo_ai::provider::Provider> = provider;
    crate::learning::run::Step {
        task,
        owner: crate::learning::run::Owner::parse(owner, &agent).unwrap(),
        args,
        today: "2026-11-28".into(),
        model: Some((provider, model)),
    }
}

// FR-6: Tidy now archives, strikes the older of a contradicting pair, and
// journals each change.
#[tokio::test]
async fn tidy_now_archives_strikes_and_journals() {
    let w = world();
    let dir = crate::config::agent_dir()
        .unwrap()
        .join("mates")
        .join("m1")
        .join("memory");
    let body = "- 2026-10-01 · agent · Lives in Hanoi\n\
- 2026-10-02 · agent · Likes tea\n\
- 2026-10-03 · agent · Works at Acme\n\
- 2026-10-04 · agent · Drives a red car\n\
- 2026-10-05 · agent · Trip to Hue · until 2026-11-01\n\
- 2026-11-20 · agent · Lives in Da Nang\n\
- 2026-11-21 · agent · Allergic to shellfish";
    memory::save_in(
        &dir,
        "about",
        "About the person",
        "user",
        body,
        None,
        memory::Meta::default(),
    )
    .unwrap();
    let args = crate::args::Args::default();
    let tidy = step(
        crate::learning::run::Task::Tidy,
        "mate:m1",
        &args,
        vec![r#"{"contradictions":[{"older":1,"newer":5,"keep":""}]}"#],
    );
    let result = crate::learning::tidy::run(&tidy).await;
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(
        (result["archived"].as_u64(), result["struck"].as_u64()),
        (Some(1), Some(1))
    );
    let file = std::fs::read_to_string(dir.join("about.md")).unwrap();
    assert!(
        file.contains("- ~~2026-10-01 · agent · Lives in Hanoi~~ · superseded 2026-11-28"),
        "{file}"
    );
    assert!(!file.contains("Trip to Hue"));
    assert!(std::fs::read_to_string(dir.join("archive.md"))
        .unwrap()
        .contains("Trip to Hue"));
    assert!(
        memory::list_dir(&dir).iter().all(|m| m.name != "archive"),
        "the archive is not a topic"
    );
    let conn = crate::app_db::open().unwrap();
    let rows = crate::app_db::journal::list(&conn, &["mate:m1"], "memory", 10).unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| (r.by.as_str(), r.action.as_str()))
            .collect::<Vec<_>>(),
        [("tidy", "updated"), ("tidy", "archived")]
    );
    drop(w);
}

// FR-24: without a model, Tidy now still archives, and says the rest needs one.
#[tokio::test]
async fn tidy_without_a_model_archives_only() {
    let _w = world();
    let dir = crate::config::agent_dir().unwrap().join("memory");
    memory::save_in(
        &dir,
        "trips",
        "",
        "user",
        "- 2026-10-05 · agent · Trip to Hue · until 2026-11-01\nKeeps a travel diary.",
        None,
        memory::Meta::default(),
    )
    .unwrap();
    let args = crate::args::Args::default();
    let mut tidy = step(crate::learning::run::Task::Tidy, "personal", &args, vec![]);
    tidy.model = None;
    let result = crate::learning::tidy::run(&tidy).await;
    assert_eq!(result["skipped"], "contradictions need a model");
    assert_eq!(result["archived"], 1);
}

fn dream_chats() -> Vec<crate::learning::dream::DreamChat> {
    use crate::learning::dream::{select, RawChat, RawLine};
    let now = crate::learning::run::now_ms();
    let chat = |id: &str, text: &str, ago: i64| RawChat {
        id: id.into(),
        title: id.into(),
        lines: vec![RawLine {
            person: true,
            text: text.into(),
            at_ms: now - ago,
        }],
        by_day: false,
        channel: None,
    };
    select(
        vec![
            chat("c1", "Shorter please, this is too long.", 3_000),
            chat("c2", "Too long again, make it shorter.", 2_000),
            chat("c3", "Send me the numbers on Monday.", 1_000),
        ],
        0,
    )
}

// FR-13, FR-14: the dream's line is journaled by Dream; its idea waits for the person.
#[tokio::test]
async fn a_dream_learns_one_line_and_journals_it() {
    let _w = world();
    let args = crate::args::Args::default();
    let answer = r#"{"learned":[{"text":"The person wants shorter answers","from":[2,3],"aboutUser":true,
        "topic":"style","confidence":0.8,"evidence":["make it shorter","Shorter please"]}],
        "ideas":[{"text":"A Monday numbers report","from":[1,2],"kind":"schedule"}]}"#;
    let dream = step(
        crate::learning::run::Task::Dream,
        "mate:m1",
        &args,
        vec![answer],
    );
    let result = crate::learning::dream::dream_with(&dream, &dream_chats()).await;
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(result["changed"].as_array().unwrap().len(), 1);
    let conn = crate::app_db::open().unwrap();
    let rows = crate::app_db::journal::list(&conn, &["mate:m1"], "memory", 5).unwrap();
    assert_eq!(
        (
            rows[0].by.as_str(),
            rows[0].name.as_str(),
            rows[0].action.as_str()
        ),
        ("dream", "style", "added")
    );
    let ideas = crate::app_db::learning::pending(&conn, &["mate:m1"]).unwrap();
    assert_eq!(ideas.len(), 1);
    assert_eq!(ideas[0].from_chats, ["c3", "c2"]);
}

// NFR-5: a model that fails changes nothing.
#[tokio::test]
async fn a_failing_dream_changes_nothing() {
    use evo_ai::providers::faux::{faux_assistant_message, FauxProvider, FauxScript, FauxStep};
    let _w = world();
    let args = crate::args::Args::default();
    let mut dream = step(crate::learning::run::Task::Dream, "mate:m1", &args, vec![]);
    let mut failed = faux_assistant_message("", evo_ai::types::StopReason::Error);
    failed.error_message = Some("503: overloaded".into());
    let script = FauxScript::new();
    script.set_responses(vec![FauxStep::Message(failed)]);
    let provider = FauxProvider::new(script);
    let model = provider.default_model().clone();
    dream.model = Some((provider, model));
    let result = crate::learning::dream::dream_with(&dream, &dream_chats()).await;
    assert_eq!(result["error"], "503: overloaded");
    let dir = crate::config::agent_dir()
        .unwrap()
        .join("mates")
        .join("m1")
        .join("memory");
    assert!(memory::list_dir(&dir).is_empty());
    // An answer that is not JSON saves nothing either.
    let mut garbled = step(
        crate::learning::run::Task::Dream,
        "mate:m1",
        &args,
        vec!["I think the person likes tea."],
    );
    garbled.today = "2026-11-28".into();
    let result = crate::learning::dream::dream_with(&garbled, &dream_chats()).await;
    assert_eq!(result["error"], "the answer was not valid");
    assert!(memory::list_dir(&dir).is_empty());
}
