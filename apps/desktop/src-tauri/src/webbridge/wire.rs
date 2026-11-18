//! The Webbridge wire on the shell's side (docs/features/browser-bridge/plan.md, 3.3).
//!
//! Frames are native messaging frames (`evo_bridge_host::frames`). A message
//! the shell sends the extension that is larger than the browser accepts from
//! a host goes as `chunk` frames the extension joins again.

use serde_json::{json, Value};

/// The wire version this shell speaks (`v` in `hello` / `welcome`).
pub const VERSION: u64 = 1;

/// The browser accepts at most 1 MB per frame from a host; stay under it.
pub const CHUNK_LIMIT: usize = 900 * 1024;

/// The frames that carry `message` to the extension: the message itself, or
/// `chunk` frames when it is too large for one.
pub fn outgoing(message: &Value, chunk_id: u64) -> Vec<Vec<u8>> {
    let text = message.to_string();
    if text.len() <= CHUNK_LIMIT {
        return vec![text.into_bytes()];
    }
    // Leave room for the chunk's own fields and escaping (JSON text inside a string doubles quotes).
    let piece = CHUNK_LIMIT / 2;
    let mut parts = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + piece).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        parts.push(&text[start..end]);
        start = end;
    }
    let count = parts.len();
    parts
        .into_iter()
        .enumerate()
        .map(|(seq, data)| json!({ "t": "chunk", "id": chunk_id, "seq": seq, "last": seq + 1 == count, "data": data }).to_string().into_bytes())
        .collect()
}

/// Join `chunk` frames (in order) back into the message; what the extension does.
#[cfg(test)]
pub fn join(frames: &[Vec<u8>]) -> Option<Value> {
    let mut text = String::new();
    for frame in frames {
        let chunk: Value = serde_json::from_slice(frame).ok()?;
        text.push_str(chunk["data"].as_str()?);
    }
    serde_json::from_str(&text).ok()
}

/// The `t` of a message.
pub fn kind(message: &Value) -> &str {
    message.get("t").and_then(Value::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_message_is_one_frame() {
        let message = json!({ "t": "list", "id": 1 });
        let frames = outgoing(&message, 9);
        assert_eq!(frames.len(), 1);
        assert_eq!(
            serde_json::from_slice::<Value>(&frames[0]).unwrap(),
            message
        );
    }

    #[test]
    fn a_large_message_is_split_under_the_limit_and_joins_back() {
        // Quotes and non-ASCII text grow when escaped and must not split a character.
        let data: String = "ảnh \"quoted\" ".repeat(400_000);
        let message = json!({ "t": "cdp", "id": 3, "params": { "data": data } });
        let frames = outgoing(&message, 42);
        assert!(frames.len() > 1);
        assert!(
            frames.iter().all(|f| f.len() <= 1024 * 1024),
            "every frame fits the browser's limit"
        );
        let first: Value = serde_json::from_slice(&frames[0]).unwrap();
        assert_eq!(
            (
                first["t"].as_str(),
                first["id"].as_u64(),
                first["last"].as_bool()
            ),
            (Some("chunk"), Some(42), Some(false))
        );
        let last: Value = serde_json::from_slice(frames.last().unwrap()).unwrap();
        assert_eq!(last["last"], true);
        assert_eq!(join(&frames).unwrap(), message);
    }
}
