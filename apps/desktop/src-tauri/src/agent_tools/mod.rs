//! Agent tools the shell answers itself (ADR 0013).
//!
//! An engine asks for an app-side tool with an `external_tool_request` line.
//! [`on_line`] (called from the engine's reader loop, beside the mesh hub) looks
//! the tool up in the registry, runs it with the chat's [`ToolCtx`] and writes
//! the `external_tool_response`. A call the engine drops (`external_tool_request_closed`)
//! is cancelled. The webview only shows what the tools do (pages, shells,
//! panels) and the questions they raise.
//!
//! Spec: docs/features/agent-browser (FR-22, FR-24), plan 3.1 – 3.5.

pub mod app;
pub mod browser;
pub mod activity;
pub mod computer;
pub mod gate;
pub mod mcp;
pub mod scheduler;
pub mod terminal;
pub mod user;

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State, Window};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::db::Db;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// What a tool does, in the words the engine's approval modes use (ADR 0014); the
/// gate asks by class. The shell adds one rule the engine does not have: an origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Only reads: never asks.
    Read,
    /// Moves the page on (clicks, typing, navigation). Like the engine's edits it asks
    /// only in "Ask every time", and only for a site that is not on this machine.
    Edit,
    /// Alters something the page or the person keeps (uploads, storage, mocks, scripts,
    /// commands). Like the engine's commands it asks in "Ask every time" and "Approve edits".
    Command,
    /// Acts in another app on the computer (computer app control, ADR 0015). With the
    /// feature's "Ask every time" it asks in every mode; otherwise like a command. The
    /// tool gates itself with [`ToolCtx::gate_control`].
    Control,
}

/// One part of a tool's result.
#[derive(Debug, Clone)]
pub enum Content {
    Text(String),
    Image { data: String, mime_type: String },
}

/// A tool's result, as the engine reads it.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: Vec<Content>,
    pub is_error: bool,
    /// Data beside the text (`structuredContent`): what the transcript renders, e.g. the file cards.
    pub structured: Option<Value>,
}

/// An image at 75% of each side, as JPEG; `None` when it cannot be decoded or is already tiny.
fn shrink_image(data: &str) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let bytes = B64.decode(data).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let (w, h) = (img.width() * 3 / 4, img.height() * 3 / 4);
    if w < 200 || h < 200 {
        return None;
    }
    let rgb = img.resize_exact(w, h, image::imageops::FilterType::Triangle).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 70).encode_image(&rgb).ok()?;
    Some(B64.encode(out))
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self { content: vec![Content::Text(text.into())], is_error: false, structured: None }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self { content: vec![Content::Text(text.into())], is_error: true, structured: None }
    }

    /// The same result with `structuredContent` attached.
    pub fn with_structured(mut self, structured: Value) -> Self {
        self.structured = Some(structured);
        self
    }

    /// An image followed by a line of text.
    pub fn image(data: String, mime_type: &str, caption: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Image { data, mime_type: mime_type.into() }, Content::Text(caption.into())],
            is_error: false,
            structured: None,
        }
    }

    /// The result with its frame under the engine's limit: images go largest first
    /// (replaced by a note), then text is cut. The engine drops a connection on an oversized frame.
    fn fit(mut self, limit: usize) -> Self {
        let size = |o: &Self| o.to_json().to_string().len();
        while size(&self) > limit {
            let biggest = self
                .content
                .iter()
                .enumerate()
                .filter_map(|(i, c)| match c {
                    Content::Image { data, .. } => Some((i, data.len())),
                    _ => None,
                })
                .max_by_key(|(_, len)| *len);
            if let Some((i, _)) = biggest {
                if let Content::Image { data, mime_type } = &self.content[i] {
                    if let Some(smaller) = shrink_image(data) {
                        self.content[i] = Content::Image { data: smaller, mime_type: "image/jpeg".into() };
                        continue;
                    }
                }
                self.content[i] = Content::Text("[image omitted: it exceeded the engine's frame size limit]".into());
                continue;
            }
            let over = size(&self) - limit;
            let mut cut = false;
            for part in self.content.iter_mut() {
                if let Content::Text(text) = part {
                    if text.len() > over + 64 {
                        let mut end = text.len() - over - 64;
                        while !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        text.truncate(end);
                        text.push_str("
[output truncated]");
                        cut = true;
                        break;
                    }
                }
            }
            if !cut {
                self.structured = None;
                if size(&self) > limit {
                    self.content = vec![Content::Text("[result omitted: too large]".into())];
                }
                break;
            }
        }
        self
    }

    fn to_json(&self) -> Value {
        let content: Vec<Value> = self
            .content
            .iter()
            .map(|part| match part {
                Content::Text(text) => json!({ "type": "text", "text": text }),
                Content::Image { data, mime_type } => json!({ "type": "image", "data": data, "mimeType": mime_type }),
            })
            .collect();
        let mut result = json!({ "content": content });
        if let Some(structured) = &self.structured {
            result["structuredContent"] = structured.clone();
        }
        if self.is_error {
            result["isError"] = json!(true);
        }
        result
    }
}

/// A tool: its definition for the engine, its class for the gate, and its body.
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    /// The JSON schema of its arguments.
    pub schema: fn() -> Value,
    pub class: Class,
    /// The body. A tool that is not `Read` must call [`ToolCtx::gate`] once it
    /// knows the page or the folder it acts on, before it changes anything.
    pub run: fn(ToolCtx, Value) -> BoxFuture<ToolOutput>,
}

/// Every tool the shell answers. Each group adds its `tools()` here.
fn registry() -> &'static Vec<Tool> {
    static REGISTRY: OnceLock<Vec<Tool>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut tools = Vec::new();
        // Groups register here, e.g. `tools.extend(browser::tools());`
        tools.extend(app::tools());
        tools.extend(browser::tools());
        tools.extend(computer::tool::tools());
        tools.extend(scheduler::tools());
        tools.extend(terminal::tools());
        tools.extend(user::tools());
        tools.shrink_to_fit();
        tools
    })
}

fn find(name: &str) -> Option<&'static Tool> {
    registry().iter().find(|tool| tool.name == name)
}

/// What a read-only session is told when it calls a tool that changes
/// something (`{tool}` is its name).
pub(crate) const READ_ONLY_REFUSAL: &str =
    "Refused: this session is read-only, and `{tool}` can change something outside it.";

/// Whether a stored role is read-only (`chats.role`, session-roles-and-mail FR-13).
fn role_is_read_only(role: Option<&Value>) -> bool {
    role.and_then(|role| role.get("access")).and_then(Value::as_str) == Some("read-only")
}

/// Whether `chat` runs as a read-only role: it gets only the tools that read,
/// and the rest are refused when called (session-roles-and-mail FR-13).
pub(crate) fn read_only_chat(app: &AppHandle, chat: &str) -> bool {
    let Some(db) = app.try_state::<crate::db::Db>() else {
        return false;
    };
    db.with(|conn| evo_store::chats::get_chat(conn, chat))
        .ok()
        .flatten()
        .is_some_and(|row| role_is_read_only(row.role.as_ref()))
}

/// Whether a registry tool is offered to, and may run for, a read-only chat:
/// only one that reads.
fn reads_only(name: &str) -> bool {
    find(name).is_some_and(|tool| tool.class == Class::Read)
}

/// Tools that are not fixed: they depend on the chat (which connectors it has,
/// which are switched off for it), so they are not in the static registry. A
/// provider lists a chat's definitions and claims and runs the calls the
/// registry does not know. The registry wins on a name both have.
pub trait Provider: Send + Sync {
    /// The extra tool definitions of `chat`, working in `folder` (the chat's own
    /// folder when `None`), as `{name, description, inputSchema}`.
    fn definitions_for(&self, app: AppHandle, chat: String, folder: Option<PathBuf>) -> BoxFuture<Vec<Value>>;
    /// Whether this provider answers a call to `name`.
    fn claims(&self, name: &str) -> bool;
    /// Run a claimed call. The body may use [`ToolCtx::ask`]; it has no class, so
    /// it does its own gating.
    fn run(&self, ctx: ToolCtx, name: String, args: Value) -> BoxFuture<ToolOutput>;
}

/// Every dynamic provider. Each module adds itself here.
static PROVIDERS: [&dyn Provider; 1] = [&mcp::Connectors];

fn providers() -> &'static [&'static dyn Provider] {
    &PROVIDERS
}

/// Who answers a call.
enum Target<'a> {
    Static(&'static Tool),
    Dynamic(&'a dyn Provider),
}

/// The registry first, then the first provider that claims `name`.
fn resolve<'a>(name: &str, providers: &'a [&'a dyn Provider]) -> Option<Target<'a>> {
    if let Some(tool) = find(name) {
        return Some(Target::Static(tool));
    }
    providers.iter().copied().find(|provider| provider.claims(name)).map(Target::Dynamic)
}

/// The registry's definitions followed by each provider's own, without a provider
/// definition whose name the registry already has (or an earlier one has).
fn merge_definitions(fixed: Vec<Value>, extra: Vec<Vec<Value>>) -> Vec<Value> {
    let mut seen: std::collections::HashSet<String> =
        fixed.iter().filter_map(|d| d.get("name").and_then(Value::as_str)).map(str::to_string).collect();
    let mut all = fixed;
    for definition in extra.into_iter().flatten() {
        match definition.get("name").and_then(Value::as_str) {
            Some(name) if seen.insert(name.to_string()) => all.push(definition),
            _ => {}
        }
    }
    all
}

pub(crate) fn static_definitions() -> Vec<Value> {
    registry()
        .iter()
        .map(|tool| json!({ "name": tool.name, "description": tool.description, "inputSchema": (tool.schema)() }))
        .collect()
}

/// Whether a registry tool is offered now: the computer app tool only while its
/// feature is on (docs/features/computer-app-control, FR-1).
fn offered(app: &AppHandle, definition: &Value) -> bool {
    match definition.get("name").and_then(Value::as_str) {
        Some(name) if name == "computer_app" => computer::tool::enabled(app),
        _ => true,
    }
}

/// What a running call knows: the app, the chat whose agent called, and when to stop.
#[derive(Clone)]
pub struct ToolCtx {
    pub app: AppHandle,
    pub chat: String,
    /// The engine's id for the call (`requestId`).
    pub request_id: String,
    /// Cancelled when the engine drops the call (the run was stopped).
    pub cancel: CancellationToken,
    tool: &'static str,
    class: Class,
}

impl ToolCtx {
    /// Question the person in the chat's approval prompt: the chosen index, or -1
    /// when the question became moot (the call was dropped).
    pub async fn ask(&self, title: &str, description: &str, options: &[&str]) -> i32 {
        ask_person(&self.app, &self.chat, &self.request_id, &self.cancel, title, description, options.iter().map(|o| o.to_string()).collect()).await
    }

    /// Run the approval gate for this call. `url` is the address it acts on
    /// (empty when there is none); `summary` is a line for the person.
    /// `Err` holds the result to return: the person declined, or the call ended.
    pub async fn gate(&self, url: &str, summary: &str) -> Result<(), ToolOutput> {
        let mode = self.approval_mode();
        let ctx = self.clone();
        let verdict = gate::check(self.tool, self.class, &mode, &self.chat, url, summary, move |title, description, options| {
            let ctx = ctx.clone();
            async move {
                let options: Vec<&str> = options.iter().map(String::as_str).collect();
                ctx.ask(&title, &description, &options).await
            }
        })
        .await;
        match verdict {
            gate::Verdict::Allow => Ok(()),
            gate::Verdict::Deny => Err(ToolOutput::error(format!("The person declined: {summary}"))),
            gate::Verdict::Gone => Err(ToolOutput::text("")),
        }
    }

    /// Run the approval gate of a call that acts in another app (`Class::Control`).
    /// `actions` are its acting steps as the person reads them; `always` is the
    /// feature's "Ask every time" setting.
    pub async fn gate_control(&self, actions: &[String], always: bool) -> Result<(), ToolOutput> {
        let mode = self.approval_mode();
        let ctx = self.clone();
        let verdict = gate::check_control(self.tool, always, &mode, &self.chat, actions, move |title, description, options| {
            let ctx = ctx.clone();
            async move {
                let options: Vec<&str> = options.iter().map(String::as_str).collect();
                ctx.ask(&title, &description, &options).await
            }
        })
        .await;
        match verdict {
            gate::Verdict::Allow => Ok(()),
            gate::Verdict::Deny => Err(ToolOutput::error(format!(
                "The person declined: {}. Do not try these actions again in this run; ask them what to do instead.",
                actions.join("; ")
            ))),
            gate::Verdict::Gone => Err(ToolOutput::text("")),
        }
    }

    /// The approval mode now (`ask`, `auto-edits`, `auto`): read on every call, so a change applies at once.
    pub fn approval_mode(&self) -> String {
        crate::engine::approval_mode(&self.app.state::<Db>()).to_string()
    }

    /// Send an event to every window.
    pub fn emit(&self, event: &str, payload: Value) {
        let _ = self.app.emit(event, payload);
    }

    /// The window showing this chat (`main` when none reported it).
    pub fn window(&self) -> String {
        window_of_chat(&self.app, &self.chat)
    }

    /// The folder the chat works in: its worktree, else its workspace, else the home folder.
    pub fn folder(&self) -> PathBuf {
        chat_folder(&self.app, &self.chat)
    }

    /// Whether the call was dropped.
    pub fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

// ── The home folder of a chat ───────────────────────────────────────────────

fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The folder a chat works in (the one its engine starts in, `supervisor::folder_of`).
pub fn chat_folder(app: &AppHandle, chat: &str) -> PathBuf {
    app.state::<Db>()
        .with(|conn| Ok(crate::supervisor::folder_of(conn, chat, &home())))
        .unwrap_or_else(|_| home())
}

// ── State: calls in flight, open questions, which window shows which chat ───

#[derive(Default)]
pub struct AgentTools {
    /// Calls in flight, by the engine's request id.
    calls: Mutex<HashMap<String, CancellationToken>>,
    /// Open questions, by question id: where to deliver the answer, and the chat.
    asks: Mutex<HashMap<String, (oneshot::Sender<i32>, String)>>,
    /// Which chat each window shows.
    windows: Mutex<HashMap<String, String>>,
    next: AtomicU64,
}

fn window_of_chat(app: &AppHandle, chat: &str) -> String {
    app.state::<AgentTools>()
        .windows
        .lock()
        .expect("windows lock")
        .iter()
        .find(|(_, shown)| shown.as_str() == chat)
        .map(|(window, _)| window.clone())
        .unwrap_or_else(|| "main".to_string())
}

/// A window shows `chat` now (or none): the webview reports it whenever its chat changes.
#[tauri::command]
pub fn agent_window_chat(state: State<'_, AgentTools>, window: Window, chat: Option<String>) {
    let mut windows = state.windows.lock().expect("windows lock");
    match chat {
        Some(chat) => windows.insert(window.label().to_string(), chat),
        None => windows.remove(window.label()),
    };
}

/// The definitions of the tools the shell answers, for `configure_external_tools`:
/// the registry's, plus (with a `chat`) the dynamic providers' for that chat
/// (connector tools), the chat working in `folder` or else in its own folder.
#[tauri::command]
pub async fn agent_tools_definitions(app: AppHandle, chat: Option<String>, folder: Option<String>) -> Vec<Value> {
    // The past-chat tools follow **Search past chats** and the chat's memory
    // flags (chat_flags.rs).
    let hidden = crate::chat_flags::hidden_tools(&app, chat.as_deref().filter(|c| !c.is_empty())).await;
    // A read-only session gets only the registry tools that read (FR-13).
    let read_only = chat.as_deref().is_some_and(|chat| !chat.is_empty() && read_only_chat(&app, chat));
    let mut extra = Vec::new();
    if let Some(chat) = chat.filter(|c| !c.is_empty()) {
        let folder = folder.filter(|f| !f.is_empty()).map(PathBuf::from);
        for provider in providers() {
            extra.push(provider.definitions_for(app.clone(), chat.clone(), folder.clone()).await);
        }
    }
    let fixed = static_definitions()
        .into_iter()
        .filter(|definition| offered(&app, definition))
        .filter(|definition| !read_only || definition.get("name").and_then(Value::as_str).is_some_and(reads_only))
        .filter(|definition| !definition.get("name").and_then(Value::as_str).is_some_and(|name| hidden.contains(&name)))
        .collect();
    merge_definitions(fixed, extra)
}

/// The person's answer to a question a tool raised (`agent://ask`).
#[tauri::command]
pub fn agent_ask_answer(state: State<'_, AgentTools>, request_id: String, index: i32) {
    if let Some((tx, _)) = state.asks.lock().expect("asks lock").remove(&request_id) {
        let _ = tx.send(index);
    }
}

async fn ask_person(
    app: &AppHandle,
    chat: &str,
    call: &str,
    cancel: &CancellationToken,
    title: &str,
    description: &str,
    options: Vec<String>,
) -> i32 {
    let state = app.state::<AgentTools>();
    let id = format!("agent-ask:{}", state.next.fetch_add(1, Ordering::Relaxed) + 1);
    let (tx, rx) = oneshot::channel();
    state.asks.lock().expect("asks lock").insert(id.clone(), (tx, chat.to_string()));
    // The supervisor keeps it with the chat's other open questions, every
    // window draws it (`engine://question`), and it is announced when no
    // window shows the chat. Only the person answers it; it never times out
    // (docs/features/shell-hosted-engines/spec.md, FR-9).
    let question = crate::supervisor::questions::OpenQuestion::from_shell(&id, call, title, description, options, crate::supervisor::now_ms());
    crate::supervisor::question_opened(app, chat, question);
    let answer = tokio::select! {
        answer = rx => answer.unwrap_or(-1),
        _ = cancel.cancelled() => {
            state.asks.lock().expect("asks lock").remove(&id);
            -1
        }
    };
    crate::supervisor::question_closed(app, chat, &id);
    answer
}

/// Run another registered tool inside this call (a batch step, a preview that
/// starts a shell and opens a page): same chat and cancellation, the called
/// tool's own class and gate.
pub async fn call_tool(ctx: &ToolCtx, name: &str, args: Value) -> ToolOutput {
    let Some(tool) = find(name) else {
        return ToolOutput::error(format!("Unknown tool {name}."));
    };
    let inner = ToolCtx { tool: tool.name, class: tool.class, ..ctx.clone() };
    (tool.run)(inner, args).await
}

// ── Dispatch ────────────────────────────────────────────────────────────────

/// One stdout line of `chat`'s engine: answer an `external_tool_request` for a
/// tool of the registry, cancel a call the engine dropped.
pub async fn on_line(app: &AppHandle, chat: &str, line: &Value) {
    match line.get("type").and_then(Value::as_str) {
        Some("external_tool_request") => {
            let (Some(name), Some(request_id)) = (line.get("toolName").and_then(Value::as_str), line.get("requestId").and_then(Value::as_str)) else {
                return;
            };
            // The registry first; a tool it does not know may be a provider's (connector tools).
            let Some(target) = resolve(name, providers()) else {
                return;
            };
            let args = line.get("arguments").cloned().unwrap_or(Value::Null);
            let args = if args.is_null() { json!({}) } else { args };
            let cancel = CancellationToken::new();
            app.state::<AgentTools>().calls.lock().expect("calls lock").insert(request_id.to_string(), cancel.clone());
            let (tool_name, class) = match &target {
                Target::Static(tool) => (tool.name, tool.class),
                Target::Dynamic(_) => ("dynamic", Class::Command),
            };
            // A read-only session: a registry tool that changes something is
            // refused before it runs or asks (FR-13). Connector tools are
            // checked by their provider, which knows the server's hint.
            if matches!(target, Target::Static(_)) && class != Class::Read && read_only_chat(app, chat) {
                let answer = json!({
                    "type": "external_tool_response",
                    "id": format!("agent-tools-{}", app.state::<AgentTools>().next.fetch_add(1, Ordering::Relaxed) + 1),
                    "requestId": request_id,
                    "result": ToolOutput::error(READ_ONLY_REFUSAL.replace("{tool}", name)).to_json(),
                });
                let _ = app.state::<crate::engine::EngineState>().write(chat, &answer).await;
                return;
            }
            let ctx = ToolCtx { app: app.clone(), chat: chat.to_string(), request_id: request_id.to_string(), cancel: cancel.clone(), tool: tool_name, class };
            let (app, chat, request_id, name) = (app.clone(), chat.to_string(), request_id.to_string(), name.to_string());
            tauri::async_runtime::spawn(async move {
                // The cards over the chat show what a browser or terminal tool is doing.
                activity::announce(&app, &chat, &name, &request_id, "start", &args);
                let shown_args = args.clone();
                let tool_label = name.clone();
                // The body runs in its own task so a panic becomes an error result, not a lost call.
                let body = match target {
                    Target::Static(tool) => tauri::async_runtime::spawn((tool.run)(ctx, args)),
                    Target::Dynamic(provider) => tauri::async_runtime::spawn(provider.run(ctx, name, args)),
                };
                let output = match body.await {
                    Ok(output) => output,
                    Err(error) => ToolOutput::error(format!("The tool failed: {error}")),
                };
                app.state::<AgentTools>().calls.lock().expect("calls lock").remove(&request_id);
                activity::announce(&app, &chat, &tool_label, &request_id, "end", &shown_args);
                // The engine dropped the call (the run was stopped): nobody waits for an answer.
                if cancel.is_cancelled() {
                    return;
                }
                let answer = json!({
                    "type": "external_tool_response",
                    "id": format!("agent-tools-{}", app.state::<AgentTools>().next.fetch_add(1, Ordering::Relaxed) + 1),
                    "requestId": request_id,
                    "result": output.fit(900_000).to_json(),
                });
                let _ = app.state::<crate::engine::EngineState>().write(&chat, &answer).await;
            });
        }
        // The run ended: the app the chat controlled goes back where it was
        // (docs/features/computer-app-control, FR-7).
        Some("agent_end") => {
            let chat = chat.to_string();
            tauri::async_runtime::spawn(async move { computer::release_chat(&chat).await });
        }
        Some("external_tool_request_closed") => {
            if let Some(request_id) = line.get("requestId").and_then(Value::as_str) {
                if let Some(cancel) = app.state::<AgentTools>().calls.lock().expect("calls lock").remove(request_id) {
                    cancel.cancel();
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod read_only_chat_tests {
    use super::*;

    // docs/features/session-roles-and-mail/spec.md (FR-13).
    #[test]
    fn read_only_chat_role_and_tool_classes() {
        assert!(role_is_read_only(Some(&json!({"name": "reviewer", "access": "read-only"}))));
        assert!(!role_is_read_only(Some(&json!({"access": "full"}))));
        assert!(!role_is_read_only(None));
        // Every registry tool that reads is offered; none that changes something is.
        for tool in registry() {
            assert_eq!(reads_only(tool.name), tool.class == Class::Read, "{}", tool.name);
        }
        assert!(registry().iter().any(|tool| tool.class != Class::Read), "the registry has writing tools");
        assert!(!reads_only("not_a_tool"));
        assert!(READ_ONLY_REFUSAL.replace("{tool}", "terminal_write").contains("`terminal_write`"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_reads_as_the_engine_expects() {
        let ok = ToolOutput::text("done").to_json();
        assert_eq!(ok["content"][0]["type"], "text");
        assert_eq!(ok["content"][0]["text"], "done");
        assert!(ok.get("isError").is_none());
        let err = ToolOutput::error("no").to_json();
        assert_eq!(err["isError"], true);
        let image = ToolOutput::image("AAAA".into(), "image/jpeg", "a page").to_json();
        assert_eq!(image["content"][0]["type"], "image");
        assert_eq!(image["content"][0]["mimeType"], "image/jpeg");
        assert_eq!(image["content"][1]["text"], "a page");
        assert!(image.get("structuredContent").is_none());
    }

    #[test]
    fn a_result_carries_structured_content_beside_its_text() {
        let sent = ToolOutput::text("Sent 1 file to the chat: a.txt.").with_structured(json!({ "files": [{ "name": "a.txt" }] })).to_json();
        assert_eq!(sent["content"][0]["text"], "Sent 1 file to the chat: a.txt.");
        assert_eq!(sent["structuredContent"]["files"][0]["name"], "a.txt");
        assert!(sent.get("isError").is_none());
        let failed = ToolOutput::error("no").with_structured(json!({ "why": 1 })).to_json();
        assert_eq!(failed["isError"], true);
        assert_eq!(failed["structuredContent"]["why"], 1);
    }

    #[test]
    fn every_registered_tool_has_a_unique_name_and_an_object_schema() {
        let mut names = std::collections::HashSet::new();
        for tool in registry() {
            assert!(names.insert(tool.name), "duplicate tool {}", tool.name);
            assert_eq!((tool.schema)()["type"], "object", "{}", tool.name);
        }
    }

    /// A provider with one made-up tool and one that clashes with the registry.
    struct Fake;

    impl Provider for Fake {
        fn definitions_for(&self, _app: AppHandle, chat: String, _folder: Option<PathBuf>) -> BoxFuture<Vec<Value>> {
            let clash = registry().first().map(|t| t.name).unwrap_or("notify");
            Box::pin(async move {
                vec![
                    json!({ "name": format!("fake_{chat}"), "description": "d", "inputSchema": { "type": "object" } }),
                    json!({ "name": clash, "description": "shadow", "inputSchema": { "type": "object" } }),
                ]
            })
        }

        fn claims(&self, name: &str) -> bool {
            name.starts_with("fake_") || registry().first().is_some_and(|t| t.name == name)
        }

        fn run(&self, _ctx: ToolCtx, name: String, _args: Value) -> BoxFuture<ToolOutput> {
            Box::pin(async move { ToolOutput::text(name) })
        }
    }

    #[test]
    fn a_provider_answers_what_the_registry_does_not_know() {
        let fake = Fake;
        let providers: [&dyn Provider; 1] = [&fake];
        assert!(matches!(resolve("fake_x", &providers), Some(Target::Dynamic(_))));
        assert!(resolve("fake_x", &[]).is_none());
        assert!(resolve("no_such_tool", &providers).is_none());
    }

    #[test]
    fn the_registry_wins_a_name_a_provider_also_claims() {
        let fake = Fake;
        let providers: [&dyn Provider; 1] = [&fake];
        let name = registry().first().expect("a registered tool").name;
        assert!(fake.claims(name));
        assert!(matches!(resolve(name, &providers), Some(Target::Static(tool)) if tool.name == name));
    }

    #[test]
    fn the_first_claiming_provider_answers() {
        struct Other;
        impl Provider for Other {
            fn definitions_for(&self, _: AppHandle, _: String, _: Option<PathBuf>) -> BoxFuture<Vec<Value>> {
                Box::pin(async { Vec::new() })
            }
            fn claims(&self, name: &str) -> bool {
                name == "fake_y"
            }
            fn run(&self, _: ToolCtx, _: String, _: Value) -> BoxFuture<ToolOutput> {
                Box::pin(async { ToolOutput::text("other") })
            }
        }
        let (other, fake) = (Other, Fake);
        let providers: [&dyn Provider; 2] = [&other, &fake];
        let Some(Target::Dynamic(provider)) = resolve("fake_y", &providers) else {
            panic!("claimed");
        };
        assert!(provider.claims("fake_y") && !provider.claims("fake_z"));
        let Some(Target::Dynamic(provider)) = resolve("fake_z", &providers) else {
            panic!("claimed");
        };
        assert!(provider.claims("fake_z"));
    }

    #[test]
    fn an_oversized_result_is_fitted_under_the_frame_limit() {
        let mut noise = image::RgbaImage::new(1600, 1200);
        for (i, p) in noise.pixels_mut().enumerate() {
            let v = (i as u32).wrapping_mul(2654435761);
            *p = image::Rgba([v as u8, (v >> 8) as u8, (v >> 16) as u8, 255]);
        }
        let mut png = std::io::Cursor::new(Vec::new());
        noise.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png.into_inner());
        let fitted = ToolOutput::image(png, "image/png", "shot").fit(900_000).to_json().to_string();
        assert!(fitted.len() <= 900_000 && fitted.contains("image/jpeg"));
        let big = ToolOutput::image("A".repeat(2_000_000), "image/png", "shot");
        let json = big.fit(900_000).to_json().to_string();
        assert!(json.len() <= 900_000 && json.contains("shot") && json.contains("omitted"));
        let text = ToolOutput::text("x".repeat(2_000_000)).fit(900_000).to_json().to_string();
        assert!(text.len() <= 900_000);
    }

    #[test]
    fn provider_definitions_follow_the_registrys_without_shadowing_it() {
        let fixed = static_definitions();
        let fixed_len = fixed.len();
        let clash = registry().first().expect("a registered tool").name;
        let extra = vec![
            vec![
                json!({ "name": "fake_c", "description": "d", "inputSchema": { "type": "object" } }),
                json!({ "name": clash, "description": "shadow", "inputSchema": { "type": "object" } }),
                json!({ "description": "nameless" }),
            ],
            vec![json!({ "name": "fake_c", "description": "again" }), json!({ "name": "fake_d" })],
        ];
        let all = merge_definitions(fixed, extra);
        assert_eq!(all.len(), fixed_len + 2);
        assert_eq!(all[fixed_len]["name"], "fake_c");
        assert_eq!(all[fixed_len]["description"], "d");
        assert_eq!(all[fixed_len + 1]["name"], "fake_d");
        assert!(all.iter().filter(|d| d["name"] == clash).all(|d| d["description"] != "shadow"));
    }

    #[test]
    fn without_a_provider_the_definitions_are_the_registrys() {
        let all = merge_definitions(static_definitions(), Vec::new());
        assert_eq!(all.len(), registry().len());
        for (definition, tool) in all.iter().zip(registry()) {
            assert_eq!(definition["name"], tool.name);
            assert_eq!(definition["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn the_connector_provider_is_registered() {
        assert!(providers().iter().any(|p| p.claims("mcp__github__create_issue")));
        assert!(providers().iter().any(|p| p.claims("mcp_search_tools")));
        // No static tool is named like a connector tool.
        assert!(registry().iter().all(|t| !mcp::is_mcp_tool(t.name) && t.name != mcp::SEARCH_TOOL && t.name != mcp::CALL_TOOL));
    }
}
