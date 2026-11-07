//! Page dialogs (alert, confirm, prompt, beforeunload) waiting for an answer
//! (agent browser, FR-4). On Windows the shell holds the WebView2 dialog open
//! with a deferral; this module is the bookkeeping around it: at most one
//! dialog per tab, answered by the caller or dismissed after
//! [`DIALOG_TTL_MS`] so a forgotten one never wedges the page. The clock is
//! passed in (milliseconds on any monotonic scale), which keeps the state
//! machine testable.

use std::collections::HashMap;

use serde::Serialize;

/// A dialog nobody answers is dismissed after this long.
pub const DIALOG_TTL_MS: u64 = 60_000;

/// What the person or the agent decided.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub accept: bool,
    pub text: Option<String>,
}

/// What `browser_dialog_pending` returns.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogInfo {
    /// alert | confirm | prompt | beforeunload
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_text: Option<String>,
    pub url: String,
}

struct Slot<T> {
    id: u64,
    info: DialogInfo,
    opened_ms: u64,
    token: T,
}

/// Open dialogs by tab. `T` is what completes one (the shell's deferral).
pub struct Dialogs<T> {
    slots: HashMap<String, Slot<T>>,
    next_id: u64,
}

impl<T> Default for Dialogs<T> {
    fn default() -> Self {
        Self { slots: HashMap::new(), next_id: 0 }
    }
}

impl<T> Dialogs<T> {
    /// A dialog opened on `tab`. Returns its id, and the token of a dialog that
    /// was still open there (a page cannot show two; the old one is dismissed).
    pub fn open(&mut self, tab: &str, info: DialogInfo, now_ms: u64, token: T) -> (u64, Option<T>) {
        self.next_id += 1;
        let id = self.next_id;
        let displaced = self.slots.insert(tab.to_string(), Slot { id, info, opened_ms: now_ms, token });
        (id, displaced.map(|slot| slot.token))
    }

    pub fn pending(&self, tab: &str) -> Option<&DialogInfo> {
        self.slots.get(tab).map(|slot| &slot.info)
    }

    /// Take the dialog on `tab` with its answer: an accepted prompt without
    /// text takes the prompt's default text. `None` when nothing is open.
    pub fn answer(&mut self, tab: &str, accept: bool, text: Option<String>) -> Option<(T, Answer)> {
        let slot = self.slots.remove(tab)?;
        let text = if accept && slot.info.kind == "prompt" { text.or(slot.info.default_text) } else { text };
        Some((slot.token, Answer { accept, text: if accept { text } else { None } }))
    }

    /// Dialogs open for at least [`DIALOG_TTL_MS`] at `now_ms`, removed:
    /// (tab, token), to be dismissed. (The shell uses [`Self::expire_one`], one
    /// timer per dialog.)
    #[cfg(test)]
    pub fn expire(&mut self, now_ms: u64) -> Vec<(String, T)> {
        let old: Vec<String> = self
            .slots
            .iter()
            .filter(|(_, slot)| now_ms.saturating_sub(slot.opened_ms) >= DIALOG_TTL_MS)
            .map(|(tab, _)| tab.clone())
            .collect();
        old.into_iter().filter_map(|tab| self.slots.remove(&tab).map(|slot| (tab, slot.token))).collect()
    }

    /// Expire only dialog `id` on `tab` (the timer of that very dialog), so a
    /// timer from an earlier dialog never dismisses a newer one.
    pub fn expire_one(&mut self, tab: &str, id: u64, now_ms: u64) -> Option<T> {
        let slot = self.slots.get(tab)?;
        if slot.id != id || now_ms.saturating_sub(slot.opened_ms) < DIALOG_TTL_MS {
            return None;
        }
        self.slots.remove(tab).map(|slot| slot.token)
    }

    /// The tab closed: forget its dialog, returning the token to dismiss.
    pub fn clear(&mut self, tab: &str) -> Option<T> {
        self.slots.remove(tab).map(|slot| slot.token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(kind: &str, default_text: Option<&str>) -> DialogInfo {
        DialogInfo {
            kind: kind.to_string(),
            message: "sure?".to_string(),
            default_text: default_text.map(str::to_string),
            url: "https://example.com/".to_string(),
        }
    }

    #[test]
    fn a_pending_dialog_is_answered_once() {
        let mut dialogs = Dialogs::default();
        assert!(dialogs.pending("browser-1").is_none());
        let (_, displaced) = dialogs.open("browser-1", info("confirm", None), 0, "token");
        assert!(displaced.is_none());
        assert_eq!(dialogs.pending("browser-1").unwrap().kind, "confirm");
        assert!(dialogs.pending("browser-2").is_none());

        let (token, answer) = dialogs.answer("browser-1", true, None).unwrap();
        assert_eq!(token, "token");
        assert_eq!(answer, Answer { accept: true, text: None });
        assert!(dialogs.answer("browser-1", true, None).is_none(), "no dialog is pending any more");
    }

    #[test]
    fn a_prompt_takes_the_given_text_or_its_default() {
        let mut dialogs = Dialogs::default();
        dialogs.open("t", info("prompt", Some("anon")), 0, 1);
        let (_, answer) = dialogs.answer("t", true, Some("alice".into())).unwrap();
        assert_eq!(answer.text.as_deref(), Some("alice"));

        dialogs.open("t", info("prompt", Some("anon")), 0, 2);
        let (_, answer) = dialogs.answer("t", true, None).unwrap();
        assert_eq!(answer.text.as_deref(), Some("anon"));

        dialogs.open("t", info("prompt", Some("anon")), 0, 3);
        let (_, answer) = dialogs.answer("t", false, Some("ignored".into())).unwrap();
        assert_eq!(answer, Answer { accept: false, text: None });
    }

    #[test]
    fn a_second_dialog_on_a_tab_displaces_the_first() {
        let mut dialogs = Dialogs::default();
        dialogs.open("t", info("alert", None), 0, "first");
        let (_, displaced) = dialogs.open("t", info("confirm", None), 5, "second");
        assert_eq!(displaced, Some("first"));
        assert_eq!(dialogs.pending("t").unwrap().kind, "confirm");
    }

    #[test]
    fn a_forgotten_dialog_expires_after_sixty_seconds() {
        let mut dialogs = Dialogs::default();
        dialogs.open("old", info("alert", None), 1_000, "old-token");
        dialogs.open("new", info("alert", None), 30_000, "new-token");

        assert!(dialogs.expire(60_999).is_empty(), "one millisecond early");
        assert!(dialogs.pending("old").is_some());

        let expired = dialogs.expire(61_000);
        assert_eq!(expired, vec![("old".to_string(), "old-token")]);
        assert!(dialogs.pending("old").is_none());
        assert!(dialogs.pending("new").is_some(), "the newer dialog keeps waiting");

        let expired = dialogs.expire(90_000);
        assert_eq!(expired, vec![("new".to_string(), "new-token")]);
    }

    #[test]
    fn a_timer_only_dismisses_its_own_dialog() {
        let mut dialogs = Dialogs::default();
        let (first, _) = dialogs.open("t", info("alert", None), 0, "first");
        dialogs.answer("t", true, None);
        let (second, _) = dialogs.open("t", info("alert", None), 50_000, "second");
        assert_ne!(first, second);
        // The first dialog's timer fires at 60 s: the second is only 10 s old.
        assert_eq!(dialogs.expire_one("t", first, 60_000), None);
        assert!(dialogs.pending("t").is_some());
        assert_eq!(dialogs.expire_one("t", second, 109_999), None);
        assert_eq!(dialogs.expire_one("t", second, 110_000), Some("second"));
    }

    #[test]
    fn closing_the_tab_clears_its_dialog() {
        let mut dialogs = Dialogs::default();
        dialogs.open("t", info("beforeunload", None), 0, "token");
        assert_eq!(dialogs.clear("t"), Some("token"));
        assert_eq!(dialogs.clear("t"), None);
        assert!(dialogs.pending("t").is_none());
    }

    #[test]
    fn pending_serializes_in_camel_case_without_empty_default_text() {
        let json = serde_json::to_value(info("prompt", Some("x"))).unwrap();
        assert_eq!(json["defaultText"], "x");
        assert_eq!(json["kind"], "prompt");
        let json = serde_json::to_value(info("alert", None)).unwrap();
        assert!(json.get("defaultText").is_none());
    }
}
