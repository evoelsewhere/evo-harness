//! Turn checkpoints in the engine: every top-level run of a chat whose folder
//! is in a git repository snapshots the folder when it starts and when it
//! settles (`evo_worktrees::checkpoint`), holds its first file-changing tool
//! call until the start snapshot exists, reports the files it changed
//! (`agent_settled.checkpoint` and a `checkpoint` session entry), and
//! `revert_turn` puts a turn's files back and tells the agent.
//!
//! Spec: `docs/features/source-control/spec.md` (FR-9 – FR-13, FR-7 for the
//! draft in `commit_message.rs`); plan 3.3, 3.5, 3.6; decision: ADR 0024.
//!
//! * [`Gate`] holds the start snapshot of the run in progress (one per
//!   engine). Its link ([`before_tool_call`]) sits after the approval gate, so
//!   a call blocked before it never waits and is never let through by it.
//!   Reads go on; edits, commands and unknown tools wait for the snapshot for
//!   at most [`WAIT`], then go on, and the turn is then `complete: false`.
//!   Subagents inherit the chain: one in the chat's folder is part of its
//!   parent's turn, so its edits wait for the same snapshot; it takes none.
//! * [`Checkpoints`] is one chat's (the chat id in ref names is the session
//!   id). Turn numbers continue from the refs and the entries, so they survive
//!   restarts. On start, a turn that never settled (the engine stopped in the
//!   middle of a run) is closed with an end snapshot and `complete: false`.
//! * A revert appends the checkpoint entry again with `reverted` set; the
//!   newest entry of an id wins.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use evo_ai::types::{Content, UserContent};
use evo_harness::agent_harness::{AgentHarness, CancelQueuedOutcome};
use evo_harness::session::types::{BranchBounds, Entry, EntryOrder, EntryQuery};
use evo_protocol::protocol::{
    checkpoint_id, checkpoint_undo_id, parse_checkpoint_id, Checkpoint, CheckpointFile,
    CheckpointRevert, ListCheckpointsResult, RevertTurnRequest, RevertTurnResult, SkippedFile,
    CHECKPOINT_ENTRY, REVERT_BUSY, SKIPPED_CHANGED_SINCE,
};
use evo_worktrees::checkpoint::{self, Kind, KEEP_TURNS};
use futures::future::{BoxFuture, Shared};
use futures::FutureExt;
use serde_json::Value;

/// How long a file-changing call waits for the start snapshot (plan 3.5).
pub const WAIT: Duration = Duration::from_secs(10);

/// Characters of the turn's prompt quoted in the revert note.
const NOTE_PROMPT_CHARS: usize = 80;

/// The start snapshot of a run: its turn number and commit, `None` when it
/// failed (then the turn has no checkpoint).
type Start = Shared<BoxFuture<'static, Option<(u64, String)>>>;

struct Open {
    start: Start,
    /// A call stopped waiting for it: the checkpoint is incomplete.
    late: Arc<AtomicBool>,
}

/// The start snapshot of the run in progress, which file-changing tool calls
/// wait for.
pub struct Gate {
    open: Mutex<Option<Open>>,
    wait: Duration,
}

impl Default for Gate {
    fn default() -> Self {
        Self::with_wait(WAIT)
    }
}

impl Gate {
    /// A gate whose calls wait at most `wait`.
    pub fn with_wait(wait: Duration) -> Self {
        Self {
            open: Mutex::new(None),
            wait,
        }
    }

    fn open(&self, start: Start) -> Arc<AtomicBool> {
        let late = Arc::new(AtomicBool::new(false));
        *self.open.lock().unwrap_or_else(|e| e.into_inner()) = Some(Open {
            start,
            late: Arc::clone(&late),
        });
        late
    }

    fn close(&self) {
        *self.open.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn current(&self) -> Option<(Start, Arc<AtomicBool>)> {
        self.open
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|open| (open.start.clone(), Arc::clone(&open.late)))
    }
}

/// This engine's gate: the session's tool chain and the RPC loop share it.
pub fn global() -> &'static Arc<Gate> {
    static GATE: OnceLock<Arc<Gate>> = OnceLock::new();
    GATE.get_or_init(|| Arc::new(Gate::default()))
}

/// The checkpoint link, behind `previous` (the guards, hooks, plugin handlers
/// and the approval gate): it runs after them, returns a block of theirs at
/// once, and never changes their answer.
pub fn before_tool_call(
    previous: Option<evo_agent::BeforeToolCall>,
    gate: Arc<Gate>,
) -> Option<evo_agent::BeforeToolCall> {
    Some(Arc::new(
        move |ctx: evo_agent::BeforeToolCallContext<'_>, signal| {
            let tool = ctx.tool_call.name.clone();
            let args = ctx.args.clone();
            let earlier = previous.as_ref().map(|hook| hook(ctx, signal.clone()));
            let gate = Arc::clone(&gate);
            Box::pin(async move {
                let inner = match earlier {
                    Some(earlier) => earlier.await,
                    None => None,
                };
                if inner.as_ref().is_some_and(|result| result.block) {
                    return inner;
                }
                // What will run: a hook may have rewritten the arguments.
                let args = inner
                    .as_ref()
                    .and_then(|result| result.args.clone())
                    .unwrap_or(args);
                if crate::approval::classify(&tool, &args) == crate::approval::Kind::Read {
                    return inner;
                }
                let Some((start, late)) = gate.current() else {
                    return inner;
                };
                if start.peek().is_some() {
                    return inner;
                }
                tokio::select! {
                    _ = start => {}
                    _ = signal.cancelled() => {}
                    _ = tokio::time::sleep(gate.wait) => {
                        late.store(true, Ordering::Release);
                        log("the start snapshot took too long; this turn's checkpoint may miss changes");
                    }
                }
                inner
            })
        },
    ))
}

/// Log a checkpoint problem once per engine (a missing git would otherwise
/// repeat at every turn).
fn log(message: impl Into<String>) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let message = message.into();
    let mut seen = SEEN
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if seen.len() < 64 && seen.insert(message.clone()) {
        eprintln!("[evo] checkpoint: {message}");
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Whether `dir` is inside a git working tree, without starting git: a
/// `.git` folder or file in it or a folder above it.
fn in_repository(dir: &Path) -> bool {
    dir.ancestors().any(|folder| folder.join(".git").exists())
}

fn chat_id(harness: &AgentHarness) -> String {
    harness.session().storage().metadata().id.clone()
}

/// Run git work off the async workers.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| format!("checkpoint task failed: {error}"))?
}

const GONE: &str = "this turn's checkpoint is no longer kept";

fn wire_file(file: checkpoint::TurnFile) -> CheckpointFile {
    CheckpointFile {
        path: file.path,
        status: file.status.to_string(),
        from: file.from,
        added: file.added.map(|n| n as u64),
        removed: file.removed.map(|n| n as u64),
    }
}

/// The end snapshot of turn `seq`, the files it changed since `start`, and
/// the chat pruned to its newest turns.
fn close_turn(
    cwd: &Path,
    chat: &str,
    seq: u64,
    start: &str,
) -> Result<(String, Vec<CheckpointFile>), String> {
    let end = checkpoint::snapshot(cwd, chat, seq, Kind::End)?;
    let files = checkpoint::changes_between(cwd, start, &end)?
        .into_iter()
        .map(wire_file)
        .collect();
    if let Err(error) = checkpoint::prune(cwd, chat, KEEP_TURNS) {
        log(format!("could not remove old checkpoints: {error}"));
    }
    Ok((end, files))
}

/// The text of a user message (its text blocks).
fn user_text(content: &UserContent) -> String {
    match content {
        UserContent::Text(text) => text.clone(),
        UserContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                Content::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// A turn's prompt: its entry id and time.
struct Prompt {
    id: String,
    at: i64,
}

/// The prompt of the run whose entries come after `since` (an entry id and
/// its seq; `None` for the start of the branch): the last user message before
/// its first response. Notes queued for the run (mail, a revert note) come
/// before the prompt, steering after the first response.
async fn prompt_after(harness: &AgentHarness, since: (Option<String>, u64)) -> Option<Prompt> {
    let (stop, after_seq) = since;
    let mut entries = harness
        .find_entries_on_branch(
            &EntryQuery {
                entry_type: Some("message"),
                order: Some(EntryOrder::NewestFirst),
                ..Default::default()
            },
            &BranchBounds {
                stop_at_id: stop.clone(),
                ..Default::default()
            },
        )
        .await
        .ok()?;
    entries.reverse();
    let mut prompt = None;
    for entry in entries {
        let Entry::Message(entry) = entry else {
            continue;
        };
        if stop.as_deref() == Some(entry.base.id.as_str()) || entry.base.seq <= after_seq {
            continue;
        }
        match &entry.message {
            evo_agent::AgentMessage::User(_) => {
                prompt = Some(Prompt {
                    id: entry.base.id.clone(),
                    at: entry.base.timestamp,
                })
            }
            evo_agent::AgentMessage::Assistant(_) => break,
            _ => {}
        }
    }
    prompt
}

/// The text of the user message `entry_id`.
async fn prompt_text(harness: &AgentHarness, entry_id: &str) -> Option<String> {
    let entry = harness
        .find_entry(&EntryQuery {
            entry_id: Some(entry_id.to_string()),
            ..Default::default()
        })
        .await
        .ok()??;
    match entry {
        Entry::Message(entry) => match &entry.message {
            evo_agent::AgentMessage::User(message) => Some(user_text(&message.content)),
            _ => None,
        },
        _ => None,
    }
}

/// The chat's saved checkpoints, newest entry of each id, by turn number.
async fn saved(harness: &AgentHarness) -> Vec<Checkpoint> {
    let entries = harness
        .find_entries(&EntryQuery {
            entry_type: Some("custom"),
            custom_type: Some(CHECKPOINT_ENTRY.to_string()),
            order: Some(EntryOrder::OldestFirst),
            ..Default::default()
        })
        .await
        .unwrap_or_default();
    let mut by_seq: HashMap<u64, Checkpoint> = HashMap::new();
    for entry in entries {
        let Entry::Custom(entry) = entry else {
            continue;
        };
        let Some(checkpoint) = entry
            .data
            .and_then(|data| serde_json::from_value::<Checkpoint>(data).ok())
        else {
            continue;
        };
        by_seq.insert(checkpoint.seq, checkpoint);
    }
    let mut all: Vec<Checkpoint> = by_seq.into_values().collect();
    all.sort_by_key(|checkpoint| checkpoint.seq);
    all
}

/// The newest checkpoint entry on the current branch: its id and seq.
async fn last_on_branch(harness: &AgentHarness) -> (Option<String>, u64) {
    let found = harness
        .find_entries_on_branch(
            &EntryQuery {
                entry_type: Some("custom"),
                custom_type: Some(CHECKPOINT_ENTRY.to_string()),
                order: Some(EntryOrder::NewestFirst),
                limit: Some(1),
                ..Default::default()
            },
            &BranchBounds::default(),
        )
        .await
        .unwrap_or_default();
    found
        .first()
        .map(|entry| (Some(entry.base().id.clone()), entry.base().seq))
        .unwrap_or((None, 0))
}

async fn save(harness: &AgentHarness, checkpoint: &Checkpoint) {
    let data = serde_json::to_value(checkpoint).ok();
    if let Err(error) = harness.append_custom_entry(CHECKPOINT_ENTRY, data).await {
        log(format!("could not save {}: {error}", checkpoint.id));
    }
}

/// The note the agent's next run starts with after a revert (FR-12), or
/// after the revert is undone. The opening tag is the one the window shows as
/// an event line.
fn note(
    seq: u64,
    prompt: Option<&str>,
    undone: bool,
    done: &[String],
    skipped: &[String],
) -> String {
    let quoted = prompt
        .map(|text| {
            let flat = text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .replace(['<', '>', '"'], "");
            let mut short: String = flat.chars().take(NOTE_PROMPT_CHARS).collect();
            if flat.chars().count() > NOTE_PROMPT_CHARS {
                short.push('…');
            }
            short
        })
        .filter(|text| !text.is_empty())
        .map(|text| format!(" (\"{text}\")"))
        .unwrap_or_default();
    let files = done.join(", ");
    let body = if undone {
        let verb = if done.len() == 1 { "has" } else { "have" };
        format!(
            "The person undid their revert of an earlier turn{quoted}: {files} {verb} that turn's changes again."
        )
    } else {
        let (verb, was) = if done.len() == 1 {
            ("is", "it was")
        } else {
            ("are", "they were")
        };
        let mut text = format!(
            "The person reverted your changes from an earlier turn{quoted}: {files} {verb} back as {was} before it."
        );
        if !skipped.is_empty() {
            text.push_str(&format!(
                " Skipped (changed since): {}.",
                skipped.join(", ")
            ));
        }
        text
    };
    format!(
        "<turn-reverted checkpoint=\"{}\">{body}</turn-reverted>",
        checkpoint_id(seq)
    )
}

/// A top-level run's checkpoint while it runs.
pub struct Turn {
    start: Start,
    late: Arc<AtomicBool>,
    started_at: i64,
}

#[derive(Default)]
struct State {
    /// Turn numbers and unsettled turns were read.
    ready: bool,
    /// The newest turn number used.
    last: u64,
    /// Revert notes still queued for the next run, by turn: an undo before
    /// that run takes the note back instead of adding another.
    notes: HashMap<u64, Vec<String>>,
}

/// One chat's checkpoints in its folder.
pub struct Checkpoints {
    gate: Arc<Gate>,
    cwd: PathBuf,
    state: tokio::sync::Mutex<State>,
    #[cfg(test)]
    start_delay: Duration,
}

impl Checkpoints {
    /// Checkpoints of the chat that works in `cwd`, waited for through `gate`
    /// (the one in the session's tool chain).
    pub fn new(gate: Arc<Gate>, cwd: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            gate,
            cwd,
            state: tokio::sync::Mutex::new(State::default()),
            #[cfg(test)]
            start_delay: Duration::ZERO,
        })
    }

    /// For tests: a start snapshot that begins `delay` late.
    #[cfg(test)]
    pub(crate) fn with_start_delay(gate: Arc<Gate>, cwd: PathBuf, delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            gate,
            cwd,
            state: tokio::sync::Mutex::new(State::default()),
            start_delay: delay,
        })
    }

    /// Read the turn numbers and close a turn the last engine left open.
    pub async fn recover(&self, harness: &AgentHarness) {
        let mut state = self.state.lock().await;
        self.ready(&mut state, harness).await;
    }

    async fn ready(&self, state: &mut State, harness: &AgentHarness) {
        if state.ready {
            return;
        }
        state.ready = true;
        let chat = chat_id(harness);
        let saved = saved(harness).await;
        let turns = if in_repository(&self.cwd) {
            let (cwd, chat) = (self.cwd.clone(), chat.clone());
            blocking(move || checkpoint::turns(&cwd, &chat))
                .await
                .unwrap_or_else(|error| {
                    log(format!("could not list checkpoints: {error}"));
                    Vec::new()
                })
        } else {
            Vec::new()
        };
        state.last = turns
            .iter()
            .copied()
            .chain(saved.iter().map(|checkpoint| checkpoint.seq))
            .max()
            .unwrap_or(0);
        // A turn with refs and no entry never settled (plan 3.6).
        let open: Vec<u64> = turns
            .into_iter()
            .filter(|seq| !saved.iter().any(|checkpoint| checkpoint.seq == *seq))
            .collect();
        if open.is_empty() {
            return;
        }
        let prompt = prompt_after(harness, last_on_branch(harness).await).await;
        for seq in open {
            let (cwd, chat) = (self.cwd.clone(), chat.clone());
            let closed = blocking(move || {
                let Some(start) = checkpoint::find(&cwd, &chat, seq, Kind::Start)? else {
                    return Ok(None);
                };
                let end = checkpoint::find(&cwd, &chat, seq, Kind::End)?;
                let (end, files) = match end {
                    Some(end) => {
                        let files = checkpoint::changes_between(&cwd, &start, &end)?
                            .into_iter()
                            .map(wire_file)
                            .collect();
                        (end, files)
                    }
                    None => close_turn(&cwd, &chat, seq, &start)?,
                };
                Ok(Some((start, end, files)))
            })
            .await;
            let (start, end, files) = match closed {
                Ok(Some(closed)) => closed,
                Ok(None) => continue,
                Err(error) => {
                    log(format!("could not close turn {seq}: {error}"));
                    continue;
                }
            };
            let checkpoint = Checkpoint {
                id: checkpoint_id(seq),
                seq,
                prompt_entry_id: prompt.as_ref().map(|prompt| prompt.id.clone()),
                run_id: String::new(),
                started_at: prompt.as_ref().map_or_else(now_ms, |prompt| prompt.at),
                start,
                end: Some(end),
                files,
                complete: false,
                reverted: None,
            };
            save(harness, &checkpoint).await;
        }
    }

    async fn next_seq(&self, harness: &AgentHarness) -> u64 {
        let mut state = self.state.lock().await;
        self.ready(&mut state, harness).await;
        state.last += 1;
        state.last
    }

    /// A top-level run starts: snapshot the folder off the run's task and
    /// make file-changing calls wait for it. `None` outside a repository.
    pub fn begin(self: &Arc<Self>, harness: &AgentHarness) -> Option<Turn> {
        if !in_repository(&self.cwd) {
            return None;
        }
        let this = Arc::clone(self);
        let harness = harness.clone();
        let task = tokio::spawn(async move {
            #[cfg(test)]
            tokio::time::sleep(this.start_delay).await;
            let seq = this.next_seq(&harness).await;
            let (cwd, chat) = (this.cwd.clone(), chat_id(&harness));
            match blocking(move || checkpoint::snapshot(&cwd, &chat, seq, Kind::Start)).await {
                Ok(commit) => Some((seq, commit)),
                Err(error) => {
                    log(format!("no checkpoint for this turn: {error}"));
                    None
                }
            }
        });
        let start: Start = async move { task.await.ok().flatten() }.boxed().shared();
        let late = self.gate.open(start.clone());
        Some(Turn {
            start,
            late,
            started_at: now_ms(),
        })
    }

    /// The run settled: snapshot the folder again, save the checkpoint as a
    /// session entry and return it for `agent_settled`. `since` is where the
    /// run started on the branch (its prompt comes after it).
    pub async fn finish(
        &self,
        turn: Turn,
        harness: &AgentHarness,
        run_id: Option<&str>,
        since: (Option<String>, u64),
    ) -> Option<Checkpoint> {
        self.gate.close();
        let (seq, start) = turn.start.await?;
        let complete = !turn.late.load(Ordering::Acquire);
        let prompt = prompt_after(harness, since).await;
        let (cwd, chat, from) = (self.cwd.clone(), chat_id(harness), start.clone());
        let (end, files) = match blocking(move || close_turn(&cwd, &chat, seq, &from)).await {
            Ok(closed) => closed,
            Err(error) => {
                log(format!("no checkpoint for this turn: {error}"));
                return None;
            }
        };
        let checkpoint = Checkpoint {
            id: checkpoint_id(seq),
            seq,
            prompt_entry_id: prompt.map(|prompt| prompt.id),
            run_id: run_id.unwrap_or_default().to_string(),
            started_at: turn.started_at,
            start,
            end: Some(end),
            files,
            complete,
            reverted: None,
        };
        save(harness, &checkpoint).await;
        Some(checkpoint)
    }

    /// `list_checkpoints`: the chat's checkpoints whose snapshots are kept.
    pub async fn list(&self, harness: &AgentHarness) -> Result<Value, String> {
        self.recover(harness).await;
        let mut checkpoints = saved(harness).await;
        if in_repository(&self.cwd) {
            let (cwd, chat) = (self.cwd.clone(), chat_id(harness));
            if let Ok(kept) = blocking(move || checkpoint::turns(&cwd, &chat)).await {
                checkpoints.retain(|checkpoint| kept.contains(&checkpoint.seq));
            }
        }
        serde_json::to_value(ListCheckpointsResult { checkpoints }).map_err(|e| e.to_string())
    }

    /// `revert_turn` (FR-11 – FR-13): refused while a run is active.
    pub async fn revert(
        &self,
        harness: &AgentHarness,
        msg: &Value,
        running: bool,
    ) -> Result<Value, String> {
        if running {
            return Err(REVERT_BUSY.into());
        }
        let request: RevertTurnRequest = serde_json::from_value(msg.clone())
            .map_err(|error| format!("invalid revert_turn: {error}"))?;
        let (seq, undo) = parse_checkpoint_id(&request.checkpoint)
            .ok_or_else(|| format!("not a checkpoint id: {}", request.checkpoint))?;
        let mut state = self.state.lock().await;
        self.ready(&mut state, harness).await;
        let mut checkpoint = saved(harness)
            .await
            .into_iter()
            .find(|checkpoint| checkpoint.seq == seq)
            .ok_or_else(|| format!("no checkpoint {}", checkpoint_id(seq)))?;
        let (cwd, chat, force) = (self.cwd.clone(), chat_id(harness), request.force);
        let lane = harness.lane("main");
        let prompt = match &checkpoint.prompt_entry_id {
            Some(id) => prompt_text(harness, id).await,
            None => None,
        };
        let message = |text: String| {
            evo_agent::AgentMessage::from(evo_ai::types::UserMessage::new(text, now_ms()))
        };
        if !undo {
            let paths = request.paths.clone();
            let restored = blocking(move || {
                let start = checkpoint::find(&cwd, &chat, seq, Kind::Start)?.ok_or(GONE)?;
                let end = checkpoint::find(&cwd, &chat, seq, Kind::End)?.ok_or(GONE)?;
                checkpoint::snapshot(&cwd, &chat, seq, Kind::Undo)?;
                checkpoint::restore(&cwd, &start, &end, paths.as_deref(), force)
            })
            .await?;
            let result = RevertTurnResult {
                reverted: restored.reverted.clone(),
                skipped: restored
                    .skipped
                    .iter()
                    .map(|path| SkippedFile {
                        path: path.clone(),
                        reason: SKIPPED_CHANGED_SINCE.into(),
                    })
                    .collect(),
                undo: (!restored.reverted.is_empty()).then(|| checkpoint_undo_id(seq)),
            };
            if !restored.reverted.is_empty() {
                // A second revert (of files skipped the first time) adds to it.
                let mut paths = checkpoint
                    .reverted
                    .take()
                    .map(|reverted| reverted.paths)
                    .unwrap_or_default();
                for path in &restored.reverted {
                    if !paths.contains(path) {
                        paths.push(path.clone());
                    }
                }
                checkpoint.reverted = Some(CheckpointRevert {
                    at: now_ms(),
                    undo: checkpoint_undo_id(seq),
                    paths,
                });
                save(harness, &checkpoint).await;
                let text = note(
                    seq,
                    prompt.as_deref(),
                    false,
                    &restored.reverted,
                    &restored.skipped,
                );
                match lane.next_run(message(text)).await {
                    Ok(queued) => state.notes.entry(seq).or_default().push(queued.entry_id),
                    Err(error) => log(format!("could not queue the revert note: {error}")),
                }
            }
            return serde_json::to_value(result).map_err(|e| e.to_string());
        }

        let reverted = checkpoint
            .reverted
            .clone()
            .ok_or_else(|| format!("{} has no revert to undo", checkpoint_id(seq)))?;
        let paths = request.paths.clone().unwrap_or(reverted.paths.clone());
        let restored = blocking(move || {
            let source = checkpoint::find(&cwd, &chat, seq, Kind::Undo)?.ok_or(GONE)?;
            let start = checkpoint::find(&cwd, &chat, seq, Kind::Start)?.ok_or(GONE)?;
            checkpoint::restore(&cwd, &source, &start, Some(&paths), force)
        })
        .await?;
        let left: Vec<String> = reverted
            .paths
            .iter()
            .filter(|path| !restored.reverted.contains(path))
            .cloned()
            .collect();
        checkpoint.reverted = (!left.is_empty()).then(|| CheckpointRevert {
            paths: left.clone(),
            ..reverted
        });
        save(harness, &checkpoint).await;
        // The agent never saw the revert: take its note back. Otherwise tell
        // it the files are back.
        let mut taken_back = false;
        if left.is_empty() {
            if let Some(queued) = state.notes.remove(&seq) {
                taken_back = true;
                for entry in queued {
                    taken_back &= matches!(
                        lane.cancel_queued(&entry)
                            .await
                            .map(|result| result.outcome),
                        Ok(CancelQueuedOutcome::Cancelled)
                    );
                }
            }
        }
        if !taken_back && !restored.reverted.is_empty() {
            let text = note(seq, prompt.as_deref(), true, &restored.reverted, &[]);
            if let Err(error) = lane.next_run(message(text)).await {
                log(format!("could not queue the undo note: {error}"));
            }
        }
        serde_json::to_value(RevertTurnResult {
            reverted: restored.reverted,
            skipped: restored
                .skipped
                .into_iter()
                .map(|path| SkippedFile {
                    path,
                    reason: SKIPPED_CHANGED_SINCE.into(),
                })
                .collect(),
            undo: None,
        })
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests;
