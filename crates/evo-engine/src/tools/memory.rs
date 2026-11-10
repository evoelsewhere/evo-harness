//! The `memory` tool: save, update, read, list and delete the agent's memory
//! topics. The storage, the index and the prompt block are in
//! [`crate::memory`].
//!
//! Spec: `docs/features/agent-tools-layers/spec.md` (FR-2);
//! `docs/features/memory-topics-and-chat-recall/spec.md` (FR-3, FR-5, FR-7,
//! FR-17) for `update`, the sensitive mark and the personal switch.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use evo_agent::agent_tool::AgentTool;
use evo_agent::error::AgentError;
use evo_agent::types::{AgentToolResult, TextContentOrImage, ToolExecutionMode, ToolResultPartial};
use evo_ai::types::{Schema, Tool};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::memory::{
    delete_in, dir, enabled, entry, find_in, index_lines, list_dir, personal_enabled, save_in,
    sensitive_enabled, update_in, Meta, Scope, SENSITIVE_OFF, TYPES,
};

/// Who is using the tool; written to a topic as who changed it last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// The agent in a chat.
    Chat,
    /// Automatic indexing (`crate::memory_index`).
    Index,
}

impl Origin {
    fn by(&self) -> &'static str {
        match self {
            Origin::Chat => "chat",
            Origin::Index => "index",
        }
    }
}

/// Told about every change a tool made to a topic: the scope, the topic's
/// name, and its file before and after (`None`: no file). The index run
/// records these so each change can be shown and undone.
pub type Recorder = Arc<dyn Fn(Scope, &str, Option<String>, Option<String>) + Send + Sync>;

/// The settings a call depends on, read once per call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Switches {
    /// Memory is not paused.
    pub enabled: bool,
    /// This project uses personal topics.
    pub personal: bool,
    /// Sensitive subjects may be saved.
    pub sensitive: bool,
}

pub struct MemoryTool {
    schema: Tool,
    cwd: PathBuf,
    origin: Origin,
    recorder: Option<Recorder>,
}

pub fn create_memory_tool(cwd: PathBuf) -> Arc<dyn AgentTool> {
    Arc::new(MemoryTool {
        schema: schema(),
        cwd,
        origin: Origin::Chat,
        recorder: None,
    })
}

/// The tool as the index run uses it: topics it changes say `by: index`, and
/// `recorder` hears each change.
pub fn index_memory_tool(cwd: PathBuf, recorder: Recorder) -> MemoryTool {
    MemoryTool {
        schema: schema(),
        cwd,
        origin: Origin::Index,
        recorder: Some(recorder),
    }
}

pub(crate) fn schema() -> Tool {
    Tool {
        name: "memory".into(),
        description: "Your memory across chats (see # Memory in the system prompt). Actions: \
             `save` (name, description, type, content, scope; the same name replaces the topic), \
             `update` (name, scope, and `old_text` + `new_text` to replace one part of a topic, \
             or `new_text` alone to add at its end), `read` (name, scope), `list` (scope, \
             optional), `delete` (name, scope)."
            .into(),
        parameters: Schema::new(json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["save", "update", "read", "list", "delete"] },
                "scope": { "type": "string", "enum": ["personal", "project"], "description": "personal: every project; project: this one. Default project." },
                "name": { "type": "string", "description": "Short kebab-case name, e.g. commit-style." },
                "description": { "type": "string", "description": "One line saying what the topic holds, used to judge relevance later." },
                "type": { "type": "string", "enum": TYPES },
                "content": { "type": "string", "description": "save: the whole topic. For feedback and project topics, add why and how to apply it." },
                "old_text": { "type": "string", "description": "update: the text to replace, copied exactly from the topic; it must occur once." },
                "new_text": { "type": "string", "description": "update: the replacement (empty removes `old_text`); without `old_text` it is added at the end." },
                "sensitive": { "type": "boolean", "description": "save / update: the change holds a sensitive subject (health, race or ethnicity, religion, political views, sexual orientation or gender identity)." }
            },
            "required": ["action"],
            "additionalProperties": false
        })),
        constrained_sampling: None,
    }
}

fn arg<'a>(params: &'a Value, key: &str) -> &'a str {
    params.get(key).and_then(Value::as_str).unwrap_or("")
}

impl MemoryTool {
    /// One call outside an agent loop: the text for the model, or the error.
    pub fn call(&self, params: &Value) -> Result<String, String> {
        self.run(params).map(|(text, _)| text)
    }

    fn run(&self, params: &Value) -> Result<(String, Value), String> {
        let switches = Switches {
            enabled: enabled(),
            personal: personal_enabled(&self.cwd),
            sensitive: sensitive_enabled(),
        };
        self.run_with(params, switches, &|scope| dir(scope, &self.cwd))
    }

    /// One call, with the settings and the scope folders given.
    pub(crate) fn run_with(
        &self,
        params: &Value,
        switches: Switches,
        folder_of: &dyn Fn(Scope) -> Result<PathBuf, String>,
    ) -> Result<(String, Value), String> {
        if !switches.enabled {
            return Err("memory is switched off in the app".into());
        }
        let action = arg(params, "action");
        let scope_text = arg(params, "scope");
        let scope = if scope_text.is_empty() {
            Scope::Project
        } else {
            Scope::parse(scope_text)?
        };
        if scope == Scope::Personal && !switches.personal {
            return Err(
                "this project keeps its own memory: personal topics are switched off here \
                 (Use personal memory); use scope `project`"
                    .into(),
            );
        }
        let folder = folder_of(scope)?;
        let meta = Meta {
            by: Some(self.origin.by()),
            sensitive: params
                .get("sensitive")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
        if meta.sensitive && !switches.sensitive && matches!(action, "save" | "update") {
            return Err(SENSITIVE_OFF.into());
        }
        // The topic file a change is about, read before and after it.
        let watched = self
            .recorder
            .as_ref()
            .filter(|_| matches!(action, "save" | "update" | "delete"))
            .and_then(|_| crate::memory::clean_name(arg(params, "name")).ok())
            .map(|name| {
                let path = folder.join(format!("{name}.md"));
                (name, path)
            });
        let before = watched
            .as_ref()
            .and_then(|(_, path)| std::fs::read_to_string(path).ok());
        let outcome = match action {
            "save" => {
                let kind = match arg(params, "type") {
                    "" => "project",
                    k => k,
                };
                let existed = find_in(&folder, arg(params, "name")).is_some();
                let m = save_in(
                    &folder,
                    arg(params, "name"),
                    arg(params, "description"),
                    kind,
                    arg(params, "content"),
                    None,
                    meta,
                )?;
                let verb = if existed { "Updated" } else { "Saved" };
                Ok((
                    format!("{verb} {} memory `{}`.", scope.label(), m.name),
                    json!({
                        "memory": entry(&m, scope),
                        "action": if existed { "updated" } else { "saved" },
                        "sensitive": m.sensitive,
                    }),
                ))
            }
            "update" => {
                let old_text = Some(arg(params, "old_text")).filter(|text| !text.is_empty());
                let (m, diff) = update_in(
                    &folder,
                    arg(params, "name"),
                    old_text,
                    arg(params, "new_text"),
                    meta,
                )?;
                Ok((
                    format!("Updated {} memory `{}`.", scope.label(), m.name),
                    json!({
                        "memory": entry(&m, scope),
                        "action": "updated",
                        "diff": { "removed": diff.removed, "added": diff.added },
                        "sensitive": meta.sensitive,
                    }),
                ))
            }
            "read" => {
                let m = find_in(&folder, arg(params, "name")).ok_or_else(|| {
                    format!(
                        "no {} memory named `{}`",
                        scope.label(),
                        arg(params, "name")
                    )
                })?;
                Ok((
                    format!(
                        "{} ({}, {}): {}\n\n{}",
                        m.name,
                        m.kind,
                        scope.label(),
                        m.description,
                        m.content
                    ),
                    json!({ "memory": entry(&m, scope), "action": "read" }),
                ))
            }
            "list" => {
                let scopes: Vec<Scope> = if !scope_text.is_empty() {
                    vec![scope]
                } else if switches.personal {
                    vec![Scope::Personal, Scope::Project]
                } else {
                    vec![Scope::Project]
                };
                let mut text = String::new();
                for s in scopes {
                    let items = list_dir(&folder_of(s)?);
                    text.push_str(&format!("## {}\n{}\n\n", s.label(), index_lines(&items)));
                }
                Ok((text.trim_end().to_string(), json!({ "action": "list" })))
            }
            "delete" => {
                delete_in(&folder, arg(params, "name"))?;
                Ok((
                    format!(
                        "Deleted {} memory `{}`.",
                        scope.label(),
                        arg(params, "name")
                    ),
                    json!({ "action": "deleted", "memory": { "name": arg(params, "name"), "scope": scope.label() } }),
                ))
            }
            other => Err(format!("unknown action `{other}`")),
        };
        if let (Ok(_), Some((name, path)), Some(recorder)) = (&outcome, &watched, &self.recorder) {
            let after = std::fs::read_to_string(path).ok();
            if before != after {
                recorder(scope, name, before, after);
            }
        }
        outcome
    }
}

#[async_trait]
impl AgentTool for MemoryTool {
    fn schema(&self) -> &Tool {
        &self.schema
    }

    fn label(&self) -> &str {
        "memory"
    }

    fn execution_mode(&self) -> ToolExecutionMode {
        ToolExecutionMode::Sequential
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        params: Value,
        _signal: CancellationToken,
        _on_update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let (text, details) = self.run(&params).map_err(AgentError::Validation)?;
        Ok(AgentToolResult {
            content: vec![TextContentOrImage::text(text)],
            details,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: Switches = Switches {
        enabled: true,
        personal: true,
        sensitive: false,
    };

    /// A tool whose personal and project folders are under a scratch folder.
    struct Fixture {
        root: tempfile::TempDir,
        tool: MemoryTool,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                root: tempfile::tempdir().unwrap(),
                tool: MemoryTool {
                    schema: schema(),
                    cwd: PathBuf::from("/work/app"),
                    origin: Origin::Chat,
                    recorder: None,
                },
            }
        }

        fn call(&self, switches: Switches, params: Value) -> Result<(String, Value), String> {
            let root = self.root.path().to_path_buf();
            self.tool.run_with(
                &params,
                switches,
                &move |scope| Ok(root.join(scope.label())),
            )
        }
    }

    // docs/features/memory-topics-and-chat-recall/spec.md (FR-3, FR-5).
    #[test]
    fn memory_tool_update_changes_one_part_and_reports_the_lines() {
        let f = Fixture::new();
        let (text, details) = f
            .call(
                ON,
                json!({ "action": "save", "name": "Commit style", "type": "feedback",
                        "description": "How to write commits",
                        "content": "Small commits.\nConventional subjects." }),
            )
            .unwrap();
        assert_eq!(text, "Saved project memory `commit-style`.");
        assert_eq!(details["memory"]["by"], "chat");

        let (text, details) = f
            .call(
                ON,
                json!({ "action": "update", "name": "commit-style",
                        "old_text": "Small commits.", "new_text": "Small commits, one task each." }),
            )
            .unwrap();
        assert!(text.starts_with("Updated "), "{text}");
        assert_eq!(details["action"], "updated");
        assert_eq!(details["diff"]["removed"], json!(["Small commits."]));
        assert_eq!(
            details["diff"]["added"],
            json!(["Small commits, one task each."])
        );
        assert_eq!(
            details["memory"]["content"],
            "Small commits, one task each.\nConventional subjects."
        );
        assert_eq!(details["memory"]["description"], "How to write commits");

        // New text alone goes at the end.
        let (_, details) = f
            .call(
                ON,
                json!({ "action": "update", "name": "commit-style", "new_text": "No force pushes." }),
            )
            .unwrap();
        assert_eq!(details["diff"]["added"], json!(["No force pushes."]));
        assert_eq!(details["diff"]["removed"], json!([]));

        // Text that is not there fails and leaves the topic as it was.
        let error = f
            .call(
                ON,
                json!({ "action": "update", "name": "commit-style", "old_text": "Rebase daily.", "new_text": "x" }),
            )
            .unwrap_err();
        assert!(error.contains("not found"), "{error}");
        let (read, _) = f
            .call(ON, json!({ "action": "read", "name": "commit-style" }))
            .unwrap();
        assert!(
            read.ends_with("Conventional subjects.\nNo force pushes."),
            "{read}"
        );

        let schema = schema();
        let actions = &schema.parameters.as_value()["properties"]["action"]["enum"];
        assert_eq!(
            actions,
            &json!(["save", "update", "read", "list", "delete"])
        );
        let types = &schema.parameters.as_value()["properties"]["type"]["enum"];
        assert_eq!(types, &json!(["user", "feedback", "project", "reference"]));
    }

    // FR-17: a change marked sensitive needs the setting.
    #[test]
    fn memory_tool_update_refuses_sensitive_subjects_until_allowed() {
        let f = Fixture::new();
        let save = json!({ "action": "save", "scope": "personal", "name": "health", "type": "user",
                           "content": "Has a bad back.", "sensitive": true });
        let error = f.call(ON, save.clone()).unwrap_err();
        assert!(error.contains("**Include sensitive subjects**"), "{error}");
        assert!(f
            .call(
                ON,
                json!({ "action": "read", "scope": "personal", "name": "health" })
            )
            .is_err());

        let allowed = Switches {
            sensitive: true,
            ..ON
        };
        let (_, details) = f.call(allowed, save).unwrap();
        assert_eq!(details["sensitive"], true);
        assert_eq!(details["memory"]["sensitive"], true);
        // The mark stays on the topic through an ordinary update.
        let (_, details) = f
            .call(
                allowed,
                json!({ "action": "update", "scope": "personal", "name": "health", "new_text": "Stands at work." }),
            )
            .unwrap();
        assert_eq!(details["memory"]["sensitive"], true);
        // With the setting off again, a sensitive update is refused too.
        assert!(f
            .call(
                ON,
                json!({ "action": "update", "scope": "personal", "name": "health",
                        "new_text": "Sees a physio.", "sensitive": true }),
            )
            .is_err());
    }

    // FR-29: the index run hears each change, with the topic before and after.
    #[test]
    fn memory_tool_tells_the_recorder_what_changed() {
        type Heard = Vec<(String, String, Option<String>, Option<String>)>;
        let heard: Arc<std::sync::Mutex<Heard>> = Arc::default();
        let sink = heard.clone();
        let root = tempfile::tempdir().unwrap();
        let tool = index_memory_tool(
            PathBuf::from("/work/app"),
            Arc::new(move |scope, name, before, after| {
                sink.lock()
                    .unwrap()
                    .push((scope.label().into(), name.into(), before, after));
            }),
        );
        let folder = root.path().to_path_buf();
        let call = |params: Value| {
            let folder = folder.clone();
            tool.run_with(&params, ON, &move |scope| Ok(folder.join(scope.label())))
        };
        let (_, details) = call(json!({ "action": "save", "name": "Commit style", "type": "feedback", "content": "Small commits." })).unwrap();
        assert_eq!(details["memory"]["by"], "index");
        call(json!({ "action": "update", "name": "commit-style", "new_text": "No force pushes." }))
            .unwrap();
        // Reads, lists and failed changes are not changes.
        call(json!({ "action": "read", "name": "commit-style" })).unwrap();
        call(json!({ "action": "list" })).unwrap();
        assert!(call(json!({ "action": "update", "name": "commit-style", "old_text": "absent", "new_text": "x" })).is_err());
        call(json!({ "action": "delete", "scope": "project", "name": "commit-style" })).unwrap();

        let heard = heard.lock().unwrap();
        assert_eq!(heard.len(), 3);
        let (scope, name, before, after) = &heard[0];
        assert_eq!(
            (scope.as_str(), name.as_str(), before),
            ("project", "commit-style", &None)
        );
        assert!(after.as_deref().unwrap().contains("Small commits."));
        assert_eq!(
            heard[1].2, heard[0].3,
            "an update starts from what the save left"
        );
        assert!(heard[1].3.as_deref().unwrap().contains("No force pushes."));
        assert_eq!(
            (heard[2].2.as_ref(), &heard[2].3),
            (heard[1].3.as_ref(), &None)
        );
    }

    // FR-7, FR-8: the personal switch and the pause.
    #[test]
    fn memory_tool_follows_the_personal_switch_and_the_pause() {
        let f = Fixture::new();
        for scope in ["personal", "project"] {
            f.call(
                ON,
                json!({ "action": "save", "scope": scope, "name": format!("{scope}-topic"),
                        "type": "user", "content": "Text." }),
            )
            .unwrap();
        }
        let (both, _) = f.call(ON, json!({ "action": "list" })).unwrap();
        assert!(both.contains("## personal") && both.contains("## project"));

        let alone = Switches {
            personal: false,
            ..ON
        };
        let (listed, _) = f.call(alone, json!({ "action": "list" })).unwrap();
        assert!(!listed.contains("personal-topic") && listed.contains("project-topic"));
        for action in ["save", "update", "read", "delete", "list"] {
            let error = f
                .call(
                    alone,
                    json!({ "action": action, "scope": "personal", "name": "personal-topic",
                            "type": "user", "content": "x", "new_text": "x" }),
                )
                .unwrap_err();
            assert!(error.contains("Use personal memory"), "{action}: {error}");
        }
        // The project scope still works there.
        f.call(alone, json!({ "action": "read", "name": "project-topic" }))
            .unwrap();

        let paused = Switches {
            enabled: false,
            ..ON
        };
        assert!(f
            .call(paused, json!({ "action": "list" }))
            .unwrap_err()
            .contains("switched off"));
    }
}
