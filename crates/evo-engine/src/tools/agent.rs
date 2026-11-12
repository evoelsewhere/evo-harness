//! Subagents: the built-in `agent` and `message_agent` tools.
//!
//! `agent` hands a self-contained task to a child agent. The child starts with
//! only its brief (no copy of this conversation), works with the parent's
//! tools and model unless its agent type narrows them (see [`crate::agents`]),
//! optionally in its own Git worktree, and its final reply becomes the result.
//! Its steps never enter the parent's context.
//!
//! * **Foreground** (default): the tool waits for the child. While it runs, a
//!   cumulative snapshot of the child (status, turns, tool rows, the tail of
//!   its text, usage) goes out through `tool_execution_update` as
//!   `details.child`, and stays on the result so a reopened chat shows it.
//!   The child's usage is the result's `usage`. The result's text is the
//!   report and a last line with the agent's id. A run that fails, is
//!   cancelled or times out is an error result that still carries the final
//!   snapshot and the usage ([`AgentError::StructuredTool`]).
//! * **Background** (`background: true`): the tool returns at once with the
//!   agent's id and the child keeps running past the end of the parent's run
//!   (until it finishes, is cancelled, or the session closes). When it
//!   finishes, its report is queued for the parent: into the running run
//!   (steering) or, when the parent is idle, for its next run, and a mesh
//!   wake request is put on the parent's session node so the host can start
//!   that run (`meta.wake`, see [`crate::mesh::LocalRouter::request_wake`]).
//!
//! A finished agent keeps its context: `message_agent` sends it another
//! message (foreground or background) and it continues where it left off. It
//! is named by its id, `a_<id>`, its node address or its description.
//!
//! Every agent is also a node of this engine's session mesh
//! ([`crate::mesh`]) at `s_<session>/a_<agent>`: its state changes, tool rows,
//! text, usage and report go out as mesh events, and `cancel` reaches it.
//!
//! Children cannot delegate further (the harness refuses nested children and
//! hides these tools from them). At most [`MAX_RUNNING`] agents of a session
//! run at once: a foreground call waits for a free slot, a background one is
//! refused. [`MAX_KEPT`] are kept per session (64 per engine); the oldest
//! finished ones are closed, and so are those of a session that is gone.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use evo_agent::agent_tool::AgentTool;
use evo_agent::error::AgentError;
use evo_agent::types::{AgentToolResult, TextContentOrImage, ToolExecutionMode, ToolResultPartial};
use evo_agent::{AgentEmitter, AgentEvent, AgentMessage};
use evo_ai::types::{AssistantMessageEvent, Schema, Tool, Usage};
use evo_harness::agent_harness::{AgentHarness, WeakAgentHarness};
use evo_harness::session::memory::{InMemorySessionStorage, SystemClock};
use evo_harness::session::session::DefaultIdGenerator;
use evo_harness::session::types::SessionMetadata;
use evo_harness::session::Session;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::agents::{self, AgentType};
use crate::mesh::{self, Address, MeshUsage, NodeKind, NodeState, Origin, RunStatus, ToolRow};

/// Subagents running at once in one session.
pub const MAX_RUNNING: usize = 4;
/// Agents kept (running or able to continue) per session.
pub const MAX_KEPT: usize = 16;
/// Agents kept per engine, over every session it has had.
const MAX_KEPT_ENGINE: usize = 4 * MAX_KEPT;
/// Bytes of a report the requester gets; the rest stays with the agent,
/// which `message_agent` can ask for.
const MAX_REPORT: usize = 16 * 1024;
/// A subagent run that takes longer is stopped.
const TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Progress updates at most this often (the final one always goes out).
const UPDATE_EVERY: Duration = Duration::from_millis(250);
/// Tool rows kept in the snapshot (the most recent).
const MAX_ROWS: usize = 50;
/// Bytes of the child's latest text kept in the snapshot.
const MAX_TEXT: usize = 4096;
/// Argument fields shown for a child's tool call (what the UI labels it by).
const ARG_FIELDS: &[&str] = &[
    "command",
    "path",
    "file_path",
    "pattern",
    "query",
    "url",
    "description",
];
/// A foreground run's tie to its tool call ([`drive`]).
const ATTACHED: u8 = 0;
/// The call returned on a steering message; the run goes on in the background.
const DETACHED: u8 = 1;
/// The run ended while the call still waited for it.
const FINISHED: u8 = 2;

/// Steering the person or another session sent, per session: a counter that
/// waiting foreground agents watch ([`steered`]).
static STEERED: LazyLock<Mutex<HashMap<String, tokio::sync::watch::Sender<u64>>>> =
    LazyLock::new(Default::default);

/// A steering message for the session `session_id` was queued (RPC
/// `queue_message`, `deliverAs: steer`): foreground agents its run waits for
/// move to the background, so the run reads the message at its next step.
pub fn steered(session_id: &str) {
    if let Some(sender) = lock(&STEERED).get(session_id) {
        sender.send_modify(|count| *count = count.wrapping_add(1));
    }
}

/// Changes after now of [`steered`] for `session_id`.
fn steering(session_id: &str) -> tokio::sync::watch::Receiver<u64> {
    lock(&STEERED)
        .entry(session_id.to_string())
        .or_insert_with(|| tokio::sync::watch::channel(0).0)
        .subscribe()
}

/// Messages about background agents start with this, so hosts can show them
/// as events rather than as something the user typed.
pub const NOTE_TAG: &str = "<background-agent";

/// The background agents a session's history started that never reported:
/// `(agent id, description)`, oldest first. `messages` is the branch as the
/// transcript projects it (JSON, oldest first); `queued` is the text of
/// messages still waiting for the next run (a report there is on its way).
///
/// A background agent lives in the engine that started it. Read when an
/// engine opens a session, these ran in an earlier engine process that is
/// gone (the app closed or crashed), so their reports will never come.
pub fn unreported_background_agents(messages: &[Value], queued: &str) -> Vec<(String, String)> {
    let mut open: Vec<(String, String)> = Vec::new();
    for message in messages {
        match message["role"].as_str() {
            // A call that returned at once, leaving the agent running (an
            // `agent` call, or a `message_agent` that continued one).
            Some("toolResult") => {
                let child = &message["details"]["child"];
                if child["background"] == json!(true) && child["status"] == json!("running") {
                    if let Some(id) = child["agentId"].as_str().filter(|id| !id.is_empty()) {
                        open.retain(|(open_id, _)| open_id != id);
                        open.push((
                            id.to_string(),
                            child["description"].as_str().unwrap_or("").to_string(),
                        ));
                    }
                }
            }
            Some("user") => {
                let text = match &message["content"] {
                    Value::String(text) => text.clone(),
                    Value::Array(blocks) => blocks
                        .iter()
                        .filter_map(|b| b["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                if text.contains(NOTE_TAG) {
                    open.retain(|(id, _)| !has_note(&text, id));
                }
            }
            _ => {}
        }
    }
    // The queue is JSON, so the note's quotes are escaped there.
    open.retain(|(id, _)| {
        !queued.contains(&format!("{NOTE_TAG} id=\\\"{id}\\\""))
            && !queued.contains(&format!("{NOTE_TAG} id=\"{id}\""))
    });
    open
}

/// Whether `text` holds the note about agent `id`: its opening tag starts a
/// line, as every note the engine writes does (a quoted one inside a line, or
/// one inside a report — see [`inert`] — does not count).
fn has_note(text: &str, id: &str) -> bool {
    let tag = format!("{NOTE_TAG} id=\"{id}\"");
    text.lines().any(|line| line.starts_with(&tag))
}

/// A description as a note's attribute value: no quotes, one line.
fn attribute(description: &str) -> String {
    description.replace('"', "'").replace(['\r', '\n'], " ")
}

/// `text` with the note's own tags made inert (`<background-agent…` and
/// `</background-agent…` become `&lt;…`), so a report cannot end its note or
/// pass for another agent's.
fn inert(text: &str) -> String {
    let name = &NOTE_TAG[1..];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        let tag = rest.strip_prefix('/').unwrap_or(rest);
        let ours = tag
            .get(..name.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(name));
        out.push_str(if ours { "&lt;" } else { "<" });
    }
    out.push_str(rest);
    out
}

/// The report a lost background agent never sent, written the way a real one
/// is, so the model and the app read it like any other.
pub fn lost_agent_note(id: &str, description: &str) -> String {
    format!(
        "{NOTE_TAG} id=\"{id}\" description=\"{}\" status=\"error\">\nThe subagent stopped: the engine running it exited \
         (the app closed or crashed) before it finished, so it has no report. Start it again if its work is still needed.\n\
         </background-agent>",
        attribute(description)
    )
}

/// Tell this session's next run about background agents an earlier engine
/// left unfinished (see [`unreported_background_agents`]). Queued for the
/// next run, like a report that arrives while the chat is idle, but without a
/// wake: opening a chat does not start a run by itself. Returns their ids.
pub async fn report_lost_background_agents(
    harness: &AgentHarness,
    messages: &[Value],
) -> Vec<String> {
    use evo_harness::session::types::RecordQuery;
    // Reports already queued by an earlier start (durable queue records).
    let queued = harness
        .session()
        .storage()
        .find_records(&RecordQuery {
            record_type: Some("queue_enqueued"),
            pending_queue_only: true,
            ..Default::default()
        })
        .await
        .ok()
        .and_then(|records| serde_json::to_string(&records).ok())
        .unwrap_or_default();
    let lost = unreported_background_agents(messages, &queued);
    if lost.is_empty() {
        return Vec::new();
    }
    let note = lost
        .iter()
        .map(|(id, description)| lost_agent_note(id, description))
        .collect::<Vec<_>>()
        .join("\n\n");
    let message = AgentMessage::from(evo_ai::types::UserMessage::new(note, now_ms() as i64));
    match harness.lane("main").next_run(message).await {
        Ok(_) => lost.into_iter().map(|(id, _)| id).collect(),
        Err(error) => {
            eprintln!("[evo] could not queue the lost background agents' reports: {error}");
            Vec::new()
        }
    }
}

/// The parent harness, bound once it exists (tools are built before it).
pub type HarnessCell = Arc<OnceLock<WeakAgentHarness>>;

type Update = Arc<dyn Fn(ToolResultPartial) + Send + Sync>;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

// ── Progress ────────────────────────────────────────────────────────────────

/// What the UI shows of a child.
struct Progress {
    agent_id: String,
    agent_type: String,
    description: String,
    model: String,
    background: bool,
    status: &'static str,
    turns: u32,
    tool_uses: u32,
    usage: Usage,
    started_at: u64,
    ended_at: Option<u64>,
    rows: VecDeque<Value>,
    text: String,
    /// The text of this run's last message that had any, kept when a later
    /// message (a tool call without text) clears `text`.
    said: String,
    worktree: Option<Value>,
    error: Option<String>,
}

impl Progress {
    fn snapshot(&self) -> Value {
        json!({
            "agentId": self.agent_id,
            "agentType": self.agent_type,
            "description": self.description,
            "model": self.model,
            "background": self.background,
            "status": self.status,
            "turns": self.turns,
            "toolUses": self.tool_uses,
            "usage": {
                "input": self.usage.input,
                "output": self.usage.output,
                "cacheRead": self.usage.cache_read,
                "cacheWrite": self.usage.cache_write,
                "totalTokens": self.usage.total_tokens,
                "cost": self.usage.cost.total,
            },
            "startedAt": self.started_at,
            "endedAt": self.ended_at,
            "tools": self.rows,
            "text": self.text,
            "worktree": self.worktree,
            "error": self.error,
        })
    }

    /// What `event` changed, for a mesh activity event: the tool row it
    /// touched, or the text tail after a text delta.
    fn activity(&self, event: &AgentEvent) -> (Option<ToolRow>, Option<String>) {
        match event {
            AgentEvent::ToolExecutionStart { tool_call_id, .. }
            | AgentEvent::ToolExecutionEnd { tool_call_id, .. } => {
                let row = self
                    .rows
                    .iter()
                    .rev()
                    .find(|row| row["callId"] == *tool_call_id)
                    .map(|row| ToolRow {
                        call_id: tool_call_id.clone(),
                        name: row["name"].as_str().unwrap_or_default().to_string(),
                        args: row["args"].clone(),
                        phase: row["phase"].as_str().unwrap_or("running").to_string(),
                    });
                (row, None)
            }
            AgentEvent::MessageUpdate { .. } | AgentEvent::MessageStart { .. } => {
                (None, Some(self.text.clone()))
            }
            _ => (None, None),
        }
    }

    fn observe(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::TurnEnd { .. } => self.turns += 1,
            AgentEvent::MessageStart {
                message: AgentMessage::Assistant(_),
                ..
            } => {
                let text = std::mem::take(&mut self.text);
                if !text.trim().is_empty() {
                    self.said = text;
                }
            }
            AgentEvent::MessageUpdate {
                assistant_message_event: AssistantMessageEvent::TextDelta { delta, .. },
                ..
            } => {
                self.text.push_str(delta);
                if self.text.len() > MAX_TEXT {
                    let mut cut = self.text.len() - MAX_TEXT;
                    while !self.text.is_char_boundary(cut) {
                        cut += 1;
                    }
                    self.text.drain(..cut);
                }
            }
            AgentEvent::MessageEnd {
                message: AgentMessage::Assistant(message),
                ..
            } => {
                self.usage.add(&message.usage);
                crate::usage_log::record(message, crate::usage_log::Source::Subagent);
            }
            AgentEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
            } => {
                self.tool_uses += 1;
                let shown: serde_json::Map<String, Value> = ARG_FIELDS
                    .iter()
                    .filter_map(|key| {
                        let text = args.get(*key)?.as_str()?;
                        Some((
                            key.to_string(),
                            Value::String(text.chars().take(200).collect()),
                        ))
                    })
                    .collect();
                self.rows.push_back(json!({
                    "callId": tool_call_id,
                    "name": tool_name,
                    "args": shown,
                    "phase": "running",
                }));
                while self.rows.len() > MAX_ROWS {
                    self.rows.pop_front();
                }
            }
            AgentEvent::ToolExecutionEnd {
                tool_call_id,
                is_error,
                ..
            } => {
                if let Some(row) = self
                    .rows
                    .iter_mut()
                    .rev()
                    .find(|row| row["callId"] == *tool_call_id)
                {
                    row["phase"] = json!(if *is_error { "error" } else { "done" });
                }
            }
            _ => {}
        }
    }
}

/// Feeds the child's events into its snapshot and, while a tool call waits
/// for it, into that call's updates.
struct ProgressEmitter {
    progress: Arc<Mutex<Progress>>,
    update: Arc<Mutex<Option<Update>>>,
    last: Mutex<Option<Instant>>,
    /// The agent's mesh node.
    address: Address,
}

impl ProgressEmitter {
    fn observe(&self, event: AgentEvent) {
        let router = mesh::router();
        router.event(&self.address, &event);
        let snapshot = {
            let mut progress = lock(&self.progress);
            progress.observe(&event);
            if matches!(
                event,
                AgentEvent::MessageEnd {
                    message: AgentMessage::Assistant(_),
                    ..
                }
            ) {
                router.usage(&self.address, mesh::usage_of(&progress.usage));
            }
            let mut last = lock(&self.last);
            let due = last.is_none_or(|at| at.elapsed() >= UPDATE_EVERY)
                || matches!(
                    event,
                    AgentEvent::ToolExecutionStart { .. } | AgentEvent::ToolExecutionEnd { .. }
                );
            if !due {
                return;
            }
            *last = Some(Instant::now());
            // Folded by the router even without subscribers, for later ones.
            let (tool, text) = progress.activity(&event);
            router.activity(
                &self.address,
                progress.turns,
                progress.tool_uses,
                tool,
                text,
            );
            progress.snapshot()
        };
        let update = lock(&self.update).clone();
        if let Some(update) = update {
            update(partial(snapshot));
        }
    }
}

impl AgentEmitter for ProgressEmitter {
    fn emit(&self, event: AgentEvent) -> futures::future::BoxFuture<'static, ()> {
        self.observe(event);
        Box::pin(async {})
    }
    fn try_emit(&self, event: AgentEvent) {
        self.observe(event);
    }
}

/// An update carrying only the snapshot (no text output yet).
fn partial(snapshot: Value) -> AgentToolResult {
    AgentToolResult {
        content: Vec::new(),
        details: json!({ "child": snapshot }),
        ..Default::default()
    }
}

// ── Registry ────────────────────────────────────────────────────────────────

/// One subagent: running, or finished and able to continue.
pub struct Job {
    id: String,
    /// Its mesh node.
    address: Address,
    parent: WeakAgentHarness,
    /// The parent's session; limits and lookups are per session.
    session_id: String,
    /// The parent's tool call that started or last continued it.
    call_id: Mutex<String>,
    progress: Arc<Mutex<Progress>>,
    update: Arc<Mutex<Option<Update>>>,
    /// The child; `None` once closed (evicted, or its worktree was removed).
    child: Mutex<Option<ChildParts>>,
    /// The current run's cancel switch.
    cancel: Mutex<CancellationToken>,
    running: AtomicBool,
    background: AtomicBool,
    /// This run was started to answer an `ask` (tools/mesh.rs): its report
    /// goes to the asker, not to the parent as a background note.
    quiet: AtomicBool,
    worktree: Mutex<Option<crate::worktrees::Created>>,
    cwd: PathBuf,
    last_used: Mutex<Instant>,
    /// Its run slot, held while `running` ([`slots`]).
    slot: Mutex<Option<Slot>>,
}

#[derive(Clone)]
struct ChildParts {
    harness: AgentHarness,
    execution_scope: Arc<evo_agent::tool_execution_scope::ToolExecutionScope>,
}

#[derive(Default)]
pub struct Registry {
    jobs: Mutex<Vec<Arc<Job>>>,
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::default);

/// This engine's subagents.
pub fn registry() -> &'static Registry {
    &REGISTRY
}

impl Registry {
    fn get(&self, id: &str) -> Option<Arc<Job>> {
        lock(&self.jobs).iter().find(|job| job.id == id).cloned()
    }

    /// The agent of the session `session_id` that `wanted` names: its id,
    /// `a_<id>`, its node address or — when only one agent has it — its
    /// description (any case).
    fn find(&self, session_id: &str, wanted: &str) -> Result<Arc<Job>, AgentError> {
        let wanted = wanted.trim_matches(|c: char| c.is_whitespace() || "`'\"".contains(c));
        let jobs: Vec<Arc<Job>> = lock(&self.jobs)
            .iter()
            .filter(|job| job.session_id == session_id)
            .cloned()
            .collect();
        let by_id = jobs.iter().find(|job| {
            let address = job.address.as_str();
            job.id == wanted
                || address == wanted
                || address.rsplit('/').next() == Some(wanted)
                || wanted.strip_prefix("a_") == Some(job.id.as_str())
        });
        if let Some(job) = by_id {
            return Ok(job.clone());
        }
        let listed = |jobs: &[&Arc<Job>]| {
            jobs.iter()
                .map(|job| format!("{} ({})", job.id, lock(&job.progress).description))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let named: Vec<&Arc<Job>> = jobs
            .iter()
            .filter(|job| lock(&job.progress).description.eq_ignore_ascii_case(wanted))
            .collect();
        match named.as_slice() {
            [job] => Ok((*job).clone()),
            [] if jobs.is_empty() => Err(AgentError::Validation(format!(
                "no subagent `{wanted}` in this session: it has none (start one with agent)"
            ))),
            [] => Err(AgentError::Validation(format!(
                "no subagent `{wanted}` in this session; its subagents: {}",
                listed(&jobs.iter().collect::<Vec<_>>())
            ))),
            several => Err(AgentError::Validation(format!(
                "several subagents are called `{wanted}`: {}; use the id",
                listed(several)
            ))),
        }
    }

    /// Add a job, closing its session's oldest finished ones beyond
    /// [`MAX_KEPT`], the finished ones of sessions that are gone (nobody can
    /// continue those), and the engine's oldest beyond [`MAX_KEPT_ENGINE`].
    fn insert(&self, job: Arc<Job>) {
        let evicted: Vec<Arc<Job>> = {
            let mut jobs = lock(&self.jobs);
            let session_id = job.session_id.clone();
            jobs.push(job);
            let idle = |job: &Job| !job.running.load(Ordering::Acquire);
            let (kept, mut evicted): (Vec<_>, Vec<_>) = std::mem::take(&mut *jobs)
                .into_iter()
                .partition(|job| !idle(job) || job.parent.upgrade().is_some());
            *jobs = kept;
            // The oldest finished job, of one session or of them all.
            let oldest = |jobs: &[Arc<Job>], session: Option<&str>| {
                jobs.iter()
                    .enumerate()
                    .filter(|(_, job)| idle(job) && session.is_none_or(|id| job.session_id == id))
                    .min_by_key(|(_, job)| *lock(&job.last_used))
                    .map(|(index, _)| index)
            };
            while jobs.iter().filter(|j| j.session_id == session_id).count() > MAX_KEPT {
                let Some(index) = oldest(&jobs, Some(&session_id)) else {
                    break;
                };
                evicted.push(jobs.remove(index));
            }
            while jobs.len() > MAX_KEPT_ENGINE {
                let Some(index) = oldest(&jobs, None) else {
                    break;
                };
                evicted.push(jobs.remove(index));
            }
            evicted
        };
        for job in evicted {
            tokio::spawn(async move { job.close().await });
        }
    }

    /// Stop a running agent; false when it is not running.
    pub fn cancel(&self, id: &str) -> bool {
        let Some(job) = self.get(id) else {
            return false;
        };
        // Under the switch's lock: a run admitted meanwhile has its own
        // switch already ([`Job::admit`]), so this reaches that run.
        let cancel = lock(&job.cancel);
        if job.running.load(Ordering::Acquire) {
            cancel.cancel();
            true
        } else {
            false
        }
    }
}

impl Job {
    /// Take the right to run this agent once more: false when it runs. The
    /// run gets a new cancel switch at the same moment, so a cancel that
    /// arrives before the run starts stops it rather than a finished one.
    fn admit(&self) -> bool {
        let mut cancel = lock(&self.cancel);
        if self
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        *cancel = CancellationToken::new();
        true
    }

    async fn close(&self) {
        mesh::router().transition(&self.address, NodeState::Closed, None, None);
        let child = lock(&self.child).take();
        if let Some(child) = child {
            let _ = child.harness.close().await;
        }
        let worktree = lock(&self.worktree).take();
        if let Some(created) = worktree {
            crate::child_agents::finish_worktree(created, self.cwd.clone()).await;
        }
    }
}

// ── Running ─────────────────────────────────────────────────────────────────

enum Outcome {
    Report(String),
    Failed(String),
}

/// One run of `job` on `brief`. `foreground` is the waiting tool call's signal;
/// `scope` ends with the parent's run (foreground) or session (background).
async fn run(
    job: Arc<Job>,
    brief: String,
    foreground: Option<CancellationToken>,
    scope: CancellationToken,
) -> Outcome {
    let Some(child) = lock(&job.child).clone() else {
        return Outcome::Failed("the agent was closed".into());
    };
    // Set when the run was admitted ([`Job::admit`]), or with the job.
    let cancel = lock(&job.cancel).clone();
    {
        let mut progress = lock(&job.progress);
        progress.status = "running";
        // The clock is this run's; the counters and the usage go on.
        progress.started_at = now_ms();
        progress.ended_at = None;
        progress.error = None;
        progress.text.clear();
        progress.said.clear();
    }
    let call_id = lock(&job.call_id).clone();
    let background = job.background.load(Ordering::Acquire);
    mesh::router().set_meta(
        &job.address,
        json!({ "callId": call_id, "background": background }),
    );
    mesh::router().transition(&job.address, NodeState::Running, None, None);
    let lane = child.harness.lane("main");
    let prompt = lane.prompt_text(&brief, Vec::new());
    tokio::pin!(prompt);
    let foreground_abort = async {
        match &foreground {
            Some(signal) => signal.cancelled().await,
            None => std::future::pending::<()>().await,
        }
    };
    let outcome = tokio::select! {
        biased;
        _ = cancel.cancelled() => Err("cancelled".to_string()),
        _ = foreground_abort => Err("cancelled".to_string()),
        // Read now: a foreground run may have moved to the background since it started.
        _ = scope.cancelled() => Err(if job.background.load(Ordering::Acquire) {
            "the session closed"
        } else {
            "the parent run ended"
        }
        .to_string()),
        _ = tokio::time::sleep(TIMEOUT) => Err(format!("timed out after {} minutes", TIMEOUT.as_secs() / 60)),
        result = &mut prompt => result.map_err(|e| e.to_string()),
    };
    if outcome.is_err() {
        let _ = lane.abort().await;
        let _ = tokio::time::timeout(Duration::from_secs(2), &mut prompt).await;
    }
    let _ = tokio::time::timeout(Duration::from_secs(5), child.execution_scope.wait_idle()).await;
    match outcome {
        Ok(result) => {
            let outcome = crate::extensions_actions::run_outcome_json(result.outcome);
            let text = outcome
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if outcome.get("status").and_then(Value::as_str) == Some("completed") {
                Outcome::Report(if text.is_empty() {
                    // It ended on a step without text (its turn limit, say).
                    format!("(the subagent returned no text){}", last_words(&job))
                } else {
                    text
                })
            } else {
                let status = outcome["status"].as_str().unwrap_or("failed");
                Outcome::Failed(match outcome["error"].as_str() {
                    Some(error) if !error.is_empty() => format!(
                        "its run {status}: {}",
                        error.chars().take(300).collect::<String>()
                    ),
                    _ => format!("its run {status}"),
                })
            }
        }
        Err(reason) => Outcome::Failed(reason),
    }
}

/// A report cut to [`MAX_REPORT`] bytes, saying how to get the rest (an agent
/// that cannot continue has no rest to give).
fn capped(mut text: String, id: &str, resumable: bool) -> String {
    if text.len() <= MAX_REPORT {
        return text;
    }
    let total = text.len();
    let mut cut = MAX_REPORT;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
    text.push_str(&if resumable {
        format!(
            "\n\n[Report cut at {cut} of {total} bytes. Ask agent {id} with message_agent for the rest or a shorter summary.]"
        )
    } else {
        format!("\n\n[Report cut at {cut} of {total} bytes; the agent was closed, so the rest is lost.]")
    });
    text
}

/// What the agent last said in this run, for a run that ended without a
/// report: ` Its last words:\n…` (the kept tail, at most [`MAX_TEXT`] bytes),
/// or nothing when it said nothing.
fn last_words(job: &Job) -> String {
    let progress = lock(&job.progress);
    let said = match progress.text.trim() {
        "" => progress.said.trim(),
        text => text,
    };
    if said.is_empty() {
        String::new()
    } else {
        format!(" Its last words:\n{said}")
    }
}

/// Record how a run ended and settle the worktree. Returns the final snapshot.
async fn settle(job: &Job, outcome: &Outcome, cancelled: bool) -> Value {
    // An unchanged worktree is removed; the agent cannot continue without it.
    let created = lock(&job.worktree).clone();
    let mut worktree_report = None;
    if let Some(created) = created {
        if crate::worktrees::Worktrees::changed(&created.worktree.path, Some(&created.base)) {
            worktree_report = Some(crate::child_agents::worktree_json(&created, Some(true)));
        } else {
            lock(&job.worktree).take();
            let child = lock(&job.child).take();
            if let Some(child) = child {
                let _ = child.harness.close().await;
            }
            worktree_report =
                Some(crate::child_agents::finish_worktree(created, job.cwd.clone()).await);
        }
    }
    let mut progress = lock(&job.progress);
    progress.status = match outcome {
        Outcome::Report(_) => "done",
        Outcome::Failed(_) if cancelled => "cancelled",
        Outcome::Failed(_) => "error",
    };
    progress.ended_at = Some(now_ms());
    progress.error = match outcome {
        Outcome::Failed(reason) => Some(reason.clone()),
        Outcome::Report(_) => None,
    };
    if let Some(report) = worktree_report {
        progress.worktree = Some(report);
    }
    *lock(&job.last_used) = Instant::now();
    // Not running first, then the slot: whoever gets it may continue this agent.
    job.running.store(false, Ordering::Release);
    lock(&job.slot).take();
    let snapshot = progress.snapshot();
    let usage = mesh::usage_of(&progress.usage);
    drop(progress);
    announce(job, outcome, &snapshot, usage);
    snapshot
}

/// Tell the mesh how a run ended: the report, then the settled state.
fn announce(job: &Job, outcome: &Outcome, snapshot: &Value, usage: MeshUsage) {
    let router = mesh::router();
    let status = match snapshot["status"].as_str() {
        Some("done") => RunStatus::Done,
        Some("cancelled") => RunStatus::Cancelled,
        _ => RunStatus::Failed,
    };
    let worktree = snapshot.get("worktree").filter(|w| !w.is_null()).cloned();
    let resumable = lock(&job.child).is_some();
    router.set_meta(
        &job.address,
        json!({ "worktree": worktree, "resumable": resumable }),
    );
    router.usage(&job.address, usage);
    let report = report_text(job, outcome, snapshot);
    // `SubagentStop` hooks (session-roles-and-mail FR-10): once per ended run.
    crate::hooks::subagent_stop(snapshot, &report);
    router.report(&job.address, status, report.clone(), usage, worktree);
    let reason = snapshot["error"].as_str().map(str::to_string);
    router.transition(&job.address, NodeState::Settled, Some(status), reason);
    // Questions it did not `reply` to get its report as their answer.
    crate::tools::mesh::run_ended(&job.address, &report);
}

/// The report for the requester (call it once the run is settled): cut to
/// [`MAX_REPORT`]; for a failed run, why it stopped and what it last said;
/// then a kept worktree's place.
fn report_text(job: &Job, outcome: &Outcome, snapshot: &Value) -> String {
    let mut text = match outcome {
        Outcome::Report(text) => capped(text.clone(), &job.id, lock(&job.child).is_some()),
        Outcome::Failed(reason) => {
            format!("The subagent stopped: {reason}.{}", last_words(job))
        }
    };
    text.push_str(&kept_worktree(snapshot));
    text
}

/// Where a kept worktree is, as the last paragraph of a report.
fn kept_worktree(snapshot: &Value) -> String {
    if snapshot["worktree"]["kept"] != json!(true) {
        return String::new();
    }
    format!(
        "\n\n[Its changes are in the worktree {} on branch {}.]",
        snapshot["worktree"]["path"].as_str().unwrap_or(""),
        snapshot["worktree"]["branch"].as_str().unwrap_or("")
    )
}

/// A background run ended: queue its report for the parent — into the
/// running run, or for the next one with a wake request for the host.
async fn tell_parent(job: &Job, outcome: &Outcome, snapshot: &Value) {
    if job.quiet.swap(false, Ordering::AcqRel) {
        return;
    }
    let Some(parent) = job.parent.upgrade() else {
        return;
    };
    let status = snapshot["status"].as_str().unwrap_or("done");
    let note = format!(
        "{NOTE_TAG} id=\"{}\" description=\"{}\" status=\"{status}\">\n{}\n</background-agent>",
        job.id,
        attribute(snapshot["description"].as_str().unwrap_or("")),
        inert(&report_text(job, outcome, snapshot))
    );
    let message = AgentMessage::from(evo_ai::types::UserMessage::new(note, now_ms() as i64));
    let lane = parent.lane("main");
    let queued = if parent.has_active_run() {
        lane.steer(message).await
    } else {
        lane.next_run(message).await
    };
    // Idle now (or its run ended meanwhile): it needs a run to read the report.
    if queued.is_ok() && !parent.has_active_run() {
        mesh::router().request_wake(&Address::session(&job.session_id), &job.address, None);
    }
}

fn result(text: String, snapshot: Value, usage: Option<Usage>) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextContentOrImage::text(text)],
        details: json!({ "child": snapshot }),
        usage,
        ..Default::default()
    }
}

/// `after` minus `before`: one run's usage on a cumulative counter.
fn usage_since(after: &Usage, before: &Usage) -> Usage {
    let mut usage = after.clone();
    usage.input -= before.input;
    usage.output -= before.output;
    usage.cache_read -= before.cache_read;
    usage.cache_write -= before.cache_write;
    usage.total_tokens -= before.total_tokens;
    usage.cost.input -= before.cost.input;
    usage.cost.output -= before.cost.output;
    usage.cost.cache_read -= before.cost.cache_read;
    usage.cost.cache_write -= before.cost.cache_write;
    usage.cost.total -= before.cost.total;
    usage
}

/// Run `job` on `brief` for a tool call: wait for it (foreground) or let it
/// run on (background).
async fn drive(
    job: Arc<Job>,
    brief: String,
    background: bool,
    scope: CancellationToken,
    signal: CancellationToken,
    on_update: Update,
) -> Result<AgentToolResult, AgentError> {
    job.background.store(background, Ordering::Release);
    lock(&job.progress).background = background;
    if background {
        *lock(&job.update) = None;
        let snapshot = lock(&job.progress).snapshot();
        let worker = job.clone();
        tokio::spawn(async move {
            let outcome = run(worker.clone(), brief, None, scope).await;
            let cancelled = lock(&worker.cancel).is_cancelled();
            let snapshot = settle(&worker, &outcome, cancelled).await;
            tell_parent(&worker, &outcome, &snapshot).await;
        });
        return Ok(result(
            format!(
                "Started background agent {} ({}). It keeps working after this turn; its report will be \
                 delivered to you as a message when it finishes. Do not wait for it or poll it.",
                job.id,
                snapshot["description"].as_str().unwrap_or("")
            ),
            snapshot,
            None,
        ));
    }

    let before = lock(&job.progress).usage.clone();
    let Some(session) = job
        .parent
        .upgrade()
        .map(|parent| parent.session_scope_token())
    else {
        // Settled, so the agent is not left "running" with a slot it never used.
        let outcome = Outcome::Failed("the session is gone".into());
        let snapshot = settle(&job, &outcome, false).await;
        return Err(failure(
            &job,
            "the session is gone",
            snapshot,
            Usage::zero(),
        ));
    };
    *lock(&job.update) = Some(on_update.clone());
    // The session waits on this agent until it reports: the agent asking the
    // session would never end (tools/mesh.rs refuses it).
    let _waits = mesh::router().add_wait(&Address::session(&job.session_id), &job.address);
    on_update(partial(lock(&job.progress).snapshot()));
    let mut steered = steering(&job.session_id);

    // The run goes in its own task, so a steering message can release this
    // call while the agent keeps working (then on the session's scope).
    let state = Arc::new(std::sync::atomic::AtomicU8::new(ATTACHED));
    let stop = CancellationToken::new();
    let life = CancellationToken::new();
    tokio::spawn(lifetime(
        state.clone(),
        stop.clone(),
        life.clone(),
        signal.clone(),
        scope,
        session,
    ));
    let (sender, receiver) = tokio::sync::oneshot::channel();
    {
        let (job, state, life) = (job.clone(), state.clone(), life.clone());
        tokio::spawn(async move {
            let outcome = run(job.clone(), brief, Some(stop), life.clone()).await;
            if state
                .compare_exchange(ATTACHED, FINISHED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let _ = sender.send(outcome);
            } else {
                // Released: it reports like a background agent.
                let cancelled = lock(&job.cancel).is_cancelled();
                let snapshot = settle(&job, &outcome, cancelled).await;
                tell_parent(&job, &outcome, &snapshot).await;
            }
            life.cancel();
        });
    }
    tokio::pin!(receiver);
    let mut watching = true;
    let outcome = loop {
        tokio::select! {
            outcome = &mut receiver => break outcome.unwrap_or_else(|_| Outcome::Failed("its task ended".into())),
            changed = steered.changed(), if watching => {
                if changed.is_err() {
                    watching = false;
                } else if state.compare_exchange(ATTACHED, DETACHED, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                    return Ok(released(&job));
                }
                // Otherwise it finished meanwhile: its outcome is on the way.
            }
        }
    };
    let cancelled = signal.is_cancelled() || lock(&job.cancel).is_cancelled();
    let snapshot = settle(&job, &outcome, cancelled).await;
    *lock(&job.update) = None;
    on_update(partial(snapshot.clone()));
    // This run's usage only: earlier runs of a continued agent were counted then.
    let usage = usage_since(&lock(&job.progress).usage, &before);
    match &outcome {
        Outcome::Report(_) => {
            let mut text = report_text(&job, &outcome, &snapshot);
            // After the report and outside its size cap: the model sees only
            // this text, and `message_agent` needs the id.
            if lock(&job.child).is_some() {
                text.push_str(&format!(
                    "\n\n[This agent's id is {}: pass it as agentId to message_agent to continue it with its context.]",
                    job.id
                ));
            }
            Ok(result(text, snapshot, Some(usage)))
        }
        Outcome::Failed(reason) => Err(failure(&job, reason, snapshot, usage)),
    }
}

/// A foreground run that did not end with a report, as the call's error. The
/// model reads the text an [`AgentError::Tool`] would give it, plus what the
/// agent last said; unlike that error, the result keeps the agent's final
/// snapshot (`details.child`) and this run's usage, for the transcript and
/// the cost of the turn.
fn failure(job: &Job, reason: &str, snapshot: Value, usage: Usage) -> AgentError {
    let words = last_words(job);
    let text = format!(
        "tool error: subagent {} stopped: {reason}{}{words}{}",
        job.id,
        if words.is_empty() { "" } else { "." },
        kept_worktree(&snapshot)
    );
    AgentError::StructuredTool(Box::new(result(text, snapshot, Some(usage))))
}

/// How long a foreground run may go on. While its call waits (`ATTACHED`), the
/// call's signal stops it (`stop`: cancelled) and the end of the call or the
/// parent's run ends it (`life`); once released (`DETACHED`) only the
/// session's end does. Returns when the run is over (`life` cancelled).
async fn lifetime(
    state: Arc<std::sync::atomic::AtomicU8>,
    stop: CancellationToken,
    life: CancellationToken,
    signal: CancellationToken,
    scope: CancellationToken,
    session: CancellationToken,
) {
    tokio::select! {
        biased;
        _ = life.cancelled() => return,
        _ = session.cancelled() => {
            life.cancel();
            return;
        }
        _ = signal.cancelled() => {}
        _ = scope.cancelled() => {}
    }
    if state.load(Ordering::Acquire) != DETACHED {
        if signal.is_cancelled() {
            stop.cancel();
        } else {
            life.cancel();
        }
        return;
    }
    tokio::select! {
        _ = life.cancelled() => {}
        _ = session.cancelled() => life.cancel(),
    }
}

/// A foreground agent's call let go on a steering message: the agent goes on
/// in the background, and the parent reads the message now.
fn released(job: &Job) -> AgentToolResult {
    job.background.store(true, Ordering::Release);
    *lock(&job.update) = None;
    let snapshot = {
        let mut progress = lock(&job.progress);
        progress.background = true;
        progress.snapshot()
    };
    mesh::router().set_meta(&job.address, json!({ "background": true }));
    result(
        format!(
            "A new message arrived while agent {} ({}) was working, so it moved to the background and keeps \
             working. Its report will be delivered to you as a message when it finishes; do not wait for it \
             or poll it. Read the new message now.",
            job.id,
            snapshot["description"].as_str().unwrap_or("")
        ),
        snapshot,
        None,
    )
}

/// Put a mail message in front of the agent at `address` (tools/mesh.rs):
/// into its current run, or for its next one. A question (`ask`) to an idle
/// agent continues it in the background to answer.
pub(crate) async fn deliver(
    address: &Address,
    text: String,
    ask: bool,
) -> Result<&'static str, String> {
    let job = lock(&registry().jobs)
        .iter()
        .find(|job| job.address == *address)
        .cloned()
        .ok_or("that agent is gone")?;
    let child = lock(&job.child)
        .clone()
        .ok_or("that agent was closed and takes no messages")?;
    let lane = child.harness.lane("main");
    let message =
        |text: String| AgentMessage::from(evo_ai::types::UserMessage::new(text, now_ms() as i64));
    if job.running.load(Ordering::Acquire) {
        lane.steer(message(text)).await.map_err(|e| e.to_string())?;
        return Ok("delivered into its current run");
    }
    if !ask {
        lane.next_run(message(text))
            .await
            .map_err(|e| e.to_string())?;
        return Ok("queued: it reads it when it is continued");
    }
    let parent = job.parent.upgrade().ok_or("the session is gone")?;
    let slot = slot_now(&job.session_id).map_err(|e| e.to_string())?;
    if !job.admit() {
        // Another question continued it just now: this one joins that run.
        lane.steer(message(text)).await.map_err(|e| e.to_string())?;
        return Ok("delivered into its current run");
    }
    *lock(&job.slot) = Some(slot);
    job.quiet.store(true, Ordering::Release);
    let scope = parent.session_scope_token();
    drive(
        job,
        text,
        true,
        scope,
        CancellationToken::new(),
        Arc::new(|_| {}),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok("it was continued to answer")
}

/// A child whose setup failed after it was created: close it and remove its
/// (still unchanged) worktree.
async fn discard(
    child: &AgentHarness,
    worktree: Option<crate::worktrees::Created>,
    cwd: &std::path::Path,
) {
    let _ = child.close().await;
    if let Some(created) = worktree {
        crate::child_agents::finish_worktree(created, cwd.to_path_buf()).await;
    }
}

fn parent_of(cell: &HarnessCell) -> Result<AgentHarness, AgentError> {
    cell.get()
        .and_then(WeakAgentHarness::upgrade)
        .ok_or_else(|| AgentError::Tool("the session is not ready for subagents".into()))
}

fn refuse_nested() -> Result<(), AgentError> {
    if evo_agent::tool_execution_scope::is_child_execution() {
        return Err(AgentError::Tool(
            "subagents cannot start or message subagents".into(),
        ));
    }
    Ok(())
}

/// The right to run one agent; a session has [`MAX_RUNNING`]. A running
/// agent holds it ([`Job::slot`]) until its run is settled.
type Slot = tokio::sync::OwnedSemaphorePermit;

/// Free run slots, per session.
static SLOTS: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Semaphore>>>> =
    LazyLock::new(Default::default);

fn slots(session_id: &str) -> Arc<tokio::sync::Semaphore> {
    lock(&SLOTS)
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(MAX_RUNNING)))
        .clone()
}

/// A run slot if one is free: background calls do not wait for one.
fn slot_now(session_id: &str) -> Result<Slot, AgentError> {
    slots(session_id).try_acquire_owned().map_err(|_| {
        AgentError::Tool(format!(
            "at most {MAX_RUNNING} subagents run at once; wait for one to finish"
        ))
    })
}

/// A run slot for a foreground call, waiting for one while [`MAX_RUNNING`]
/// agents run (in the order the calls came). The wait ends at once when the
/// call is cancelled or the session closes, and on a steering message, which
/// the parent's run must read now (a running agent moves to the background
/// then; one still waiting here does not run).
async fn wait_for_slot(
    parent: &AgentHarness,
    session_id: &str,
    signal: &CancellationToken,
) -> Result<Slot, AgentError> {
    let slots = slots(session_id);
    if let Ok(slot) = slots.clone().try_acquire_owned() {
        return Ok(slot);
    }
    let session = parent.session_scope_token();
    let mut steered = steering(session_id);
    let stopped = |why: &str| {
        AgentError::Tool(format!(
            "{why} while this call waited for a free subagent slot (at most {MAX_RUNNING} run at once), \
             so the subagent did not run"
        ))
    };
    tokio::select! {
        biased;
        _ = signal.cancelled() => Err(stopped("the call was cancelled")),
        _ = session.cancelled() => Err(stopped("the session closed")),
        Ok(()) = steered.changed() => Err(stopped(
            "a new message arrived (read it now, then call again if this is still needed)",
        )),
        slot = slots.acquire_owned() => slot.map_err(|_| stopped("the session closed")),
    }
}

/// A model by id (`id` or `provider/id`, any case) among the harness's models.
fn find_model(models: &[evo_ai::Model], wanted: &str) -> Option<evo_ai::Model> {
    let wanted = wanted.trim();
    models
        .iter()
        .find(|m| {
            m.id.eq_ignore_ascii_case(wanted)
                || format!("{}/{}", m.provider, m.id).eq_ignore_ascii_case(wanted)
        })
        .cloned()
}

// ── The `agent` tool ────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    #[serde(default)]
    description: Option<String>,
    prompt: String,
    #[serde(default)]
    agent_type: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    isolation: Option<String>,
    #[serde(default)]
    background: bool,
}

pub struct SubagentTool {
    schema: Tool,
    harness: HarnessCell,
    cwd: PathBuf,
}

pub fn create_agent_tool(harness: HarnessCell, cwd: PathBuf) -> Arc<dyn AgentTool> {
    let types = agents::load(&cwd);
    Arc::new(SubagentTool {
        schema: schema(&types),
        harness,
        cwd,
    })
}

fn schema(types: &[AgentType]) -> Tool {
    let listing: String = types
        .iter()
        .map(|t| format!("\n- {}: {}", t.name, t.description))
        .collect();
    let names: Vec<&str> = types.iter().map(|t| t.name.as_str()).collect();
    Tool {
        name: "agent".into(),
        description: format!(
            "Hand a self-contained task to a subagent and get its final report back. The subagent starts \
             with ONLY your prompt (not this conversation), so the prompt must carry everything it needs: \
             the goal, what you already know or ruled out, the files or areas involved, and what the report \
             should contain. Its intermediate steps stay out of this conversation, which keeps your context \
             small: use it for wide searches, investigations across many files, or independent pieces of \
             work. Call it several times in one message to run independent tasks in parallel (at most \
             {MAX_RUNNING} run at once: further calls wait for a free slot, background ones are refused). \
             Do not delegate a quick lookup you can do with one or two tool calls. \
             Subagents cannot start subagents. Set isolation to \"worktree\" to have it work in its own Git \
             worktree: it starts from the last commit and does not contain the workspace's uncommitted \
             changes; a worktree it changed is kept and its path and branch are reported. Set background \
             to true to let it run while you continue: the call returns its id at once and its report \
             arrives as a message when it finishes (do not wait for it or poll it). A finished agent keeps \
             its context: continue it with message_agent and the id its result ends with.\n\nAgent types:{listing}"
        ),
        parameters: Schema::new(json!({
            "type": "object",
            "properties": {
                "description": { "type": "string", "description": "A short (3-5 words) description of the task, shown to the user." },
                "prompt": { "type": "string", "description": "The complete brief for the subagent." },
                // No enum: agent files added later must work without a reload.
                "agentType": { "type": "string", "description": format!("Which kind of agent: {}. Default: general.", names.join(", ")) },
                "model": { "type": "string", "description": "Optional model id for the subagent; default: its type's model, else yours." },
                "isolation": { "type": "string", "enum": ["worktree"], "description": "Run in a separate Git worktree of this repository." },
                "background": { "type": "boolean", "description": "Run in the background and get its report later as a message." }
            },
            "required": ["description", "prompt"],
            "additionalProperties": false
        })),
        constrained_sampling: None,
    }
}

#[async_trait]
impl AgentTool for SubagentTool {
    fn schema(&self) -> &Tool {
        &self.schema
    }

    fn label(&self) -> &str {
        "agent"
    }

    fn execution_mode(&self) -> ToolExecutionMode {
        ToolExecutionMode::Parallel
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        params: Value,
        signal: CancellationToken,
        on_update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let input: Input = serde_json::from_value(params)
            .map_err(|e| AgentError::Validation(format!("agent input invalid: {e}")))?;
        refuse_nested()?;
        let task = input.prompt.trim();
        if task.is_empty() || task.len() > 50_000 {
            return Err(AgentError::Validation(
                "prompt must contain 1-50000 bytes".into(),
            ));
        }
        let description = input
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .unwrap_or("Agent");
        let description: String = description.chars().take(200).collect();

        // Agent types are read per call, so a new `.evo/agents` file works at once.
        let types = agents::load(&self.cwd);
        let wanted = input.agent_type.as_deref().unwrap_or(agents::DEFAULT_TYPE);
        let agent = types
            .iter()
            .find(|t| t.name == wanted)
            .cloned()
            .ok_or_else(|| {
                let names: Vec<_> = types.iter().map(|t| t.name.as_str()).collect();
                AgentError::Validation(format!(
                    "unknown agent type `{wanted}`; available: {}",
                    names.join(", ")
                ))
            })?;
        let isolated = match input.isolation.as_deref() {
            None => agent.worktree,
            Some("worktree") => true,
            Some("none") => false,
            Some(other) => {
                return Err(AgentError::Validation(format!(
                    "unknown isolation `{other}`"
                )))
            }
        };
        let parent = parent_of(&self.harness)?;
        let session_id = parent.session().storage().metadata().id.clone();
        // Before anything is created; the job holds it until its run is settled.
        let slot = if input.background {
            slot_now(&session_id)?
        } else {
            wait_for_slot(&parent, &session_id, &signal).await?
        };

        // An agent type's tool list narrows the parent's active tools.
        let requested = agent.tools.as_ref().map(|wanted| {
            let active: Vec<String> = parent
                .execution_context()
                .ok()
                .and_then(|context| context.get("tools").cloned())
                .and_then(|tools| serde_json::from_value(tools).ok())
                .unwrap_or_default();
            // Talking to other agents is part of every agent type.
            wanted
                .iter()
                .map(String::as_str)
                .chain(crate::tools::mesh::TOOL_NAMES)
                .filter(|name| active.iter().any(|a| a == name))
                .map(str::to_string)
                .collect::<Vec<_>>()
        });
        let model =
            match input.model.as_deref().or(agent.model.as_deref()) {
                Some(id) => Some(find_model(&parent.available_models(), id).ok_or_else(|| {
                    AgentError::Validation(format!("model `{id}` is not available"))
                })?),
                None => None,
            };

        let worktree = if isolated {
            Some(
                crate::child_agents::create_worktree(&self.cwd)
                    .await
                    .map_err(|e| AgentError::Tool(format!("could not create a worktree: {e}")))?,
            )
        } else {
            None
        };
        let mut replacements = worktree
            .as_ref()
            .map(|created| crate::tools::builtin_tools_in(&created.cwd))
            .unwrap_or_default();

        let id = parent.session().id_generator().next();
        let progress = Arc::new(Mutex::new(Progress {
            agent_id: id.clone(),
            agent_type: agent.name.clone(),
            description: description.clone(),
            model: model.as_ref().map(|m| m.id.clone()).unwrap_or_default(),
            background: input.background,
            status: "running",
            turns: 0,
            tool_uses: 0,
            usage: Usage::zero(),
            started_at: now_ms(),
            ended_at: None,
            rows: VecDeque::new(),
            text: String::new(),
            said: String::new(),
            worktree: worktree
                .as_ref()
                .map(|created| crate::child_agents::worktree_json(created, None)),
            error: None,
        }));
        let update: Arc<Mutex<Option<Update>>> = Arc::new(Mutex::new(None));
        let address = Address::session(&session_id).child(NodeKind::Agent, &id);
        let emitter = Arc::new(ProgressEmitter {
            progress: progress.clone(),
            update: update.clone(),
            last: Mutex::new(None),
            address: address.clone(),
        });
        let session = Session::new(
            Arc::new(InMemorySessionStorage::new(
                SessionMetadata {
                    id: id.clone(),
                    created_at: 0,
                    parent_session_id: Some(parent.session().storage().metadata().id.clone()),
                },
                Arc::new(SystemClock),
                Arc::new(DefaultIdGenerator::new()),
            )),
            None,
        );
        // Its own send_message / ask / reply / list_nodes, acting as its node.
        replacements.extend(
            crate::tools::mesh::create(self.harness.clone(), Some(address.clone()))
                .into_iter()
                .map(evo_harness::types::HarnessTool::new),
        );
        // A read-only type is held to it by a guard on every tool call.
        let guard = agent.read_only.then(|| {
            Arc::new(crate::read_only::guard) as evo_harness::agent_harness::ChildToolGuard
        });
        let child = match parent
            .create_child_guarded(
                session,
                emitter,
                agent.max_turns(),
                requested,
                replacements,
                guard,
            )
            .await
        {
            Ok(child) => child,
            Err(error) => {
                if let Some(created) = worktree {
                    crate::child_agents::finish_worktree(created, self.cwd.clone()).await;
                }
                return Err(AgentError::Tool(format!(
                    "could not start the subagent: {error}"
                )));
            }
        };
        let lane = child.harness.lane("main");
        if let Some(model) = model {
            if let Err(error) = lane.set_model(model).await {
                discard(&child.harness, worktree, &self.cwd).await;
                return Err(AgentError::Tool(format!(
                    "could not set the subagent's model: {error}"
                )));
            }
        } else if let Ok(model) = lane.get_model().await {
            lock(&progress).model = model.id;
        }
        // The type's thinking level, as close as its model allows.
        if let (Some(level), Ok(model)) = (agent.thinking, lane.get_model().await) {
            if model.reasoning {
                if let Err(error) = lane
                    .set_thinking_level(model.clamp_thinking_level(level))
                    .await
                {
                    discard(&child.harness, worktree, &self.cwd).await;
                    return Err(AgentError::Tool(format!(
                        "could not set the subagent's thinking level: {error}"
                    )));
                }
            }
        }
        // A background child outlives this run; a foreground one ends with it.
        let scope = if input.background {
            parent.session_scope_token()
        } else {
            child.scope.clone()
        };

        let place = match &worktree {
            Some(created) => format!(
                "\n\nYour working folder is an isolated Git worktree at {} on branch `{}`, cut from the last \
                 commit: the workspace's uncommitted changes are not in it. Make your changes there and do \
                 not commit them: the requester reviews them.",
                created.cwd.display(),
                created.worktree.branch
            ),
            None => String::new(),
        };
        let brief = format!(
            "{}{place}\n\nYou are a subagent: you cannot delegate to other agents, and the requester sees only \
             your final reply, so end with a complete, concise report.\n\nTask:\n{task}",
            agent.prompt
        );

        let meta = {
            let progress = lock(&progress);
            json!({
                "agentType": agent.name,
                "model": progress.model,
                "background": input.background,
                "callId": tool_call_id,
                "worktree": progress.worktree,
                "resumable": true,
            })
        };
        let cancel_id = id.clone();
        mesh::router().spawn(
            address.clone(),
            &description,
            Origin::Agent,
            meta,
            Some(Arc::new(move || registry().cancel(&cancel_id))),
        );
        let job = Arc::new(Job {
            id,
            address,
            parent: parent.downgrade(),
            session_id,
            call_id: Mutex::new(tool_call_id.to_string()),
            progress,
            update,
            child: Mutex::new(Some(ChildParts {
                harness: child.harness.clone(),
                execution_scope: child.execution_scope.clone(),
            })),
            cancel: Mutex::new(CancellationToken::new()),
            running: AtomicBool::new(true),
            background: AtomicBool::new(input.background),
            quiet: AtomicBool::new(false),
            worktree: Mutex::new(worktree),
            cwd: self.cwd.clone(),
            last_used: Mutex::new(Instant::now()),
            slot: Mutex::new(Some(slot)),
        });
        registry().insert(job.clone());
        drive(job, brief, input.background, scope, signal, on_update).await
    }
}

// ── The `message_agent` tool ────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageInput {
    agent_id: String,
    message: String,
    #[serde(default)]
    background: bool,
}

pub struct MessageAgentTool {
    schema: Tool,
    harness: HarnessCell,
}

pub fn create_message_agent_tool(harness: HarnessCell) -> Arc<dyn AgentTool> {
    Arc::new(MessageAgentTool {
        harness,
        schema: Tool {
            name: "message_agent".into(),
            description: "Send a finished subagent another message; it continues with its own context (what it \
                          read and did before), which is cheaper than starting a new agent with the whole \
                          brief again. Name it by its id: the last line of its agent result, or the id in \
                          the message that started or reported it. Set background to true to let it run \
                          while you continue; its reply then arrives as a message."
                .into(),
            parameters: Schema::new(json!({
                "type": "object",
                "properties": {
                    "agentId": { "type": "string", "description": "The subagent's id. Its node address (from list_nodes) or its description also work." },
                    "message": { "type": "string", "description": "What to tell or ask it." },
                    "background": { "type": "boolean", "description": "Run in the background." }
                },
                "required": ["agentId", "message"],
                "additionalProperties": false
            })),
            constrained_sampling: None,
        },
    })
}

#[async_trait]
impl AgentTool for MessageAgentTool {
    fn schema(&self) -> &Tool {
        &self.schema
    }

    fn label(&self) -> &str {
        "message_agent"
    }

    fn execution_mode(&self) -> ToolExecutionMode {
        ToolExecutionMode::Parallel
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        params: Value,
        signal: CancellationToken,
        on_update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let input: MessageInput = serde_json::from_value(params)
            .map_err(|e| AgentError::Validation(format!("message_agent input invalid: {e}")))?;
        refuse_nested()?;
        let message = input.message.trim();
        if message.is_empty() || message.len() > 50_000 {
            return Err(AgentError::Validation(
                "message must contain 1-50000 bytes".into(),
            ));
        }
        let parent = parent_of(&self.harness)?;
        let session_id = parent.session().storage().metadata().id.clone();
        let job = registry().find(&session_id, &input.agent_id)?;
        let busy = || AgentError::Tool(format!("subagent {} is still running", job.id));
        if job.running.load(Ordering::Acquire) {
            return Err(busy());
        }
        if lock(&job.child).is_none() {
            return Err(AgentError::Tool(format!(
                "subagent {} cannot continue (it was closed, or its unchanged worktree was removed); start a new one",
                job.id
            )));
        }
        let slot = if input.background {
            slot_now(&session_id)?
        } else {
            wait_for_slot(&parent, &session_id, &signal).await?
        };
        // Only one call continues it (another may have, while this one waited).
        if !job.admit() {
            return Err(busy());
        }
        *lock(&job.slot) = Some(slot);
        *lock(&job.call_id) = tool_call_id.to_string();
        // Background: on the session's scope; foreground: ends with this call.
        let scope = if input.background {
            parent.session_scope_token()
        } else {
            signal.child_token()
        };
        drive(
            job,
            message.to_string(),
            input.background,
            scope,
            signal,
            on_update,
        )
        .await
    }
}

#[cfg(test)]
mod tests;
