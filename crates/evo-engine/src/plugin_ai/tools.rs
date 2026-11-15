//! Evo's own tools in a plugin's run (docs/features/plugin-ai/spec.md,
//! FR-16; plan 3.5, 3.7): the groups the person allowed the plugin
//! (`ai.tools:read`, `edit`, `command`, `web`), working in the run's folder
//! under the process's sandbox config, each call gated by the person's
//! approval mode as in a chat: reads never ask, `ask` asks before edits and
//! commands, `auto-edits` before commands, `auto` never; a command that
//! would stop evo always asks. Questions name the plugin.

use std::path::Path;
use std::sync::Arc;

use evo_agent::AgentTool;
use evo_tools::{
    create_bash_tool, create_edit_tool, create_find_tool, create_grep_tool, create_ls_tool,
    create_read_tool, create_write_tool, ExecutionToolContext, OsExecutionEnv,
};
use serde_json::Value;

use crate::approval::{ApprovalMode, Kind};

/// The options of an approval question, in order.
pub const OPTIONS: [&str; 3] = ["Allow once", "Allow for this run", "Deny"];

/// Evo's tools of `groups` (`read`, `edit`, `command`, `web`), working in
/// `folder`. Other names give nothing: `apps` (the browser, terminal and
/// other app tools) run in the desktop shell, for chats only.
pub fn build(groups: &[String], folder: &Path) -> Vec<Arc<dyn AgentTool>> {
    let has = |group: &str| groups.iter().any(|g| g == group);
    let env = Arc::new(
        OsExecutionEnv::with_cwd(folder.to_path_buf())
            .with_shell_env_provider(crate::office::shell_env_provider()),
    );
    let env_dyn: Arc<dyn evo_tools::ExecutionEnv> = env.clone();
    let mut_env: Arc<dyn evo_tools::MutatingEnv> = env;
    let ctx = ExecutionToolContext::new(env_dyn, Some(mut_env));
    let mut tools: Vec<Arc<dyn AgentTool>> = Vec::new();
    if has("read") {
        tools.push(create_read_tool(
            &ctx,
            Some(evo_tools::tools::read::ReadToolOptions {
                image_processor: Some(Arc::new(evo_tools::image_processing::InlineImageProcessor)),
                ..Default::default()
            }),
        ));
        tools.push(create_grep_tool(&ctx, None));
        tools.push(create_find_tool(&ctx, None));
        tools.push(create_ls_tool(&ctx, None));
    }
    if has("edit") {
        tools.push(create_edit_tool(&ctx));
        tools.push(create_write_tool(&ctx));
    }
    if has("command") {
        tools.push(create_bash_tool(&ctx, Some(crate::tools::bash_options())));
        #[cfg(target_os = "windows")]
        tools.push(evo_tools::create_powershell_tool(&ctx));
    }
    if has("web") && crate::tools::web::enabled() {
        tools.push(crate::tools::web::create_web_search_tool());
        tools.push(crate::tools::web::create_web_fetch_tool());
    }
    tools
}

/// What the run allowed for itself ("Allow for this run").
#[derive(Debug, Clone, Copy, Default)]
pub struct Grants {
    pub edit: bool,
    pub command: bool,
}

impl Grants {
    pub fn grant(&mut self, kind: Kind) {
        match kind {
            Kind::Edit => self.edit = true,
            Kind::Command => self.command = true,
            Kind::Read => {}
        }
    }
}

/// Whether a call runs, or the person is asked first.
#[derive(Debug, Clone, PartialEq)]
pub enum Gate {
    Run,
    /// Ask with this title and detail; the call's kind for "Allow for this run".
    Ask {
        title: String,
        detail: String,
        kind: Kind,
        always: bool,
    },
}

/// The gate for one call of `tool` with `args` by `plugin`'s run in `folder`.
pub fn gate(
    tool: &str,
    args: &Value,
    mode: ApprovalMode,
    grants: Grants,
    plugin: &str,
    folder: &Path,
) -> Gate {
    let kind = crate::approval::classify(tool, args);
    let (title, summary) = crate::approval::describe(tool, kind, args);
    let detail = |extra: &str| {
        format!(
            "{plugin}'s AI asks to do this in {}, not a chat's agent.{extra}\n{summary}",
            folder.display()
        )
    };
    if matches!(tool, "bash" | "powershell") {
        let command = args.get("command").and_then(Value::as_str).unwrap_or("");
        if let Some(reason) =
            crate::self_kill::reason(command, &crate::self_kill::Protected::from_env())
        {
            return Gate::Ask {
                title: format!("{plugin}: {title}"),
                detail: detail(&format!(" {reason}")),
                kind,
                always: true,
            };
        }
    }
    let asks = match kind {
        Kind::Read => false,
        Kind::Edit => mode == ApprovalMode::Ask && !grants.edit,
        Kind::Command => mode != ApprovalMode::Auto && !grants.command,
    };
    if asks {
        Gate::Ask {
            title: format!("{plugin}: {title}"),
            detail: detail(""),
            kind,
            always: false,
        }
    } else {
        Gate::Run
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn groups_give_their_tools_only() {
        let dir = std::env::temp_dir();
        let names = |groups: &[&str]| -> Vec<String> {
            let groups: Vec<String> = groups.iter().map(|g| g.to_string()).collect();
            build(&groups, &dir)
                .iter()
                .map(|t| t.schema().name.clone())
                .collect()
        };
        assert_eq!(names(&["read"]), ["read", "grep", "find", "ls"]);
        assert_eq!(names(&["edit"]), ["edit", "write"]);
        assert!(names(&["command"]).contains(&"bash".to_string()));
        assert!(names(&[]).is_empty());
        assert!(
            names(&["apps", "everything"]).is_empty(),
            "unknown groups give nothing"
        );
    }

    #[test]
    fn the_approval_mode_decides_as_in_a_chat_and_names_the_plugin() {
        let folder = Path::new("C:/work");
        let none = Grants::default();
        let read = json!({ "path": "a.md" });
        let write = json!({ "path": "a.md", "content": "x" });
        let rm = json!({ "command": "rm -rf build" });
        let ls = json!({ "command": "ls" });
        assert_eq!(
            gate("read", &read, ApprovalMode::Ask, none, "stocks", folder),
            Gate::Run
        );
        assert_eq!(
            gate("bash", &ls, ApprovalMode::Ask, none, "stocks", folder),
            Gate::Run,
            "a read-only command is a read"
        );
        match gate("write", &write, ApprovalMode::Ask, none, "stocks", folder) {
            Gate::Ask {
                title,
                detail,
                kind,
                always,
            } => {
                assert!(title.starts_with("stocks: "));
                assert!(detail.contains("stocks's AI asks") && detail.contains("a.md"));
                assert_eq!((kind, always), (Kind::Edit, false));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            gate("write", &write, ApprovalMode::AutoEdits, none, "s", folder),
            Gate::Run
        );
        assert!(matches!(
            gate("bash", &rm, ApprovalMode::AutoEdits, none, "s", folder),
            Gate::Ask { .. }
        ));
        assert_eq!(
            gate("bash", &rm, ApprovalMode::Auto, none, "s", folder),
            Gate::Run
        );
        let mut grants = Grants::default();
        grants.grant(Kind::Command);
        assert_eq!(
            gate("bash", &rm, ApprovalMode::Ask, grants, "s", folder),
            Gate::Run,
            "allowed for this run"
        );
        assert!(
            matches!(
                gate("write", &write, ApprovalMode::Ask, grants, "s", folder),
                Gate::Ask { .. }
            ),
            "a command grant is not an edit grant"
        );
    }

    #[test]
    fn a_command_that_would_stop_evo_always_asks() {
        let kill = json!({ "command": "taskkill /F /IM evo-engine.exe" });
        match gate(
            "bash",
            &kill,
            ApprovalMode::Auto,
            Grants {
                edit: true,
                command: true,
            },
            "s",
            Path::new("."),
        ) {
            Gate::Ask { always, .. } => assert!(always),
            other => panic!("{other:?}"),
        }
    }
}
