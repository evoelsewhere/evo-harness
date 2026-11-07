//! The one table of facts about the tools the engine knows by name.
//!
//! Spec: `docs/features/agent-tools-layers/spec.md` (FR-1, NFR-1); decision:
//! ADR 0014.
//!
//! Approval ([`crate::approval`]), the read-only policy ([`crate::read_only`]),
//! the system prompt's default tool set ([`crate::session`]) and the reserved
//! names of `configure_external_tools` ([`crate::modes`]) read this table and
//! keep no tool-name lists of their own. A new tool adds one row.

/// What a tool call does to the machine. The vocabulary of the approval mode:
/// reads never ask, `ask` asks for edits and commands, `auto-edits` for
/// commands only, `auto` for none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Read,
    Edit,
    Command,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Command => "command",
        }
    }
}

/// Facts about one tool.
#[derive(Debug, Clone, Copy)]
pub struct ToolInfo {
    pub name: &'static str,
    /// The class of the tool's calls. `bash` and `powershell` are `Command`
    /// here and judged per command by `approval::classify`; `memory` is `Read`
    /// here and an `Edit` for its `save` / `delete` actions.
    pub class: Class,
    /// A read-only agent may call it (it has no side effects).
    pub read_only_safe: bool,
    /// A plugin or the app may not define a tool with this name.
    pub reserved: bool,
}

const fn row(name: &'static str, class: Class, read_only_safe: bool, reserved: bool) -> ToolInfo {
    ToolInfo {
        name,
        class,
        read_only_safe,
        reserved,
    }
}

/// The tools the default prompt lists, in order (the coding tools every chat
/// has). Not a classification: it is the prompt's default selection.
pub const DEFAULT_PROMPT_TOOLS: &[&str] = &["read", "bash", "edit", "write", "docs"];

/// One row per tool name. A name not in the table (a plugin's or a
/// connector's) is a command that a read-only agent may not call and that is
/// not reserved.
///
/// Rows for names the engine does not register as a tool: `grep`, `find`,
/// `ls` and `image` (library tools the session leaves out), `send_file` and
/// `notify` (the app answers them; kept so they classify as reads as before),
/// and `permissions`, `permission`, `plan_mode_start`, `plan_mode_complete`
/// (names reserved for security and native features).
///
/// `reserved` reproduces the static list `configure_external_tools` always
/// kept; it also reserves every tool that is registered at that moment, so a
/// tool switched off at launch (`web_search`, `memory`, ...) can be taken by
/// the app, as before.
pub const TOOLS: &[ToolInfo] = &[
    // Files and commands.
    row("read", Class::Read, true, true),
    row("write", Class::Edit, false, true),
    row("edit", Class::Edit, false, true),
    row("bash", Class::Command, false, true),
    row("powershell", Class::Command, false, true),
    row("grep", Class::Read, true, true),
    row("find", Class::Read, true, true),
    row("ls", Class::Read, true, true),
    row("image", Class::Read, false, true),
    // Lookup, network and display.
    row("docs", Class::Read, true, true),
    row("document_preview", Class::Read, true, false),
    row("lsp", Class::Read, true, false),
    row("web_search", Class::Read, true, false),
    row("web_fetch", Class::Read, true, false),
    row("visualize_read_me", Class::Read, false, false),
    row("show_widget", Class::Read, false, false),
    // Working with the person.
    row("ask_user", Class::Read, true, true),
    row("update_todos", Class::Read, true, false),
    // Session state and delegation. A subagent's own calls are checked one by
    // one, so `agent` and `message_agent` are reads; a skill's `` !`command` ``
    // lines are governed by Customize -> Skills.
    row("memory", Class::Read, false, false),
    row("skill", Class::Read, false, false),
    row("agent", Class::Read, false, false),
    row("message_agent", Class::Read, false, false),
    // The session mesh.
    row("send_message", Class::Read, true, false),
    row("ask", Class::Read, true, false),
    row("reply", Class::Read, true, false),
    row("list_nodes", Class::Read, true, false),
    row("cancel_node", Class::Read, true, false),
    // Answered by the app.
    row("send_file", Class::Read, false, false),
    row("notify", Class::Read, false, false),
    // Reserved only.
    row("permissions", Class::Command, false, true),
    row("permission", Class::Command, false, true),
    row("plan_mode_start", Class::Command, false, true),
    row("plan_mode_complete", Class::Command, false, true),
];

pub fn info(name: &str) -> Option<&'static ToolInfo> {
    TOOLS.iter().find(|row| row.name == name)
}

/// The class of a tool's calls; a name the table does not know is a command.
pub fn class_of(name: &str) -> Class {
    info(name).map_or(Class::Command, |row| row.class)
}

/// Whether a read-only agent may call the tool.
pub fn read_only_safe(name: &str) -> bool {
    info(name).is_some_and(|row| row.read_only_safe)
}

/// The names a plugin or the app may not take.
pub fn reserved_names() -> impl Iterator<Item = &'static str> {
    TOOLS.iter().filter(|row| row.reserved).map(|row| row.name)
}

#[cfg(test)]
mod tests;
