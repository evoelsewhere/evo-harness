//! The engine supervisor: the shell starts chats' engines and sets them up
//! (docs/features/shell-hosted-engines/spec.md, FR-1 – FR-4, FR-9, FR-10,
//! NFR-1, NFR-3, NFR-5; plan 3.1 – 3.7; ADR 0018).
//!
//! Everything that needs a chat's engine asks [`EngineSupervisor::ensure`]:
//! a window that shows the chat (`engine_ensure`), the mesh hub's request
//! for a host (`Effect::Host`, FR-4) and the app's start, for chats with
//! undelivered mail. Only the supervisor starts engines
//! (`engine::start_process`), and it starts one per chat.
//!
//! When an engine says `ready`, the supervisor gives it the set-up a window
//! gave it before (FR-1), once, whichever window watches:
//!
//! 1. the chat row's model (`set_model`), or the saved default model for a
//!    chat that names none; a reopened chat keeps the model of its session;
//! 2. its tools (`configure_external_tools`): the session tools for a
//!    top-level chat, then the shell's app and connector tools for its
//!    folder. A refusal because a run is active is tried again when the run
//!    settles;
//! 3. a mesh subscription, so the shell follows the chat's agents (FR-3);
//! 4. the hub's `on_hosted`: mail waiting for the chat goes out.
//!
//! Then it emits `engine://state` `ready`; a window's prompts wait for it.
//! Tools are set up again when they change: connectors, the shell's own
//! tools, a chat's connector switches, a chat becoming top-level.
//!
//! Windows are viewers (FR-2): `engine_attach` says a window shows a chat
//! (a window shows one at a time), `engine_detach` that it no longer does,
//! and a closed window shows none. Closing or reloading a window never
//! stops an engine. A chat has work while a run is active, one of its
//! agents works, reports or mail wait for it, or a question is open
//! ([`questions`]). Engines with no work and no viewer are stopped after a
//! run settles or a window lets go, beyond [`IDLE_ENGINES`] over the whole
//! app, least recently used first (FR-3). The shell announces questions and
//! the ends of runs in chats no focused window shows ([`notices`], FR-9,
//! FR-10); it never answers a question.
//!
//! An engine that crashes with mail waiting is restarted at most
//! [`RESTARTS`] times in [`RESTART_WINDOW`] (FR-4); one stopped on purpose
//! keeps its mail until new mail comes or a window opens the chat. One a
//! window stopped around a worktree Apply is held: its mail waits until the
//! window resumes it.
//!
//! The shell wakes idle chats for the reports, notes and mail that wait for
//! them, within the guard rails ([`wakes`], FR-5), and records every
//! finished turn in its chat's row ([`turns`], FR-6), once, whatever number
//! of windows are open (NFR-1).
//!
//! Settings reach every engine it runs, viewed or not: the approval mode,
//! providers, resources and the sandbox ([`settings`], FR-8).
//!
//! The tray, the quit question and the windows' sidebars read which chats
//! work or wait for the person from [`EngineSupervisor::work`] (FR-11,
//! FR-12, NFR-5; `system.rs`).

pub mod notices;
pub mod questions;
pub mod rpc;
pub mod settings;
pub mod turns;
pub mod wakes;

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use evo_store::chats;
use evo_store::logs::NewLog;
use evo_store::rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::chat_flags::ChatFlags;
use crate::db::Db;

/// Restarts of a crashed engine for its mail, at most, within [`RESTART_WINDOW`] (FR-4).
pub const RESTARTS: usize = 3;
/// See [`RESTARTS`].
pub const RESTART_WINDOW: Duration = Duration::from_secs(120);
/// Engines kept running with no work and no viewer, over the whole app (FR-3).
pub const IDLE_ENGINES: usize = 4;
/// An engine used or set up this recently is not stopped as idle yet: a
/// window may be about to send it a prompt (a scheduled task's first prompt,
/// an automated wake) that the shell does not see coming.
pub const IDLE_GRACE: Duration = Duration::from_secs(10);
/// Connector changes come in bursts (a server connecting): tools are set up
/// again once the burst is over.
const CONNECTORS_SETTLE: Duration = Duration::from_millis(300);

/// Why an engine's state changed, as `engine://state` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Started: a window shows the chat.
    Viewer,
    /// Started: the hub has mail for it (a new nested session's brief, a note, a message).
    Mail,
    /// Started: it crashed with mail waiting.
    Restart,
    /// Stopped: no work and no viewer, beyond the idle limit (FR-3).
    Idle,
    /// Still running: the last window that showed it closed (FR-2).
    Detached,
    /// Started again: Settings → Sandbox changed while it had no work (FR-8).
    Settings,
    /// Started: a scheduled task's run (FR-7).
    Schedule,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Viewer => "viewer",
            Reason::Mail => "mail",
            Reason::Restart => "restart",
            Reason::Idle => "idle",
            Reason::Detached => "detached",
            Reason::Settings => "settings",
            Reason::Schedule => "schedule",
        }
    }
}

/// What starting a chat's engine did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// A new process, of this generation.
    Started(u64),
    /// One already runs, of this generation.
    Running(u64),
}

/// What the supervisor reads of a chat's row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    /// The model the chat runs on: provider and model id.
    pub model: Option<(String, String)>,
    /// No other chat started it: it gets the session tools.
    pub top_level: bool,
    pub archived: bool,
}

impl Row {
    pub fn of(chat: &chats::Chat) -> Self {
        Row {
            model: chat.provider.clone().zip(chat.model.clone()),
            top_level: chat.parent_chat_id.is_none(),
            archived: chat.archived_at.is_some(),
        }
    }
}

/// What the supervisor needs of the app: the database, the processes, the
/// engines' stdin, the shell's tools, the hub and the windows. The app's is
/// [`AppHost`]; the tests use a fake engine.
pub trait Host: Send + Sync {
    /// `chat`'s row; `None` for a draft (no row yet) or a deleted chat.
    fn row(&self, chat: &str) -> Option<Row>;
    /// The folder `chat` works in: its worktree, else its workspace, else home.
    fn folder(&self, chat: &str) -> PathBuf;
    /// Start `chat`'s engine in `folder`, unless one runs and `restart` is false.
    fn start(&self, chat: &str, folder: &Path, options: Option<ChatFlags>, restart: bool) -> impl Future<Output = Result<Start, String>> + Send;
    /// Send an RPC command to `chat`'s engine and wait for its answer.
    fn request(&self, chat: &str, command: &str, payload: Value) -> impl Future<Output = Result<Value, String>> + Send;
    /// The external tools of `chat` working in `folder`: the session tools
    /// first when `top_level`, then the shell's app and connector tools.
    fn tools(&self, chat: &str, folder: &Path, top_level: bool) -> impl Future<Output = Vec<Value>> + Send;
    /// The hub's `on_hosted`: send what waits for `chat`.
    fn hosted(&self, chat: &str) -> impl Future<Output = ()> + Send;
    /// Tell the windows (`engine://state`); `restore`: it reopens a saved session.
    fn state(&self, chat: &str, generation: u64, status: &str, reason: Option<Reason>, restore: bool);
    fn log(&self, chat: &str, level: &str, message: String);
    /// Whether mail waits for `chat` in the hub's store (it counts as work, FR-3).
    fn pending_mail(&self, chat: &str) -> bool;
    /// Write a line to `chat`'s engine, expecting no response (a mesh command).
    fn send(&self, chat: &str, line: Value) -> impl Future<Output = Result<(), String>> + Send;
    /// Stop `chat`'s engine (not the person's stop: its mail stays).
    fn stop(&self, chat: &str) -> impl Future<Output = ()> + Send;
    /// Announce `event` of `chat` when no focused window shows it (FR-9, FR-10).
    fn notice(&self, chat: &str, event: notices::Event);
    /// Tell the windows a question of `chat` opened or closed (`engine://question`).
    fn question(&self, chat: &str, change: &questions::Change);
    /// Check `chat`'s wake of `tick` ([`EngineSupervisor::check_wake`]) once
    /// [`wakes::WAKE_DELAY`] has passed.
    fn wake_later(&self, chat: &str, tick: u64);
    /// Wake `chat` with `text`: hub mail delivered as its prompt (FR-5).
    fn wake(&self, chat: &str, text: String) -> impl Future<Output = ()> + Send;
    /// Whether window `label` is visible and focused.
    fn focused(&self, label: &str) -> bool;
    /// Count a finished turn in `chat`'s row and tell the windows (FR-6).
    fn record(&self, chat: &str, record: chats::TurnRecord);
    /// `chat`'s engine follows a changed setting now (`what`: `approval`,
    /// `providers`, `resources`): the windows read its state again (FR-8).
    fn reloaded(&self, chat: &str, what: &str);
}

/// What `ensure` is asked to do.
#[derive(Debug, Clone)]
pub struct EnsureRequest {
    pub chat: String,
    /// The folder to start in; the chat's own folder when `None`.
    pub cwd: Option<String>,
    /// What a draft was set to (memory off, incognito); a saved chat's row decides.
    pub options: Option<ChatFlags>,
    /// The engine reopens a saved session (it keeps that session's model).
    pub restore: bool,
    /// Replace a running engine (the person restarts it, moves the chat, …).
    pub restart: bool,
    pub reason: Reason,
}

/// `engine_ensure`'s answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Ensured {
    pub generation: u64,
    /// A new process was started (its `ready` follows); false: one already ran.
    pub started: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    /// Started; set-up not finished.
    Starting,
    /// Set up: tools configured, hosted.
    Ready,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Starting => "starting",
            Status::Ready => "ready",
        }
    }
}

/// One chat's engine as the supervisor started it.
struct Hosted {
    generation: u64,
    folder: PathBuf,
    restore: bool,
    status: Status,
    /// Its tools could not be set during a run: set them when it settles.
    tools_stale: bool,
    /// Whether its tools were last set up as a top-level chat's.
    top_level: Option<bool>,
    /// Settings changes it could not take yet (FR-8).
    pending: settings::Pending,
}

/// Crash restarts per chat (FR-4).
#[derive(Default)]
pub struct RestartLimiter(HashMap<String, VecDeque<Instant>>);

impl RestartLimiter {
    /// Whether `chat` may be restarted at `now`; counts the restart when it may.
    pub fn allow(&mut self, chat: &str, now: Instant) -> bool {
        let recent = self.0.entry(chat.to_string()).or_default();
        while recent.front().is_some_and(|at| now.saturating_duration_since(*at) >= RESTART_WINDOW) {
            recent.pop_front();
        }
        if recent.len() >= RESTARTS {
            return false;
        }
        recent.push_back(now);
        true
    }
}

/// Whether a chat with work runs or waits for the person (`engine_working`,
/// the tray and the quit question; FR-11, FR-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkState {
    Working,
    /// A question is open.
    Waiting,
}

/// What a window draws at once when it shows a chat (`engine_attach`, plan 3.3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AttachState {
    /// The engine's generation; `None` when no engine runs.
    pub generation: Option<u64>,
    /// `starting`, `ready`, or `stopped` (none runs).
    pub status: &'static str,
    /// A run, working agents, reports or mail waiting, or an open question.
    pub working: bool,
    /// Every open question of the chat (FR-9).
    pub questions: Vec<questions::OpenQuestion>,
}

/// What the supervisor follows of a chat, with or without an engine (FR-2, FR-3).
#[derive(Default)]
struct Follow {
    /// A run is active: from `agent_start` to `agent_settled`.
    run: bool,
    /// The chat's agents (subagents, plugin children) by mesh node: the seq of
    /// the last state applied (replays are older), and whether it works.
    agents: HashMap<String, (u64, bool)>,
    /// The agents' titles by mesh node, for a wake's text.
    agent_titles: HashMap<String, String>,
    /// Its session node: the reports and mail waiting to be read (`meta.wake`).
    session: wakes::SessionNode,
    /// Its wakes (FR-5).
    wakes: wakes::WakeState,
    /// A window stopped its engine on purpose around a worktree Apply or
    /// Discard: mail does not start it until the window resumes it.
    held: bool,
    /// Order of last use (larger is later), and when.
    used: u64,
    used_at: Option<Instant>,
}

impl Follow {
    fn busy(&self) -> bool {
        self.run || self.session.waiting() || self.agents.values().any(|(_, working)| *working)
    }

    /// Its engine is gone: none of its runs or agents goes on, and what a new
    /// engine reports is new. Wakes in a row still count.
    fn engine_gone(&mut self) {
        self.run = false;
        self.session = wakes::SessionNode::default();
        self.agents.clear();
        self.agent_titles.clear();
        self.wakes.handled.clear();
    }
}

/// The shell's engines (managed as Tauri state).
#[derive(Default)]
pub struct EngineSupervisor {
    pub rpc: rpc::ShellRpc,
    /// Open questions per chat (FR-9).
    pub questions: questions::Questions,
    chats: Mutex<HashMap<String, Hosted>>,
    /// Work, hold and last use per chat.
    follow: Mutex<HashMap<String, Follow>>,
    /// The chat each window shows, by window label (FR-2).
    viewers: Mutex<HashMap<String, String>>,
    /// Ticks for [`Follow::used`].
    clock: AtomicU64,
    /// Held while an engine starts and is recorded, or idle ones are
    /// stopped: two asks for one chat start one engine, a set-up never reads
    /// a record not yet written, and the trim never stops one being started.
    gate: tokio::sync::Mutex<()>,
    restarts: Mutex<RestartLimiter>,
    /// Bumped by each connector change; the last one of a burst sets tools up.
    connectors: AtomicU64,
    /// Drafts in windows' composers: (window label, chat). A draft holds the
    /// chat's wakes (FR-5).
    drafts: Mutex<HashSet<(String, String)>>,
    /// Which engines give turn totals; the runs recorded (FR-6).
    pub turns: turns::Turns,
}

/// Whether an engine refused a command because a run is active.
fn run_is_active(error: &str) -> bool {
    error.to_ascii_lowercase().contains("run is active")
}

impl EngineSupervisor {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Hosted>> {
        self.chats.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Run `f` on `chat`'s record if it is of `generation`.
    fn with_record<T>(&self, chat: &str, generation: u64, f: impl FnOnce(&mut Hosted) -> T) -> Option<T> {
        self.lock().get_mut(chat).filter(|hosted| hosted.generation == generation).map(f)
    }

    /// The generation of `chat`'s engine when it is set up.
    fn ready_generation(&self, chat: &str) -> Option<u64> {
        self.lock().get(chat).filter(|hosted| hosted.status == Status::Ready).map(|hosted| hosted.generation)
    }

    /// Make sure `chat`'s engine runs; start it when none does (or replace it
    /// with `restart`). A new engine is set up when it is `ready` ([`Self::on_ready`]).
    pub async fn ensure<H: Host>(&self, host: &H, request: EnsureRequest) -> Result<Ensured, String> {
        let _gate = self.gate.lock().await;
        let chat = request.chat.as_str();
        // A window wants the engine: a hold for a worktree Apply is over.
        if request.reason == Reason::Viewer {
            self.follow(chat, |follow| follow.held = false);
        }
        self.touch(chat, Instant::now());
        let folder = request
            .cwd
            .as_deref()
            .filter(|cwd| !cwd.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| host.folder(chat));
        if request.restart {
            // What was asked of the old process gets no answer.
            self.rpc.fail_chat(chat, "engine restarted");
        }
        match host.start(chat, &folder, request.options, request.restart).await? {
            Start::Running(generation) => Ok(Ensured { generation, started: false }),
            Start::Started(generation) => {
                // Nothing of an earlier process goes on: its runs, its agents, its questions.
                self.follow(chat, Follow::engine_gone);
                for id in self.questions.engine_gone(chat) {
                    host.question(chat, &questions::Change::Closed(id));
                }
                self.lock().insert(
                    chat.to_string(),
                    Hosted {
                        generation,
                        folder,
                        restore: request.restore,
                        status: Status::Starting,
                        tools_stale: false,
                        top_level: None,
                        pending: settings::Pending::default(),
                    },
                );
                host.state(chat, generation, "starting", Some(request.reason), request.restore);
                Ok(Ensured { generation, started: true })
            }
        }
    }

    /// `chat`'s engine of `generation` said `ready`: set it up as a window did
    /// (FR-1) — model, tools, then the hub's mail.
    pub async fn on_ready<H: Host>(&self, host: &H, chat: &str, generation: u64) {
        // The `ensure` that started it has recorded it.
        drop(self.gate.lock().await);
        let Some(restore) = self.with_record(chat, generation, |hosted| hosted.restore) else {
            return;
        };
        if !restore {
            self.set_model(host, chat).await;
        }
        self.configure_tools(host, chat, generation).await;
        if self.with_record(chat, generation, |_| ()).is_none() {
            return;
        }
        // Follow its agents: an engine whose agents work is never stopped as
        // idle (FR-3). An engine has one subscriber, its stdout, and the last
        // `subscribe` sets its level: the shell asks for the level windows use
        // (`summary`), which includes node states, so neither lowers the other's.
        let id = self.clock.fetch_add(1, Ordering::Relaxed) + 1;
        if let Err(error) = host.send(chat, subscribe_line(id)).await {
            host.log(chat, "warn", format!("could not follow the chat's agents: {error}"));
        }
        // Settings changed while it started (FR-8), before any mail starts a run.
        self.flush_pending(host, chat, generation).await;
        // Set up: the hub may send what waits for it (a session's brief, a
        // message from its parent, notes).
        host.hosted(chat).await;
        if self.with_record(chat, generation, |hosted| hosted.status = Status::Ready).is_some() {
            self.touch(chat, Instant::now());
            host.state(chat, generation, "ready", None, restore);
            // Reports that waited for an engine may wake it now (FR-5).
            self.schedule_wake(host, chat);
        }
    }

    /// The model a new chat starts on: its row's (a nested session's, a chat
    /// whose draft named one), else the saved default when the engine picked
    /// another (it does not know every provider). Never for a draft the person
    /// already switched: windows send their own `set_model` after the set-up.
    async fn set_model<H: Host>(&self, host: &H, chat: &str) {
        let state = host.request(chat, "get_state", json!({})).await.ok();
        let current = state.as_ref().map(|state| {
            let text = |key: &str| state.get(key).and_then(Value::as_str).map(str::to_string);
            (text("provider"), text("modelId").or_else(|| text("model")))
        });
        if let Some((provider, model)) = host.row(chat).and_then(|row| row.model) {
            if current.as_ref() == Some(&(Some(provider.clone()), Some(model.clone()))) {
                return;
            }
            match host.request(chat, "set_model", json!({ "provider": provider, "modelId": model })).await {
                Ok(_) => return,
                Err(error) => host.log(chat, "warn", format!("could not start on the chat's model: {error}")),
            }
            // That model is gone (its provider was removed): the default, as for any new chat.
        }
        let Ok(saved) = host.request(chat, "get_default_model", json!({})).await else {
            return;
        };
        let text = |key: &str| saved.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
        let (Some(provider), Some(model)) = (text("provider"), text("modelId")) else {
            return;
        };
        let Some((Some(on_provider), on_model)) = current else {
            return;
        };
        if on_provider == provider && on_model.as_deref() == Some(model.as_str()) {
            return;
        }
        if let Err(error) = host.request(chat, "set_model", json!({ "provider": provider, "modelId": model })).await {
            host.log(chat, "warn", format!("could not start on the default model: {error}"));
        }
    }

    /// Give `chat`'s engine its tools. Refused because a run is active: again
    /// when the run settles.
    async fn configure_tools<H: Host>(&self, host: &H, chat: &str, generation: u64) {
        let Some(folder) = self.with_record(chat, generation, |hosted| hosted.folder.clone()) else {
            return;
        };
        // A draft has no row: it has no parent either.
        let top_level = host.row(chat).is_none_or(|row| row.top_level);
        let tools = host.tools(chat, &folder, top_level).await;
        let result = host.request(chat, "configure_external_tools", json!({ "tools": tools })).await;
        self.with_record(chat, generation, |hosted| match &result {
            Ok(_) => {
                hosted.tools_stale = false;
                hosted.top_level = Some(top_level);
            }
            Err(error) => hosted.tools_stale = hosted.tools_stale || run_is_active(error),
        });
        if let Err(error) = result {
            host.log(chat, "warn", format!("could not add the session tools: {error}"));
        }
    }

    /// `chat`'s run settled: tools refused during it are set now, and
    /// settings changed during it reach it (FR-8).
    pub async fn on_settled<H: Host>(&self, host: &H, chat: &str, generation: u64) {
        if self.with_record(chat, generation, |hosted| hosted.tools_stale) == Some(true) {
            self.configure_tools(host, chat, generation).await;
        }
        self.flush_pending(host, chat, generation).await;
    }

    /// Set `chat`'s tools up again, when it is set up (its connector switches changed).
    pub async fn reconfigure<H: Host>(&self, host: &H, chat: &str) {
        if let Some(generation) = self.ready_generation(chat) {
            self.configure_tools(host, chat, generation).await;
        }
    }

    /// Set every set-up engine's tools up again (connectors or the shell's tools changed).
    pub async fn reconfigure_all<H: Host>(&self, host: &H) {
        let ready: Vec<(String, u64)> = self
            .lock()
            .iter()
            .filter(|(_, hosted)| hosted.status == Status::Ready)
            .map(|(chat, hosted)| (chat.clone(), hosted.generation))
            .collect();
        for (chat, generation) in ready {
            self.configure_tools(host, &chat, generation).await;
        }
    }

    /// Chat rows changed (a parent was set, cleared or deleted): a chat that
    /// became top-level gets the session tools, one that became nested loses them.
    pub async fn rows_changed<H: Host>(&self, host: &H) {
        let ready: Vec<(String, u64, Option<bool>)> = self
            .lock()
            .iter()
            .filter(|(_, hosted)| hosted.status == Status::Ready)
            .map(|(chat, hosted)| (chat.clone(), hosted.generation, hosted.top_level))
            .collect();
        for (chat, generation, was) in ready {
            let now = host.row(&chat).is_none_or(|row| row.top_level);
            if was != Some(now) {
                self.configure_tools(host, &chat, generation).await;
            }
        }
    }

    /// `chat`'s engine of `generation` exited (`crashed`: by itself) or was
    /// stopped. Its runs, agents and questions end with it; a crash of a
    /// chat that worked or was shown is announced (FR-10).
    pub fn on_exit<H: Host>(&self, host: &H, chat: &str, generation: u64, crashed: bool) {
        let mut chats = self.lock();
        if !chats.get(chat).is_some_and(|hosted| hosted.generation == generation) {
            return;
        }
        chats.remove(chat);
        drop(chats);
        self.rpc.fail_chat(chat, "engine exited");
        let worked = self.follow(chat, |follow| {
            let busy = follow.busy();
            follow.engine_gone();
            busy
        }) || self.questions.has(chat);
        for id in self.questions.engine_gone(chat) {
            host.question(chat, &questions::Change::Closed(id));
        }
        if crashed && (worked || self.has_viewer(chat)) {
            host.notice(chat, notices::Event::EngineStopped);
        }
    }

    // ── Viewers, work and the idle trim (FR-2, FR-3) ────────────────────────

    fn follow<T>(&self, chat: &str, f: impl FnOnce(&mut Follow) -> T) -> T {
        let mut follow = self.follow.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        f(follow.entry(chat.to_string()).or_default())
    }

    fn viewers(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.viewers.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// `chat` was used at `now` (shown, started, a run settled).
    fn touch(&self, chat: &str, now: Instant) {
        let tick = self.clock.fetch_add(1, Ordering::Relaxed) + 1;
        self.follow(chat, |follow| {
            follow.used = tick;
            follow.used_at = Some(now);
        });
    }

    /// The windows that show `chat`.
    pub fn viewers_of(&self, chat: &str) -> Vec<String> {
        let mut labels: Vec<String> = self.viewers().iter().filter(|(_, shown)| shown.as_str() == chat).map(|(label, _)| label.clone()).collect();
        labels.sort();
        labels
    }

    pub fn has_viewer(&self, chat: &str) -> bool {
        self.viewers().values().any(|shown| shown == chat)
    }

    /// `window` shows `chat` now; a window shows one chat at a time, so the
    /// one it showed before (returned when another) lost this viewer.
    pub fn attach(&self, window: &str, chat: &str, now: Instant) -> Option<String> {
        let before = self.viewers().insert(window.to_string(), chat.to_string());
        self.touch(chat, now);
        let left = before.filter(|before| before != chat)?;
        self.touch(&left, now);
        Some(left)
    }

    /// `window` no longer shows `chat`; false when it showed another.
    pub fn detach(&self, window: &str, chat: &str, now: Instant) -> bool {
        let mut viewers = self.viewers();
        if viewers.get(window).map(String::as_str) != Some(chat) {
            return false;
        }
        viewers.remove(window);
        drop(viewers);
        self.touch(chat, now);
        true
    }

    /// `window` closed (or was destroyed): the chat it showed, if any.
    pub fn window_gone(&self, window: &str, now: Instant) -> Option<String> {
        let chat = self.viewers().remove(window)?;
        self.touch(&chat, now);
        Some(chat)
    }

    /// `chat` lost a window that closed: with none left and an engine
    /// running, the windows hear it (`engine://state`, reason `detached`) so
    /// one keeps following the chat's runs (FR-2).
    pub fn left<H: Host>(&self, host: &H, chat: &str) {
        if self.has_viewer(chat) {
            return;
        }
        let Some((generation, status)) = self.lock().get(chat).map(|hosted| (hosted.generation, hosted.status)) else {
            return;
        };
        host.state(chat, generation, status.as_str(), Some(Reason::Detached), true);
    }

    /// Whether `chat` has work: a run, working agents, reports or mail
    /// waiting, or an open question (FR-3).
    pub fn working<H: Host>(&self, host: &H, chat: &str) -> bool {
        let busy = self.follow.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(chat).is_some_and(Follow::busy);
        busy || self.questions.has(chat) || host.pending_mail(chat)
    }

    /// The chats whose engines run with work, as the tray, the quit question
    /// and the sidebar show them (FR-11, FR-12, NFR-5): `Waiting` while a
    /// question is open (a run that waits for an approval waits for the
    /// person), else `Working`. Mail for a chat with no engine (stopped by
    /// the person, held, archived) waits without working. By chat id.
    pub fn work<H: Host>(&self, host: &H) -> Vec<(String, WorkState)> {
        let mut chats: Vec<String> = self.lock().keys().cloned().collect();
        chats.sort();
        chats
            .into_iter()
            .filter_map(|chat| {
                if self.questions.has(&chat) {
                    Some((chat, WorkState::Waiting))
                } else if self.working(host, &chat) {
                    Some((chat, WorkState::Working))
                } else {
                    None
                }
            })
            .collect()
    }

    /// What a window that shows `chat` draws at once (`engine_attach`).
    pub fn attach_state<H: Host>(&self, host: &H, chat: &str) -> AttachState {
        let record = self.lock().get(chat).map(|hosted| (hosted.generation, hosted.status));
        AttachState {
            generation: record.map(|(generation, _)| generation),
            status: record.map_or("stopped", |(_, status)| status.as_str()),
            working: self.working(host, chat),
            questions: self.questions.of(chat),
        }
    }

    /// One stdout line of `chat`'s engine of `generation`, at `at` (ms):
    /// what it does to the chat's work, questions, wakes and turn records,
    /// and the notices it brings (FR-3, FR-5, FR-6, FR-9, FR-10). True when
    /// a run settled: the idle trim runs then.
    pub fn follow_line<H: Host>(&self, host: &H, chat: &str, generation: u64, line: &Value, at: u64) -> bool {
        let kind = line.get("type").and_then(Value::as_str);
        if !matches!(kind, Some("ready" | "agent_start" | "agent_settled" | "ui_dialog_request" | "ui_dialog_closed" | evo_mesh::wire::WIRE_TYPE)) {
            return false;
        }
        // A line of a replaced engine changes nothing.
        if self.lock().get(chat).is_some_and(|hosted| hosted.generation != generation) {
            return false;
        }
        match kind {
            Some("ready") => {
                self.turns.on_ready(chat, generation, line);
                false
            }
            Some("agent_start") => {
                self.follow(chat, |follow| follow.run = true);
                false
            }
            Some("agent_settled") => {
                self.follow(chat, |follow| follow.run = false);
                self.touch(chat, Instant::now());
                // The turn, once per run; unread unless a focused window shows it (FR-6).
                let unread = notices::wanted(&self.viewers_of(chat), |label| host.focused(label));
                match self.turns.on_settled(chat, generation, line, unread) {
                    turns::Settled::Record(record) => host.record(chat, record),
                    turns::Settled::NoTotals { first: true } => host.log(chat, "warn", turns::NO_TOTALS.to_string()),
                    _ => {}
                }
                // A compaction is not a turn; a stopped run is the person's doing.
                if line.get("compact") != Some(&Value::Bool(true)) {
                    match line.get("outcome").and_then(Value::as_str) {
                        Some("completed") => host.notice(chat, notices::Event::ReplyReady),
                        Some("failed") => host.notice(chat, notices::Event::RunFailed),
                        _ => {}
                    }
                }
                // Reports that came during the run may wake it (FR-5).
                self.schedule_wake(host, chat);
                true
            }
            Some("ui_dialog_request" | "ui_dialog_closed") => {
                if let Some(change) = self.questions.on_line(chat, line, at) {
                    host.question(chat, &change);
                    match &change {
                        questions::Change::Opened(question) => {
                            let event = if question.kind == questions::Kind::Questions { notices::Event::Question } else { notices::Event::Approval };
                            host.notice(chat, event);
                        }
                        // A wake held back for the question may go (FR-5).
                        questions::Change::Closed(_) => self.schedule_wake(host, chat),
                    }
                }
                false
            }
            Some(evo_mesh::wire::WIRE_TYPE) => {
                if self.follow_mesh(chat, line) {
                    self.schedule_wake(host, chat);
                }
                false
            }
            _ => false,
        }
    }

    /// A mesh event of `chat`'s engine: its agents' states and titles, and
    /// its session node (what waits to be read, `meta.wake`). True when the
    /// session node changed: a wake may be due.
    fn follow_mesh(&self, chat: &str, line: &Value) -> bool {
        let text = |key: &str| line.get(key).and_then(Value::as_str);
        if text("kind") != Some("evt") || text("op") != Some("node_state") {
            return false;
        }
        let Some(node) = line.pointer("/body/node") else {
            return false;
        };
        let field = |key: &str| node.get(key).and_then(Value::as_str);
        let (Some(id), Some(kind), Some(state)) = (field("id"), field("kind"), field("state")) else {
            return false;
        };
        let seq = line.get("seq").and_then(Value::as_u64).unwrap_or(0);
        self.follow(chat, |follow| {
            if kind == "session" {
                if follow.session.seq > seq {
                    return false;
                }
                follow.session = wakes::SessionNode::of(seq, node);
                // A run read every report and note that waited.
                if state == "running" {
                    follow.wakes.run_started();
                }
                return true;
            }
            // A replay older than what was applied changes nothing.
            if follow.agents.get(id).is_some_and(|(known, _)| *known > seq) {
                return false;
            }
            follow.agents.insert(id.to_string(), (seq, matches!(state, "starting" | "running" | "waiting")));
            if let Some(title) = field("title").filter(|title| !title.is_empty()) {
                follow.agent_titles.insert(id.to_string(), title.to_string());
            }
            false
        })
    }

    // ── Wakes (FR-5) ────────────────────────────────────────────────────────

    /// Something that may wake `chat` happened: (re)start its gather; the
    /// check runs after [`wakes::WAKE_DELAY`].
    pub fn schedule_wake<H: Host>(&self, host: &H, chat: &str) {
        let tick = self.follow(chat, |follow| {
            follow.wakes.tick += 1;
            follow.wakes.tick
        });
        host.wake_later(chat, tick);
    }

    /// Whether a window's composer holds a draft for `chat`.
    pub fn drafting(&self, chat: &str) -> bool {
        self.drafts.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().any(|(_, shown)| shown == chat)
    }

    /// Window `window`'s composer holds a draft for `chat` (`on`), or no
    /// longer does. A wake held back for drafts may go once none is left.
    pub fn set_composing<H: Host>(&self, host: &H, window: &str, chat: &str, on: bool) {
        let mut drafts = self.drafts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = (window.to_string(), chat.to_string());
        if on {
            drafts.insert(key);
            return;
        }
        let removed = drafts.remove(&key);
        drop(drafts);
        if removed && !self.drafting(chat) {
            self.schedule_wake(host, chat);
        }
    }

    /// Window `window` closed or reloaded: its composers hold nothing.
    pub fn drop_drafts<H: Host>(&self, host: &H, window: &str) {
        let mut drafts = self.drafts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let chats: Vec<String> = drafts.iter().filter(|(label, _)| label == window).map(|(_, chat)| chat.clone()).collect();
        drafts.retain(|(label, _)| label != window);
        drop(drafts);
        for chat in chats {
            if !self.drafting(&chat) {
                self.schedule_wake(host, &chat);
            }
        }
    }

    /// A window sent the person's prompt to `chat`: automated wakes may go on.
    pub fn prompted(&self, chat: &str) {
        self.follow(chat, |follow| follow.wakes.prompted());
    }

    /// The gather of `tick` ended: wake `chat` with one automated turn for
    /// what waits, unless a later event restarted the gather, the chat runs
    /// or is not set up, a draft or a question holds it, it is archived, or
    /// it was woken [`wakes::MAX_AUTO_WAKES`] times in a row (FR-5). True
    /// when it was woken.
    pub async fn check_wake<H: Host>(&self, host: &H, chat: &str, tick: u64) -> bool {
        if self.follow(chat, |follow| follow.wakes.tick) != tick || self.ready_generation(chat).is_none() {
            return false;
        }
        // Not while the person writes there or is asked something: their
        // Send would become a steer. Both look again when they end.
        if self.drafting(chat) || self.questions.has(chat) {
            return false;
        }
        // An archived (or deleted, or unsaved) chat is never woken.
        if !host.row(chat).is_some_and(|row| !row.archived) {
            return false;
        }
        let woken = self.follow(chat, |follow| {
            if follow.run {
                return None;
            }
            let woken = wakes::gather(&follow.session, &follow.agent_titles, &mut follow.wakes.handled);
            if woken.is_empty() {
                return None;
            }
            // Over the limit, the reports stay queued: they reach the model
            // with the person's next prompt.
            Some(follow.wakes.take_one().then_some(woken))
        });
        match woken {
            None => false,
            Some(None) => {
                host.log(chat, "info", format!("a wake waits: {} automated wakes in a row; reports wait for the next prompt", wakes::MAX_AUTO_WAKES));
                false
            }
            Some(Some(woken)) => {
                self.touch(chat, Instant::now());
                host.wake(chat, wakes::text(&woken)).await;
                true
            }
        }
    }

    /// Stop the engines with no work and no viewer beyond the newest
    /// [`IDLE_ENGINES`], over the whole app (FR-3). Engines starting, or used
    /// within [`IDLE_GRACE`] of `now`, are left for a later pass.
    pub async fn trim<H: Host>(&self, host: &H, now: Instant) {
        let _gate = self.gate.lock().await;
        let ready: Vec<(String, u64)> = self
            .lock()
            .iter()
            .filter(|(_, hosted)| hosted.status == Status::Ready)
            .map(|(chat, hosted)| (chat.clone(), hosted.generation))
            .collect();
        let mut idle = Vec::new();
        for (chat, generation) in ready {
            if self.has_viewer(&chat) || self.working(host, &chat) {
                continue;
            }
            let (used, used_at) = self.follow(&chat, |follow| (follow.used, follow.used_at));
            if used_at.is_some_and(|at| now.saturating_duration_since(at) < IDLE_GRACE) {
                continue;
            }
            idle.push((used, chat, generation));
        }
        idle.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, chat, generation) in idle.into_iter().skip(IDLE_ENGINES) {
            if self.with_record(&chat, generation, |_| ()).is_none() {
                continue;
            }
            self.lock().remove(&chat);
            self.rpc.fail_chat(&chat, "engine stopped");
            self.follow(&chat, Follow::engine_gone);
            host.stop(&chat).await;
            host.log(&chat, "info", "engine stopped: no work and no window, beyond the idle limit".to_string());
            host.state(&chat, generation, "stopped", Some(Reason::Idle), false);
        }
    }

    // ── The hold around a worktree Apply ────────────────────────────────────

    /// A window stops `chat`'s engine on purpose (its worktree is about to be
    /// applied or discarded): mail does not start it again meanwhile.
    pub fn hold(&self, chat: &str) {
        self.follow(chat, |follow| follow.held = true);
    }

    pub fn is_held(&self, chat: &str) -> bool {
        self.follow.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(chat).is_some_and(|follow| follow.held)
    }

    /// The window is done with `chat`'s folder. Mail that came meanwhile goes
    /// out now when no engine runs; a window that starts the engine itself
    /// (`ensure`) ends the hold and its set-up delivers the mail.
    pub async fn release<H: Host>(&self, host: &H, chat: &str) {
        let was = self.follow(chat, |follow| std::mem::take(&mut follow.held));
        if was && self.lock().get(chat).is_none() && host.pending_mail(chat) {
            self.host_for_mail(host, chat, None, true, Reason::Mail).await;
        }
    }

    /// The hub has mail for `chat` and no engine set up to take it
    /// (`Effect::Host`, FR-4): start one, in `cwd` when given (a new
    /// session's worktree). A running, set-up engine takes it at once. An
    /// archived or unsaved chat is not hosted: its mail waits.
    pub async fn host_for_mail<H: Host>(&self, host: &H, chat: &str, cwd: Option<String>, restore: bool, reason: Reason) {
        match host.row(chat) {
            None => {
                host.log(chat, "warn", "mail for a chat that is not saved: not hosted".to_string());
                return;
            }
            Some(row) if row.archived => return,
            Some(_) => {}
        }
        if self.is_held(chat) {
            host.log(chat, "info", "mail waits: the chat's engine is held while its worktree is applied".to_string());
            return;
        }
        let request = EnsureRequest { chat: chat.to_string(), cwd, options: None, restore, restart: false, reason };
        match self.ensure(host, request).await {
            Ok(Ensured { started: false, generation }) => {
                // Being set up: its set-up hosts it.
                if self.ready_generation(chat) == Some(generation) {
                    host.hosted(chat).await;
                }
            }
            Ok(_) => {}
            Err(error) => {
                host.log(chat, "error", format!("could not start the engine for its mail: {error}"));
                host.notice(chat, notices::Event::CouldNotStart);
            }
        }
    }

    /// At app start: every chat with undelivered mail is hosted (FR-4).
    pub async fn host_pending<H: Host>(&self, host: &H, chats: Vec<String>) {
        for chat in chats {
            self.host_for_mail(host, &chat, None, true, Reason::Mail).await;
        }
    }

    /// `chat`'s engine crashed with mail waiting: start it again, at most
    /// [`RESTARTS`] times in [`RESTART_WINDOW`] (FR-4).
    pub async fn restart_after_crash<H: Host>(&self, host: &H, chat: &str, now: Instant) {
        if self.is_held(chat) {
            return;
        }
        let allowed = self.restarts.lock().unwrap_or_else(std::sync::PoisonError::into_inner).allow(chat, now);
        if !allowed {
            host.log(chat, "warn", "the engine keeps stopping: its mail waits until the chat is opened".to_string());
            host.notice(chat, notices::Event::KeepsStopping);
            return;
        }
        host.log(chat, "info", "restarting the engine to deliver mail".to_string());
        self.host_for_mail(host, chat, None, true, Reason::Restart).await;
    }
}

/// The mesh `subscribe` the shell sends an engine it set up (plan 3.3 "Shell → engine").
fn subscribe_line(id: u64) -> Value {
    json!({
        "type": evo_mesh::wire::WIRE_TYPE,
        "v": evo_mesh::wire::VERSION,
        "id": format!("{}mesh-{id}", rpc::ID_PREFIX),
        "kind": "cmd",
        "op": "subscribe",
        "from": "client:shell",
        "to": evo_mesh::wire::ROUTER,
        "seq": 0,
        "ts": 0,
        "body": { "level": "summary" },
    })
}

/// The folder `chat` works in: its worktree, else its workspace, else `home`
/// (as a window's `chatFolder` picks it).
pub fn folder_of(conn: &Connection, chat: &str, home: &Path) -> PathBuf {
    let Ok(Some(row)) = chats::get_chat(conn, chat) else {
        return home.to_path_buf();
    };
    if let Some(worktree) = row.worktree_path.filter(|path| !path.is_empty()) {
        return PathBuf::from(worktree);
    }
    row.workspace_id
        .and_then(|workspace| chats::get_workspace(conn, &workspace).ok().flatten())
        .map(|workspace| PathBuf::from(workspace.path))
        .unwrap_or_else(|| home.to_path_buf())
}

// ── The app's host ──────────────────────────────────────────────────────────

/// [`Host`] over the running app.
#[derive(Clone)]
pub(crate) struct AppHost(pub AppHandle);

impl Host for AppHost {
    fn row(&self, chat: &str) -> Option<Row> {
        let db = self.0.state::<Db>();
        db.with(|conn| chats::get_chat(conn, chat)).ok().flatten().map(|row| Row::of(&row))
    }

    fn folder(&self, chat: &str) -> PathBuf {
        crate::agent_tools::chat_folder(&self.0, chat)
    }

    fn start(&self, chat: &str, folder: &Path, options: Option<ChatFlags>, restart: bool) -> impl Future<Output = Result<Start, String>> + Send {
        let (app, chat, folder) = (self.0.clone(), chat.to_string(), folder.to_path_buf());
        async move { crate::engine::start_process(&app, &chat, &folder, options, restart).await }
    }

    fn request(&self, chat: &str, command: &str, payload: Value) -> impl Future<Output = Result<Value, String>> + Send {
        let (app, chat, command) = (self.0.clone(), chat.to_string(), command.to_string());
        async move {
            let supervisor = app.state::<EngineSupervisor>();
            let writer = (app.clone(), chat.clone());
            rpc::request(&supervisor.rpc, &chat, &command, payload, rpc::TIMEOUT, move |line| async move {
                writer.0.state::<crate::engine::EngineState>().write(&writer.1, &line).await
            })
            .await
        }
    }

    fn tools(&self, chat: &str, folder: &Path, top_level: bool) -> impl Future<Output = Vec<Value>> + Send {
        let (app, chat, folder) = (self.0.clone(), chat.to_string(), folder.display().to_string());
        async move {
            // Session tools are for top-level chats; the hub defines and answers
            // them (mesh.rs). Every chat gets the shell's tools: browser,
            // terminal, Files, scheduler, and its connectors' tools.
            let mut tools = if top_level {
                evo_mesh::hub::session_tools().as_array().cloned().unwrap_or_default()
            } else {
                Vec::new()
            };
            tools.extend(crate::agent_tools::agent_tools_definitions(app, Some(chat), Some(folder)).await);
            tools
        }
    }

    fn hosted(&self, chat: &str) -> impl Future<Output = ()> + Send {
        let (app, chat) = (self.0.clone(), chat.to_string());
        async move {
            let effects = app.state::<crate::mesh::MeshHub>().0.on_hosted(&chat);
            crate::mesh::apply(&app, effects).await;
        }
    }

    fn state(&self, chat: &str, generation: u64, status: &str, reason: Option<Reason>, restore: bool) {
        let _ = self.0.emit(
            "engine://state",
            json!({ "chat": chat, "generation": generation, "status": status, "reason": reason.map(Reason::as_str), "restore": restore }),
        );
        crate::system::work_changed(&self.0);
    }

    fn log(&self, chat: &str, level: &str, message: String) {
        self.0.state::<Db>().log(NewLog::new(level, "shell", message).chat(Some(chat)));
    }

    fn pending_mail(&self, chat: &str) -> bool {
        self.0
            .state::<Db>()
            .with(|conn| evo_store::mail::pending_mail(conn, chat))
            .is_ok_and(|mail| !mail.is_empty())
    }

    fn send(&self, chat: &str, line: Value) -> impl Future<Output = Result<(), String>> + Send {
        let (app, chat) = (self.0.clone(), chat.to_string());
        async move { app.state::<crate::engine::EngineState>().write(&chat, &line).await }
    }

    fn stop(&self, chat: &str) -> impl Future<Output = ()> + Send {
        let (app, chat) = (self.0.clone(), chat.to_string());
        async move {
            app.state::<crate::engine::EngineState>().stop(&chat).await;
        }
    }

    fn notice(&self, chat: &str, event: notices::Event) {
        notices::announce(&self.0, chat.to_string(), event);
    }

    fn question(&self, chat: &str, change: &questions::Change) {
        match change {
            questions::Change::Opened(question) => {
                let _ = self.0.emit("engine://question", json!({ "chat": chat, "question": question }));
            }
            questions::Change::Closed(request_id) => {
                let _ = self.0.emit("engine://question-closed", json!({ "chat": chat, "requestId": request_id }));
            }
        }
        crate::system::work_changed(&self.0);
    }

    fn wake_later(&self, chat: &str, tick: u64) {
        let (app, chat) = (self.0.clone(), chat.to_string());
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(wakes::WAKE_DELAY).await;
            let host = AppHost(app.clone());
            app.state::<EngineSupervisor>().check_wake(&host, &chat, tick).await;
        });
    }

    fn wake(&self, chat: &str, text: String) -> impl Future<Output = ()> + Send {
        let (app, chat) = (self.0.clone(), chat.to_string());
        async move {
            AppHost(app.clone()).log(&chat, "info", "waking the chat for what waits for it".to_string());
            // Mail to itself, delivered as a prompt: kept until its engine
            // takes it, and shown as an event chip (mesh FR-28).
            let (hub_app, to) = (app.clone(), chat.clone());
            let effects = tauri::async_runtime::spawn_blocking(move || {
                hub_app.state::<crate::mesh::MeshHub>().0.post(&to, &to, evo_mesh::wire::MailBody::new(text, evo_mesh::wire::Deliver::Prompt))
            })
            .await
            .unwrap_or_default();
            crate::mesh::apply(&app, effects).await;
        }
    }

    fn focused(&self, label: &str) -> bool {
        self.0
            .get_webview_window(label)
            .is_some_and(|window| window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false))
    }

    fn record(&self, chat: &str, record: chats::TurnRecord) {
        let (app, chat) = (self.0.clone(), chat.to_string());
        tauri::async_runtime::spawn_blocking(move || {
            let db = app.state::<Db>();
            match db.with(|conn| chats::record_turn(conn, &chat, &record)) {
                // Every window's sidebar and session list (session-store.ts).
                Ok(Some(row)) => {
                    let _ = app.emit("chat://updated", json!({ "from": "shell", "chat": row }));
                }
                Ok(None) => {}
                Err(error) => db.log(NewLog::new("warn", "shell", format!("could not record the turn: {error}")).chat(Some(&chat))),
            }
        });
    }

    fn reloaded(&self, chat: &str, what: &str) {
        let _ = self.0.emit("engine://reloaded", json!({ "chat": chat, "what": what }));
    }
}

/// Now, in ms since the epoch.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// One stdout line of `chat`'s engine of `generation`, after the hub and
/// the shell's tools read it: answers to the shell's requests, the chat's
/// work and questions, `ready` (set up) and settled runs (tools refused
/// during the run, then the idle trim).
pub async fn on_line(app: &AppHandle, chat: &str, generation: u64, line: &Value) {
    let supervisor = app.state::<EngineSupervisor>();
    if supervisor.rpc.on_line(chat, line) {
        return;
    }
    // Providers changed in this engine: the others reload them (FR-8).
    if settings::provider_change(line) {
        settings::providers_changed(app, chat);
    }
    let settled = supervisor.follow_line(&AppHost(app.clone()), chat, generation, line, now_ms());
    let kind = line.get("type").and_then(Value::as_str);
    // What works and waits may have changed: the tray and the sidebar follow (FR-12, NFR-5).
    if matches!(kind, Some("agent_start" | "agent_settled" | evo_mesh::wire::WIRE_TYPE)) {
        crate::system::work_changed(app);
    }
    let ready = kind == Some("ready");
    if !(ready || settled) {
        return;
    }
    let (app, chat) = (app.clone(), chat.to_string());
    // Off the reader: the set-up waits for answers the reader delivers.
    tauri::async_runtime::spawn(async move {
        let host = AppHost(app.clone());
        let supervisor = app.state::<EngineSupervisor>();
        if ready {
            supervisor.on_ready(&host, &chat, generation).await;
        } else {
            supervisor.on_settled(&host, &chat, generation).await;
            supervisor.trim(&host, Instant::now()).await;
        }
    });
}

/// `chat`'s engine of `generation` exited (`crashed`: by itself) or was stopped.
pub fn on_exit(app: &AppHandle, chat: &str, generation: u64, crashed: bool) {
    app.state::<EngineSupervisor>().on_exit(&AppHost(app.clone()), chat, generation, crashed);
    crate::system::work_changed(app);
}

/// A window wrote `line` to `chat`'s engine: an answer closes its question
/// (FR-9), and the person's prompt lets automated wakes go on (FR-5). The
/// shell's own lines and the hub's (wakes, mail) do not pass here.
pub fn sent(app: &AppHandle, chat: &str, line: &Value) {
    let supervisor = app.state::<EngineSupervisor>();
    let host = AppHost(app.clone());
    if line.get("type").and_then(Value::as_str) == Some("prompt") {
        supervisor.prompted(chat);
    }
    if let Some(change) = supervisor.questions.on_sent(chat, line) {
        host.question(chat, &change);
        // A wake held back for the question may go.
        supervisor.schedule_wake(&host, chat);
    }
}

/// One of the shell's tools asks the person `question` in `chat`: kept until
/// [`question_closed`], drawn by every window, announced (FR-9).
pub fn question_opened(app: &AppHandle, chat: &str, question: questions::OpenQuestion) {
    let host = AppHost(app.clone());
    if app.state::<EngineSupervisor>().questions.open(chat, question.clone()) {
        host.question(chat, &questions::Change::Opened(question));
        host.notice(chat, notices::Event::Approval);
    }
}

/// The shell's question `request_id` was answered or its call ended.
pub fn question_closed(app: &AppHandle, chat: &str, request_id: &str) {
    let supervisor = app.state::<EngineSupervisor>();
    if supervisor.questions.close(chat, request_id) {
        let host = AppHost(app.clone());
        host.question(chat, &questions::Change::Closed(request_id.to_string()));
        supervisor.schedule_wake(&host, chat);
    }
}

/// Engines idle beyond the limit are stopped, off the caller.
fn trim_later(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        app.state::<EngineSupervisor>().trim(&AppHost(app.clone()), Instant::now()).await;
    });
}

/// Window `label` closed or was destroyed: it shows no chat any more. Its
/// chat's engine goes on (FR-2); with no window left on it, the windows hear
/// it, so one keeps following its runs.
pub fn window_gone(app: &AppHandle, label: &str) {
    let supervisor = app.state::<EngineSupervisor>();
    // Its composers' drafts went with it (FR-5).
    supervisor.drop_drafts(&AppHost(app.clone()), label);
    if let Some(chat) = supervisor.window_gone(label, Instant::now()) {
        supervisor.left(&AppHost(app.clone()), &chat);
        trim_later(app);
    }
}

/// This window shows `chat` (a window shows one chat at a time): what to draw
/// at once — the engine's state, whether the chat works, and every open
/// question (FR-2, FR-9).
#[tauri::command]
pub async fn engine_attach(app: AppHandle, window: tauri::Window, chat: String) -> AttachState {
    let supervisor = app.state::<EngineSupervisor>();
    let host = AppHost(app.clone());
    if supervisor.attach(window.label(), &chat, Instant::now()).is_some() {
        // The chat it showed before may be idle now.
        trim_later(&app);
    }
    supervisor.attach_state(&host, &chat)
}

/// This window no longer shows `chat` (it moved to another window, it was
/// closed or archived). Its engine goes on while it has work (FR-2).
#[tauri::command]
pub async fn engine_detach(app: AppHandle, window: tauri::Window, chat: String) {
    if app.state::<EngineSupervisor>().detach(window.label(), &chat, Instant::now()) {
        trim_later(&app);
    }
}

/// `held`: a window is about to stop `chat`'s engine around a worktree Apply
/// or Discard, and mail must not start it meanwhile. `false`: the window is
/// done; mail that waited goes out when no engine runs.
#[tauri::command]
pub async fn engine_hold(app: AppHandle, chat: String, held: bool) {
    let supervisor = app.state::<EngineSupervisor>();
    if held {
        supervisor.hold(&chat);
    } else {
        supervisor.release(&AppHost(app.clone()), &chat).await;
    }
}

/// This window's composer holds a draft for `chat` (`composing`), or no
/// longer does: a draft holds the chat's automated wakes (FR-5). With no
/// `chat` and `composing` false, the window's composers hold nothing (it
/// loaded again).
#[tauri::command]
pub fn engine_set_composing(app: AppHandle, window: tauri::Window, chat: Option<String>, composing: bool) {
    let supervisor = app.state::<EngineSupervisor>();
    let host = AppHost(app.clone());
    match chat {
        Some(chat) => supervisor.set_composing(&host, window.label(), &chat, composing),
        None if !composing => supervisor.drop_drafts(&host, window.label()),
        None => {}
    }
}

/// Every open question of `chat` (a window that rebuilds its transcript draws them again, FR-9).
#[tauri::command]
pub async fn engine_questions(app: AppHandle, chat: String) -> Vec<questions::OpenQuestion> {
    app.state::<EngineSupervisor>().questions.of(&chat)
}

/// The hub asks for a host for `chat` (`Effect::Host`).
pub fn host_for_mail(app: &AppHandle, chat: String, cwd: Option<String>, restore: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let host = AppHost(app.clone());
        app.state::<EngineSupervisor>().host_for_mail(&host, &chat, cwd, restore, Reason::Mail).await;
    });
}

/// `chat`'s engine crashed with mail waiting.
pub fn restart_for_mail(app: &AppHandle, chat: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let host = AppHost(app.clone());
        app.state::<EngineSupervisor>().restart_after_crash(&host, &chat, Instant::now()).await;
    });
}

/// At app start, with or without a window: host every chat with undelivered
/// mail (archived chats excluded), FR-4.
pub fn setup(app: &AppHandle) {
    // The connectors switched off per chat, for every chat's tools (FR-8).
    crate::agent_tools::mcp::load_switches(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let pending = app.state::<Db>().with(|conn| evo_store::mail::chats_with_pending_mail(conn)).unwrap_or_default();
        let host = AppHost(app.clone());
        app.state::<EngineSupervisor>().host_pending(&host, pending).await;
    });
}

/// The connectors changed (the MCP pool): every set-up chat gets its tools
/// again, once the burst of changes is over.
pub fn connectors_changed(app: &AppHandle) {
    let Some(supervisor) = app.try_state::<EngineSupervisor>() else {
        return;
    };
    let tick = supervisor.connectors.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CONNECTORS_SETTLE).await;
        let supervisor = app.state::<EngineSupervisor>();
        if supervisor.connectors.load(Ordering::Relaxed) == tick {
            supervisor.reconfigure_all(&AppHost(app.clone())).await;
        }
    });
}

/// The shell's own tool list changed (computer app control, Search past chats).
pub fn tools_changed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        app.state::<EngineSupervisor>().reconfigure_all(&AppHost(app.clone())).await;
    });
}

/// The connectors switched off for `chat` changed.
pub fn chat_tools_changed(app: &AppHandle, chat: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        app.state::<EngineSupervisor>().reconfigure(&AppHost(app.clone()), &chat).await;
    });
}

/// Chat rows changed which chats are top-level.
pub fn chat_rows_changed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        app.state::<EngineSupervisor>().rows_changed(&AppHost(app.clone())).await;
    });
}

/// A window shows `chat`: make sure its engine runs (FR-1). `cwd` and
/// `options` are what a draft was set to (a saved chat's row decides its
/// folder's default and its flags); `restore`: the engine reopens the saved
/// session; `restart`: replace a running engine (the person restarted it,
/// moved the chat, changed a draft's flags, a sandbox change).
#[tauri::command]
pub async fn engine_ensure(
    app: AppHandle,
    chat: String,
    cwd: Option<String>,
    options: Option<ChatFlags>,
    restore: Option<bool>,
    restart: Option<bool>,
) -> Result<Ensured, String> {
    let host = AppHost(app.clone());
    let request = EnsureRequest {
        chat,
        cwd,
        options,
        restore: restore.unwrap_or(false),
        restart: restart.unwrap_or(false),
        reason: Reason::Viewer,
    };
    app.state::<EngineSupervisor>().ensure(&host, request).await
}

// docs/features/shell-hosted-engines/spec.md, FR-1 (T1) and FR-4 (T5).
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::{HashSet, VecDeque};
    use std::sync::atomic::AtomicU32;

    /// A fake engine and app: rows, running engines, scripted answers, and a
    /// log of everything the supervisor did, in order.
    #[derive(Default)]
    pub(crate) struct Fake {
        pub(crate) rows: HashMap<String, Row>,
        folders: HashMap<String, PathBuf>,
        running: Mutex<HashMap<String, u64>>,
        generation: AtomicU64,
        /// Answers per command, in order; an empty queue answers `{}`.
        answers: Mutex<HashMap<String, VecDeque<Result<Value, String>>>>,
        log: Mutex<Vec<String>>,
        /// Chats with mail waiting in the hub's store.
        mail: Mutex<HashSet<String>>,
        /// Wakes gathered (chat, tick), in order; the timer never fires by itself.
        ticks: Mutex<Vec<(String, u64)>>,
        /// Focused windows.
        focused: Mutex<HashSet<String>>,
    }

    impl Fake {
        pub(crate) fn with_row(mut self, chat: &str, row: Row) -> Self {
            self.rows.insert(chat.into(), row);
            self
        }

        fn with_folder(mut self, chat: &str, folder: &str) -> Self {
            self.folders.insert(chat.into(), PathBuf::from(folder));
            self
        }

        fn answer(&self, command: &str, reply: Result<Value, String>) {
            self.answers.lock().unwrap().entry(command.into()).or_default().push_back(reply);
        }

        fn note(&self, line: String) {
            self.log.lock().unwrap().push(line);
        }

        pub(crate) fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }

        /// The tick of `chat`'s latest gathered wake.
        pub(crate) fn last_tick(&self, chat: &str) -> Option<u64> {
            self.ticks.lock().unwrap().iter().rev().find(|(shown, _)| shown == chat).map(|(_, tick)| *tick)
        }

        pub(crate) fn focus(&self, window: &str) {
            self.focused.lock().unwrap().insert(window.to_string());
        }

        /// The log lines starting with `prefix`.
        pub(crate) fn lines(&self, prefix: &str) -> Vec<String> {
            self.log().into_iter().filter(|line| line.starts_with(prefix)).collect()
        }

        fn crash(&self, chat: &str) {
            self.running.lock().unwrap().remove(chat);
        }

        pub(crate) fn mail_for(&self, chat: &str, waiting: bool) {
            let mut mail = self.mail.lock().unwrap();
            if waiting {
                mail.insert(chat.to_string());
            } else {
                mail.remove(chat);
            }
        }
    }

    impl Host for Fake {
        fn row(&self, chat: &str) -> Option<Row> {
            self.rows.get(chat).cloned()
        }

        fn folder(&self, chat: &str) -> PathBuf {
            self.folders.get(chat).cloned().unwrap_or_else(|| PathBuf::from("/home"))
        }

        fn start(&self, chat: &str, folder: &Path, options: Option<ChatFlags>, restart: bool) -> impl Future<Output = Result<Start, String>> + Send {
            let started = {
                let mut running = self.running.lock().unwrap();
                match running.get(chat) {
                    Some(generation) if !restart => Start::Running(*generation),
                    _ => {
                        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
                        running.insert(chat.to_string(), generation);
                        Start::Started(generation)
                    }
                }
            };
            if let Start::Started(generation) = started {
                self.note(format!("start {chat} {} {options:?} gen={generation}", folder.display()));
            }
            async move {
                // Another task may run meanwhile, as a real spawn awaits.
                tokio::task::yield_now().await;
                Ok(started)
            }
        }

        fn request(&self, chat: &str, command: &str, payload: Value) -> impl Future<Output = Result<Value, String>> + Send {
            let reply = self.answers.lock().unwrap().get_mut(command).and_then(VecDeque::pop_front).unwrap_or_else(|| Ok(json!({})));
            let shown = if command == "configure_external_tools" {
                let names: Vec<String> = payload["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
                names.join(",")
            } else if command == "set_model" {
                format!("{}/{}", payload["provider"].as_str().unwrap(), payload["modelId"].as_str().unwrap())
            } else {
                payload.to_string()
            };
            self.note(format!("request {chat} {command} {shown}"));
            async move { reply }
        }

        fn tools(&self, _chat: &str, folder: &Path, top_level: bool) -> impl Future<Output = Vec<Value>> + Send {
            let mut tools = Vec::new();
            if top_level {
                tools.push(json!({ "name": "start_session" }));
            }
            tools.push(json!({ "name": format!("browser@{}", folder.display()) }));
            async move { tools }
        }

        fn hosted(&self, chat: &str) -> impl Future<Output = ()> + Send {
            self.note(format!("hosted {chat}"));
            async {}
        }

        fn state(&self, chat: &str, generation: u64, status: &str, reason: Option<Reason>, _restore: bool) {
            self.note(format!("state {chat} {generation} {status} {}", reason.map_or("-", Reason::as_str)));
        }

        fn log(&self, chat: &str, level: &str, message: String) {
            self.note(format!("log {chat} {level} {message}"));
        }

        fn pending_mail(&self, chat: &str) -> bool {
            self.mail.lock().unwrap().contains(chat)
        }

        fn send(&self, chat: &str, line: Value) -> impl Future<Output = Result<(), String>> + Send {
            self.note(format!("send {chat} {} {}", line["op"].as_str().unwrap_or("-"), line["body"]));
            async { Ok(()) }
        }

        fn stop(&self, chat: &str) -> impl Future<Output = ()> + Send {
            self.running.lock().unwrap().remove(chat);
            self.note(format!("stop {chat}"));
            async {}
        }

        fn notice(&self, chat: &str, event: notices::Event) {
            self.note(format!("notice {chat} {}", event.as_str()));
        }

        fn question(&self, chat: &str, change: &questions::Change) {
            match change {
                questions::Change::Opened(question) => self.note(format!("question {chat} open {}", question.request_id)),
                questions::Change::Closed(id) => self.note(format!("question {chat} closed {id}")),
            }
        }

        fn wake_later(&self, chat: &str, tick: u64) {
            self.ticks.lock().unwrap().push((chat.to_string(), tick));
        }

        fn wake(&self, chat: &str, text: String) -> impl Future<Output = ()> + Send {
            self.note(format!("wake {chat} {text}"));
            async {}
        }

        fn focused(&self, label: &str) -> bool {
            self.focused.lock().unwrap().contains(label)
        }

        fn record(&self, chat: &str, record: chats::TurnRecord) {
            self.note(format!(
                "record {chat} cost={} tokens={} unread={} preview={}",
                record.cost_usd,
                record.tokens,
                record.unread,
                record.preview.as_deref().unwrap_or("-")
            ));
        }

        fn reloaded(&self, chat: &str, what: &str) {
            self.note(format!("reloaded {chat} {what}"));
        }
    }

    pub(crate) fn viewer(chat: &str, cwd: Option<&str>, restore: bool) -> EnsureRequest {
        EnsureRequest { chat: chat.into(), cwd: cwd.map(str::to_string), options: None, restore, restart: false, reason: Reason::Viewer }
    }

    pub(crate) fn top(model: Option<(&str, &str)>) -> Row {
        Row { model: model.map(|(p, m)| (p.into(), m.into())), top_level: true, archived: false }
    }

    #[tokio::test]
    async fn ensure_starts_one_engine_per_chat() {
        let fake = Fake::default();
        let supervisor = EngineSupervisor::default();
        // A window and the hub ask at the same time.
        let (a, b) = tokio::join!(supervisor.ensure(&fake, viewer("c", Some("/w"), false)), supervisor.ensure(&fake, viewer("c", Some("/w"), false)));
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(fake.lines("start").len(), 1, "{:?}", fake.log());
        assert_eq!(a.generation, b.generation);
        assert!(a.started != b.started);
        // Asked again later: the running engine.
        assert_eq!(supervisor.ensure(&fake, viewer("c", None, true)).await.unwrap(), Ensured { generation: a.generation, started: false });
        assert_eq!(fake.lines("start").len(), 1);
        assert_eq!(fake.lines("state c"), vec![format!("state c {} starting viewer", a.generation)]);
        // A restart replaces it.
        let restarted = supervisor
            .ensure(&fake, EnsureRequest { restart: true, ..viewer("c", Some("/w"), true) })
            .await
            .unwrap();
        assert!(restarted.started && restarted.generation > a.generation);
    }

    #[tokio::test]
    async fn ensure_starts_in_the_folder_asked_for_else_the_chats_own_with_the_flags() {
        let fake = Fake::default().with_folder("n", "/repo/.worktrees/n");
        let supervisor = EngineSupervisor::default();
        let flags = ChatFlags { memory_off: true, incognito: false };
        supervisor.ensure(&fake, EnsureRequest { options: Some(flags), ..viewer("d", Some("/drafts/here"), false) }).await.unwrap();
        supervisor.ensure(&fake, viewer("n", None, true)).await.unwrap();
        supervisor.ensure(&fake, viewer("h", Some(""), true)).await.unwrap();
        let starts = fake.lines("start");
        assert!(starts[0].starts_with("start d /drafts/here Some(ChatFlags { memory_off: true, incognito: false })"), "{starts:?}");
        assert!(starts[1].starts_with("start n /repo/.worktrees/n None"), "{starts:?}");
        assert!(starts[2].starts_with("start h /home None"), "{starts:?}");
    }

    /// A scratch app database, removed on drop.
    struct Store {
        root: PathBuf,
        conn: Connection,
    }

    impl Store {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!("evo-supervisor-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&root).unwrap();
            let conn = evo_store::open_at(&root.join("evo.db")).unwrap();
            Self { root, conn }
        }
    }

    impl Drop for Store {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_chats_folder_is_its_worktree_else_its_workspace_else_home() {
        let store = Store::new();
        let workspace = chats::ensure_workspace(&store.conn, "/repo").unwrap();
        let new = |id: &str, workspace: Option<&str>, worktree: Option<&str>| chats::NewChat {
            id: id.into(),
            title: id.into(),
            workspace_id: workspace.map(str::to_string),
            worktree_path: worktree.map(str::to_string),
            ..Default::default()
        };
        chats::create_chat(&store.conn, &new("wt", Some(&workspace.id), Some("/repo-wt/a"))).unwrap();
        chats::create_chat(&store.conn, &new("ws", Some(&workspace.id), None)).unwrap();
        chats::create_chat(&store.conn, &new("local", None, None)).unwrap();
        let home = Path::new("/home/me");
        assert_eq!(folder_of(&store.conn, "wt", home), PathBuf::from("/repo-wt/a"));
        assert_eq!(folder_of(&store.conn, "ws", home), PathBuf::from(&workspace.path));
        assert_eq!(folder_of(&store.conn, "local", home), PathBuf::from("/home/me"));
        // A draft has no row yet.
        assert_eq!(folder_of(&store.conn, "draft", home), PathBuf::from("/home/me"));
    }

    #[tokio::test]
    async fn set_up_on_ready_sets_the_rows_model_then_the_tools_then_hosts() {
        let fake = Fake::default().with_row("c", top(Some(("acme", "acme-large"))));
        fake.answer("get_state", Ok(json!({ "provider": "other", "modelId": "small" })));
        let supervisor = EngineSupervisor::default();
        let started = supervisor.ensure(&fake, viewer("c", Some("/w"), false)).await.unwrap();
        supervisor.on_ready(&fake, "c", started.generation).await;
        let generation = started.generation;
        assert_eq!(
            fake.log()[1..],
            [
                format!("state c {generation} starting viewer"),
                "request c get_state {}".to_string(),
                "request c set_model acme/acme-large".to_string(),
                "request c configure_external_tools start_session,browser@/w".to_string(),
                r#"send c subscribe {"level":"summary"}"#.to_string(),
                "hosted c".to_string(),
                format!("state c {generation} ready -"),
            ],
        );
    }

    #[tokio::test]
    async fn a_nested_session_gets_no_session_tools_and_keeps_a_model_it_runs_on() {
        let fake = Fake::default().with_row("n", Row { model: Some(("acme".into(), "m".into())), top_level: false, archived: false });
        fake.answer("get_state", Ok(json!({ "provider": "acme", "modelId": "m" })));
        let supervisor = EngineSupervisor::default();
        let started = supervisor.ensure(&fake, viewer("n", Some("/w"), false)).await.unwrap();
        supervisor.on_ready(&fake, "n", started.generation).await;
        assert!(fake.lines("request n set_model").is_empty(), "{:?}", fake.log());
        assert_eq!(fake.lines("request n configure"), vec!["request n configure_external_tools browser@/w"]);
        assert_eq!(fake.lines("hosted"), vec!["hosted n"]);
    }

    #[tokio::test]
    async fn a_reopened_chat_keeps_its_sessions_model() {
        let fake = Fake::default().with_row("c", top(Some(("acme", "acme-large"))));
        let supervisor = EngineSupervisor::default();
        let started = supervisor.ensure(&fake, viewer("c", None, true)).await.unwrap();
        supervisor.on_ready(&fake, "c", started.generation).await;
        assert!(fake.lines("request c get_state").is_empty() && fake.lines("request c set_model").is_empty(), "{:?}", fake.log());
        assert_eq!(fake.lines("hosted"), vec!["hosted c"]);
    }

    #[tokio::test]
    async fn a_chat_naming_no_model_starts_on_the_saved_default() {
        // A draft (no row) on a model the engine picked itself.
        let fake = Fake::default();
        fake.answer("get_state", Ok(json!({ "provider": "auto", "modelId": "picked" })));
        fake.answer("get_default_model", Ok(json!({ "provider": "acme", "modelId": "fav" })));
        let supervisor = EngineSupervisor::default();
        let started = supervisor.ensure(&fake, viewer("d", Some("/w"), false)).await.unwrap();
        supervisor.on_ready(&fake, "d", started.generation).await;
        assert_eq!(fake.lines("request d set_model"), vec!["request d set_model acme/fav"]);
        // Already on it, or nothing saved: no switch.
        for (state, saved) in [(json!({ "provider": "acme", "modelId": "fav" }), json!({ "provider": "acme", "modelId": "fav" })), (json!({ "provider": "auto", "modelId": "picked" }), json!({ "provider": null, "modelId": null }))] {
            let fake = Fake::default();
            fake.answer("get_state", Ok(state));
            fake.answer("get_default_model", Ok(saved));
            let started = supervisor.ensure(&fake, EnsureRequest { restart: true, ..viewer("e", Some("/w"), false) }).await.unwrap();
            supervisor.on_ready(&fake, "e", started.generation).await;
            assert!(fake.lines("request e set_model").is_empty(), "{:?}", fake.log());
        }
        // The row's model is gone (its provider was removed): the default.
        let fake = Fake::default().with_row("g", top(Some(("gone", "old"))));
        fake.answer("get_state", Ok(json!({ "provider": "auto", "modelId": "picked" })));
        fake.answer("set_model", Err("unknown provider gone".into()));
        fake.answer("get_default_model", Ok(json!({ "provider": "acme", "modelId": "fav" })));
        let started = supervisor.ensure(&fake, viewer("g", Some("/w"), false)).await.unwrap();
        supervisor.on_ready(&fake, "g", started.generation).await;
        assert_eq!(fake.lines("request g set_model").len(), 2);
        assert_eq!(fake.lines("log g warn").len(), 1);
    }

    #[tokio::test]
    async fn tools_refused_during_a_run_are_set_when_it_settles() {
        let fake = Fake::default().with_row("c", top(None));
        fake.answer("configure_external_tools", Err("cannot configure external tools while a run is active".into()));
        let supervisor = EngineSupervisor::default();
        let started = supervisor.ensure(&fake, viewer("c", Some("/w"), true)).await.unwrap();
        supervisor.on_ready(&fake, "c", started.generation).await;
        // Hosted anyway: its mail does not wait for the tools.
        assert_eq!(fake.lines("hosted"), vec!["hosted c"]);
        assert_eq!(fake.lines("request c configure").len(), 1);
        supervisor.on_settled(&fake, "c", started.generation).await;
        assert_eq!(fake.lines("request c configure").len(), 2);
        // Set now: the next settle leaves them.
        supervisor.on_settled(&fake, "c", started.generation).await;
        assert_eq!(fake.lines("request c configure").len(), 2);
        // Another failure is not retried on settle.
        fake.answer("configure_external_tools", Err("bad tool".into()));
        supervisor.reconfigure(&fake, "c").await;
        supervisor.on_settled(&fake, "c", started.generation).await;
        assert_eq!(fake.lines("request c configure").len(), 3);
    }

    #[tokio::test]
    async fn a_ready_from_a_replaced_or_exited_engine_sets_nothing_up() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let old = supervisor.ensure(&fake, viewer("c", Some("/w"), true)).await.unwrap();
        let new = supervisor.ensure(&fake, EnsureRequest { restart: true, ..viewer("c", Some("/w"), true) }).await.unwrap();
        supervisor.on_ready(&fake, "c", old.generation).await;
        assert!(fake.lines("request").is_empty() && fake.lines("hosted").is_empty());
        supervisor.on_exit(&fake, "c", new.generation, false);
        supervisor.on_ready(&fake, "c", new.generation).await;
        assert!(fake.lines("request").is_empty() && fake.lines("hosted").is_empty());
    }

    #[tokio::test]
    async fn tools_are_set_again_for_set_up_chats_when_they_change() {
        let fake = Fake::default().with_row("a", top(None)).with_row("b", top(None));
        let supervisor = EngineSupervisor::default();
        let a = supervisor.ensure(&fake, viewer("a", Some("/a"), true)).await.unwrap();
        supervisor.ensure(&fake, viewer("b", Some("/b"), true)).await.unwrap();
        supervisor.on_ready(&fake, "a", a.generation).await;
        // `b` is still starting: its own set-up gives it the new tools.
        supervisor.reconfigure_all(&fake).await;
        assert_eq!(fake.lines("request a configure").len(), 2);
        assert!(fake.lines("request b").is_empty());
        supervisor.reconfigure(&fake, "a").await;
        supervisor.reconfigure(&fake, "b").await;
        assert_eq!(fake.lines("request a configure").len(), 3);
        assert!(fake.lines("request b").is_empty());
    }

    #[tokio::test]
    async fn a_chat_that_becomes_nested_or_top_level_gets_its_tools_again() {
        let mut fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let started = supervisor.ensure(&fake, viewer("c", Some("/w"), true)).await.unwrap();
        supervisor.on_ready(&fake, "c", started.generation).await;
        supervisor.rows_changed(&fake).await;
        assert_eq!(fake.lines("request c configure").len(), 1);
        fake.rows.insert("c".into(), Row { top_level: false, ..top(None) });
        supervisor.rows_changed(&fake).await;
        assert_eq!(fake.lines("request c configure"), vec!["request c configure_external_tools start_session,browser@/w", "request c configure_external_tools browser@/w"]);
    }

    // ── Hosting for the mesh (FR-4, T5) ────────────────────────────────────

    #[tokio::test]
    async fn mail_hosts_its_chat_in_the_effects_folder() {
        let fake = Fake::default().with_row("s", Row { top_level: false, ..top(None) }).with_folder("s", "/repo");
        let supervisor = EngineSupervisor::default();
        // A new session in its own worktree, not restored.
        supervisor.host_for_mail(&fake, "s", Some("/repo-wt/s".into()), false, Reason::Mail).await;
        let starts = fake.lines("start");
        assert_eq!(starts.len(), 1);
        assert!(starts[0].starts_with("start s /repo-wt/s None"), "{starts:?}");
        assert_eq!(fake.lines("state s"), vec!["state s 1 starting mail"]);
        // Set up on `ready`, hosted by its set-up.
        supervisor.on_ready(&fake, "s", 1).await;
        assert_eq!(fake.lines("hosted"), vec!["hosted s"]);
        // More mail while it runs and is set up: hosted again, not started.
        supervisor.host_for_mail(&fake, "s", None, true, Reason::Mail).await;
        assert_eq!(fake.lines("start").len(), 1);
        assert_eq!(fake.lines("hosted").len(), 2);
    }

    #[tokio::test]
    async fn mail_for_an_engine_being_set_up_waits_for_its_set_up() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        supervisor.ensure(&fake, viewer("c", Some("/w"), true)).await.unwrap();
        supervisor.host_for_mail(&fake, "c", None, true, Reason::Mail).await;
        assert_eq!(fake.lines("start").len(), 1);
        assert!(fake.lines("hosted").is_empty());
    }

    #[tokio::test]
    async fn pending_chats_are_hosted_at_setup_except_archived_and_unsaved_ones() {
        let fake = Fake::default()
            .with_row("a", top(None))
            .with_row("old", Row { archived: true, ..top(None) })
            .with_folder("a", "/ws/a");
        let supervisor = EngineSupervisor::default();
        supervisor.host_pending(&fake, vec!["a".into(), "old".into(), "gone".into()]).await;
        let starts = fake.lines("start");
        assert_eq!(starts.len(), 1, "{:?}", fake.log());
        assert!(starts[0].starts_with("start a /ws/a"));
        assert_eq!(fake.lines("log gone warn").len(), 1);
        let started: HashSet<String> = fake.running.lock().unwrap().keys().cloned().collect();
        assert_eq!(started, HashSet::from(["a".to_string()]));
    }

    #[test]
    fn an_engine_is_restarted_at_most_three_times_in_two_minutes() {
        let mut limiter = RestartLimiter::default();
        let t0 = Instant::now();
        assert!(limiter.allow("c", t0));
        assert!(limiter.allow("c", t0 + Duration::from_secs(10)));
        assert!(limiter.allow("c", t0 + Duration::from_secs(20)));
        assert!(!limiter.allow("c", t0 + Duration::from_secs(30)));
        // Another chat has its own count.
        assert!(limiter.allow("d", t0 + Duration::from_secs(30)));
        // Two minutes after the first restart, one more may go.
        assert!(limiter.allow("c", t0 + Duration::from_secs(120)));
        assert!(!limiter.allow("c", t0 + Duration::from_secs(125)));
    }

    #[tokio::test]
    async fn a_crashing_engine_with_mail_is_restarted_three_times_then_left() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let t0 = Instant::now();
        for second in 0..5u64 {
            supervisor.restart_after_crash(&fake, "c", t0 + Duration::from_secs(second)).await;
            fake.crash("c");
        }
        assert_eq!(fake.lines("start").len(), RESTARTS, "{:?}", fake.log());
        assert!(fake.lines("state c").iter().all(|line| line.ends_with("starting restart")));
        assert_eq!(fake.lines("log c warn the engine keeps stopping").len(), 2);
        // The person hears it (FR-4, FR-10).
        assert_eq!(fake.lines("notice c keeps-stopping").len(), 2);
    }

    // ── Viewers, work and the idle trim (FR-2, FR-3, T8) ───────────────────

    /// `chat`'s engine started and set up; its generation.
    pub(crate) async fn set_up(fake: &Fake, supervisor: &EngineSupervisor, chat: &str) -> u64 {
        let started = supervisor.ensure(fake, viewer(chat, Some("/w"), true)).await.unwrap();
        supervisor.on_ready(fake, chat, started.generation).await;
        started.generation
    }

    /// A mesh `node_state` event of node `id`.
    fn node_state(seq: u64, id: &str, kind: &str, state: &str, wake: &[&str]) -> Value {
        json!({
            "type": "mesh", "v": 1, "id": format!("e{seq}"), "kind": "evt", "op": "node_state",
            "from": id, "to": "*", "seq": seq, "ts": 0,
            "body": { "node": { "id": id, "kind": kind, "state": state, "meta": { "wake": wake } } },
        })
    }

    /// Well after every engine's grace.
    fn later() -> Instant {
        Instant::now() + Duration::from_secs(3600)
    }

    #[test]
    fn a_window_shows_one_chat_and_a_chat_may_have_several_windows() {
        let supervisor = EngineSupervisor::default();
        let now = Instant::now();
        assert_eq!(supervisor.attach("main", "a", now), None);
        assert_eq!(supervisor.attach("main", "a", now), None, "the same chat again");
        // The main window shows another chat: `a` lost it.
        assert_eq!(supervisor.attach("main", "b", now), Some("a".into()));
        assert!(!supervisor.has_viewer("a"));
        supervisor.attach("chat-b", "b", now);
        assert_eq!(supervisor.viewers_of("b"), ["chat-b", "main"]);
        // A window lets go only of the chat it shows.
        assert!(!supervisor.detach("main", "a", now));
        assert!(supervisor.detach("main", "b", now));
        assert_eq!(supervisor.viewers_of("b"), ["chat-b"]);
        // A closed window shows nothing.
        assert_eq!(supervisor.window_gone("chat-b", now), Some("b".into()));
        assert_eq!(supervisor.window_gone("chat-b", now), None);
        assert!(!supervisor.has_viewer("b"));
    }

    #[tokio::test]
    async fn work_is_a_run_working_agents_waiting_reports_mail_or_an_open_question() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let generation = set_up(&fake, &supervisor, "c").await;
        let follow = |value: Value| supervisor.follow_line(&fake, "c", generation, &value, 1);
        assert!(!supervisor.working(&fake, "c"));
        // A run.
        assert!(!follow(json!({ "type": "agent_start" })));
        assert!(supervisor.working(&fake, "c"));
        assert!(follow(json!({ "type": "agent_settled", "outcome": "completed" })), "a settled run trims");
        assert!(!supervisor.working(&fake, "c"));
        // A background agent, from the shell's mesh subscription.
        follow(node_state(5, "s_c/a_1", "agent", "running", &[]));
        assert!(supervisor.working(&fake, "c"));
        follow(node_state(6, "s_c/a_1", "agent", "settled", &[]));
        assert!(!supervisor.working(&fake, "c"));
        // A replay of an older state changes nothing.
        follow(node_state(5, "s_c/a_1", "agent", "running", &[]));
        assert!(!supervisor.working(&fake, "c"));
        // Its report waits for the chat to read it (`meta.wake`).
        follow(node_state(7, "s_c", "session", "idle", &["s_c/a_1"]));
        assert!(supervisor.working(&fake, "c"));
        follow(node_state(8, "s_c", "session", "running", &[]));
        assert!(!supervisor.working(&fake, "c"));
        // Mail in the hub's store.
        fake.mail_for("c", true);
        assert!(supervisor.working(&fake, "c"));
        fake.mail_for("c", false);
        // An open question.
        follow(json!({ "type": "ui_dialog_request", "requestId": "approve:1", "ui": { "kind": "confirm", "title": "Run it?" } }));
        assert!(supervisor.working(&fake, "c"));
        follow(json!({ "type": "ui_dialog_closed", "requestId": "approve:1" }));
        assert!(!supervisor.working(&fake, "c"));
        // A line of a replaced engine is not this one's.
        assert!(!supervisor.follow_line(&fake, "c", generation + 7, &json!({ "type": "agent_start" }), 1));
        assert!(!supervisor.working(&fake, "c"));
    }

    #[tokio::test]
    async fn the_trim_keeps_the_newest_four_idle_engines_and_never_one_with_work_or_a_viewer() {
        let chats = ["a", "b", "c", "d", "e", "f", "g"];
        let mut fake = Fake::default();
        for chat in chats {
            fake = fake.with_row(chat, top(None));
        }
        let supervisor = EngineSupervisor::default();
        let mut generations = HashMap::new();
        for chat in chats {
            generations.insert(chat, set_up(&fake, &supervisor, chat).await);
        }
        // `a` is the oldest and has a background agent working; `b` is shown.
        supervisor.follow_line(&fake, "a", generations["a"], &node_state(3, "s_a/a_1", "agent", "running", &[]), 1);
        supervisor.attach("main", "b", Instant::now());
        // Just used: none is stopped yet.
        supervisor.trim(&fake, Instant::now()).await;
        assert!(fake.lines("stop").is_empty(), "{:?}", fake.log());
        // Five idle (c – g): the oldest goes.
        supervisor.trim(&fake, later()).await;
        assert_eq!(fake.lines("stop"), ["stop c"]);
        assert_eq!(fake.lines("state c").last().unwrap(), &format!("state c {} stopped idle", generations["c"]));
        assert_eq!(supervisor.attach_state(&fake, "c").status, "stopped");
        // Four idle are kept.
        supervisor.trim(&fake, later()).await;
        assert_eq!(fake.lines("stop").len(), 1);
        // The window shows `a` now and its agent finished: `b` is idle again
        // (just left, so the newest); of b, d – g the oldest, `d`, goes.
        supervisor.attach("main", "a", Instant::now());
        supervisor.follow_line(&fake, "a", generations["a"], &node_state(4, "s_a/a_1", "agent", "settled", &[]), 1);
        supervisor.trim(&fake, later()).await;
        assert_eq!(fake.lines("stop"), ["stop c", "stop d"]);
    }

    #[tokio::test]
    async fn a_chat_with_a_running_background_agent_is_never_stopped() {
        let chats = ["old", "b", "c", "d", "e", "f"];
        let mut fake = Fake::default();
        for chat in chats {
            fake = fake.with_row(chat, top(None));
        }
        let supervisor = EngineSupervisor::default();
        let old = set_up(&fake, &supervisor, "old").await;
        for chat in &chats[1..] {
            set_up(&fake, &supervisor, chat).await;
        }
        supervisor.follow_line(&fake, "old", old, &node_state(2, "s_old/a_9", "agent", "running", &[]), 1);
        for _ in 0..3 {
            supervisor.trim(&fake, later()).await;
        }
        // Five idle (b – f): one goes, never `old`.
        assert_eq!(fake.lines("stop"), ["stop b"]);
    }

    #[tokio::test]
    async fn a_closed_window_leaves_its_chats_engine_running_and_says_so() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let generation = set_up(&fake, &supervisor, "c").await;
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "agent_start" }), 1);
        supervisor.attach("chat-c", "c", Instant::now());
        // The chat window closes mid-turn.
        assert_eq!(supervisor.window_gone("chat-c", Instant::now()), Some("c".into()));
        supervisor.left(&fake, "c");
        assert_eq!(fake.lines("state c").last().unwrap(), &format!("state c {generation} ready detached"));
        supervisor.trim(&fake, later()).await;
        assert!(fake.lines("stop").is_empty());
        assert!(supervisor.working(&fake, "c"));
        // Another window still shows it: nothing to say.
        supervisor.attach("main", "c", Instant::now());
        supervisor.attach("chat-c", "c", Instant::now());
        supervisor.window_gone("chat-c", Instant::now());
        supervisor.left(&fake, "c");
        assert_eq!(fake.lines("state c").iter().filter(|line| line.ends_with("detached")).count(), 1);
    }

    #[tokio::test]
    async fn attaching_returns_the_engines_state_its_work_and_every_open_question() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        assert_eq!(
            supervisor.attach_state(&fake, "c"),
            AttachState { generation: None, status: "stopped", working: false, questions: Vec::new() },
        );
        let generation = set_up(&fake, &supervisor, "c").await;
        let ask = |id: &str, kind: &str| json!({ "type": "ui_dialog_request", "requestId": id, "ui": { "kind": kind, "title": "?" } });
        supervisor.follow_line(&fake, "c", generation, &ask("approve:1", "confirm"), 1);
        supervisor.follow_line(&fake, "c", generation, &ask("ask:2", "questions"), 2);
        supervisor.questions.open("c", questions::OpenQuestion::from_shell("agent-ask:3", "req", "Allow?", "", vec!["Allow".into()], 3));
        let state = supervisor.attach_state(&fake, "c");
        assert_eq!((state.generation, state.status, state.working), (Some(generation), "ready", true));
        assert_eq!(state.questions.iter().map(|q| q.request_id.as_str()).collect::<Vec<_>>(), ["approve:1", "ask:2", "agent-ask:3"]);
        // Each new question was told to the windows and announced.
        assert_eq!(fake.lines("question c"), ["question c open approve:1", "question c open ask:2"]);
        assert_eq!(fake.lines("notice c"), ["notice c approval", "notice c question"]);
        // The engine goes: its questions with it, its calls' too.
        supervisor.on_exit(&fake, "c", generation, false);
        assert_eq!(fake.lines("question c closed"), ["question c closed approve:1", "question c closed ask:2", "question c closed agent-ask:3"]);
        assert!(supervisor.attach_state(&fake, "c").questions.is_empty());
    }

    #[tokio::test]
    async fn the_end_of_a_run_and_a_crash_are_announced() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let generation = set_up(&fake, &supervisor, "c").await;
        let settle = |outcome: &str, compact: bool| {
            supervisor.follow_line(&fake, "c", generation, &json!({ "type": "agent_settled", "outcome": outcome, "compact": compact }), 1);
        };
        settle("completed", false);
        settle("failed", false);
        // A compaction is no turn; a stopped run is the person's own doing.
        settle("completed", true);
        settle("aborted", false);
        assert_eq!(fake.lines("notice c"), ["notice c reply-ready", "notice c run-failed"]);
        // An idle engine with no window that ends by itself: nothing to say.
        fake.crash("c");
        supervisor.on_exit(&fake, "c", generation, true);
        assert_eq!(fake.lines("notice c").len(), 2);
        // One in a run that crashes: said.
        let generation = set_up(&fake, &supervisor, "c").await;
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "agent_start" }), 1);
        fake.crash("c");
        supervisor.on_exit(&fake, "c", generation, true);
        assert_eq!(fake.lines("notice c").last().unwrap(), "notice c engine-stopped");
        assert!(!supervisor.working(&fake, "c"), "its run ended with it");
        // Stopped on purpose: nothing.
        fake.crash("c");
        let generation = set_up(&fake, &supervisor, "c").await;
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "agent_start" }), 1);
        supervisor.on_exit(&fake, "c", generation, false);
        assert_eq!(fake.lines("notice c").len(), 3);
    }

    // ── The hold around a worktree Apply (plan 5, "To do in Phase 3") ──────

    #[tokio::test]
    async fn mail_does_not_start_an_engine_held_for_a_worktree_apply_until_it_is_resumed() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let generation = set_up(&fake, &supervisor, "c").await;
        // The window holds it, then stops it (its folder is about to go).
        supervisor.hold("c");
        fake.crash("c");
        supervisor.on_exit(&fake, "c", generation, false);
        // Mail comes during the Apply: it waits.
        fake.mail_for("c", true);
        supervisor.host_for_mail(&fake, "c", None, true, Reason::Mail).await;
        supervisor.restart_after_crash(&fake, "c", Instant::now()).await;
        assert_eq!(fake.lines("start").len(), 1, "{:?}", fake.log());
        assert_eq!(fake.lines("log c info mail waits").len(), 1);
        // The window is done and starts nothing itself: the mail goes out now.
        supervisor.release(&fake, "c").await;
        assert_eq!(fake.lines("start").len(), 2);
        assert!(fake.lines("state c").last().unwrap().ends_with("starting mail"));
        // Released twice: nothing more.
        supervisor.release(&fake, "c").await;
        assert_eq!(fake.lines("start").len(), 2);
    }

    #[tokio::test]
    async fn a_window_that_starts_the_engine_again_ends_the_hold() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let generation = set_up(&fake, &supervisor, "c").await;
        supervisor.hold("c");
        fake.crash("c");
        supervisor.on_exit(&fake, "c", generation, false);
        assert!(supervisor.is_held("c"));
        // The window resumes it (in its workspace folder after an Apply).
        set_up(&fake, &supervisor, "c").await;
        assert!(!supervisor.is_held("c"));
        // Mail now reaches the running engine.
        supervisor.host_for_mail(&fake, "c", None, true, Reason::Mail).await;
        assert_eq!(fake.lines("hosted c").len(), 3);
        // Nothing left to release: no other start.
        fake.mail_for("c", true);
        supervisor.release(&fake, "c").await;
        assert_eq!(fake.lines("start").len(), 2);
    }
}
