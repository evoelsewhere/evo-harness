//! Mentions of the person in channels, and muting
//! (docs/features/evo-mates-messaging/spec.md, FR-15, FR-18).
//!
//! A mate's channel line that writes `@you` is kept in the app setting
//! [`MENTIONS_KEY`] (at most [`MENTIONS_MAX`], until the person opens the
//! channel) for Today › Mentions, and announced like a reply, under the
//! channel's mute and level. The mute of each mate and channel lives in the
//! app setting [`MUTE_KEY`]: `{ "<chat>": { "until": ms | null, "level":
//! "all" | "mentions" | "nothing" } }`.

use evo_store::rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::Db;

pub const MENTIONS_KEY: &str = "mates.mentions";
pub const MENTIONS_MAX: usize = 50;
pub const MUTE_KEY: &str = "mates.mute";
pub const CHANGED: &str = "mates://mentions";

/// One line that mentioned the person.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mention {
    /// The channel's chat.
    pub chat: String,
    pub line: String,
    /// The mate's id and name.
    pub author: String,
    pub name: String,
    pub excerpt: String,
    pub at: i64,
}

/// Whether `text` mentions the person: `@you` as a word, any case.
pub fn mentions_person(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.match_indices("@you").any(|(at, _)| {
        let before = lower[..at].chars().last();
        let after = lower[at + 4..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric()) && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// The mention a channel entry makes, if any: a mate's message with `@you`.
pub fn mention_of(chat: &str, entry: &Value) -> Option<Mention> {
    if entry.get("kind").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let author = entry.get("author").and_then(Value::as_str)?;
    if author == "person" {
        return None;
    }
    let text = entry.get("text").and_then(Value::as_str)?;
    if !mentions_person(text) {
        return None;
    }
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let excerpt = if flat.chars().count() > 140 { format!("{}…", flat.chars().take(139).collect::<String>()) } else { flat };
    Some(Mention {
        chat: chat.to_string(),
        line: entry.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
        author: author.to_string(),
        name: entry.get("name").and_then(Value::as_str).unwrap_or(author).to_string(),
        excerpt,
        at: entry.get("at").and_then(Value::as_i64).unwrap_or_default(),
    })
}

/// The saved mentions with `mention` added, newest last, at most [`MENTIONS_MAX`].
pub fn push_mention(saved: Option<Value>, mention: Mention) -> Vec<Mention> {
    let mut all = read_mentions(saved);
    all.retain(|m| !(m.chat == mention.chat && m.line == mention.line));
    all.push(mention);
    let over = all.len().saturating_sub(MENTIONS_MAX);
    all.drain(..over);
    all
}

pub fn read_mentions(saved: Option<Value>) -> Vec<Mention> {
    match saved {
        Some(Value::Array(items)) => items.into_iter().filter_map(|item| serde_json::from_value(item).ok()).collect(),
        _ => Vec::new(),
    }
}

/// How a muted place takes a notice (FR-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// A reply landed, a run failed.
    Reply,
    /// An approval, a question, a sign-in or a secret waits.
    NeedsYou,
    /// A line mentioned the person.
    Mention,
}

/// Whether a notice of `kind` for `chat` goes out under the saved mutes at `now`.
pub fn mute_wanted(saved: Option<&Value>, chat: &str, kind: NoticeKind, now: i64) -> bool {
    let Some(rule) = saved.and_then(|all| all.get(chat)) else {
        return true;
    };
    // Muted until a time (or with `until: null` and no level: always).
    let has_until = rule.get("until").is_some();
    let muted = match rule.get("until") {
        Some(Value::Number(until)) => until.as_i64().is_some_and(|until| until > now),
        Some(Value::Null) => true,
        _ => false,
    };
    let level = rule.get("level").and_then(Value::as_str).unwrap_or("all");
    if muted && has_until {
        // Muted: only what needs the person still notifies, unless the level is Nothing.
        return kind == NoticeKind::NeedsYou && level != "nothing";
    }
    match level {
        "nothing" => false,
        "mentions" => kind != NoticeKind::Reply,
        _ => true,
    }
}

/// The channel chat a seat chat belongs to.
pub fn channel_chat_of_seat(conn: &Connection, seat: &str) -> Option<String> {
    conn.query_row(
        "SELECT c.chat_id FROM channel_members m JOIN channels c ON c.id = m.channel_id WHERE m.seat_chat_id = ?1",
        [seat],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

/// A channel line from the coordinator (`channel_event`): keep and announce a mention.
pub fn on_channel_event(app: &AppHandle, chat: &str, entry: &Value) {
    let Some(mention) = mention_of(chat, entry) else {
        return;
    };
    let db = app.state::<Db>();
    let saved = db.with(|conn| evo_store::settings::get(conn, MENTIONS_KEY)).ok().flatten();
    let all = push_mention(saved, mention);
    let _ = db.with(|conn| evo_store::settings::set(conn, MENTIONS_KEY, &json!(all)));
    let _ = app.emit(CHANGED, json!({ "chat": chat }));
    crate::supervisor::notices::announce(app, chat.to_string(), crate::supervisor::notices::Event::Mention);
}

/// Every kept mention, newest last.
#[tauri::command]
pub fn mates_mentions(app: AppHandle) -> Result<Vec<Mention>, String> {
    let saved = app.state::<Db>().with(|conn| evo_store::settings::get(conn, MENTIONS_KEY))?;
    Ok(read_mentions(saved))
}

/// The person opened `chat`: its mentions are read.
#[tauri::command]
pub fn mates_mentions_read(app: AppHandle, chat: String) -> Result<(), String> {
    let db = app.state::<Db>();
    let mut all = read_mentions(db.with(|conn| evo_store::settings::get(conn, MENTIONS_KEY))?);
    let before = all.len();
    all.retain(|m| m.chat != chat);
    if all.len() != before {
        db.with(|conn| evo_store::settings::set(conn, MENTIONS_KEY, &json!(all)))?;
        let _ = app.emit(CHANGED, json!({ "chat": chat }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_you_is_a_word() {
        assert!(mentions_person("@you can you check this?"));
        assert!(mentions_person("Done — over to @You."));
        assert!(!mentions_person("@youssef please"));
        assert!(!mentions_person("mail@you.com"));
        assert!(!mentions_person("thank you"));
    }

    #[test]
    fn only_a_mates_message_mentions() {
        let line = json!({ "kind": "message", "id": "r-1", "author": "kai", "name": "Kai", "text": "@you the build is red", "at": 5 });
        let mention = mention_of("room", &line).unwrap();
        assert_eq!((mention.line.as_str(), mention.name.as_str(), mention.at), ("r-1", "Kai", 5));
        assert!(mention_of("room", &json!({ "kind": "message", "author": "person", "text": "@you" })).is_none());
        assert!(mention_of("room", &json!({ "kind": "handoff", "author": "kai", "text": "@you" })).is_none());
    }

    #[test]
    fn mentions_keep_the_newest_once_each() {
        let m = |line: &str| Mention { chat: "c".into(), line: line.into(), author: "k".into(), name: "K".into(), excerpt: "x".into(), at: 0 };
        let mut saved = None;
        for i in 0..(MENTIONS_MAX + 3) {
            saved = Some(json!(push_mention(saved, m(&format!("r-{i}")))));
        }
        let all = read_mentions(saved.clone());
        assert_eq!(all.len(), MENTIONS_MAX);
        assert_eq!(all.last().unwrap().line, format!("r-{}", MENTIONS_MAX + 2));
        assert_eq!(push_mention(saved, m("r-52")).len(), MENTIONS_MAX, "the same line once");
    }

    #[test]
    fn mutes_and_levels() {
        let now = 1_000;
        let all = json!({
            "muted-hour": { "until": 2_000 },
            "muted-over": { "until": 500 },
            "muted-always": { "until": null },
            "mentions": { "level": "mentions" },
            "nothing": { "level": "nothing" },
            "muted-nothing": { "until": 2_000, "level": "nothing" },
        });
        let w = |chat: &str, kind| mute_wanted(Some(&all), chat, kind, now);
        assert!(w("other", NoticeKind::Reply));
        assert!(!w("muted-hour", NoticeKind::Reply) && !w("muted-hour", NoticeKind::Mention));
        assert!(w("muted-hour", NoticeKind::NeedsYou), "needs you still notifies");
        assert!(w("muted-over", NoticeKind::Reply), "the hour is over");
        assert!(!w("muted-always", NoticeKind::Reply));
        assert!(!w("mentions", NoticeKind::Reply) && w("mentions", NoticeKind::Mention) && w("mentions", NoticeKind::NeedsYou));
        assert!(!w("nothing", NoticeKind::NeedsYou));
        assert!(!w("muted-nothing", NoticeKind::NeedsYou));
        assert!(mute_wanted(None, "x", NoticeKind::Reply, now));
    }
}
