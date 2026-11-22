//! The person's own words survive compaction.
//!
//! The summary model paraphrases: after a compaction the agent only has its
//! reading of what was asked, and constraints worded once at the start ("only
//! these eight folders", "never commit") blur or go missing. So the person's
//! messages in the part being summarised are kept word for word, in a block
//! after the summary ([`render`]). The next compaction takes the block out of
//! the previous summary before the model rewrites it ([`split`]) and carries
//! its messages on, so they pass through every later compaction unchanged.
//!
//! The block is bounded ([`MAX_EACH`], [`MAX_TOTAL`]): the first message (the
//! original request) is always kept, then the most recent ones that fit.

use evo_agent::message::AgentMessage;
use evo_ai::types::{Content, UserContent};

const OPEN: &str = "<user-messages>";
const CLOSE: &str = "</user-messages>";
const ITEM_OPEN: &str = "<message>";
const ITEM_CLOSE: &str = "</message>";

/// Characters kept of one message.
pub const MAX_EACH: usize = 2_000;
/// Characters kept of all messages together (the first one always).
pub const MAX_TOTAL: usize = 6_000;

/// `summary` without its block of kept messages, and those messages.
pub fn split(summary: &str) -> (String, Vec<String>) {
    let Some(start) = summary.rfind(OPEN) else {
        return (summary.to_string(), Vec::new());
    };
    let Some(len) = summary[start..].find(CLOSE) else {
        return (summary.to_string(), Vec::new());
    };
    let block = &summary[start + OPEN.len()..start + len];
    let messages = block
        .split(ITEM_OPEN)
        .skip(1)
        .filter_map(|item| item.split(ITEM_CLOSE).next())
        .map(|item| unescape(item.trim_matches('\n')))
        .collect();
    let rest = format!(
        "{}{}",
        summary[..start].trim_end(),
        &summary[start + len + CLOSE.len()..]
    );
    (rest, messages)
}

/// The person's messages among `messages`, oldest first: the text of each
/// user message, without the notes the engine puts in the user role (a text
/// that starts with a tag, such as a background agent's report).
pub fn of(messages: &[AgentMessage]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::User(user) => Some(text_of(&user.content)),
            _ => None,
        })
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty() && !is_note(text))
        .collect()
}

/// The block for `messages` (oldest first), or "" when there are none.
pub fn render(messages: &[String]) -> String {
    let kept = within_budget(messages);
    if kept.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "\n\n{OPEN}\nThe person's own messages from the summarised part of this conversation, word for word. \
         The summary above is a paraphrase; where they differ, these words and the constraints in them hold."
    );
    for item in kept {
        match item {
            Kept::Message(text) => {
                out.push_str(&format!("\n{ITEM_OPEN}\n{}\n{ITEM_CLOSE}", escape(text)))
            }
            Kept::Gap(count) => out.push_str(&format!(
                "\n({count} message{} left out for length.)",
                if count == 1 { "" } else { "s" }
            )),
        }
    }
    out.push('\n');
    out.push_str(CLOSE);
    out
}

enum Kept<'a> {
    Message(&'a str),
    Gap(usize),
}

/// The first message, then the most recent ones that fit [`MAX_TOTAL`] (each
/// cut to [`MAX_EACH`] already), in order, with a gap where some were left out.
fn within_budget(messages: &[String]) -> Vec<Kept<'_>> {
    let Some((first, rest)) = messages.split_first() else {
        return Vec::new();
    };
    let mut used = first.chars().count();
    let mut tail = Vec::new();
    for message in rest.iter().rev() {
        let len = message.chars().count();
        if used + len > MAX_TOTAL {
            break;
        }
        used += len;
        tail.push(message.as_str());
    }
    tail.reverse();
    let mut kept = vec![Kept::Message(first.as_str())];
    let left_out = rest.len() - tail.len();
    if left_out > 0 {
        kept.push(Kept::Gap(left_out));
    }
    kept.extend(tail.into_iter().map(Kept::Message));
    kept
}

/// `text` cut to [`MAX_EACH`] characters, saying so.
pub fn cut(text: &str) -> String {
    if text.chars().count() <= MAX_EACH {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_EACH).collect();
    format!("{kept} … (cut)")
}

fn text_of(content: &UserContent) -> String {
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

/// A text the engine wrote in the user role: it opens with a tag
/// (`<background-agent id=…>`, `<session-note …>`).
fn is_note(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next() == Some('<')
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && text.lines().next().is_some_and(|line| line.contains('>'))
}

/// The block's own tags made inert inside a message, so a message cannot end
/// its item or the block.
fn escape(text: &str) -> String {
    text.replace("</message>", "&lt;/message>")
        .replace("</user-messages>", "&lt;/user-messages>")
}

fn unescape(text: &str) -> String {
    text.replace("&lt;/message>", "</message>")
        .replace("&lt;/user-messages>", "</user-messages>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_ai::types::UserMessage;

    fn user(text: &str) -> AgentMessage {
        AgentMessage::from(UserMessage::new(text.to_string(), 0))
    }

    #[test]
    fn a_block_round_trips_with_its_tags_made_inert() {
        let messages = vec![
            "Only touch the 8 changes under openspec/changes/<mine>.".to_string(),
            "Never commit.\nAnd a tricky </message> inside.".to_string(),
        ];
        let summary = format!("## Goal\nDo it.{}", render(&messages));
        let (rest, back) = split(&summary);
        assert_eq!(rest, "## Goal\nDo it.");
        assert_eq!(back, messages);
    }

    #[test]
    fn a_summary_without_a_block_is_left_alone() {
        assert_eq!(split("## Goal\nx"), ("## Goal\nx".to_string(), Vec::new()));
        assert_eq!(render(&[]), "");
    }

    #[test]
    fn only_the_persons_messages_are_kept() {
        let messages = vec![
            user("Fix the parser."),
            user("<background-agent id=\"a1\" status=\"done\">\nreport\n</background-agent>"),
            user("   "),
            user("<b>bold</b> is fine too? no: it opens with a tag"),
            user("Use tabs."),
        ];
        assert_eq!(
            of(&messages),
            vec!["Fix the parser.".to_string(), "Use tabs.".to_string()]
        );
    }

    #[test]
    fn the_first_message_and_the_latest_that_fit_are_kept() {
        let long = "x".repeat(MAX_EACH);
        let mut messages = vec!["The original request.".to_string()];
        messages.extend((0..5).map(|n| format!("{n}{long}")));
        let block = render(&messages);
        assert!(block.contains("The original request."));
        // 6000 characters: the first, then the two latest of 2001 each.
        assert!(
            block.contains("(3 messages left out for length.)"),
            "{block}"
        );
        assert!(block.contains(&format!("4{long}")) && block.contains(&format!("3{long}")));
        assert!(!block.contains(&format!("2{long}")));
        assert_eq!(cut(&format!("{long}yz")), format!("{long} … (cut)"));
    }
}
