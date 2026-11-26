//! How many mates run turns at once (docs/features/evo-mates/spec.md, NFR-6;
//! plan 3.5 "Running limit"): when mail would start the engine of a mate's
//! chat or seat while `mates.maxRunning` of them already work, the chat
//! waits in order and its mate shows **Queued**; each time a mate's work
//! settles, the next waiting chat starts. Harness chats are not counted, and
//! the person's own messages are never held.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::db::Db;

/// The app setting for mates running at once.
pub const MAX_RUNNING_KEY: &str = "mates.maxRunning";
const MAX_RUNNING: usize = 4;

/// A chat waiting to start, with how it was to be hosted.
#[derive(Debug, Clone, PartialEq)]
pub struct Waiting {
    pub chat: String,
    pub cwd: Option<String>,
    pub restore: bool,
}

fn queue() -> &'static Mutex<VecDeque<Waiting>> {
    static QUEUE: OnceLock<Mutex<VecDeque<Waiting>>> = OnceLock::new();
    QUEUE.get_or_init(Default::default)
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether one more mate may start, with `running` working and `limit`.
pub fn may_start(running: usize, limit: usize) -> bool {
    running < limit.max(1)
}

/// Add `waiting` to the queue once (a chat waits only once, in its first place).
pub fn enqueue(queue: &mut VecDeque<Waiting>, waiting: Waiting) -> bool {
    if queue.iter().any(|w| w.chat == waiting.chat) {
        return false;
    }
    queue.push_back(waiting);
    true
}

fn limit(app: &AppHandle) -> usize {
    app.state::<Db>()
        .with(|conn| evo_store::settings::get(conn, MAX_RUNNING_KEY))
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .filter(|v| (1..=16).contains(v))
        .unwrap_or(MAX_RUNNING)
}

/// Whether `chat` is a mate's chat or seat (the chats the limit counts).
fn counted(app: &AppHandle, chat: &str) -> bool {
    app.state::<Db>()
        .with(|conn| evo_store::chats::get_chat(conn, chat))
        .ok()
        .flatten()
        .is_some_and(|row| row.kind == "mate" || row.kind == "seat")
}

/// The mates' chats and seats that work now.
fn running(app: &AppHandle) -> usize {
    let host = crate::supervisor::AppHost(app.clone());
    let supervisor = app.state::<crate::supervisor::EngineSupervisor>();
    let chats: Vec<String> = app
        .state::<Db>()
        .with(|conn| {
            let mut statement = conn.prepare("SELECT id FROM chats WHERE kind IN ('mate', 'seat')")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            Ok(rows.collect::<Result<Vec<String>, _>>()?)
        })
        .unwrap_or_default();
    chats.iter().filter(|chat| supervisor.working(&host, chat)).count()
}

fn tell(app: &AppHandle) {
    let chats: Vec<String> = locked(queue()).iter().map(|w| w.chat.clone()).collect();
    let _ = app.emit("mates://queued", json!({ "chats": chats }));
}

/// Mail wants `chat` hosted: true to go ahead now, false when it was queued.
pub fn admit(app: &AppHandle, chat: &str, cwd: Option<String>, restore: bool) -> bool {
    if !counted(app, chat) || may_start(running(app), limit(app)) {
        return true;
    }
    let added = enqueue(&mut locked(queue()), Waiting { chat: chat.to_string(), cwd, restore });
    if added {
        app.state::<Db>().log(evo_store::logs::NewLog::new("info", "mates", "queued: too many mates working").chat(Some(chat)));
        tell(app);
    }
    false
}

/// Work changed: start waiting chats while there is room.
pub fn release(app: &AppHandle) {
    if locked(queue()).is_empty() {
        return;
    }
    let mut started = Vec::new();
    loop {
        if !may_start(running(app) + started.len(), limit(app)) {
            break;
        }
        let Some(next) = locked(queue()).pop_front() else { break };
        started.push(next);
    }
    if started.is_empty() {
        return;
    }
    tell(app);
    for waiting in started {
        crate::supervisor::host_for_mail_now(app, waiting.chat, waiting.cwd, waiting.restore);
    }
}

/// The chats waiting now (a window that opens asks).
#[tauri::command]
pub fn mates_queued() -> Vec<String> {
    locked(queue()).iter().map(|w| w.chat.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_holds_and_a_chat_waits_once() {
        assert!(may_start(3, 4));
        assert!(!may_start(4, 4));
        assert!(may_start(0, 0), "a limit of 0 reads as 1");
        let mut queue = VecDeque::new();
        let wait = |chat: &str| Waiting { chat: chat.into(), cwd: None, restore: false };
        assert!(enqueue(&mut queue, wait("a")));
        assert!(enqueue(&mut queue, wait("b")));
        assert!(!enqueue(&mut queue, wait("a")), "already waiting");
        assert_eq!(queue.iter().map(|w| w.chat.as_str()).collect::<Vec<_>>(), ["a", "b"], "in order");
    }
}
