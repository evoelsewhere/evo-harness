//! Hooks for this session (`evo-hooks`): which ones apply, and the points
//! where the engine runs them.
//!
//! * `SessionStart` — once per engine, when its session is set up (`startup`,
//!   or `resume` for a saved one), in the background so the engine is ready
//!   at once; the first prompt waits for it and takes its context. A
//!   plugin's `SessionStart` hook may run before the desktop's
//!   `set_plugin_host`, so without its host key.
//! * `UserPromptSubmit` — before a prompt runs: a hook can block it, or add
//!   context (sent with the prompt in a `<hook-context>` block).
//! * `PreToolUse` — before a tool runs (`before_tool_call`): block it (the
//!   reason becomes the tool's error), ask the person first (the same
//!   approval prompt extensions use), allow it, or rewrite its input.
//! * `PostToolUse` / `PostToolUseFailure` — after it ran (`after_tool_call`):
//!   feedback and context are added to the tool's result, and a
//!   `PostToolUse` hook may replace what the agent sees
//!   (`updatedToolOutput`; docs/features/code-mode/spec.md, FR-21).
//! * `Stop` — when a run ends; `SessionEnd` — when the engine shuts down.
//! * `SubagentStop` — when a subagent's run ends; it cannot block and its
//!   output is dropped (docs/features/session-roles-and-mail/spec.md, FR-10).
//!
//! Hooks come from `hooks` in `~/.evo/agent/settings.json` and, when the
//! project is trusted, `.evo/settings.json`. `disableAllHooks: true` (user
//! settings) turns them all off. The tool-call hooks are installed when the
//! session starts with hooks for those events; the others follow the
//! settings as they are now (RPC `reload_resources`).
//!
//! Plugins' hooks run only when the person approved `hooks` (the plugin
//! store gates them). Their commands get `EVO_PLUGIN_HOST` and
//! `EVO_PLUGIN_KEY` for the plugin host API, which the desktop sends with
//! `set_plugin_host` (docs/features/plugin-settings/spec.md, FR-8, FR-26):
//! only in that plugin's hook processes, never in the engine's environment,
//! which the agent's shell commands inherit.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use evo_hooks::{Hooks, Outcome, RunRecord};
use serde_json::{json, Value};

/// Runs kept for Customize → Hooks.
const RECENT: usize = 100;

struct Runtime {
    cwd: PathBuf,
    trusted: bool,
    session_id: Mutex<String>,
    hooks: RwLock<Hooks>,
    recent: Mutex<VecDeque<RunRecord>>,
    /// `SessionStart` context waiting for the first prompt.
    pending_context: Mutex<Vec<String>>,
    /// `SessionStart` ran or runs (once per engine).
    started: AtomicBool,
    /// Its run, which the first prompt waits for.
    session_start: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Runtime {
    fn new(cwd: &Path, trusted: bool, hooks: Hooks) -> Runtime {
        Runtime {
            cwd: cwd.to_path_buf(),
            trusted,
            session_id: Mutex::new(String::new()),
            hooks: RwLock::new(hooks),
            recent: Mutex::new(VecDeque::new()),
            pending_context: Mutex::new(Vec::new()),
            started: AtomicBool::new(false),
            session_start: Mutex::new(None),
        }
    }

    fn has(&self, event: &str) -> bool {
        let hooks = self.hooks.read().unwrap();
        !hooks.disabled && hooks.entries.iter().any(|e| e.event == event)
    }
}

static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();

/// The plugin host API: its URL and a key per plugin id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginHost {
    pub url: String,
    pub keys: std::collections::HashMap<String, String>,
}

static PLUGIN_HOST: RwLock<Option<PluginHost>> = RwLock::new(None);

/// `set_plugin_host`: keys for the plugins' hooks from now on.
pub fn set_plugin_host(host: PluginHost) {
    *PLUGIN_HOST.write().unwrap() = Some(host);
}

/// The environment of each plugin's hook commands, by hook source
/// (`plugin:<name>`), for the plugins in `plugins`.
fn plugin_env(
    plugins: &[evo_plugins::Plugin],
) -> std::collections::HashMap<String, Vec<(String, String)>> {
    let Some(host) = PLUGIN_HOST.read().unwrap().clone() else {
        return Default::default();
    };
    plugins
        .iter()
        .filter_map(|plugin| {
            let key = host.keys.get(&plugin.id)?;
            Some((
                format!("plugin:{}", plugin.name),
                vec![
                    ("EVO_PLUGIN_HOST".to_string(), host.url.clone()),
                    ("EVO_PLUGIN_KEY".to_string(), key.clone()),
                ],
            ))
        })
        .collect()
}

/// The session's folder, once [`init`] ran.
pub fn cwd() -> Option<PathBuf> {
    RUNTIME.get().map(|rt| rt.cwd.clone())
}

/// Whether the session's folder is trusted (its project files are read).
pub fn trusted() -> bool {
    RUNTIME.get().is_some_and(|rt| rt.trusted)
}

fn user_settings() -> Option<PathBuf> {
    crate::config::settings_path().ok()
}

fn project_settings(cwd: &Path, trusted: bool) -> Option<PathBuf> {
    trusted.then(|| crate::resource_dirs::project_config_file(cwd, "settings.json"))
}

fn load(cwd: &Path, trusted: bool) -> Hooks {
    let mut hooks = load_settings(cwd, trusted);
    // Enabled plugins' hooks/hooks.json (and inline hooks).
    for (plugin, events) in crate::plugins::hooks() {
        hooks.add_events(
            &events,
            &format!("plugin:{plugin}"),
            &format!("plugin {plugin}"),
        );
    }
    hooks
}

fn load_settings(cwd: &Path, trusted: bool) -> Hooks {
    Hooks::load(
        user_settings().as_deref(),
        project_settings(cwd, trusted).as_deref(),
    )
}

/// Load this session's hooks (once per engine process).
pub fn init(cwd: &Path, trusted: bool) {
    let _ = RUNTIME.set(Arc::new(Runtime::new(cwd, trusted, load(cwd, trusted))));
}

pub fn set_session_id(id: &str) {
    if let Some(rt) = RUNTIME.get() {
        *rt.session_id.lock().unwrap() = id.to_string();
    }
}

/// Read the settings again.
pub fn reload() {
    if let Some(rt) = RUNTIME.get() {
        *rt.hooks.write().unwrap() = load(&rt.cwd, rt.trusted);
    }
}

/// Whether any hook is configured for `event` (and hooks are on).
pub fn has(event: &str) -> bool {
    RUNTIME.get().is_some_and(|rt| rt.has(event))
}

/// Run `event`'s hooks with `input`; records the runs.
pub async fn run(event: &str, target: Option<&str>, input: Value) -> Outcome {
    let Some(rt) = RUNTIME.get().cloned() else {
        return Outcome::default();
    };
    run_in(&rt, event, target, input).await
}

async fn run_in(rt: &Runtime, event: &str, target: Option<&str>, mut input: Value) -> Outcome {
    let mut hooks = rt.hooks.read().unwrap().clone();
    hooks.env = plugin_env(&crate::plugins::enabled());
    if let Some(object) = input.as_object_mut() {
        object.insert("session_id".into(), json!(*rt.session_id.lock().unwrap()));
    }
    let outcome = evo_hooks::run_event(&hooks, event, target, input, &rt.cwd).await;
    if !outcome.records.is_empty() {
        let mut recent = rt.recent.lock().unwrap();
        for record in &outcome.records {
            if recent.len() >= RECENT {
                recent.pop_front();
            }
            recent.push_back(record.clone());
        }
    }
    for message in &outcome.messages {
        tracing::info!(event, "hook: {message}");
    }
    outcome
}

/// Everything for Customize → Hooks: the hooks, problems, and recent runs.
pub fn describe() -> Value {
    let Some(rt) = RUNTIME.get() else {
        return json!({ "entries": [], "disabled": false, "problems": [], "recent": [], "events": evo_hooks::EVENTS });
    };
    let hooks = rt.hooks.read().unwrap();
    let recent: Vec<RunRecord> = rt.recent.lock().unwrap().iter().rev().cloned().collect();
    json!({
        "entries": hooks.entries,
        "disabled": hooks.disabled,
        "problems": hooks.problems,
        "recent": recent,
        "events": evo_hooks::EVENTS,
        "userSettings": user_settings(),
        "projectSettings": project_settings(&rt.cwd, rt.trusted),
    })
}

/// Turn every hook off (or back on) in user settings.
pub fn set_disabled(disabled: bool) -> Result<(), String> {
    crate::settings::update_settings_value(|settings| {
        if disabled {
            settings.insert("disableAllHooks".into(), json!(true));
        } else {
            settings.remove("disableAllHooks");
        }
    })?;
    reload();
    Ok(())
}

// ── Session start / prompt / stop / end ─────────────────────────────────────

/// `SessionStart` in the background, once per engine (`source`: `startup`,
/// or `resume` for a saved session); its context is kept for the first prompt.
pub fn session_start(source: &str) {
    if let Some(rt) = RUNTIME.get().cloned() {
        session_start_in(rt, source);
    }
}

fn session_start_in(rt: Arc<Runtime>, source: &str) {
    if rt.started.swap(true, Ordering::SeqCst) || !rt.has("SessionStart") {
        return;
    }
    let input = json!({ "source": source });
    let task = rt.clone();
    let handle = tokio::spawn(async move {
        let outcome = run_in(&task, "SessionStart", None, input).await;
        task.pending_context.lock().unwrap().extend(outcome.context);
    });
    *rt.session_start.lock().unwrap() = Some(handle);
}

/// The `SessionStart` context, once its hooks are done.
async fn start_context(rt: &Runtime) -> Vec<String> {
    let running = rt.session_start.lock().unwrap().take();
    if let Some(running) = running {
        let _ = running.await;
    }
    std::mem::take(&mut *rt.pending_context.lock().unwrap())
}

/// `UserPromptSubmit` for `text`: `Err(reason)` when a hook blocks it,
/// otherwise the context to send along (with any `SessionStart` context).
pub async fn prompt_submit(text: &str) -> Result<Vec<String>, String> {
    let mut context: Vec<String> = match RUNTIME.get().cloned() {
        Some(rt) => start_context(&rt).await,
        None => Vec::new(),
    };
    if has("UserPromptSubmit") {
        let outcome = run("UserPromptSubmit", None, json!({ "prompt": text })).await;
        if let Some(reason) = outcome.block.or(outcome.stop) {
            return Err(reason);
        }
        context.extend(outcome.context);
    }
    Ok(context)
}

/// The block the context rides in, after the prompt.
pub fn context_block(context: &[String]) -> Option<String> {
    (!context.is_empty())
        .then(|| format!("<hook-context>\n{}\n</hook-context>", context.join("\n\n")))
}

/// `Stop`, after a run (in the background).
pub fn stop(outcome: &str) {
    if !has("Stop") {
        return;
    }
    let outcome = outcome.to_string();
    tokio::spawn(async move {
        run(
            "Stop",
            None,
            json!({ "outcome": outcome, "stop_hook_active": false }),
        )
        .await;
    });
}

/// Characters of a subagent's report a `SubagentStop` hook gets.
pub const SUBAGENT_REPORT_CHARS: usize = 2_000;

/// `SubagentStop`, after a subagent's run ended (in the background). The
/// matcher is the agent type; the outcome is not used: it cannot block.
pub fn subagent_stop(snapshot: &Value, report: &str) {
    if !has("SubagentStop") {
        return;
    }
    let input = subagent_stop_input(snapshot, report);
    let agent_type = input["agent_type"].as_str().map(str::to_string);
    tokio::spawn(async move {
        run("SubagentStop", agent_type.as_deref(), input).await;
    });
}

/// A `SubagentStop` hook's input from a subagent's settled snapshot.
pub fn subagent_stop_input(snapshot: &Value, report: &str) -> Value {
    let status = match snapshot["status"].as_str() {
        Some("done") => "finished",
        Some("cancelled") => "cancelled",
        Some("stopped") => "stopped",
        _ => "failed",
    };
    let worktree = snapshot
        .get("worktree")
        .filter(|w| w.get("kept").and_then(Value::as_bool).unwrap_or(true) && !w.is_null())
        .map(|w| json!({ "path": w["path"], "branch": w["branch"] }));
    json!({
        "agent_id": snapshot["agentId"],
        "agent_type": snapshot["agentType"],
        "description": snapshot["description"],
        "status": status,
        "report": report.chars().take(SUBAGENT_REPORT_CHARS).collect::<String>(),
        "worktree": worktree,
    })
}

/// `SessionEnd`, with a short budget (the engine is closing).
pub async fn session_end(reason: &str) {
    if !has("SessionEnd") {
        return;
    }
    let _ = tokio::time::timeout(
        Duration::from_millis(1500),
        run("SessionEnd", None, json!({ "reason": reason })),
    )
    .await;
}

// ── Tool calls ──────────────────────────────────────────────────────────────

/// Ask the person to allow a call (the approval prompt in the chat input).
pub(crate) async fn confirm(
    ui_dialog: &evo_extensions::UiDialogMailbox,
    tool_call_id: &str,
    tool: &str,
    reason: &str,
    signal: &tokio_util::sync::CancellationToken,
) -> bool {
    let request_id = format!("hook:{tool_call_id}");
    let opened = ui_dialog.handle(json!({
        "op": "open",
        "requestId": request_id,
        "toolCallId": tool_call_id,
        "ui": {
            "kind": "confirm",
            "title": format!("Allow {tool}?"),
            "description": reason,
            "options": [{ "title": "Allow" }, { "title": "Deny" }],
        },
    }));
    if opened.is_err() {
        // No one to ask (headless): a hook asking means no.
        return false;
    }
    loop {
        if signal.is_cancelled() {
            let _ = ui_dialog.cancel(&request_id);
            return false;
        }
        match ui_dialog.poll(&request_id) {
            Ok(state) => match state.get("status").and_then(Value::as_str) {
                Some("answered") => {
                    let answer = state.get("answer").cloned().unwrap_or(Value::Null);
                    let index = answer.get("index").and_then(Value::as_u64);
                    let value = answer.get("value").and_then(Value::as_str).unwrap_or("");
                    return index == Some(0) || value.eq_ignore_ascii_case("allow");
                }
                Some("cancelled") => return false,
                _ => {}
            },
            Err(_) => return false,
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `PreToolUse` in front of the engine's other approval hook (`previous`).
/// Installed only when the session starts with `PreToolUse` hooks.
pub fn before_tool_call(
    previous: Option<evo_agent::BeforeToolCall>,
    ui_dialog: evo_extensions::UiDialogMailbox,
) -> Option<evo_agent::BeforeToolCall> {
    if !has("PreToolUse") {
        return previous;
    }
    Some(Arc::new(
        move |ctx: evo_agent::BeforeToolCallContext<'_>, signal| {
            let tool = ctx.tool_call.name.clone();
            let id = ctx.tool_call.id.clone();
            let args = ctx.args.clone();
            let earlier = previous.as_ref().map(|hook| hook(ctx, signal.clone()));
            let ui_dialog = ui_dialog.clone();
            Box::pin(async move {
                let input = json!({ "tool_name": tool, "tool_input": args, "tool_use_id": id });
                let outcome = run("PreToolUse", Some(&tool), input).await;
                let block = |reason: String| {
                    Some(evo_agent::BeforeToolCallResult {
                        block: true,
                        reason: Some(reason),
                        terminate: false,
                        args: None,
                    })
                };
                if let Some(reason) = outcome.block.or(outcome.stop) {
                    return block(format!("Blocked by a hook: {reason}"));
                }
                if let Some(reason) = outcome.ask.filter(|_| !outcome.allow) {
                    if !confirm(&ui_dialog, &id, &tool, &reason, &signal).await {
                        return block("The user declined this tool call.".into());
                    }
                    // The person answered once: the approval mode does not ask again.
                    crate::approval::global().cover(&id);
                } else if outcome.allow {
                    // An explicit `allow` skips the mode's prompt.
                    crate::approval::global().cover(&id);
                }
                if let Some(earlier) = earlier {
                    if let Some(result) = earlier.await {
                        if result.block {
                            return Some(result);
                        }
                    }
                }
                outcome
                    .updated_input
                    .map(|input| evo_agent::BeforeToolCallResult {
                        block: false,
                        reason: None,
                        terminate: false,
                        args: Some(input),
                    })
            })
        },
    ))
}

fn result_text(result: &evo_agent::types::AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|part| match part {
            evo_agent::types::TextContentOrImage::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// How a tool with a structured response shows it to `PostToolUse` hooks and
/// takes a hook's changed copy back (docs/features/code-mode/spec.md, FR-17,
/// FR-21). Tools without one show their text and take a string.
#[derive(Clone, Copy)]
pub struct OutputView {
    /// What hooks get as `tool_response`; `None` falls back to the text.
    pub view: fn(&evo_agent::types::AgentToolResult) -> Option<Value>,
    /// The result rebuilt from a hook's copy; `None` means the copy is
    /// malformed and the original output stays.
    pub apply:
        fn(&evo_agent::types::AgentToolResult, &Value) -> Option<evo_agent::types::AgentToolResult>,
}

type OutputViews = RwLock<std::collections::HashMap<&'static str, OutputView>>;

fn output_views() -> &'static OutputViews {
    static VIEWS: OnceLock<OutputViews> = OnceLock::new();
    VIEWS.get_or_init(Default::default)
}

/// Register `tool`'s structured view for hooks (once, when the tool is built).
pub fn register_output_view(tool: &'static str, view: OutputView) {
    output_views()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .insert(tool, view);
}

fn output_view(tool: &str) -> Option<OutputView> {
    output_views()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(tool)
        .copied()
}

/// `tool_response` for the tool hooks: the tool's structured view, else its text.
fn tool_response(tool: &str, result: &evo_agent::types::AgentToolResult) -> Value {
    output_view(tool)
        .and_then(|view| (view.view)(result))
        .unwrap_or_else(|| json!(result_text(result)))
}

/// The result after a hook's `updatedToolOutput`, or `None` when there is
/// none or it does not fit: a string replaces the text and keeps the images;
/// an object goes through the tool's [`OutputView`]; `PostToolUseFailure`
/// never replaces (FR-21).
fn replaced_output(
    event: &str,
    tool: &str,
    result: &evo_agent::types::AgentToolResult,
    updated: Option<&Value>,
) -> Option<evo_agent::types::AgentToolResult> {
    use evo_agent::types::TextContentOrImage;
    if event != "PostToolUse" {
        return None;
    }
    match updated? {
        Value::String(text) => {
            let mut replaced = result.clone();
            replaced.content = std::iter::once(TextContentOrImage::text(text.clone()))
                .chain(
                    result
                        .content
                        .iter()
                        .filter(|part| matches!(part, TextContentOrImage::Image(_)))
                        .cloned(),
                )
                .collect();
            Some(replaced)
        }
        copy @ Value::Object(_) => output_view(tool).and_then(|view| (view.apply)(result, copy)),
        _ => None,
    }
}

/// What `after_tool_call` changes for one hook outcome: the replaced output
/// (FR-21), then the hooks' feedback and context, with their messages in the
/// result's details for the app. `None` when nothing changes.
fn post_tool_result(
    event: &str,
    tool: &str,
    result: &evo_agent::types::AgentToolResult,
    outcome: &Outcome,
) -> Option<evo_agent::types::AfterToolCallResult> {
    let replaced = replaced_output(event, tool, result, outcome.updated_output.as_ref());
    let mut notes = Vec::new();
    if let Some(reason) = &outcome.block {
        notes.push(format!("Hook feedback: {reason}"));
    }
    notes.extend(outcome.context.iter().map(|c| format!("Hook context: {c}")));
    if replaced.is_none() && notes.is_empty() && outcome.messages.is_empty() {
        return None;
    }
    let was_replaced = replaced.is_some();
    let base = replaced.unwrap_or_else(|| result.clone());
    let mut content = base.content;
    if !notes.is_empty() {
        content.push(evo_agent::types::TextContentOrImage::text(notes.join("\n")));
    }
    let mut details = if base.details.is_object() {
        base.details
    } else {
        json!({})
    };
    details["hooks"] = json!({ "event": event, "messages": outcome.messages, "notes": notes });
    if was_replaced {
        details["hooks"]["replacedOutput"] = json!(true);
    }
    Some(evo_agent::types::AfterToolCallResult {
        content: Some(content),
        details: Some(details),
        ..Default::default()
    })
}

/// `PostToolUse` / `PostToolUseFailure`: see [`post_tool_result`].
pub fn after_tool_call() -> Option<evo_agent::AfterToolCall> {
    if !has("PostToolUse") && !has("PostToolUseFailure") {
        return None;
    }
    Some(Arc::new(
        move |ctx: evo_agent::AfterToolCallContext<'_>, _signal| {
            let event = if ctx.is_error {
                "PostToolUseFailure"
            } else {
                "PostToolUse"
            };
            let tool = ctx.tool_call.name.clone();
            let input = json!({
                "tool_name": tool,
                "tool_input": ctx.args,
                "tool_use_id": ctx.tool_call.id,
                "tool_response": tool_response(&tool, ctx.result),
                "is_error": ctx.is_error,
            });
            let result = ctx.result.clone();
            Box::pin(async move {
                let outcome = run(event, Some(&tool), input).await;
                post_tool_result(event, &tool, &result, &outcome)
            })
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // docs/features/code-mode/spec.md (FR-17, FR-21).
    mod output {
        use super::*;
        use evo_agent::types::{AgentToolResult, TextContentOrImage};
        use evo_ai::types::{ImageContent, ImageContentType};

        fn image() -> TextContentOrImage {
            TextContentOrImage::Image(ImageContent {
                kind: ImageContentType,
                data: "aGk=".into(),
                mime_type: "image/png".into(),
            })
        }

        fn result() -> AgentToolResult {
            AgentToolResult {
                content: vec![TextContentOrImage::text("token abc123"), image()],
                details: json!({ "exitCode": 0 }),
                ..Default::default()
            }
        }

        fn outcome(updated: Value) -> Outcome {
            Outcome {
                updated_output: Some(updated),
                ..Default::default()
            }
        }

        fn texts(content: &[TextContentOrImage]) -> Vec<String> {
            content
                .iter()
                .filter_map(|part| match part {
                    TextContentOrImage::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect()
        }

        #[test]
        fn a_string_replaces_the_text_and_keeps_images() {
            let changed = post_tool_result(
                "PostToolUse",
                "bash",
                &result(),
                &outcome(json!("[hidden]")),
            )
            .expect("replaced");
            let content = changed.content.unwrap();
            assert_eq!(texts(&content), vec!["[hidden]".to_string()]);
            assert!(content
                .iter()
                .any(|part| matches!(part, TextContentOrImage::Image(_))));
            let details = changed.details.unwrap();
            assert_eq!(details["exitCode"], 0);
            assert_eq!(details["hooks"]["replacedOutput"], true);
        }

        #[test]
        fn other_values_and_failures_leave_the_output() {
            let unchanged = |event: &str, tool: &str, value: Value| {
                post_tool_result(event, tool, &result(), &outcome(value)).is_none()
            };
            assert!(unchanged("PostToolUse", "bash", json!(3)));
            // No structured view for `bash`: an object does not fit.
            assert!(unchanged("PostToolUse", "bash", json!({ "result": "x" })));
            assert!(unchanged("PostToolUseFailure", "bash", json!("[hidden]")));
        }

        fn view(result: &AgentToolResult) -> Option<Value> {
            Some(json!({ "result": result_text(result), "stdout": "" }))
        }

        fn apply(_: &AgentToolResult, copy: &Value) -> Option<AgentToolResult> {
            let text = copy.get("result")?.as_str()?;
            copy.get("stdout")?.as_str()?;
            Some(AgentToolResult {
                content: vec![TextContentOrImage::text(text)],
                details: json!({ "structured": true }),
                ..Default::default()
            })
        }

        #[test]
        fn an_object_goes_through_the_tools_output_view() {
            register_output_view("structured_test_tool", OutputView { view, apply });
            assert_eq!(
                tool_response("structured_test_tool", &result()),
                json!({ "result": "token abc123", "stdout": "" })
            );
            assert_eq!(tool_response("bash", &result()), json!("token abc123"));

            let changed = post_tool_result(
                "PostToolUse",
                "structured_test_tool",
                &result(),
                &outcome(json!({ "result": "[hidden]", "stdout": "" })),
            )
            .expect("replaced");
            assert_eq!(
                texts(&changed.content.unwrap()),
                vec!["[hidden]".to_string()]
            );
            assert_eq!(changed.details.unwrap()["structured"], true);

            // A malformed copy is ignored.
            assert!(post_tool_result(
                "PostToolUse",
                "structured_test_tool",
                &result(),
                &outcome(json!({ "result": 1 })),
            )
            .is_none());
        }

        #[test]
        fn notes_follow_the_replaced_output() {
            let mut out = outcome(json!("[hidden]"));
            out.context.push("checked".into());
            let changed = post_tool_result("PostToolUse", "bash", &result(), &out).unwrap();
            assert_eq!(
                texts(&changed.content.unwrap()),
                vec!["[hidden]".to_string(), "Hook context: checked".to_string()]
            );
        }
    }

    // docs/features/session-roles-and-mail/spec.md (FR-10).
    #[test]
    fn subagent_stop_hook_input_names_the_agent_its_status_and_a_kept_worktree() {
        let snapshot = json!({
            "agentId": "a_7", "agentType": "explore", "description": "Find the parser",
            "status": "done",
            "worktree": { "path": "/wt/x", "branch": "evo/x", "kept": true, "changed": true },
        });
        let long = "r".repeat(SUBAGENT_REPORT_CHARS + 50);
        let input = subagent_stop_input(&snapshot, &long);
        assert_eq!(input["agent_id"], "a_7");
        assert_eq!(input["agent_type"], "explore");
        assert_eq!(input["description"], "Find the parser");
        assert_eq!(input["status"], "finished");
        assert_eq!(
            input["report"].as_str().unwrap().chars().count(),
            SUBAGENT_REPORT_CHARS
        );
        assert_eq!(
            input["worktree"],
            json!({ "path": "/wt/x", "branch": "evo/x" })
        );
        for (status, said) in [
            ("error", "failed"),
            ("cancelled", "cancelled"),
            ("stopped", "stopped"),
        ] {
            let mut ended = snapshot.clone();
            ended["status"] = json!(status);
            ended["worktree"] = json!({ "path": "/wt/x", "branch": "evo/x", "kept": false });
            let input = subagent_stop_input(&ended, "x");
            assert_eq!(input["status"], said);
            assert!(
                input["worktree"].is_null(),
                "a removed worktree is not reported"
            );
        }
    }

    // docs/features/plugin-settings/spec.md (FR-26).
    #[test]
    fn each_plugins_hooks_get_only_its_own_key() {
        let plugin = |id: &str, name: &str| evo_plugins::Plugin {
            id: id.into(),
            name: name.into(),
            ..Default::default()
        };
        let plugins = [plugin("a@m", "a"), plugin("b@m", "b")];
        assert!(
            plugin_env(&plugins).is_empty(),
            "nothing before the desktop sends it"
        );
        set_plugin_host(PluginHost {
            url: "http://127.0.0.1:9".into(),
            keys: [("a@m".to_string(), "key-a".to_string())].into(),
        });
        let env = plugin_env(&plugins);
        assert_eq!(
            env["plugin:a"],
            vec![
                (
                    "EVO_PLUGIN_HOST".to_string(),
                    "http://127.0.0.1:9".to_string()
                ),
                ("EVO_PLUGIN_KEY".to_string(), "key-a".to_string()),
            ]
        );
        assert!(!env.contains_key("plugin:b"), "no key, no access");
        assert!(
            std::env::var("EVO_PLUGIN_KEY").is_err(),
            "never in the engine's environment"
        );
    }

    /// A `SessionStart` hook printing `start:<source>`.
    fn start_hook() -> evo_hooks::HookEntry {
        let (command, args) = if cfg!(windows) {
            ("powershell", vec!["-NoProfile", "-NonInteractive", "-Command",
                "$in = [Console]::In.ReadToEnd() | ConvertFrom-Json; [Console]::Out.Write('start:' + $in.source)"])
        } else {
            (
                "sh",
                vec!["-c", r#"sed -n 's/.*"source":"\([a-z]*\)".*/start:\1/p'"#],
            )
        };
        evo_hooks::HookEntry {
            event: "SessionStart".into(),
            matcher: None,
            handler: evo_hooks::Handler::Command {
                command: command.into(),
                args: Some(args.into_iter().map(str::to_string).collect()),
                shell: None,
                timeout: Some(60),
            },
            source: "user".into(),
            file: "settings.json".into(),
        }
    }

    // docs/features/customize-skills-plugins-connectors/spec.md (FR-31).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn session_start_runs_once_and_its_context_waits_for_the_first_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = Hooks {
            entries: vec![start_hook()],
            ..Default::default()
        };
        let rt = Arc::new(Runtime::new(dir.path(), false, hooks));
        session_start_in(rt.clone(), "resume");
        session_start_in(rt.clone(), "startup");
        assert_eq!(start_context(&rt).await, vec!["start:resume".to_string()]);
        assert!(
            start_context(&rt).await.is_empty(),
            "only the first prompt gets it"
        );
        assert_eq!(rt.recent.lock().unwrap().len(), 1, "it ran once");
    }

    #[tokio::test]
    async fn no_session_start_hook_runs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let rt = Arc::new(Runtime::new(dir.path(), false, Hooks::default()));
        session_start_in(rt.clone(), "startup");
        assert!(rt.session_start.lock().unwrap().is_none());
        assert!(start_context(&rt).await.is_empty());
    }

    #[test]
    fn subagent_stop_hook_is_a_known_event() {
        assert!(evo_hooks::EVENTS.contains(&"SubagentStop"));
    }
}
