//! Notices the shell sends about chats no focused window shows
//! (docs/features/shell-hosted-engines/spec.md, FR-9, FR-10; plan 3.5
//! "Open questions", 3.7 "Notification contents").
//!
//! A question that waits for the person and the end of a run (reply ready,
//! run failed, engine stopped, engine could not start or keeps stopping) are
//! announced once, by the shell, whatever number of windows are open
//! (NFR-1), and only when no focused window shows the chat. Each notice is:
//!
//! - a bell entry: `notify://entry` to one window (the main one first), or,
//!   with no window open, kept in the `notifications.inbox` app setting (at
//!   most [`INBOX_MAX`]) until a window takes them (`notifications_take`);
//! - an OS notification when no window is focused, unless the person muted
//!   it (Do not disturb or the agent muted: `agent_tools::user`'s flag).
//!   Clicking it opens the chat (`app://open-chat`).
//!
//! Notices carry titles only, never what a chat says; an incognito chat is
//! "A chat".

use std::sync::atomic::{AtomicU64, Ordering};

use evo_store::logs::NewLog;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Db;

/// The app setting that keeps bell entries recorded with no window open.
pub const INBOX_KEY: &str = "notifications.inbox";
/// Bell entries kept in the inbox, at most; the oldest go first.
pub const INBOX_MAX: usize = 100;
/// What an incognito chat is called in a notice.
pub const INCOGNITO_NAME: &str = "A chat";

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    ReplyReady,
    RunFailed,
    /// An approval waits (an engine's or a shell tool's).
    Approval,
    /// The agent's questions (`ask_user`) wait.
    Question,
    /// The engine ended by itself while the chat worked or was shown.
    EngineStopped,
    /// The shell could not start the engine for the chat's mail.
    CouldNotStart,
    /// The engine crashed more than the restart limit allows (FR-4).
    KeepsStopping,
}

impl Event {
    /// The bell's and the OS notification's title, its level and the source
    /// it is muted by in the bell (`notify.ts`).
    fn text(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Event::ReplyReady => ("Reply ready", "success", "agent"),
            Event::RunFailed => ("A run failed", "error", "agent"),
            Event::Approval => ("Approval needed", "warning", "agent"),
            Event::Question => ("A question for you", "warning", "agent"),
            Event::EngineStopped => ("The engine stopped", "error", "engine"),
            Event::CouldNotStart => ("The engine could not start", "error", "engine"),
            Event::KeepsStopping => ("The engine keeps stopping", "error", "engine"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Event::ReplyReady => "reply-ready",
            Event::RunFailed => "run-failed",
            Event::Approval => "approval",
            Event::Question => "question",
            Event::EngineStopped => "engine-stopped",
            Event::CouldNotStart => "could-not-start",
            Event::KeepsStopping => "keeps-stopping",
        }
    }
}

/// One bell entry, as a window adds it (`notification-store.ts`): clicking
/// it opens `chat`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BellEntry {
    pub id: String,
    pub title: String,
    /// The chat's title, or "A chat" for an incognito one.
    pub body: String,
    pub level: String,
    pub source: String,
    pub chat: String,
    /// Replaces an earlier entry of the same chat and source.
    pub key: String,
    /// ms since the epoch.
    pub at: u64,
}

/// The entry for `event` in `chat`, called `name` (`None`: incognito, or no title).
pub fn entry(event: Event, chat: &str, name: Option<&str>, id: String, at: u64) -> BellEntry {
    let (title, level, source) = event.text();
    BellEntry {
        id,
        title: title.to_string(),
        body: name.filter(|name| !name.trim().is_empty()).unwrap_or(INCOGNITO_NAME).to_string(),
        level: level.to_string(),
        source: source.to_string(),
        chat: chat.to_string(),
        key: format!("{source}:{chat}"),
        at,
    }
}

/// Whether the shell announces something in a chat shown by `viewers`
/// (window labels): not when one of them is focused, since the person sees it.
pub fn wanted(viewers: &[String], focused: impl Fn(&str) -> bool) -> bool {
    !viewers.iter().any(|label| focused(label))
}

/// The OS notification goes out when no window is focused and it is not muted.
pub fn system_wanted(any_window_focused: bool, muted: bool) -> bool {
    !any_window_focused && !muted
}

/// The window that adds the bell entry: the main one, else any; `None` with
/// no window open (the entry goes to the inbox).
pub fn route(windows: &[String]) -> Option<String> {
    windows.iter().find(|label| label.as_str() == "main").or_else(|| windows.iter().min()).cloned()
}

/// The inbox (`notifications.inbox`) with `entry` added: an older entry with
/// its key goes, and at most [`INBOX_MAX`] stay, the newest last.
pub fn push_inbox(saved: Option<Value>, entry: BellEntry) -> Vec<BellEntry> {
    let mut inbox = read_inbox(saved);
    inbox.retain(|kept| kept.key != entry.key);
    inbox.push(entry);
    let over = inbox.len().saturating_sub(INBOX_MAX);
    inbox.drain(..over);
    inbox
}

/// The saved inbox; anything unreadable is dropped.
pub fn read_inbox(saved: Option<Value>) -> Vec<BellEntry> {
    let Some(Value::Array(items)) = saved else {
        return Vec::new();
    };
    items.into_iter().filter_map(|item| serde_json::from_value(item).ok()).collect()
}

// ── The app ─────────────────────────────────────────────────────────────────

/// One writer of the inbox at a time (read, add, write).
static INBOX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Announce `event` of `chat`, off the caller: nothing when a focused window
/// shows the chat; otherwise a bell entry and, with no window focused, an OS
/// notification (FR-9, FR-10).
pub(crate) fn announce(app: &AppHandle, chat: String, event: Event) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let viewers = app.state::<super::EngineSupervisor>().viewers_of(&chat);
        let focused = |label: &str| {
            app.get_webview_window(label)
                .is_some_and(|window| window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false))
        };
        if !wanted(&viewers, focused) {
            return;
        }
        let db = app.state::<Db>();
        db.log(NewLog::new("info", "shell", format!("notice: {}", event.as_str())).chat(Some(&chat)));
        let row = db.with(|conn| evo_store::chats::get_chat(conn, &chat)).ok().flatten();
        // Nothing an incognito chat says is kept, its title included.
        let name = row.filter(|row| !row.incognito).map(|row| row.title);
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let at = super::now_ms();
        let id = format!("shell-{at:x}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let entry = entry(event, &chat, name.as_deref(), id, at);
        deliver(&app, entry, Some(chat));
    });
}

/// Hand `entry` to one window's bell, or keep it in the inbox with no window
/// open; with no window focused and not muted, an OS notification that opens
/// `chat` (FR-10; the Scheduler's run notices too, FR-7).
pub(crate) fn deliver(app: &AppHandle, entry: BellEntry, chat: Option<String>) {
    let db = app.state::<Db>();
    let windows: Vec<String> = app.webview_windows().into_keys().collect();
    match route(&windows) {
        Some(window) => {
            let _ = app.emit("notify://entry", json!({ "entry": entry, "window": window }));
        }
        None => {
            let _one = INBOX.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let saved = db.with(|conn| evo_store::settings::get(conn, INBOX_KEY)).ok().flatten();
            let inbox = push_inbox(saved, entry.clone());
            if let Err(error) = db.with(|conn| evo_store::settings::set(conn, INBOX_KEY, &json!(inbox))) {
                db.log(NewLog::new("warn", "shell", format!("notice not kept: {error}")).chat(chat.as_deref()));
            }
        }
    }
    if system_wanted(crate::agent_tools::user::any_window_focused(app), crate::agent_tools::user::muted()) {
        crate::agent_tools::user::send_os(app, &entry.title, Some(&entry.body), chat.as_deref());
    }
}

/// The bell entries the shell kept while no window was open; they are
/// handed over once (`notifications_take`, FR-10).
#[tauri::command]
pub fn notifications_take(db: State<'_, Db>) -> Vec<BellEntry> {
    let _one = INBOX.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let inbox = read_inbox(db.with(|conn| evo_store::settings::get(conn, INBOX_KEY)).ok().flatten());
    if !inbox.is_empty() {
        let _ = db.with(|conn| evo_store::settings::set(conn, INBOX_KEY, &Value::Null));
    }
    inbox
}

/// Show `chat` to the person: the chat window that shows it, else the main
/// window, which opens it (`app://open-chat`; built again first when it was
/// destroyed, FR-11).
pub(crate) fn open_chat(app: &AppHandle, chat: &str) {
    let shown = app.state::<super::EngineSupervisor>().viewers_of(chat).into_iter().find(|label| label != "main");
    if let Some(window) = shown.and_then(|label| app.get_webview_window(&label)) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    crate::system::open_in_main(app, chat);
}

/// A click on one of the shell's OS notifications opens its chat.
pub(crate) fn setup(app: &AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    let handle = app.clone();
    let _ = app.notification().on_action(move |action| {
        if action.action_id() != "tap" {
            return;
        }
        let chat = action.notification().and_then(|notification| notification.extra().get("chat")).and_then(Value::as_str);
        if let Some(chat) = chat {
            open_chat(&handle, chat);
        }
    });
}

// docs/features/shell-hosted-engines/spec.md, FR-9, FR-10 (T14).
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_notice_says_what_happened_and_the_chats_title_only() {
        let reply = entry(Event::ReplyReady, "c", Some("Fix the build"), "n1".into(), 5);
        assert_eq!((reply.title.as_str(), reply.body.as_str(), reply.level.as_str()), ("Reply ready", "Fix the build", "success"));
        assert_eq!((reply.source.as_str(), reply.key.as_str(), reply.chat.as_str()), ("agent", "agent:c", "c"));
        let approval = entry(Event::Approval, "c", Some("Fix the build"), "n2".into(), 6);
        assert_eq!((approval.title.as_str(), approval.level.as_str()), ("Approval needed", "warning"));
        let stopped = entry(Event::EngineStopped, "c", Some("Fix the build"), "n3".into(), 7);
        assert_eq!((stopped.source.as_str(), stopped.key.as_str()), ("engine", "engine:c"));
        for event in [Event::RunFailed, Event::Question, Event::CouldNotStart, Event::KeepsStopping] {
            assert!(!entry(event, "c", Some("t"), "n".into(), 0).title.is_empty());
        }
    }

    #[test]
    fn an_incognito_chat_is_a_chat() {
        assert_eq!(entry(Event::Approval, "c", None, "n".into(), 0).body, "A chat");
        assert_eq!(entry(Event::Approval, "c", Some("  "), "n".into(), 0).body, "A chat");
    }

    #[test]
    fn notices_wait_for_no_focused_window_showing_the_chat() {
        let viewers = vec!["main".to_string(), "chat-c".to_string()];
        assert!(wanted(&[], |_| true), "no window shows it");
        assert!(wanted(&viewers, |_| false), "shown, but nobody looks");
        assert!(!wanted(&viewers, |label| label == "chat-c"), "a focused window shows it");
        // The OS notification: only with no window in front and not muted.
        assert!(system_wanted(false, false));
        assert!(!system_wanted(true, false));
        assert!(!system_wanted(false, true));
    }

    #[test]
    fn the_bell_entry_goes_to_the_main_window_else_any_else_the_inbox() {
        assert_eq!(route(&["chat-b".into(), "main".into()]), Some("main".into()));
        assert_eq!(route(&["chat-b".into(), "chat-a".into()]), Some("chat-a".into()));
        assert_eq!(route(&[]), None);
    }

    #[test]
    fn the_inbox_keeps_the_newest_hundred_and_one_entry_per_key() {
        let mut saved = None;
        for n in 0..(INBOX_MAX as u64 + 5) {
            let inbox = push_inbox(saved, entry(Event::ReplyReady, &format!("c{n}"), Some("t"), format!("n{n}"), n));
            saved = Some(serde_json::to_value(inbox).unwrap());
        }
        let inbox = read_inbox(saved.clone());
        assert_eq!(inbox.len(), INBOX_MAX);
        assert_eq!((inbox[0].chat.as_str(), inbox.last().unwrap().chat.as_str()), ("c5", "c104"));
        // The same chat again: its earlier entry goes.
        let inbox = push_inbox(saved, entry(Event::RunFailed, "c50", Some("t"), "x".into(), 200));
        assert_eq!(inbox.iter().filter(|e| e.chat == "c50").count(), 1);
        assert_eq!(inbox.last().unwrap().title, "A run failed");
        // Something unreadable is dropped.
        assert_eq!(read_inbox(Some(json!([{ "nope": 1 }, 3]))), Vec::new());
        assert_eq!(read_inbox(Some(json!({}))), Vec::new());
    }
}
