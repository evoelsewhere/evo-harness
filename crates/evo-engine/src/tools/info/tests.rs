use super::*;
use std::collections::BTreeSet;

/// Every tool the engine registers in `session.rs` (`build_tools`), by name.
/// `powershell` only on Windows.
const REGISTERED: &[&str] = &[
    "read",
    "bash",
    "edit",
    "write",
    "docs",
    "agent",
    "message_agent",
    "send_message",
    "ask",
    "reply",
    "list_nodes",
    "cancel_node",
    "skill",
    "memory",
    "ask_user",
    "update_todos",
    "visualize_read_me",
    "show_widget",
    "document_preview",
    "lsp",
    "web_search",
    "web_fetch",
    "powershell",
];

/// Rows for names the engine does not register as a tool today: library tools
/// that `evo-tools` offers but the session leaves out (`grep`, `find`, `ls`,
/// `image`), tools the app answers (`send_file`, `notify`), and names kept
/// reserved for security / native features (`permissions`, `permission`,
/// `plan_mode_start`, `plan_mode_complete`).
const NOT_REGISTERED: &[&str] = &[
    "grep",
    "find",
    "ls",
    "image",
    "send_file",
    "notify",
    "permissions",
    "permission",
    "plan_mode_start",
    "plan_mode_complete",
];

/// `approval.rs` `READ_TOOLS` before the table existed.
const OLD_READ_TOOLS: &[&str] = &[
    "read",
    "grep",
    "find",
    "ls",
    "lsp",
    "docs",
    "document_preview",
    "web_search",
    "web_fetch",
    "ask_user",
    "update_todos",
    "send_file",
    "notify",
    "skill",
    "agent",
    "message_agent",
    "image",
    "show_widget",
    "visualize_read_me",
];

/// `approval.rs` `EDIT_TOOLS` before the table existed.
const OLD_EDIT_TOOLS: &[&str] = &["write", "edit"];

/// `tools/mesh.rs` `TOOL_NAMES`.
const MESH_TOOLS: &[&str] = &["send_message", "ask", "reply", "list_nodes", "cancel_node"];

/// `read_only.rs` `SAFE_TOOLS` before the table existed (the mesh tools passed
/// beside it).
const OLD_SAFE_TOOLS: &[&str] = &[
    "read",
    "grep",
    "find",
    "ls",
    "docs",
    "document_preview",
    "lsp",
    "web_search",
    "web_fetch",
    "ask_user",
    "update_todos",
];

/// The static part of the reserved names in `modes.rs`
/// `rpc_configure_external_tools` before the table existed (every registered
/// tool is added to it at run time).
const OLD_RESERVED: &[&str] = &[
    "read",
    "write",
    "edit",
    "bash",
    "powershell",
    "docs",
    "grep",
    "find",
    "ls",
    "image",
    "permissions",
    "permission",
    "ask_user",
    "plan_mode_start",
    "plan_mode_complete",
];

fn names<'a>(list: impl IntoIterator<Item = &'a str>) -> BTreeSet<&'a str> {
    list.into_iter().collect()
}

#[test]
fn names_are_unique() {
    let all = TOOLS.iter().map(|row| row.name).collect::<Vec<_>>();
    assert_eq!(all.len(), names(all.iter().copied()).len(), "{all:?}");
}

#[test]
fn every_registered_tool_has_a_row() {
    for name in REGISTERED {
        assert!(info(name).is_some(), "no row for registered tool `{name}`");
    }
    for name in MESH_TOOLS {
        assert!(
            REGISTERED.contains(name),
            "`{name}` missing from the test list"
        );
    }
}

#[test]
fn every_tool_the_session_builds_has_a_row() {
    use evo_tools::{ExecutionEnv, MutatingEnv, OsExecutionEnv};
    let env = std::sync::Arc::new(OsExecutionEnv::with_cwd(std::path::PathBuf::from(".")));
    let env_dyn: std::sync::Arc<dyn ExecutionEnv> = env.clone();
    let mut_env: std::sync::Arc<dyn MutatingEnv> = env;
    let context = evo_tools::ExecutionToolContext::new(env_dyn, Some(mut_env));
    for (key, tool) in crate::tools::register_all(&context, &Default::default()) {
        let name = tool.tool.schema().name.clone();
        assert_eq!(key, name, "registered under another name than its schema's");
        assert!(
            info(&name).is_some(),
            "the session builds `{name}` but the table has no row"
        );
        assert!(
            REGISTERED.contains(&name.as_str()),
            "`{name}` missing from the test list"
        );
    }
}

#[test]
fn rows_without_a_registration_are_the_known_ones() {
    let rows = names(TOOLS.iter().map(|row| row.name));
    let expected = names(REGISTERED.iter().chain(NOT_REGISTERED).copied());
    assert_eq!(rows, expected);
}

#[test]
fn classes_equal_the_old_lists() {
    for row in TOOLS {
        let expected = if OLD_EDIT_TOOLS.contains(&row.name) {
            Class::Edit
        } else if OLD_READ_TOOLS.contains(&row.name)
            || MESH_TOOLS.contains(&row.name)
            // Its calls are read or edit by action (`approval::classify`).
            || row.name == "memory"
        {
            Class::Read
        } else {
            // `bash` / `powershell` (judged per command) and every other name.
            Class::Command
        };
        assert_eq!(row.class, expected, "{}", row.name);
        assert_eq!(class_of(row.name), expected, "{}", row.name);
    }
}

#[test]
fn unknown_names_are_commands() {
    assert_eq!(class_of("brand_new_tool"), Class::Command);
    assert_eq!(class_of(""), Class::Command);
    assert!(info("brand_new_tool").is_none());
    assert!(!read_only_safe("brand_new_tool"));
}

#[test]
fn read_only_safe_equals_the_old_list_plus_the_mesh_tools() {
    let safe = names(
        TOOLS
            .iter()
            .filter(|row| row.read_only_safe)
            .map(|row| row.name),
    );
    let expected = names(OLD_SAFE_TOOLS.iter().chain(MESH_TOOLS).copied());
    assert_eq!(safe, expected);
    for name in &expected {
        assert!(read_only_safe(name), "{name}");
    }
    assert!(!read_only_safe("write"));
    assert!(!read_only_safe("bash"));
}

#[test]
fn a_read_only_safe_tool_is_a_read() {
    for row in TOOLS.iter().filter(|row| row.read_only_safe) {
        assert_eq!(row.class, Class::Read, "{}", row.name);
    }
}

#[test]
fn reserved_names_equal_the_old_list() {
    assert_eq!(names(reserved_names()), names(OLD_RESERVED.iter().copied()));
}

#[test]
fn the_default_prompt_tools_have_rows() {
    for name in DEFAULT_PROMPT_TOOLS {
        assert!(info(name).is_some(), "{name}");
    }
}

#[test]
fn mesh_names_match_the_mesh_module() {
    assert_eq!(
        names(crate::tools::mesh::TOOL_NAMES),
        names(MESH_TOOLS.iter().copied())
    );
}
