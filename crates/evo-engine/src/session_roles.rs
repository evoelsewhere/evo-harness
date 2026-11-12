//! Roles for nested sessions (docs/features/session-roles-and-mail/spec.md,
//! FR-1 – FR-4; design in plan.md, "start_session").
//!
//! A `start_session` call may ask for an agent type, a model and a thinking
//! level. The parent's engine resolves them here, before the call leaves for
//! the shell's mesh hub: the type's instructions, tools and access, a model
//! from the parent's providers, and a thinking level the model supports. The
//! hub stores the resolved [`SessionRole`] with the new chat, and the
//! session's own engine applies it at every start.
//!
//! The model never supplies a resolved role: a `role` it wrote is dropped,
//! and a read-only parent only starts read-only sessions (FR-4).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use evo_ai::types::ThinkingLevel;
use evo_ai::Model;
use evo_protocol::mesh::SessionRole;
use serde_json::Value;

use crate::agents::AgentType;

/// The `start_session` arguments that ask for a role.
pub const ROLE_ARGUMENTS: [&str; 3] = ["agentType", "model", "thinking"];

/// What the parent session brings to a role.
#[derive(Debug, Clone, Default)]
pub struct Parent {
    /// The session's folder (project agent types are found from it).
    pub cwd: PathBuf,
    /// The models its providers offer.
    pub models: Vec<Model>,
    /// The model it runs on.
    pub model: Option<Model>,
    /// Its thinking level.
    pub thinking: Option<ThinkingLevel>,
    /// It is itself read-only: its sessions are too.
    pub read_only: bool,
}

/// The role this session's own engine runs as, set once at start.
static CURRENT: OnceLock<SessionRole> = OnceLock::new();

/// Record the role this engine runs as (from `--role-file`).
pub fn set_current(role: SessionRole) {
    let _ = CURRENT.set(role);
}

/// The role this engine runs as, if any.
pub fn current() -> Option<&'static SessionRole> {
    CURRENT.get()
}

/// Whether this engine runs as a read-only role.
pub fn current_is_read_only() -> bool {
    current().is_some_and(SessionRole::is_read_only)
}

/// The role in a `--role-file`.
pub fn load_file(path: &Path) -> Result<SessionRole, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Run this engine as `role`: its tool list narrows the built-in tools (the
/// tools for talking to other agents stay), and its instructions join the
/// system prompt ([`prompt_block`]). An explicit `--tools` list is kept.
pub fn apply(role: SessionRole, args: &mut crate::args::Args) {
    narrow_tools(&role, args);
    // A read-only session gets no language servers: they may build the
    // project (FR-13).
    if role.is_read_only() {
        args.exclude_tools
            .get_or_insert_with(Vec::new)
            .push("lsp".to_string());
    }
    set_current(role);
}

/// Whether this session's role lets it have the app tool `name`: every
/// tool without a role tool list, else the tools the list names.
pub fn allows_external(name: &str) -> bool {
    allows(current(), name)
}

fn allows(role: Option<&SessionRole>, name: &str) -> bool {
    match role.and_then(|role| role.tools.as_ref()) {
        Some(tools) => tools.iter().any(|tool| tool == name),
        None => true,
    }
}

/// Why a read-only session may not make this call, if it may not. Tools of
/// the engine go through the read-only checker (as for read-only
/// subagents); `lsp` is refused; tools the app registered are left to the
/// app, which offers a read-only chat only tools that read and refuses the
/// rest again when called (FR-13). Anything else (an extension's tool) is
/// refused by the checker as unknown.
pub fn read_only_refusal(tool: &str, args: &Value, external: bool) -> Option<String> {
    if tool == "lsp" {
        return Some(
            "Refused: this session is read-only, and language servers may build the project."
                .into(),
        );
    }
    if external {
        return None;
    }
    crate::read_only::guard(tool, args)
        .err()
        .map(|reason| reason.replace("this agent is read-only", "this session is read-only"))
}

/// The first link of a read-only session's `before_tool_call` chain: a call
/// [`read_only_refusal`] refuses goes no further (no hook, no approval
/// prompt). Other sessions keep `previous` unchanged.
pub fn before_tool_call(
    previous: Option<evo_agent::BeforeToolCall>,
) -> Option<evo_agent::BeforeToolCall> {
    if !current_is_read_only() {
        return previous;
    }
    Some(std::sync::Arc::new(
        move |ctx: evo_agent::BeforeToolCallContext<'_>, signal| {
            let tool = ctx.tool_call.name.as_str();
            let external = crate::approval::global().is_external(tool);
            let refusal = read_only_refusal(tool, ctx.args, external);
            let earlier = match refusal {
                None => previous.as_ref().map(|hook| hook(ctx, signal)),
                Some(_) => None,
            };
            Box::pin(async move {
                if let Some(reason) = refusal {
                    return Some(evo_agent::BeforeToolCallResult {
                        block: true,
                        reason: Some(reason),
                        terminate: false,
                        args: None,
                    });
                }
                match earlier {
                    Some(earlier) => earlier.await,
                    None => None,
                }
            })
        },
    ))
}

/// `args.tools` narrowed to the role's tool list, plus the tools for talking
/// to other agents; an explicit `--tools` list is kept.
fn narrow_tools(role: &SessionRole, args: &mut crate::args::Args) {
    if let (Some(tools), None) = (&role.tools, &args.tools) {
        let mut allowed = tools.clone();
        for name in crate::tools::mesh::TOOL_NAMES {
            if !allowed.iter().any(|t| t == name) {
                allowed.push(name.to_string());
            }
        }
        args.tools = Some(allowed);
    }
}

/// The system prompt's role section, for an engine that runs as a role
/// with instructions.
pub fn prompt_block() -> Option<String> {
    current().and_then(block_for)
}

fn block_for(role: &SessionRole) -> Option<String> {
    let instructions = role.instructions.as_deref()?.trim();
    if instructions.is_empty() {
        return None;
    }
    let name = role.name.as_deref().unwrap_or("session");
    let access = if role.is_read_only() {
        "\nYou are read-only: do not change files or anything else outside this conversation."
    } else {
        ""
    };
    Some(format!(
        "<role name=\"{name}\">\nYou run as the `{name}` agent type for this whole session.{access}\n\n{instructions}\n</role>"
    ))
}

/// `start_session` arguments with `role` resolved from `agentType`, `model`
/// and `thinking`, or the arguments unchanged (without any `role`) when none
/// of them is given. Refused, with what is available: an unknown type, a
/// model the parent's providers do not offer, a level that is not one.
pub fn enrich(mut args: Value, parent: &Parent, types: &[AgentType]) -> Result<Value, String> {
    let Some(object) = args.as_object_mut() else {
        return Ok(args);
    };
    // Only this function writes a role.
    object.remove("role");
    let text = |key: &str| -> Result<Option<String>, String> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
            Some(Value::String(value)) => Ok(Some(value.trim().to_string())),
            Some(_) => Err(format!("`{key}` must be a string")),
        }
    };
    let agent_type = text("agentType")?;
    let wanted_model = text("model")?;
    let wanted_thinking = text("thinking")?;
    for key in ROLE_ARGUMENTS {
        object.remove(key);
    }
    if agent_type.is_none() && wanted_model.is_none() && wanted_thinking.is_none() {
        return Ok(args);
    }

    let agent = match &agent_type {
        Some(name) => Some(types.iter().find(|t| &t.name == name).ok_or_else(|| {
            let names: Vec<_> = types.iter().map(|t| t.name.as_str()).collect();
            format!(
                "unknown agent type `{name}`; available: {}",
                names.join(", ")
            )
        })?),
        None => None,
    };

    let type_model = agent
        .and_then(|a| a.model.as_deref())
        .filter(|id| !id.eq_ignore_ascii_case("inherit"));
    let model = match wanted_model.as_deref().or(type_model) {
        Some(id) => Some(find_model(&parent.models, id).ok_or_else(|| {
            let mut ids: Vec<String> = parent
                .models
                .iter()
                .map(|m| format!("{}/{}", m.provider, m.id))
                .collect();
            ids.truncate(30);
            format!(
                "model `{id}` is not available (its provider may have no credentials); available: {}",
                ids.join(", ")
            )
        })?),
        None => parent.model.clone(),
    };

    let thinking = match &wanted_thinking {
        Some(level) => Some(
            evo_runtime::thinking::parse_thinking_level(level)
                .ok_or_else(|| format!("thinking `{level}` is not a thinking level"))?,
        ),
        None => agent.and_then(|a| a.thinking).or(parent.thinking),
    };
    // As close as the model allows; a model without reasoning thinks `off`.
    let thinking = match (&model, thinking) {
        (Some(model), Some(level)) if model.reasoning => Some(model.clamp_thinking_level(level)),
        (Some(_), Some(_)) => Some(ThinkingLevel::Off),
        (_, level) => level,
    };

    let read_only = parent.read_only || agent.is_some_and(|a| a.read_only);
    let (source, source_path) = match agent {
        Some(agent) => source_of(agent, &parent.cwd),
        None => (None, None),
    };
    let role = SessionRole {
        name: agent.map(|a| a.name.clone()),
        source,
        source_path,
        instructions: agent
            .map(|a| a.prompt.trim().to_string())
            .filter(|p| !p.is_empty()),
        tools: agent.and_then(|a| a.tools.clone()),
        access: if read_only { "read-only" } else { "full" }.into(),
        provider: model.as_ref().map(|m| m.provider.to_string()),
        model: model.as_ref().map(|m| m.id.clone()),
        thinking: thinking.and_then(|level| {
            serde_json::to_value(level)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
        }),
    };
    object.insert(
        "role".into(),
        serde_json::to_value(role).map_err(|e| e.to_string())?,
    );
    Ok(args)
}

/// Where an agent type came from, and its file for display.
fn source_of(agent: &AgentType, cwd: &Path) -> (Option<String>, Option<String>) {
    let Some(path) = &agent.source else {
        return (Some("builtin".into()), None);
    };
    let shown = path
        .strip_prefix(cwd)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.clone());
    (
        Some(agent.origin.into()),
        Some(shown.to_string_lossy().replace('\\', "/")),
    )
}

/// A model by id (`id` or `provider/id`, any case).
pub(crate) fn find_model(models: &[Model], wanted: &str) -> Option<Model> {
    let wanted = wanted.trim();
    models
        .iter()
        .find(|m| {
            m.id.eq_ignore_ascii_case(wanted)
                || format!("{}/{}", m.provider, m.id).eq_ignore_ascii_case(wanted)
        })
        .cloned()
}

#[cfg(test)]
mod tests;
