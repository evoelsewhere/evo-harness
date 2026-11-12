//! docs/features/session-roles-and-mail/spec.md (FR-1, FR-2, FR-4, NFR-3).
use super::*;
use evo_ai::types::Api;
use serde_json::json;

fn model(provider: &str, id: &str, reasoning: bool) -> Model {
    let mut model = Model::new(id, id, Api::OpenaiCompletions, provider, "");
    model.reasoning = reasoning;
    model
}

fn reviewer(cwd: &Path) -> AgentType {
    AgentType {
        name: "reviewer".into(),
        description: "Reviews".into(),
        tools: Some(vec!["read".into(), "bash".into()]),
        model: Some("acme-small".into()),
        worktree: false,
        read_only: true,
        thinking: Some(ThinkingLevel::High),
        max_turns: Some(5),
        prompt: "Review only.\n".into(),
        source: Some(cwd.join(".evo").join("agents").join("reviewer.md")),
        origin: "project",
    }
}

fn writer() -> AgentType {
    AgentType {
        name: "plugin:writer".into(),
        description: "Writes".into(),
        tools: None,
        model: Some("inherit".into()),
        worktree: false,
        read_only: false,
        thinking: None,
        max_turns: None,
        prompt: "Write docs.".into(),
        source: Some(PathBuf::from("/plugins/w/agents/writer.md")),
        origin: "plugin",
    }
}

fn parent(cwd: &Path) -> Parent {
    Parent {
        cwd: cwd.to_path_buf(),
        models: vec![
            model("acme", "acme-large", true),
            model("acme", "acme-small", true),
            model("other", "plain", false),
        ],
        model: Some(model("acme", "acme-large", true)),
        thinking: Some(ThinkingLevel::Medium),
        read_only: false,
    }
}

fn role_of(args: &Value) -> SessionRole {
    serde_json::from_value(args["role"].clone()).unwrap()
}

#[test]
fn session_roles_without_role_arguments_leave_the_call_as_it_was() {
    let cwd = PathBuf::from("/work");
    let args = json!({"title": "T", "prompt": "P", "role": {"access": "full", "model": "forged"}});
    let out = enrich(args, &parent(&cwd), &[reviewer(&cwd)]).unwrap();
    assert_eq!(
        out,
        json!({"title": "T", "prompt": "P"}),
        "a model-written role is dropped"
    );
}

#[test]
fn session_roles_resolve_the_type_its_model_and_its_thinking() {
    let cwd = PathBuf::from("/work");
    let args =
        json!({"title": "T", "prompt": "P", "agentType": "reviewer", "role": {"access": "full"}});
    let out = enrich(args, &parent(&cwd), &[reviewer(&cwd), writer()]).unwrap();
    assert!(
        out.get("agentType").is_none(),
        "the resolved call carries the role only"
    );
    let role = role_of(&out);
    assert_eq!(role.name.as_deref(), Some("reviewer"));
    assert_eq!(role.source.as_deref(), Some("project"));
    assert_eq!(role.source_path.as_deref(), Some(".evo/agents/reviewer.md"));
    assert_eq!(role.instructions.as_deref(), Some("Review only."));
    assert_eq!(
        role.tools,
        Some(vec!["read".to_string(), "bash".to_string()])
    );
    assert!(role.is_read_only());
    assert_eq!(
        (role.provider.as_deref(), role.model.as_deref()),
        (Some("acme"), Some("acme-small"))
    );
    assert_eq!(role.thinking.as_deref(), Some("high"));
}

#[test]
fn session_roles_take_the_argument_then_the_type_then_the_parent() {
    let cwd = PathBuf::from("/work");
    let types = [reviewer(&cwd), writer()];
    // The call's model and thinking win over the type's.
    let out = enrich(
        json!({"agentType": "reviewer", "model": "acme/acme-large", "thinking": "low"}),
        &parent(&cwd),
        &types,
    )
    .unwrap();
    let role = role_of(&out);
    assert_eq!(
        (role.model.as_deref(), role.thinking.as_deref()),
        (Some("acme-large"), Some("low"))
    );
    // A type that inherits takes the parent's model and thinking.
    let role =
        role_of(&enrich(json!({"agentType": "plugin:writer"}), &parent(&cwd), &types).unwrap());
    assert_eq!(
        (role.model.as_deref(), role.thinking.as_deref()),
        (Some("acme-large"), Some("medium"))
    );
    assert_eq!(role.source.as_deref(), Some("plugin"));
    assert_eq!(role.access, "full");
    assert_eq!(role.tools, None, "no tool list: the parent's tools");
    // A model alone: no type, the parent's thinking.
    let role = role_of(&enrich(json!({"model": "ACME-SMALL"}), &parent(&cwd), &types).unwrap());
    assert_eq!(
        (role.name, role.model.as_deref()),
        (None, Some("acme-small"))
    );
}

#[test]
fn session_roles_clamp_thinking_to_the_model() {
    let cwd = PathBuf::from("/work");
    let role = role_of(
        &enrich(
            json!({"model": "plain", "thinking": "high"}),
            &parent(&cwd),
            &[],
        )
        .unwrap(),
    );
    assert_eq!(
        role.thinking.as_deref(),
        Some("off"),
        "a model without reasoning does not think"
    );
    let mut capped = parent(&cwd);
    capped.models[1].thinking_level_map = Some([(ThinkingLevel::High, None)].into_iter().collect());
    let role = role_of(
        &enrich(
            json!({"model": "acme-small", "thinking": "high"}),
            &capped,
            &[],
        )
        .unwrap(),
    );
    assert_eq!(
        role.thinking.as_deref(),
        Some("medium"),
        "the nearest lower level"
    );
}

#[test]
fn session_roles_refuse_unknown_types_models_and_levels() {
    let cwd = PathBuf::from("/work");
    let types = [reviewer(&cwd)];
    let error = enrich(json!({"agentType": "nobody"}), &parent(&cwd), &types).unwrap_err();
    assert!(
        error.contains("unknown agent type `nobody`") && error.contains("reviewer"),
        "{error}"
    );
    let error = enrich(json!({"model": "elsewhere/x"}), &parent(&cwd), &types).unwrap_err();
    assert!(
        error.contains("not available") && error.contains("acme/acme-small"),
        "{error}"
    );
    let error = enrich(json!({"thinking": "loud"}), &parent(&cwd), &types).unwrap_err();
    assert!(error.contains("not a thinking level"), "{error}");
    let error = enrich(json!({"model": 3}), &parent(&cwd), &types).unwrap_err();
    assert!(error.contains("must be a string"), "{error}");
}

#[test]
fn session_roles_of_a_read_only_parent_are_read_only() {
    let cwd = PathBuf::from("/work");
    let mut locked = parent(&cwd);
    locked.read_only = true;
    let role =
        role_of(&enrich(json!({"agentType": "plugin:writer"}), &locked, &[writer()]).unwrap());
    assert!(
        role.is_read_only(),
        "access: full cannot lift a read-only parent"
    );
}

#[test]
fn role_file_round_trips_and_narrows_the_tools() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.role.json");
    let role = SessionRole {
        name: Some("reviewer".into()),
        instructions: Some("Review only.".into()),
        tools: Some(vec!["read".into(), "send_message".into()]),
        access: "read-only".into(),
        ..Default::default()
    };
    std::fs::write(&path, serde_json::to_string(&role).unwrap()).unwrap();
    assert_eq!(load_file(&path).unwrap(), role);
    std::fs::write(&path, "{not json").unwrap();
    assert!(
        load_file(&path).is_err(),
        "a damaged role file stops the start"
    );
    assert!(load_file(&dir.path().join("missing.json")).is_err());

    let mut args = crate::args::Args::default();
    narrow_tools(&role, &mut args);
    let tools = args.tools.clone().unwrap();
    assert_eq!(
        &tools[..2],
        &["read".to_string(), "send_message".to_string()]
    );
    for name in crate::tools::mesh::TOOL_NAMES {
        assert_eq!(
            tools.iter().filter(|t| *t == name).count(),
            1,
            "{name} once"
        );
    }
    // An explicit --tools list wins; a role without a list keeps every tool.
    let mut explicit = crate::args::Args {
        tools: Some(vec!["bash".into()]),
        ..Default::default()
    };
    narrow_tools(&role, &mut explicit);
    assert_eq!(explicit.tools, Some(vec!["bash".to_string()]));
    let mut open = crate::args::Args::default();
    narrow_tools(&SessionRole::default(), &mut open);
    assert_eq!(open.tools, None);
}

#[test]
fn role_file_instructions_make_the_prompt_block() {
    let role = SessionRole {
        name: Some("reviewer".into()),
        instructions: Some("  Review only.  ".into()),
        access: "read-only".into(),
        ..Default::default()
    };
    let block = block_for(&role).unwrap();
    assert!(block.starts_with("<role name=\"reviewer\">\n"), "{block}");
    assert!(block.contains("You are read-only"), "{block}");
    assert!(block.ends_with("\nReview only.\n</role>"), "{block}");
    let full = SessionRole {
        access: "full".into(),
        ..role.clone()
    };
    assert!(!block_for(&full).unwrap().contains("read-only"));
    assert_eq!(
        block_for(&SessionRole::default()),
        None,
        "no instructions, no block"
    );
}

// docs/features/session-roles-and-mail/spec.md (FR-13).
#[test]
fn read_only_session_refuses_writes_lsp_and_unknown_tools() {
    let args = json!({"command": "echo x > a.txt"});
    let refusal = read_only_refusal("bash", &args, false).unwrap();
    assert!(refusal.contains("this session is read-only"), "{refusal}");
    assert_eq!(
        read_only_refusal("bash", &json!({"command": "git log -1"}), false),
        None
    );
    assert!(read_only_refusal("write", &json!({}), false).is_some());
    assert!(read_only_refusal("edit", &json!({}), false).is_some());
    assert!(
        read_only_refusal("lsp", &json!({}), false).is_some(),
        "no language servers"
    );
    assert!(read_only_refusal("lsp", &json!({}), true).is_some());
    assert_eq!(
        read_only_refusal("read", &json!({"path": "a.txt"}), false),
        None
    );
    // An extension's tool is unknown to the checker; an app tool is the app's call.
    assert!(read_only_refusal("plugin_tool", &json!({}), false).is_some());
    assert_eq!(read_only_refusal("browser_read", &json!({}), true), None);
}

#[test]
fn read_only_session_role_narrows_app_tools() {
    let role = SessionRole {
        tools: Some(vec!["read".into(), "browser_read".into()]),
        ..Default::default()
    };
    assert!(allows(Some(&role), "browser_read"));
    assert!(!allows(Some(&role), "terminal_write"));
    assert!(
        allows(Some(&SessionRole::default()), "terminal_write"),
        "no list: every tool"
    );
    assert!(allows(None, "terminal_write"));
    let mut args = crate::args::Args::default();
    let read_only = SessionRole {
        access: "read-only".into(),
        ..Default::default()
    };
    // `apply` would set the process-wide role; check what it adds to the args.
    narrow_tools(&read_only, &mut args);
    assert_eq!(args.tools, None);
}
