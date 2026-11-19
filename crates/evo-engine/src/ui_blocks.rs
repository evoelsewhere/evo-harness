//! UI blocks (`evo-ui` fences) in a reply, as the engine sees them.
//!
//! Spec: `docs/features/rich-replies/spec.md` (FR-1, FR-4); plan 3.3
//! "Engine preview". The desktop draws a block and writes its text
//! (`apps/desktop/src/lib/ui-blocks/text.ts`); the engine only needs a
//! block's name for the one-line turn preview, so a preview never shows a
//! block's JSON. Fences are found the way `findBlocks` finds them there.

use serde_json::Value;

/// The fence info string that marks a block.
pub const BLOCK_FENCE: &str = "evo-ui";

/// The components of format version 1 (`lib/ui-blocks/types.ts` `BLOCK_TYPES`).
pub const BLOCK_TYPES: [&str; 7] = [
    "stats", "bars", "chart", "table", "steps", "callout", "actions",
];

/// A fence marker line: up to 3 spaces, then 3 or more backticks or tildes.
/// Returns the marker and the rest of the line.
fn fence_marker(line: &str) -> Option<(&str, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let ch = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = rest.len() - rest.trim_start_matches(ch).len();
    if len < 3 {
        return None;
    }
    Some(rest.split_at(len))
}

/// A known component named by `"type":"…"` in a body that may be cut short
/// (`parse.ts` `sniffType`).
fn sniff_type(partial: &str) -> Option<&'static str> {
    let mut rest = partial;
    while let Some(at) = rest.find("\"type\"") {
        rest = &rest[at + "\"type\"".len()..];
        let Some(after) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        let Some(value) = after.trim_start().strip_prefix('"') else {
            continue;
        };
        let name_len = value
            .find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
            .unwrap_or(value.len());
        if name_len == 0 || !value[name_len..].starts_with('"') {
            continue;
        }
        return BLOCK_TYPES
            .into_iter()
            .find(|known| *known == &value[..name_len]);
    }
    None
}

/// A block's name for a preview: `[title]`, else `[type]`, else `[block]`;
/// for a block cut short, `[type]` or nothing.
fn block_name(source: &str, closed: bool) -> String {
    if let Ok(Value::Object(json)) = serde_json::from_str::<Value>(source) {
        if let Some(title) = json
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
        {
            return format!("[{title}]");
        }
        if let Some(known) = json
            .get("type")
            .and_then(Value::as_str)
            .and_then(|ty| BLOCK_TYPES.into_iter().find(|known| *known == ty))
        {
            return format!("[{known}]");
        }
    }
    match sniff_type(source) {
        Some(known) => format!("[{known}]"),
        None if closed => "[block]".to_owned(),
        None => String::new(),
    }
}

/// `text` with each `evo-ui` fence replaced by its name in brackets (FR-4):
/// `[title]` when its JSON has a title, else `[type]`, else `[block]`. A
/// fence still open at the end becomes `[type]` when its type can be read,
/// else it is dropped. Fences inside other code fences are left alone. Line
/// ends become `\n`.
pub fn preview_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if !text.contains(BLOCK_FENCE) {
        return text;
    }
    struct Open<'a> {
        marker: &'a str,
        start: usize,
        body_start: usize,
        block: bool,
    }
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut fence: Option<Open> = None;
    let mut offset = 0;
    for line in text.split('\n') {
        let line_end = offset + line.len();
        match &fence {
            Some(open) => {
                let closes = fence_marker(line).is_some_and(|(marker, rest)| {
                    marker.starts_with(&open.marker[..1])
                        && marker.len() >= open.marker.len()
                        && rest.trim_matches([' ', '\t']).is_empty()
                });
                if closes {
                    if open.block {
                        let body =
                            &text[open.body_start..offset.saturating_sub(1).max(open.body_start)];
                        out.push_str(&text[copied..open.start]);
                        out.push_str(&block_name(body, true));
                        copied = line_end;
                    }
                    fence = None;
                }
            }
            None => {
                if let Some((marker, info)) = fence_marker(line) {
                    // A backtick fence's info string may not contain a backtick.
                    if !(marker.starts_with('`') && info.contains('`')) {
                        fence = Some(Open {
                            marker,
                            start: offset,
                            body_start: (line_end + 1).min(text.len()),
                            block: info.trim() == BLOCK_FENCE,
                        });
                    }
                }
            }
        }
        offset = line_end + 1;
    }
    match fence {
        Some(open) if open.block => {
            out.push_str(&text[copied..open.start]);
            out.push_str(&block_name(&text[open.body_start..], false));
        }
        _ => out.push_str(&text[copied..]),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATS: &str =
        r#"{"v":1,"type":"stats","title":"Usage this week","items":[{"label":"Cost","value":12}]}"#;

    fn fenced(body: &str) -> String {
        format!("```evo-ui\n{body}\n```")
    }

    #[test]
    fn ui_blocks_complete_fences_become_their_title_or_type() {
        let text = format!(
            "Here is the week.\n\n{}\n\nCosts fell.\n{}\nDone.",
            fenced(STATS),
            fenced(r#"{"v":1,"type":"bars","items":[{"label":"a","value":1}]}"#)
        );
        assert_eq!(
            preview_text(&text),
            "Here is the week.\n\n[Usage this week]\n\nCosts fell.\n[bars]\nDone."
        );
    }

    #[test]
    fn ui_blocks_unreadable_fences_fall_back() {
        // Invalid JSON: the type is sniffed; nothing to sniff: `[block]`.
        let cut = r#"{"v":1, "type" : "table", "rows":["#;
        assert_eq!(preview_text(&fenced(cut)), "[table]");
        assert_eq!(preview_text(&fenced("not json")), "[block]");
        // An unknown type is not a name; an empty title is no title.
        assert_eq!(
            preview_text(&fenced(r#"{"v":1,"type":"sparkle","title":"  "}"#)),
            "[block]"
        );
        assert_eq!(
            preview_text(&fenced(r#"{"v":1,"type":"chart","title":""}"#)),
            "[chart]"
        );
        // A title wins even when the block would not draw.
        assert_eq!(
            preview_text(&fenced(r#"{"title":"Plan","type":"steps"}"#)),
            "[Plan]"
        );
    }

    #[test]
    fn ui_blocks_unclosed_fence_at_the_end() {
        assert_eq!(
            preview_text("Look:\n```evo-ui\n{\"v\":1,\"type\":\"chart\",\"x\":[1,2"),
            "Look:\n[chart]"
        );
        assert_eq!(preview_text("Look:\n```evo-ui\n{\"v\":1,\"ty"), "Look:\n");
        assert_eq!(preview_text("Look:\n```evo-ui"), "Look:\n");
    }

    #[test]
    fn ui_blocks_fences_follow_markdown_rules() {
        // Tildes, longer markers, indent up to 3, CRLF line ends.
        let text = format!("a\r\n   ~~~~ evo-ui \r\n{STATS}\r\n~~~~~\r\nb");
        assert_eq!(preview_text(&text), "a\n[Usage this week]\nb");
        // A shorter or other marker does not close the fence (so it is
        // part of the body, which no longer parses: the type is sniffed).
        let text = format!("````evo-ui\n{STATS}\n```\n~~~~\n````\nafter");
        assert_eq!(preview_text(&text), "[stats]\nafter");
        // Four spaces is code, not a fence.
        let text = format!("    ```evo-ui\n{STATS}");
        assert_eq!(preview_text(&text), text);
        // Other info strings stay as they are.
        let text = format!("```json\n{STATS}\n```");
        assert_eq!(preview_text(&text), text);
    }

    #[test]
    fn ui_blocks_inside_other_fences_are_left_alone() {
        let text = format!(
            "````markdown\n{}\n````\nThen\n{}",
            fenced(STATS),
            fenced(STATS)
        );
        assert_eq!(
            preview_text(&text),
            format!(
                "````markdown\n{}\n````\nThen\n[Usage this week]",
                fenced(STATS)
            )
        );
    }

    #[test]
    fn ui_blocks_text_without_blocks_is_unchanged() {
        for text in ["", "plain", "a ```evo-ui inline mention", "```\ncode\n```"] {
            assert_eq!(preview_text(text), text);
        }
        assert_eq!(sniff_type(r#""type" "stats""#), None);
        assert_eq!(sniff_type(r#""type":1, "type":"steps""#), Some("steps"));
    }
}
