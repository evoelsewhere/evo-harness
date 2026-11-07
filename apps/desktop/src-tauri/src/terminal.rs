//! Side panel terminal: one pseudo-terminal per tab (ConPTY on Windows, a
//! Unix pty elsewhere) running the user's shell in the workspace folder.
//!
//! The web UI renders with xterm.js. Output goes to the window over a Tauri
//! [`Channel`] as UTF-8 text (a chunk split inside a multi-byte character waits
//! for the rest); keystrokes come back through `terminal_write`, queued to a
//! writer thread so a slow child never blocks the main thread and keys stay in
//! order. Each session keeps the tail of its output so a reloaded window can
//! re-attach (`terminal_list` + `terminal_attach`) instead of losing its
//! shells. A session belongs to the window that started it: closing the window
//! kills its shells.
//!
//! The agent's terminal tools (`agent_tools/terminal.rs`) share this state: every
//! session keeps a plain-text [`log::SharedLog`] fed by its reader thread, the
//! chat it belongs to and the chat that started it (`owner`). A session a tool
//! starts has no webview channel yet: its output waits in the backlog until the
//! webview adopts the tab (`terminal://created`, then `terminal_attach`). A shell
//! that exited stays listed for the tools until its tab is closed (`terminal_close`).
//!
//! Spec: docs/features/agent-terminal (FR-3 shell ids, FR-4 list, FR-11 owner,
//! FR-16 shell integration; plan 3.2-3.5) and docs/features/agent-browser
//! (ADR 0013, plan 3.2). Shell discovery is in `shells`, the output log in `log`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

pub(crate) mod log;
pub(crate) mod shells;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State, Window};

use crate::shell_integration;
use log::SharedLog;
use shells::ShellInfo;

/// Output kept per session for re-attaching after a reload.
const BACKLOG_MAX: usize = 512 * 1024;
/// One read from the pty.
const READ_CHUNK: usize = 16 * 1024;
/// Shells that exited and whose tab is still open are kept for the tools, at most this many.
const EXITED_KEPT: usize = 30;
/// The size of a session no webview has shown yet.
pub const DEFAULT_COLS: u16 = 120;
pub const DEFAULT_ROWS: u16 = 30;
/// App setting: start new terminals with shell integration (default on).
const SHELL_INTEGRATION_KEY: &str = "terminal.shellIntegration";

/// What a session sends its window.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalEvent {
    Data {
        data: String,
    },
    /// `killed`: the shell was stopped on purpose (a close or a tool), not exited by itself.
    Exit {
        code: Option<u32>,
        killed: bool,
    },
}

/// A running session, as the UI lists it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: u32,
    /// The tab's key: what the webview and the agent tools call it.
    pub key: String,
    pub shell_id: String,
    pub shell: String,
    pub cwd: String,
    pub pid: Option<u32>,
    /// Who started the tab (a chat id), if not the person.
    pub owner: Option<String>,
    /// The chat whose tabs show it; the agent of that chat sees it.
    pub chat: Option<String>,
    /// Whether the shell reports prompt and command markers (OSC 133).
    pub integration: bool,
}

struct Output {
    channel: Option<Channel<TerminalEvent>>,
    backlog: String,
}

pub struct Session {
    window: String,
    info: SessionInfo,
    chat: Mutex<Option<String>>,
    input: Mutex<Option<Sender<Vec<u8>>>>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    output: Arc<Mutex<Output>>,
    log: Arc<SharedLog>,
    killed: AtomicBool,
}

#[derive(Default)]
pub struct TerminalState {
    sessions: Mutex<HashMap<u32, Arc<Session>>>,
    next_id: AtomicU32,
    /// Where the shell integration scripts were installed, if they were.
    integration_dir: Mutex<Option<PathBuf>>,
    /// The shell id the person chose in the panel, for tools that name no shell.
    default_shell: Mutex<Option<String>>,
}

impl TerminalState {
    /// Remember the folder [`shell_integration::install`] wrote the scripts to.
    pub fn set_integration_dir(&self, dir: PathBuf) {
        *self.integration_dir.lock().expect("integration dir lock") = Some(dir);
    }

    /// The shell id the person made the default in the panel.
    pub fn default_shell(&self) -> Option<String> {
        self.default_shell
            .lock()
            .expect("default shell lock")
            .clone()
    }

    fn get(&self, id: u32) -> Result<Arc<Session>, String> {
        self.sessions
            .lock()
            .expect("terminal sessions lock")
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("terminal {id} is not running"))
    }

    /// The newest session with tab key `key` (a restarted tab keeps its key).
    pub fn find_by_key(&self, key: &str) -> Option<Arc<Session>> {
        self.sessions
            .lock()
            .expect("terminal sessions lock")
            .values()
            .filter(|session| session.info.key == key)
            .max_by_key(|session| session.info.id)
            .cloned()
    }

    /// The sessions whose tabs belong to `chat` (running or exited), oldest first.
    pub fn in_chat(&self, chat: &str) -> Vec<Arc<Session>> {
        let mut sessions: Vec<Arc<Session>> = self
            .sessions
            .lock()
            .expect("terminal sessions lock")
            .values()
            .filter(|session| session.chat().as_deref() == Some(chat))
            .cloned()
            .collect();
        sessions.sort_by_key(|session| session.info.id);
        sessions
    }

    /// Forget a session (its tab closed), ending its shell if it still runs.
    pub fn close(&self, id: u32) {
        let session = self
            .sessions
            .lock()
            .expect("terminal sessions lock")
            .remove(&id);
        if let Some(session) = session {
            if session.running() {
                session.kill();
            }
        }
    }

    /// Add a session; the oldest exited ones beyond [`EXITED_KEPT`] are dropped.
    fn insert(&self, session: Arc<Session>) {
        let mut sessions = self.sessions.lock().expect("terminal sessions lock");
        sessions.insert(session.info.id, session);
        let mut exited: Vec<u32> = sessions
            .values()
            .filter(|s| !s.running())
            .map(|s| s.info.id)
            .collect();
        if exited.len() > EXITED_KEPT {
            exited.sort_unstable();
            for id in &exited[..exited.len() - EXITED_KEPT] {
                sessions.remove(id);
            }
        }
    }

    /// Kill every shell started by `window` (it closed) and forget its sessions.
    pub fn kill_window(&self, window: &str) {
        let sessions: Vec<Arc<Session>> = {
            let mut all = self.sessions.lock().expect("terminal sessions lock");
            let ids: Vec<u32> = all
                .values()
                .filter(|session| session.window == window)
                .map(|session| session.info.id)
                .collect();
            ids.iter().filter_map(|id| all.remove(id)).collect()
        };
        for session in sessions {
            if session.running() {
                session.kill();
            }
        }
    }
}

impl Session {
    /// End the shell and everything it started.
    pub fn kill(&self) {
        self.killed.store(true, Ordering::SeqCst);
        if let Some(pid) = self.info.pid {
            kill_tree(pid);
        }
        let _ = self.killer.lock().expect("terminal killer lock").kill();
    }

    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            chat: self.chat(),
            ..self.info.clone()
        }
    }

    pub fn id(&self) -> u32 {
        self.info.id
    }

    pub fn key(&self) -> &str {
        &self.info.key
    }

    pub fn shell(&self) -> &str {
        &self.info.shell
    }

    pub fn cwd(&self) -> &str {
        &self.info.cwd
    }

    pub fn owner(&self) -> Option<&str> {
        self.info.owner.as_deref()
    }

    pub fn integration(&self) -> bool {
        self.info.integration
    }

    pub fn chat(&self) -> Option<String> {
        self.chat.lock().expect("session chat lock").clone()
    }

    pub fn log(&self) -> &Arc<SharedLog> {
        &self.log
    }

    /// The shell has not exited.
    pub fn running(&self) -> bool {
        !self.log.with(|log| log.exited())
    }

    /// The exit code once the shell exited.
    pub fn exit_code(&self) -> Option<i64> {
        self.log.with(|log| log.exit_code())
    }

    /// Type bytes into the shell, in order with everything else the tab sends.
    pub fn write(&self, data: Vec<u8>) -> Result<(), String> {
        self.input
            .lock()
            .expect("session input lock")
            .as_ref()
            .ok_or_else(|| format!("terminal {} has exited", self.info.id))?
            .send(data)
            .map_err(|_| format!("terminal {} has exited", self.info.id))
    }
}

/// Kill the process tree rooted at `pid`: `taskkill /T /F` on Windows, the
/// process group on Unix (the pty makes the shell a session leader).
fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|mut child| std::thread::spawn(move || child.wait()));
    }
    #[cfg(unix)]
    {
        if let Ok(pid) = i32::try_from(pid) {
            if pid > 1 {
                // SAFETY: plain signal to the shell's own process group.
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
            }
        }
    }
}

/// Decode what is valid UTF-8 at the front of `pending`, leaving an incomplete
/// trailing character for the next read. Invalid bytes become U+FFFD.
fn take_utf8(pending: &mut Vec<u8>) -> String {
    let mut text = String::new();
    let mut start = 0;
    loop {
        match std::str::from_utf8(&pending[start..]) {
            Ok(valid) => {
                text.push_str(valid);
                start = pending.len();
                break;
            }
            Err(error) => {
                let valid = start + error.valid_up_to();
                text.push_str(std::str::from_utf8(&pending[start..valid]).expect("valid prefix"));
                match error.error_len() {
                    Some(bad) => {
                        text.push(char::REPLACEMENT_CHARACTER);
                        start = valid + bad;
                    }
                    // Cut inside a character: keep its bytes for the next read.
                    None => {
                        start = valid;
                        break;
                    }
                }
            }
        }
    }
    pending.drain(..start);
    text
}

/// Append to the backlog, dropping whole lines from the front past the limit.
fn push_backlog(backlog: &mut String, data: &str) {
    backlog.push_str(data);
    if backlog.len() <= BACKLOG_MAX {
        return;
    }
    let mut cut = backlog.len() - BACKLOG_MAX;
    // Start the kept part on a line, so replay does not begin inside an escape sequence.
    if let Some(newline) = backlog[cut..].find('\n') {
        cut += newline + 1;
    }
    while !backlog.is_char_boundary(cut) {
        cut += 1;
    }
    backlog.drain(..cut);
}

fn home_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .home_dir()
        .ok()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

#[tauri::command]
pub fn terminal_shells() -> Vec<ShellInfo> {
    shells::discover_shells()
}

/// The slow shell sources (WSL distributions, Windows Terminal profiles, Visual
/// Studio developer shells); empty off Windows.
#[tauri::command]
pub async fn terminal_shells_extra() -> Vec<ShellInfo> {
    tauri::async_runtime::spawn_blocking(shells::discover_extra)
        .await
        .unwrap_or_default()
}

/// The shell for `id`: the fast list first, then (for `wsl:`, `wt:`, `vs:`
/// ids) the slow sources; no or an unknown id means the default shell.
fn resolve_shell(id: Option<&str>) -> Result<ShellInfo, String> {
    let list = shells::discover_shells();
    if let Some(id) = id {
        if let Some(found) = list.iter().find(|s| s.id == id) {
            return Ok(found.clone());
        }
        #[cfg(windows)]
        if let Some(distro) = id.strip_prefix("wsl:") {
            // No listing needed: `wsl -d <distribution>` starts it.
            if let Some(wsl) = shells::wsl_exe() {
                return Ok(shells::wsl_shell(distro, &wsl));
            }
        }
        if id.starts_with("wsl:") || id.starts_with("wt:") || id.starts_with("vs:") {
            if let Some(found) = shells::discover_extra().into_iter().find(|s| s.id == id) {
                return Ok(found);
            }
        }
    }
    list.iter()
        .find(|s| s.is_default)
        .or_else(|| list.first())
        .cloned()
        .ok_or_else(|| "no shell found on this computer".to_string())
}

/// Whether new terminals start with shell integration (the app setting, default on).
pub fn integration_enabled(app: &AppHandle) -> bool {
    app.state::<crate::db::Db>()
        .with(|conn| evo_store::settings::all(conn))
        .ok()
        .and_then(|settings| settings.get(SHELL_INTEGRATION_KEY).cloned())
        .is_none_or(|value| value != serde_json::Value::Bool(false))
}

/// How a session starts.
pub struct SpawnOptions {
    /// An id from `terminal_shells` (else the default shell).
    pub shell: Option<String>,
    /// The folder to start in (else the home folder).
    pub cwd: Option<String>,
    pub cols: u16,
    pub rows: u16,
    /// The chat that started the tab (an agent's), if not the person.
    pub owner: Option<String>,
    /// The chat whose tabs show it.
    pub chat: Option<String>,
    pub integration: bool,
    /// The tab key the webview already uses for it; generated when absent or taken.
    pub key: Option<String>,
    /// Where output streams; without one the output waits in the backlog until `terminal_attach`.
    pub channel: Option<Channel<TerminalEvent>>,
}

/// Start a shell for `window` (a window label) and register its session. The one
/// body behind the `terminal_spawn` command (the webview) and the agent's
/// terminal tools.
pub async fn spawn_session(
    app: &AppHandle,
    window: &str,
    options: SpawnOptions,
) -> Result<Arc<Session>, String> {
    let state = app.state::<TerminalState>();
    let SpawnOptions {
        shell,
        cwd,
        cols,
        rows,
        owner,
        chat,
        integration,
        key,
        channel,
    } = options;
    // wsl:/wt:/vs: ids may query slow sources; keep that off the async executor.
    let chosen = tauri::async_runtime::spawn_blocking(move || resolve_shell(shell.as_deref()))
        .await
        .map_err(|e| e.to_string())??;
    let cwd = cwd
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .filter(|dir| dir.is_dir())
        .unwrap_or_else(|| home_dir(app));

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: rows.max(2),
            cols: cols.max(2),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("could not open a terminal: {e}"))?;
    let integrated = if integration {
        let dir = state
            .integration_dir
            .lock()
            .expect("integration dir lock")
            .clone();
        dir.and_then(|dir| {
            let orig_zdotdir = std::env::var("ZDOTDIR")
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| home_dir(app).to_string_lossy().into_owned());
            shell_integration::plan(&chosen, &dir, &orig_zdotdir)
        })
    } else {
        None
    };
    let mut command = CommandBuilder::new(&chosen.program);
    match &integrated {
        Some(plan) => {
            command.args(&plan.args);
            for (key, value) in &plan.env {
                command.env(key, value);
            }
        }
        None => {
            command.args(&chosen.args);
        }
    }
    command.cwd(&cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "evo");
    command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|e| format!("could not start {}: {e}", chosen.name))?;
    // The child holds the other end now; ours would keep the pty open after it exits.
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let killer = child.clone_killer();

    let id = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    // The webview's own key when it is free (a restarted tab reuses its key), else a new one.
    let key = key
        .filter(|wanted| !wanted.is_empty())
        .filter(|wanted| {
            state
                .find_by_key(wanted)
                .is_none_or(|session| !session.running())
        })
        .unwrap_or_else(|| format!("t{id}"));
    let chat = chat.filter(|c| !c.is_empty());
    let info = SessionInfo {
        id,
        key,
        shell_id: chosen.id.clone(),
        shell: chosen.name.clone(),
        cwd: cwd.to_string_lossy().into_owned(),
        pid: child.process_id(),
        owner: owner.filter(|o| !o.is_empty()),
        chat: chat.clone(),
        integration: integrated.is_some(),
    };
    let output = Arc::new(Mutex::new(Output {
        channel,
        backlog: String::new(),
    }));
    let log = Arc::new(SharedLog::new());

    // Keys and pastes, in order, off the main thread.
    let (input, keys) = mpsc::channel::<Vec<u8>>();
    // The reader answers the pty's cursor-position query while no webview listens.
    let queries = input.clone();
    std::thread::Builder::new()
        .name(format!("terminal-{id}-in"))
        .spawn(move || {
            for bytes in keys {
                if writer
                    .write_all(&bytes)
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        })
        .map_err(|e| e.to_string())?;

    let reader_output = output.clone();
    let reader_log = log.clone();
    let reading = std::thread::Builder::new()
        .name(format!("terminal-{id}-out"))
        .spawn(move || {
            let mut buffer = vec![0u8; READ_CHUNK];
            let mut pending = Vec::new();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        pending.extend_from_slice(&buffer[..n]);
                        let data = take_utf8(&mut pending);
                        if data.is_empty() {
                            continue;
                        }
                        reader_log.feed(&data);
                        let mut out = reader_output.lock().expect("terminal output lock");
                        push_backlog(&mut out.backlog, &data);
                        if let Some(channel) = &out.channel {
                            // A reloaded window dropped its end; keep the backlog for re-attaching.
                            if channel.send(TerminalEvent::Data { data }).is_err() {
                                out.channel = None;
                            }
                        } else if data.contains("\x1b[6n") {
                            // ConPTY waits for an answer to its cursor query; xterm.js gives it
                            // once a window shows the tab (it does not answer a replay).
                            let _ = queries.send(b"\x1b[1;1R".to_vec());
                        }
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;

    let session = Arc::new(Session {
        window: window.to_string(),
        info,
        chat: Mutex::new(chat),
        input: Mutex::new(Some(input)),
        master: Mutex::new(Some(pair.master)),
        killer: Mutex::new(killer),
        output: output.clone(),
        log: log.clone(),
        killed: AtomicBool::new(false),
    });
    state.insert(session.clone());

    // Wait for the shell to exit, then close the pty (on Windows the reader only
    // ends once the pseudo console closes), drain the output and report the exit.
    // The session stays listed (exited) until its tab is closed.
    let waiting = session.clone();
    std::thread::Builder::new()
        .name(format!("terminal-{id}-wait"))
        .spawn(move || {
            let status = child.wait().ok();
            drop(waiting.master.lock().expect("terminal master lock").take());
            drop(waiting.input.lock().expect("session input lock").take());
            let _ = reading.join();
            let code = status.map(|s| s.exit_code());
            // Under the output lock, so `terminal_attach` sees the exit once, either way round.
            let out = output.lock().expect("terminal output lock");
            log.close(code.map(i64::from));
            if let Some(channel) = &out.channel {
                let _ = channel.send(TerminalEvent::Exit {
                    code,
                    killed: waiting.killed.load(Ordering::SeqCst),
                });
            }
        })
        .map_err(|e| e.to_string())?;

    Ok(session)
}

/// Start a shell (`shell`: an id from `terminal_shells`, else the default) in
/// `cwd` (else the home folder). Output streams to `on_event`. `chat` is the
/// chat whose tabs show it (the agent of that chat sees it); `key` the tab key
/// the webview uses for it.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn terminal_spawn(
    app: AppHandle,
    window: Window,
    shell: Option<String>,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    owner: Option<String>,
    chat: Option<String>,
    key: Option<String>,
    integration: Option<bool>,
    on_event: Channel<TerminalEvent>,
) -> Result<SessionInfo, String> {
    let session = spawn_session(
        &app,
        window.label(),
        SpawnOptions {
            shell,
            cwd,
            cols,
            rows,
            owner,
            chat,
            integration: integration.unwrap_or(true),
            key,
            channel: Some(on_event),
        },
    )
    .await?;
    Ok(session.info())
}

/// Keys or pasted text for a session.
#[tauri::command]
pub fn terminal_write(
    state: State<'_, TerminalState>,
    id: u32,
    data: String,
) -> Result<(), String> {
    state.get(id)?.write(data.into_bytes())
}

#[tauri::command]
pub fn terminal_resize(
    state: State<'_, TerminalState>,
    id: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let session = state.get(id)?;
    let master = session.master.lock().expect("terminal master lock");
    let Some(master) = master.as_ref() else {
        return Err(format!("terminal {id} has exited"));
    };
    master
        .resize(PtySize {
            rows: rows.max(2),
            cols: cols.max(2),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())
}

/// Stop a session's shell; its exit is reported on its channel.
#[tauri::command]
pub fn terminal_kill(state: State<'_, TerminalState>, id: u32) {
    if let Ok(session) = state.get(id) {
        session.kill();
    }
}

/// The tab of a session closed: stop its shell if it still runs and forget it.
#[tauri::command]
pub fn terminal_close(state: State<'_, TerminalState>, id: u32) {
    state.close(id);
}

/// The chat whose tabs show a session changed (a tab found after a reload with
/// no known chat joined the first chat shown).
#[tauri::command]
pub fn terminal_set_chat(state: State<'_, TerminalState>, id: u32, chat: Option<String>) {
    if let Ok(session) = state.get(id) {
        *session.chat.lock().expect("session chat lock") = chat.filter(|c| !c.is_empty());
    }
}

/// The shell the person made the default in the panel: tools that name no shell start it.
#[tauri::command]
pub fn terminal_set_default_shell(state: State<'_, TerminalState>, shell: Option<String>) {
    *state.default_shell.lock().expect("default shell lock") = shell.filter(|s| !s.is_empty());
}

/// The sessions this window started that are still running (after a reload).
#[tauri::command]
pub fn terminal_list(window: Window, state: State<'_, TerminalState>) -> Vec<SessionInfo> {
    let mut sessions: Vec<SessionInfo> = state
        .sessions
        .lock()
        .expect("terminal sessions lock")
        .values()
        .filter(|session| session.window == window.label() && session.running())
        .map(|session| session.info())
        .collect();
    sessions.sort_by_key(|session| session.id);
    sessions
}

/// Send a running session's output to a new channel; returns the output so far.
/// A shell that already exited reports its exit on the new channel at once.
#[tauri::command]
pub fn terminal_attach(
    state: State<'_, TerminalState>,
    id: u32,
    on_event: Channel<TerminalEvent>,
) -> Result<String, String> {
    let session = state.get(id)?;
    let mut out = session.output.lock().expect("terminal output lock");
    if session.log.with(|log| log.exited()) {
        let _ = on_event.send(TerminalEvent::Exit {
            code: session
                .exit_code()
                .and_then(|code| u32::try_from(code).ok()),
            killed: session.killed.load(Ordering::SeqCst),
        });
    }
    out.channel = Some(on_event);
    Ok(out.backlog.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_split_inside_a_character_waits_for_the_rest() {
        let bytes = "héllo ✓".as_bytes();
        let split = bytes.len() - 1;
        let mut pending = bytes[..split].to_vec();
        assert_eq!(take_utf8(&mut pending), "héllo ");
        assert_eq!(pending.len(), 2);
        pending.extend_from_slice(&bytes[split..]);
        assert_eq!(take_utf8(&mut pending), "✓");
        assert!(pending.is_empty());
    }

    #[test]
    fn invalid_utf8_becomes_replacement_characters() {
        let mut pending = vec![b'a', 0xff, b'b'];
        assert_eq!(take_utf8(&mut pending), "a\u{fffd}b");
        assert!(pending.is_empty());
    }

    #[test]
    fn backlog_keeps_whole_lines_within_the_limit() {
        let mut backlog = String::new();
        let line = format!("{}\n", "x".repeat(1023));
        for _ in 0..(BACKLOG_MAX / 1024 + 10) {
            push_backlog(&mut backlog, &line);
        }
        assert!(backlog.len() <= BACKLOG_MAX);
        assert!(backlog.starts_with('x'));
        assert!(backlog.ends_with('\n'));
    }

    #[test]
    fn finds_at_least_one_shell() {
        assert!(!shells::discover_shells().is_empty());
    }

    /// The same sequence `terminal_spawn` runs: a real shell echoes, exits, and
    /// the reader ends once the pty is dropped after the wait.
    #[test]
    fn shell_runs_echoes_and_exits() {
        let shell = shells::discover_shells()
            .into_iter()
            .next()
            .expect("a shell");
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("pty");
        let mut command = CommandBuilder::new(&shell.program);
        command.args(&shell.args);
        command.cwd(std::env::temp_dir());
        let mut child = pair.slave.spawn_command(command).expect("spawn");
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().expect("reader");
        let writer = Arc::new(Mutex::new(pair.master.take_writer().expect("writer")));
        // Never hang the suite: kill the shell if it is still running after 60s.
        let mut killer = child.clone_killer();
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watchdog_done = done.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(60));
            if !watchdog_done.load(Ordering::SeqCst) {
                let _ = killer.kill();
            }
        });
        let answer = writer.clone();
        let reading = std::thread::spawn(move || {
            let mut all = Vec::new();
            let mut pending = Vec::new();
            let mut buffer = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                pending.extend_from_slice(&buffer[..n]);
                let text = take_utf8(&mut pending);
                // ConPTY asks for the cursor position (DSR) and waits; xterm.js answers in the app.
                if text.contains("\x1b[6n") {
                    let mut w = answer.lock().unwrap();
                    let _ = w.write_all(b"\x1b[1;1R").and_then(|()| w.flush());
                }
                all.push(text);
            }
            all.concat()
        });
        let send = |bytes: &[u8]| {
            let mut w = writer.lock().unwrap();
            w.write_all(bytes).and_then(|()| w.flush()).expect("write");
        };
        std::thread::sleep(std::time::Duration::from_millis(1500));
        send(b"echo evo-terminal-ok\r");
        std::thread::sleep(std::time::Duration::from_millis(1500));
        send(b"exit\r");
        let status = child.wait().expect("wait");
        done.store(true, Ordering::SeqCst);
        drop(writer);
        drop(pair.master);
        let output = reading.join().expect("reader thread");
        assert!(output.contains("evo-terminal-ok"), "output: {output:?}");
        assert_eq!(status.exit_code(), 0);
    }
}
