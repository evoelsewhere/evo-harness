//! The engine's agent tools: one table of facts about them ([`info`]), one
//! module per tool or tool family, and [`register_all`], which builds them.
//!
//! Spec: `docs/features/agent-tools-layers/spec.md` (FR-1, FR-2); decision:
//! ADR 0014.

pub mod agent;
pub mod docs;
pub mod document_preview;
pub mod info;
pub mod interaction;
pub mod lsp;
pub mod memory;
pub mod mesh;
pub mod skill;
pub mod visualize;
pub mod web;

use std::path::Path;
use std::sync::Arc;

use evo_harness::types::{HarnessTool, ToolReplay};
#[cfg(windows)]
use evo_tools::create_powershell_tool;
use evo_tools::{
    create_bash_tool, create_edit_tool, create_read_tool, create_write_tool, ExecutionToolContext,
    OsExecutionEnv,
};

use docs::create_docs_tool;

/// Default bash timeout: 120s when the model doesn't pass one (prevents a
/// forgotten `timeout` from hanging the run forever — the "it is stuck" report).
/// `EVO_BASH_TIMEOUT` overrides; a model-supplied timeout always wins.
pub fn bash_options() -> evo_tools::tools::bash::BashToolOptions {
    use evo_tools::tools::bash::BashToolOptions;
    let default = std::env::var("EVO_BASH_TIMEOUT")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(evo_tools::tools::bash::DEFAULT_BASH_TIMEOUT_SECONDS);
    // Saved user defaults, matching how `defaultTools` is read in `build_tools`.
    // `shellCommandPrefix` is prepended to every command (upstream's key for
    // alias/`shopt` setup). Native's `shellPath` is *not* wired: neither
    // `BashToolOptions` nor `ShellCaptureOptions` carries a shell binary today, so
    // honouring it means plumbing one through evo-tools first.
    let settings = crate::settings::load_settings().unwrap_or_default();
    BashToolOptions {
        command_prefix: settings.shell_command_prefix.clone(),
        default_timeout: Some(default),
        prepare: None,
    }
}

/// The built-in file and shell tools working in `cwd` instead of the
/// session's folder, for a delegated child in its own worktree. Same options
/// as the parent's (`build_tools`); the harness swaps them in by name.
pub fn builtin_tools_in(cwd: &Path) -> Vec<HarnessTool> {
    let env = Arc::new(
        OsExecutionEnv::with_cwd(cwd.to_path_buf())
            .with_shell_env_provider(crate::office::shell_env_provider()),
    );
    let env_dyn: Arc<dyn evo_tools::ExecutionEnv> = env.clone();
    let mut_env: Arc<dyn evo_tools::MutatingEnv> = env;
    let ctx = ExecutionToolContext::new(env_dyn, Some(mut_env));
    // Only Windows adds a tool after this.
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut tools = vec![
        HarnessTool::new(create_read_tool(
            &ctx,
            Some(evo_tools::tools::read::ReadToolOptions {
                image_processor: Some(Arc::new(evo_tools::image_processing::InlineImageProcessor)),
                ..Default::default()
            }),
        ))
        .with_replay(ToolReplay::Safe),
        HarnessTool::new(create_bash_tool(&ctx, Some(bash_options()))),
        HarnessTool::new(create_edit_tool(&ctx)),
        HarnessTool::new(create_write_tool(&ctx)),
    ];
    #[cfg(target_os = "windows")]
    tools.push(HarnessTool::new(create_powershell_tool(&ctx)));
    tools
}

/// Every engine tool, by name, in registration order: the file and shell
/// tools, then `docs`, the session and agent tools, the mesh, memory, the
/// person-facing tools, and the optional ones (`web_*` unless switched off in
/// settings, `powershell` on Windows). The caller applies the person's
/// selection (`--tools`, `defaultTools`, ...) to the result.
pub fn register_all(
    ctx: &ExecutionToolContext,
    harness: &agent::HarnessCell,
) -> Vec<(&'static str, HarnessTool)> {
    // Keep the coding tools aligned with Evo and expose evo's read-only docs
    // lookup as a default assistant capability.
    //
    // Replay policy (native `ToolDeclaration.replay`): only tools that are
    // genuinely free of side effects may be re-run when a crash leaves their
    // outcome unknown — everything else must stay `Never`, because replaying
    // `bash`/`write`/`edit` would duplicate a side effect. `HarnessTool::new`
    // already defaults to `Never`, so this is opt-in per read-only tool.
    let mut all: Vec<(&'static str, HarnessTool)> = vec![
        (
            "read",
            HarnessTool::new(create_read_tool(
                ctx,
                Some(evo_tools::tools::read::ReadToolOptions {
                    image_processor: Some(Arc::new(
                        evo_tools::image_processing::InlineImageProcessor,
                    )),
                    ..Default::default()
                }),
            ))
            .with_replay(ToolReplay::Safe),
        ),
        (
            "bash",
            HarnessTool::new(create_bash_tool(ctx, Some(bash_options()))),
        ),
        ("edit", HarnessTool::new(create_edit_tool(ctx))),
        ("write", HarnessTool::new(create_write_tool(ctx))),
        (
            "docs",
            HarnessTool::new(create_docs_tool()).with_replay(ToolReplay::Safe),
        ),
        (
            "agent",
            HarnessTool::new(agent::create_agent_tool(
                harness.clone(),
                ctx.env().cwd().to_path_buf(),
            )),
        ),
        (
            "message_agent",
            HarnessTool::new(agent::create_message_agent_tool(harness.clone())),
        ),
    ];
    // Talking between the session and its agents (session mesh, tools/mesh.rs).
    for (name, tool) in mesh::TOOL_NAMES
        .into_iter()
        .zip(mesh::create(harness.clone(), None))
    {
        all.push((name, HarnessTool::new(tool)));
    }
    all.extend(vec![
        (
            "skill",
            HarnessTool::new(skill::create_skill_tool(harness.clone()))
                .with_replay(ToolReplay::Safe),
        ),
        (
            "memory",
            HarnessTool::new(memory::create_memory_tool(ctx.env().cwd().to_path_buf())),
        ),
    ]);
    // Working with the person (`interaction.rs`).
    all.push((
        "ask_user",
        HarnessTool::new(interaction::create_ask_user_tool()),
    ));
    all.push((
        "update_todos",
        HarnessTool::new(interaction::create_update_todos_tool()),
    ));
    // Inline HTML widgets in the chat (`tools/visualize.rs`); both only read.
    all.push((
        "visualize_read_me",
        HarnessTool::new(visualize::create_read_me_tool()).with_replay(ToolReplay::Safe),
    ));
    all.push((
        "show_widget",
        HarnessTool::new(visualize::create_show_widget_tool()).with_replay(ToolReplay::Safe),
    ));
    // Layout check of a rendered document (`tools/document_preview.rs`); only reads.
    all.push((
        "document_preview",
        HarnessTool::new(document_preview::create_document_preview_tool(
            ctx.env().cwd().to_path_buf(),
        ))
        .with_replay(ToolReplay::Safe),
    ));
    // Code intelligence from language servers (`tools/lsp.rs`).
    all.push((
        "lsp",
        HarnessTool::new(lsp::create_lsp_tool(ctx.env().cwd().to_path_buf()))
            .with_replay(ToolReplay::Safe),
    ));
    // Web search and fetch (`tools/web.rs`), unless switched off in settings.
    if web::enabled() {
        all.push((
            "web_search",
            HarnessTool::new(web::create_web_search_tool()).with_replay(ToolReplay::Safe),
        ));
        all.push((
            "web_fetch",
            HarnessTool::new(web::create_web_fetch_tool()).with_replay(ToolReplay::Safe),
        ));
    }

    // Add PowerShell tool on Windows
    #[cfg(target_os = "windows")]
    all.push(("powershell", HarnessTool::new(create_powershell_tool(ctx))));

    all
}
