//! A chat's memory flags and the life of an incognito chat.
//!
//! A chat can be started without memory, or incognito (no memory, and no
//! history: it is removed when it ends). The choice is made before the first
//! message, kept on the chat's row, and passed to the chat's engine as a start
//! flag every time it starts. This module holds what the shell does with the
//! flags: the engine's arguments, which tools the chat is offered, what is
//! logged for an incognito chat, and removing incognito chats when one is
//! ended, when evo quits, and at the next start after a crash.
//!
//! Spec: docs/features/memory-topics-and-chat-recall (FR-6, FR-15, FR-19 –
//! FR-22, NFR-4), plan 3.5.

use evo_store::chats::{self, Chat};
use evo_store::logs::NewLog;
use evo_store::rusqlite::Connection;
use evo_store::DbError;
use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::db::Db;
use crate::engine::EngineState;

/// The app setting behind **Settings → Memory → Search past chats** (on unless `false`).
pub const SEARCH_PAST_CHATS_KEY: &str = "memory.searchPastChats";

/// The tools that look into past chats; hidden by the setting and by the flags.
pub const PAST_CHAT_TOOLS: [&str; 2] = ["session_search", "session_read"];

/// What a chat was started with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatFlags {
    /// No memory section, no `memory` tool, no past-chat tools.
    pub memory_off: bool,
    /// As `memory_off`, and the chat leaves no history.
    pub incognito: bool,
}

impl ChatFlags {
    /// The flags of a saved chat. An incognito chat never uses memory.
    pub fn of(chat: &Chat) -> Self {
        Self { memory_off: chat.memory_off || chat.incognito, incognito: chat.incognito }
    }

    /// The flags `chat`'s engine starts with: its row decides once it exists;
    /// before that (a draft has no row yet) what the window asked for.
    pub fn for_engine(conn: &Connection, chat: &str, asked: Option<ChatFlags>) -> Self {
        match chats::get_chat(conn, chat).ok().flatten() {
            Some(row) => Self::of(&row),
            None => {
                let asked = asked.unwrap_or_default();
                Self { memory_off: asked.memory_off || asked.incognito, incognito: asked.incognito }
            }
        }
    }

    /// The engine's start flags for this chat.
    pub fn engine_args(self) -> &'static [&'static str] {
        if self.incognito {
            &["--incognito"]
        } else if self.memory_off {
            &["--no-memory"]
        } else {
            &[]
        }
    }

    pub fn uses_memory(self) -> bool {
        !self.memory_off && !self.incognito
    }
}

/// Whether a chat with `flags` is offered the past-chat tools, given the
/// saved **Search past chats** setting.
pub fn past_chat_tools_offered(setting: Option<&Value>, flags: ChatFlags) -> bool {
    setting.and_then(Value::as_bool).unwrap_or(true) && flags.uses_memory()
}

/// The registry tools `chat` is not offered now.
pub async fn hidden_tools(app: &AppHandle, chat: Option<&str>) -> &'static [&'static str] {
    let running = match chat {
        Some(chat) => app.state::<EngineState>().flags(chat).await,
        None => None,
    };
    let db = app.state::<Db>();
    let offered = db
        .with(|conn| {
            let flags = match (running, chat) {
                (Some(flags), _) => flags,
                (None, Some(chat)) => ChatFlags::for_engine(conn, chat, None),
                (None, None) => ChatFlags::default(),
            };
            let settings = evo_store::settings::all(conn)?;
            Ok(past_chat_tools_offered(settings.get(SEARCH_PAST_CHATS_KEY), flags))
        })
        .unwrap_or(true);
    if offered {
        &[]
    } else {
        &PAST_CHAT_TOOLS
    }
}

/// A run-log entry for an incognito chat: the same event with none of the
/// chat's text. Error strings, provider messages and event payloads can quote
/// what was said, so only the event's name is kept.
pub fn quiet_entry(message: &Value, entry: NewLog) -> NewLog {
    let text = match message.get("type").and_then(Value::as_str) {
        Some("response") => format!("{} failed", message.get("command").and_then(Value::as_str).unwrap_or("command")),
        Some("message_end") => "the model returned an error".to_string(),
        _ => entry.message.clone(),
    };
    NewLog { message: text, data: None, ..entry }
}

/// Remove the rows and log entries of `ids`. Their transcript files are
/// removed by the caller once no engine has them open.
fn remove_rows(conn: &mut Connection, ids: &[String]) -> Result<(), DbError> {
    for id in ids {
        chats::delete_chat(conn, id)?;
        evo_store::logs::delete_chat(conn, id)?;
    }
    Ok(())
}

/// Remove every incognito chat's row, with its nested chats and their log
/// entries. Returns the chat ids; their files are still to remove.
pub fn forget_incognito_rows(conn: &mut Connection) -> Result<Vec<String>, DbError> {
    let ids = chats::incognito_ids(conn)?;
    remove_rows(conn, &ids)?;
    Ok(ids)
}

/// A chat's Office document versions, kept by its engine under
/// `<agent>/document-versions/<chat>`.
pub fn remove_document_versions(id: &str) -> Result<(), String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Ok(());
    }
    let versions = crate::mcp::agent_dir().join("document-versions").join(id);
    if versions.is_dir() {
        std::fs::remove_dir_all(&versions).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// What deleting chat `id` must also remove: for an incognito chat, the chats
/// nested under it. Read before the chat's row is deleted.
pub struct Ending {
    incognito: bool,
    nested: Vec<String>,
}

pub fn ending(db: &Db, id: &str) -> Ending {
    db.with(|conn| {
        let incognito = chats::get_chat(conn, id)?.is_some_and(|chat| chat.incognito);
        let nested = if incognito { chats::with_nested(conn, id)?.into_iter().filter(|nested| nested != id).collect() } else { Vec::new() };
        Ok(Ending { incognito, nested })
    })
    .unwrap_or(Ending { incognito: false, nested: Vec::new() })
}

/// After chat `id` was deleted: an incognito chat's nested chats go the same
/// way (row, engine, transcript, document versions), and no log entry of any
/// of them stays.
pub async fn finish(ending: Ending, db: &Db, engines: &EngineState, id: &str) {
    if !ending.incognito {
        return;
    }
    for nested in &ending.nested {
        let _ = db.with(|conn| chats::delete_chat(conn, nested));
        engines.stop(nested).await;
        db.remove_transcript(nested);
        let _ = remove_document_versions(nested);
    }
    let mut ids = ending.nested;
    ids.push(id.to_string());
    let _ = db.with(|conn| {
        for id in &ids {
            evo_store::logs::delete_chat(conn, id)?;
        }
        Ok(())
    });
}

/// As evo quits, after every engine stopped: no incognito chat is left.
pub fn forget_incognito(app: &AppHandle) {
    let db = app.state::<Db>();
    let Ok(ids) = db.with(forget_incognito_rows) else {
        return;
    };
    for id in &ids {
        db.remove_transcript(id);
        let _ = remove_document_versions(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_store::chats::NewChat;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A scratch app database, removed on drop.
    struct Scratch {
        root: PathBuf,
        conn: Connection,
    }

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!("evo-chat-flags-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&root).unwrap();
            let conn = evo_store::open_at(&root.join("evo.db")).unwrap();
            Self { root, conn }
        }

        fn chat(&self, id: &str, parent: Option<&str>, memory_off: bool, incognito: bool) -> Chat {
            chats::create_chat(
                &self.conn,
                &NewChat { id: id.into(), title: format!("about {id}"), parent_chat_id: parent.map(str::to_string), memory_off, incognito, ..Default::default() },
            )
            .unwrap()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    // FR-6, FR-19, FR-20: what an engine is started with.
    #[test]
    fn engine_args_follow_chat_flags() {
        let s = Scratch::new();
        s.chat("plain", None, false, false);
        s.chat("quiet", None, true, false);
        s.chat("ghost", None, false, true);
        let empty: &[&str] = &[];
        assert_eq!(ChatFlags::for_engine(&s.conn, "plain", None).engine_args(), empty);
        assert_eq!(ChatFlags::for_engine(&s.conn, "quiet", None).engine_args(), ["--no-memory"]);
        assert_eq!(ChatFlags::for_engine(&s.conn, "ghost", None).engine_args(), ["--incognito"]);
        // The row wins over what a window asks for: the choice is locked.
        let asked = Some(ChatFlags { memory_off: false, incognito: false });
        assert_eq!(ChatFlags::for_engine(&s.conn, "quiet", asked).engine_args(), ["--no-memory"]);
        let asked = Some(ChatFlags { memory_off: true, incognito: true });
        assert_eq!(ChatFlags::for_engine(&s.conn, "plain", asked).engine_args(), empty);
        // A draft has no row yet: the window's choice applies.
        assert_eq!(ChatFlags::for_engine(&s.conn, "draft", None), ChatFlags::default());
        assert_eq!(ChatFlags::for_engine(&s.conn, "draft", Some(ChatFlags { memory_off: true, incognito: false })).engine_args(), ["--no-memory"]);
        let incognito = ChatFlags::for_engine(&s.conn, "draft", Some(ChatFlags { memory_off: false, incognito: true }));
        assert_eq!(incognito, ChatFlags { memory_off: true, incognito: true });
        assert!(!incognito.uses_memory());
        // What the window sends.
        let parsed: ChatFlags = serde_json::from_value(json!({ "memoryOff": true })).unwrap();
        assert_eq!(parsed, ChatFlags { memory_off: true, incognito: false });
        assert_eq!(serde_json::from_value::<ChatFlags>(json!({})).unwrap(), ChatFlags::default());
    }

    // FR-22: a nested chat is started with its parent's flags.
    #[test]
    fn nested_chats_inherit_flags() {
        let s = Scratch::new();
        let parent = s.chat("ghost", None, false, true);
        let flags = ChatFlags::of(&parent);
        assert_eq!(flags, ChatFlags { memory_off: true, incognito: true });
        let child = s.chat("child", Some("ghost"), flags.memory_off, flags.incognito);
        assert_eq!(ChatFlags::of(&child), flags);
        assert_eq!(ChatFlags::of(&s.chat("plain", None, false, false)), ChatFlags::default());
    }

    // FR-6, FR-15, FR-20: who is offered the past-chat tools.
    #[test]
    fn past_chat_tools_follow_settings() {
        let on = ChatFlags::default();
        assert!(past_chat_tools_offered(None, on), "on unless switched off");
        assert!(past_chat_tools_offered(Some(&json!(true)), on));
        assert!(!past_chat_tools_offered(Some(&json!(false)), on));
        assert!(past_chat_tools_offered(Some(&json!("no")), on), "only `false` switches it off");
        assert!(!past_chat_tools_offered(None, ChatFlags { memory_off: true, incognito: false }));
        assert!(!past_chat_tools_offered(Some(&json!(true)), ChatFlags { memory_off: true, incognito: true }));
        assert_eq!(PAST_CHAT_TOOLS, ["session_search", "session_read"]);
    }

    // NFR-4: nothing an incognito chat said reaches the run log.
    #[test]
    fn incognito_engines_log_no_text() {
        let secret = "the secret plan";
        let entry = |message: &str| NewLog::new("warn", "engine", message.to_string()).data(json!({ "error": secret }));
        let failed = json!({ "type": "response", "command": "prompt", "success": false, "error": secret });
        let quiet = quiet_entry(&failed, entry(&format!("prompt failed: {secret}")));
        assert_eq!((quiet.message.as_str(), quiet.data.clone(), quiet.level.as_str()), ("prompt failed", None, "warn"));

        let model_error = json!({ "type": "message_end", "message": { "stopReason": "error", "errorMessage": secret } });
        let quiet = quiet_entry(&model_error, entry(secret));
        assert_eq!((quiet.message.as_str(), quiet.data), ("the model returned an error", None));

        // Events whose message is only their name keep it, without their payload.
        for (kind, text) in [("agent_end", "run failed"), ("auto_compaction_start", "auto compaction start"), ("ready", "ready")] {
            let quiet = quiet_entry(&json!({ "type": kind, "error": secret, "summary": secret }), entry(text));
            assert_eq!((quiet.message.as_str(), quiet.data), (text, None));
        }
    }

    // FR-22: no incognito chat is left when evo quits or starts again.
    #[test]
    fn incognito_chats_are_removed() {
        let mut s = Scratch::new();
        s.chat("plain", None, false, false);
        s.chat("quiet", None, true, false);
        s.chat("ghost", None, false, true);
        s.chat("ghost-child", Some("ghost"), true, true);
        // A nested chat that lost the flag still goes with its parent.
        s.chat("ghost-grandchild", Some("ghost-child"), false, false);
        s.chat("plain-child", Some("plain"), false, false);
        evo_store::logs::append(
            &mut s.conn,
            &[
                NewLog::new("info", "shell", "engine started").chat(Some("ghost")),
                NewLog::new("info", "shell", "engine started").chat(Some("ghost-child")),
                NewLog::new("info", "shell", "engine started").chat(Some("plain")),
            ],
        )
        .unwrap();

        let removed = forget_incognito_rows(&mut s.conn).unwrap();
        assert_eq!(removed, ["ghost", "ghost-child", "ghost-grandchild"]);
        let left: Vec<String> = chats::list_chats(&s.conn, false).unwrap().into_iter().map(|chat| chat.id).collect();
        assert_eq!(left.len(), 3);
        assert!(["plain", "quiet", "plain-child"].iter().all(|id| left.contains(&id.to_string())), "{left:?}");
        let logged = evo_store::logs::query(&s.conn, &Default::default()).unwrap();
        assert_eq!(logged.len(), 1);
        assert_eq!(logged[0].chat_id.as_deref(), Some("plain"));
        // Nothing left to remove the second time.
        assert!(forget_incognito_rows(&mut s.conn).unwrap().is_empty());

        // Ids that are not plain chat ids never become a path.
        assert!(remove_document_versions("../evo").is_ok());
        assert!(remove_document_versions("").is_ok());
    }
}
