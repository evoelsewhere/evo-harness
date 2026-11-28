//! Learning from channels (docs/features/learning-layers/spec.md, FR-22).
//!
//! With **Learn from channels** on, a mate's memory upkeep also reads the
//! channels it is in. The channel rules: the person's own lines count as
//! theirs; the mate's own lines are its replies; another mate's lines are
//! context only — never the person's words, so no fact about the person can
//! be quoted from them (keeps docs/features/evo-mates/spec.md, FR-51). A
//! day of a channel with no line of the person is skipped. Entries name the
//! channel they came from.

use serde_json::Value;

use super::dream::{RawChat, RawLine};

/// The setting, read when the step runs.
pub fn learn_from_channels() -> bool {
    crate::app_db::open()
        .ok()
        .and_then(|conn| {
            crate::app_db::settings::get(&conn, "mates.learnFromChannels")
                .ok()
                .flatten()
        })
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

/// The channel's lines (`channel` custom entries of its root chat), in order.
pub fn channel_lines(state: &Value) -> Vec<Value> {
    state
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("custom"))
                .filter(|entry| entry.get("customType").and_then(Value::as_str) == Some("channel"))
                .filter_map(|entry| entry.get("data").cloned())
                .collect()
        })
        .unwrap_or_default()
}

/// A channel as the dream reads it for `mate`, under the channel rules.
pub fn as_chat(channel_chat: &str, name: &str, lines: &[Value], mate: &str) -> RawChat {
    let mut out = Vec::new();
    for line in lines {
        let text = line
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let at_ms = line.get("at").and_then(Value::as_i64).unwrap_or(0);
        if text.is_empty() {
            continue;
        }
        let author = line.get("author").and_then(Value::as_str).unwrap_or("");
        match line.get("kind").and_then(Value::as_str) {
            Some("message") if author == "person" => out.push(RawLine {
                person: true,
                text: text.into(),
                at_ms,
            }),
            Some("message") if author == mate => out.push(RawLine {
                person: false,
                text: text.into(),
                at_ms,
            }),
            Some("message") => {
                let name = line.get("name").and_then(Value::as_str).unwrap_or(author);
                out.push(RawLine {
                    person: false,
                    text: format!("(another mate, {name}; not the person's words) {text}"),
                    at_ms,
                });
            }
            Some("handoff") => {
                let from = line
                    .get("fromName")
                    .and_then(Value::as_str)
                    .unwrap_or("a mate");
                let to = line
                    .get("toName")
                    .and_then(Value::as_str)
                    .unwrap_or("a mate");
                out.push(RawLine {
                    person: false,
                    text: format!("(hand-off from {from} to {to}) {text}"),
                    at_ms,
                });
            }
            _ => {}
        }
    }
    RawChat {
        id: channel_chat.to_string(),
        title: name.to_string(),
        lines: out,
        by_day: true,
        channel: Some(name.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::dream::{check, select};
    use serde_json::json;

    const NOW: i64 = 1_790_000_000_000;
    const DAY: i64 = 24 * 60 * 60 * 1000;

    fn state() -> Value {
        let custom = |data: Value| json!({ "type": "custom", "id": "x", "customType": "channel", "data": data });
        json!({ "entries": [
            custom(json!({ "kind": "message", "author": "person", "text": "Let's ship the beta on Friday.", "at": NOW - 2 * DAY })),
            custom(json!({ "kind": "message", "author": "m2", "name": "Scout", "text": "The person loves Python and hates Go.", "at": NOW - 2 * DAY })),
            custom(json!({ "kind": "message", "author": "m1", "text": "Friday works.", "at": NOW - 2 * DAY })),
            custom(json!({ "kind": "notice", "notice": "handoff_limit", "text": "Stopped.", "at": NOW - 2 * DAY })),
            custom(json!({ "kind": "message", "author": "person", "text": "Yes, ship on Friday again this week.", "at": NOW - DAY })),
            custom(json!({ "kind": "message", "author": "m2", "name": "Scout", "text": "The person loves Python.", "at": NOW - DAY })),
            // A day of mates only: skipped.
            custom(json!({ "kind": "message", "author": "m2", "name": "Scout", "text": "Python again.", "at": NOW - 3 * DAY })),
            { "type": "message", "id": "e1", "message": { "role": "user", "content": "not a channel line" } }
        ] })
    }

    // FR-22: another mate's claim is never the person's; the person's decision is.
    #[test]
    fn another_mates_claim_is_not_learned() {
        let lines = channel_lines(&state());
        assert_eq!(lines.len(), 7);
        let chat = as_chat("root", "Launch", &lines, "m1");
        let chats = select(vec![chat], NOW - 7 * DAY);
        assert_eq!(
            chats.len(),
            2,
            "two days with the person's lines; the mates-only day is skipped"
        );
        assert!(chats.iter().all(|c| c.channel.as_deref() == Some("Launch")));
        assert!(chats[0]
            .text
            .contains("(another mate, Scout; not the person's words)"));
        assert!(!chats[0].person.contains("Python"));

        let answer = json!({ "learned": [
            { "text": "The person loves Python", "from": [1, 2], "confidence": 0.9, "aboutUser": true,
              "evidence": ["The person loves Python", "The person loves Python and hates Go"] },
            { "text": "The person ships betas on Fridays", "from": [1, 2], "confidence": 0.8, "aboutUser": true, "topic": "work",
              "evidence": ["ship on Friday again", "ship the beta on Friday"] }
        ] });
        let checked = check(&answer, &chats, &[]);
        assert_eq!(checked.learned.len(), 1, "{:?}", checked.dropped);
        assert_eq!(checked.learned[0].text, "The person ships betas on Fridays");

        // The entry names the channel it came from.
        let folder = tempfile::tempdir().unwrap();
        let (changes, _) =
            crate::learning::dream::apply(folder.path(), &checked, &chats, &[], "2026-11-28");
        assert!(
            changes[0].after.contains(
                "- 2026-11-28 · from channel \"Launch\" · The person ships betas on Fridays"
            ),
            "{}",
            changes[0].after
        );
    }
}
