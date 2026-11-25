//! What a mate's connector call asks (docs/features/evo-mates/spec.md, FR-41,
//! FR-42).
//!
//! A tool its server marks read-only reads; any other counts as sending
//! (it may send, post, pay or delete). A reading tool may get **Always allow
//! for this mate** (kept in `mate_grants`) or **Allow for this conversation**
//! (kept until the app quits). A sending tool never gets Always allow, and
//! under the mate's **Sending on your behalf: Ask every time** it asks on
//! every call, whatever its permission, approval mode or earlier answers.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// One answer on a mate's approval card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Once,
    Conversation,
    Mate,
    Deny,
}

impl Pick {
    pub fn label(self, mate: &str) -> String {
        match self {
            Pick::Once => "Allow once".into(),
            Pick::Conversation => "Allow for this conversation".into(),
            Pick::Mate => format!("Always allow for {mate}"),
            Pick::Deny => "Deny".into(),
        }
    }
}

/// What a call does before it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Run,
    /// Ask with these answers; `sending` marks the "on your behalf" card.
    Ask {
        picks: Vec<Pick>,
        sending: bool,
    },
}

/// What a mate's call of a connector tool does first: `read_only` from its
/// server's annotations, `asks` from the tool's permission, `sending` the
/// mate's setting (`ask` or `send`), and whether it was allowed for the mate
/// or this conversation.
pub fn plan(
    read_only: bool,
    asks: bool,
    sending: &str,
    for_mate: bool,
    for_conversation: bool,
) -> Plan {
    if read_only {
        if !asks || for_mate || for_conversation {
            return Plan::Run;
        }
        return Plan::Ask {
            picks: vec![Pick::Once, Pick::Conversation, Pick::Mate, Pick::Deny],
            sending: false,
        };
    }
    if sending != "send" {
        return Plan::Ask {
            picks: vec![Pick::Once, Pick::Deny],
            sending: true,
        };
    }
    if !asks || for_conversation {
        return Plan::Run;
    }
    Plan::Ask {
        picks: vec![Pick::Once, Pick::Conversation, Pick::Deny],
        sending: false,
    }
}

fn conversation() -> &'static Mutex<HashSet<(String, String)>> {
    static ALLOWED: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    ALLOWED.get_or_init(Default::default)
}

/// Whether `tool` was allowed for the rest of conversation `chat`.
pub fn allowed_here(chat: &str, tool: &str) -> bool {
    conversation()
        .lock()
        .expect("grants lock")
        .contains(&(chat.to_string(), tool.to_string()))
}

pub fn allow_here(chat: &str, tool: &str) {
    conversation()
        .lock()
        .expect("grants lock")
        .insert((chat.to_string(), tool.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_tool_offers_always_allow_and_honours_grants() {
        assert_eq!(plan(true, false, "ask", false, false), Plan::Run);
        assert_eq!(
            plan(true, true, "ask", false, false),
            Plan::Ask {
                picks: vec![Pick::Once, Pick::Conversation, Pick::Mate, Pick::Deny],
                sending: false
            }
        );
        assert_eq!(plan(true, true, "ask", true, false), Plan::Run);
        assert_eq!(plan(true, true, "ask", false, true), Plan::Run);
    }

    #[test]
    fn a_sending_tool_never_offers_always_allow() {
        // Ask every time: every call asks, whatever was allowed before.
        for (asks, mate, here) in [
            (false, false, false),
            (true, true, true),
            (false, true, true),
        ] {
            assert_eq!(
                plan(false, asks, "ask", mate, here),
                Plan::Ask {
                    picks: vec![Pick::Once, Pick::Deny],
                    sending: true
                }
            );
        }
        // Send without asking: the tool's own permission, never Always allow.
        assert_eq!(plan(false, false, "send", false, false), Plan::Run);
        assert_eq!(
            plan(false, true, "send", false, false),
            Plan::Ask {
                picks: vec![Pick::Once, Pick::Conversation, Pick::Deny],
                sending: false
            }
        );
        assert_eq!(
            plan(false, true, "send", true, false),
            Plan::Ask {
                picks: vec![Pick::Once, Pick::Conversation, Pick::Deny],
                sending: false
            }
        );
        assert_eq!(plan(false, true, "send", false, true), Plan::Run);
    }

    #[test]
    fn conversation_grants_are_per_chat() {
        allow_here("chat-a", "mcp__mail__read");
        assert!(allowed_here("chat-a", "mcp__mail__read"));
        assert!(!allowed_here("chat-b", "mcp__mail__read"));
    }
}
