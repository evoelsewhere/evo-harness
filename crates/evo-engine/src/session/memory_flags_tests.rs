//! A chat started without memory, and the app's tools across a reload
//! (docs/features/memory-topics-and-chat-recall/spec.md, FR-6, FR-10, FR-20).

use super::*;

struct RestoreConfig(Option<std::ffi::OsString>);

impl Drop for RestoreConfig {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var(crate::config::CONFIG_DIR_ENV, value),
            None => std::env::remove_var(crate::config::CONFIG_DIR_ENV),
        }
    }
}

async fn tool_names(harness: &AgentHarness) -> Vec<String> {
    harness
        .get_tools()
        .await
        .unwrap()
        .iter()
        .map(|tool| tool.tool.schema().name.clone())
        .collect()
}

async fn prompt(harness: &AgentHarness) -> String {
    harness
        .get_system_prompt()
        .await
        .unwrap()
        .unwrap_or_default()
}

fn args(no_memory: bool) -> Args {
    Args {
        no_extensions: true,
        no_skills: true,
        no_prompt_templates: true,
        no_context_files: true,
        system_prompt: Some("test".into()),
        no_memory,
        ..Default::default()
    }
}

#[tokio::test]
async fn no_memory_flag_drops_the_prompt_section_and_the_tool() {
    let temporary = tempfile::tempdir().unwrap();
    let sessions = temporary.path().join("sessions");
    // Holds the shared environment lock for the whole test.
    let _env = crate::config::test_support::SessionDirEnv::new(Some(&sessions));
    let _config = RestoreConfig(std::env::var_os(crate::config::CONFIG_DIR_ENV));
    let agent = temporary.path().join("agent");
    std::fs::create_dir_all(&agent).unwrap();
    std::env::set_var(crate::config::CONFIG_DIR_ENV, &agent);
    std::fs::write(agent.join("settings.json"), "{}").unwrap();
    crate::memory::save_in(
        &agent.join("memory"),
        "prefers-tabs",
        "Uses tabs",
        "user",
        "Tabs, width 4.",
        None,
        crate::memory::Meta::default(),
    )
    .unwrap();
    let cwd = temporary.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    let resolved = crate::provider::resolve(
        Some("anthropic"),
        Some(crate::provider::DEFAULT_MODEL_ID),
        None,
        Some("test-key"),
        None,
    )
    .unwrap();

    // A chat with memory: the section, the topic's line and the tool.
    let usual = args(false);
    let prepared = prepare_extensions(&usual, &cwd, false).unwrap();
    let (harness, _events, context) = build_prepared(&resolved, &usual, &cwd, false, prepared)
        .await
        .unwrap();
    for _ in 0..2 {
        let text = prompt(&harness).await;
        assert!(text.contains("# Memory"), "{text}");
        assert!(text.contains("- prefers-tabs (user): Uses tabs"), "{text}");
        assert!(tool_names(&harness).await.contains(&"memory".to_string()));
        assert!(
            !reload_extension_resources(&harness, &context)
                .await
                .had_errors
        );
    }

    // A chat started without it: neither, at build and after a reload.
    let quiet = args(true);
    let prepared = prepare_extensions(&quiet, &cwd, false).unwrap();
    let (harness, _events, context) = build_prepared(&resolved, &quiet, &cwd, false, prepared)
        .await
        .unwrap();
    for _ in 0..2 {
        let text = prompt(&harness).await;
        assert!(!text.contains("# Memory"), "{text}");
        assert!(!text.contains("prefers-tabs"), "{text}");
        let names = tool_names(&harness).await;
        assert!(!names.contains(&"memory".to_string()), "{names:?}");
        assert!(names.contains(&"read".to_string()), "other tools stay");
        assert!(
            !reload_extension_resources(&harness, &context)
                .await
                .had_errors
        );
    }
}

#[test]
fn incognito_implies_no_memory() {
    let parsed = crate::args::parse_args(&["--incognito".to_string()]);
    assert!(parsed.incognito && parsed.no_memory);
    let parsed = crate::args::parse_args(&["--no-memory".to_string()]);
    assert!(parsed.no_memory && !parsed.incognito);
    let parsed = crate::args::parse_args(&[]);
    assert!(!parsed.no_memory && !parsed.incognito);
}

#[tokio::test]
async fn external_tools_survive_reload() {
    let temporary = tempfile::tempdir().unwrap();
    let sessions = temporary.path().join("sessions");
    let _env = crate::config::test_support::SessionDirEnv::new(Some(&sessions));
    let _config = RestoreConfig(std::env::var_os(crate::config::CONFIG_DIR_ENV));
    let agent = temporary.path().join("agent");
    std::fs::create_dir_all(&agent).unwrap();
    std::env::set_var(crate::config::CONFIG_DIR_ENV, &agent);
    std::fs::write(agent.join("settings.json"), "{}").unwrap();
    let cwd = temporary.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    let resolved = crate::provider::resolve(
        Some("anthropic"),
        Some(crate::provider::DEFAULT_MODEL_ID),
        None,
        Some("test-key"),
        None,
    )
    .unwrap();
    let usual = args(false);
    let prepared = prepare_extensions(&usual, &cwd, false).unwrap();
    let (harness, _events, context) = build_prepared(&resolved, &usual, &cwd, false, prepared)
        .await
        .unwrap();

    // What `configure_external_tools` does: the app's tools join the registry
    // and the active set, and the approval state knows their names.
    let (writer, _writer_task, _) =
        crate::rpc::writer::start_rpc_writer(std::io::sink(), Default::default());
    let backend = crate::rpc::external_tools::ExternalTools::detached(writer.clone());
    let external = |name: &str| {
        let definition = serde_json::from_value(serde_json::json!({
            "name": name,
            "description": "Runs in the app",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }))
        .unwrap();
        HarnessTool::new(evo_tools::tools::external::create_external_tool(
            definition,
            backend.clone(),
        ))
    };
    let mut registry = harness.get_tools().await.unwrap();
    let mut active = harness.lane("main").get_active_tools().await.unwrap();
    registry.push(external("session_search"));
    registry.push(external("app_idle_tool"));
    active.push("session_search".into());
    harness.set_tools(registry, Some(active)).await.unwrap();
    crate::approval::global()
        .set_external_tools(["session_search".to_string(), "app_idle_tool".to_string()]);

    assert!(
        !reload_extension_resources(&harness, &context)
            .await
            .had_errors
    );

    let names = tool_names(&harness).await;
    assert!(
        names.contains(&"session_search".to_string())
            && names.contains(&"app_idle_tool".to_string()),
        "a reload dropped the app's tools: {names:?}"
    );
    let active = harness.lane("main").get_active_tools().await.unwrap();
    assert!(active.contains(&"session_search".to_string()));
    assert!(
        !active.contains(&"app_idle_tool".to_string()),
        "a tool that was not active stays inactive"
    );
    assert!(names.contains(&"memory".to_string()), "built-in tools stay");
    assert_eq!(
        names
            .iter()
            .filter(|name| *name == "session_search")
            .count(),
        1
    );

    crate::approval::global().set_external_tools(Vec::<String>::new());
    backend.shutdown();
}
