//! Which chat each browser page belongs to (docs/features/agent-browser T6,
//! FR-24; docs/features/session-panels FR-2, FR-5): the page registry the
//! agent tools resolve a `tabId` against. Pure bookkeeping: the clock for the
//! agent-driven marker is passed in, which keeps it testable.
//!
//! A page has an owner window (it streams events there) and a chat. The shell
//! remembers the visible page of each chat (the webview reports it, tools move
//! it), which pages the webview currently shows, and the last bounds the side
//! panel reported, so a page a tool creates gets sensible bounds.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// How long a page counts as driven by an agent after a tool last used it.
pub const DRIVEN_MS: u64 = 120_000;

/// Where pages show, in window logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    /// Used for a page created before the panel ever reported bounds.
    pub const FALLBACK: Bounds = Bounds { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
}

/// What the shell knows about one page.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    /// The window that owns the page (receives its events).
    pub window: String,
    /// The chat the page belongs to; `None` when unknown (a page the webview made with no chat showing).
    pub chat: Option<String>,
    pub url: String,
    pub title: String,
    pub loading: bool,
}

#[derive(Default)]
pub struct Tabs {
    tabs: HashMap<String, Tab>,
    /// Chat → the page it shows.
    visible: HashMap<String, String>,
    /// Pages the webview made visible (the panel shows them).
    shown: HashSet<String>,
    bounds: Option<Bounds>,
    /// Grows with every bounds report, so a waiter can tell a fresh one arrived.
    bounds_seq: u64,
    /// Page → when a tool last used it (ms on the caller's clock).
    driven: HashMap<String, u64>,
    /// Pages the window was told an agent is involved with (once each).
    announced: HashSet<String>,
    /// Pages on the stage (stage.rs): the panel no longer places or hides them.
    staged: HashSet<String>,
}

/// `browser-12` → 12 (unknown labels sort first).
fn number(label: &str) -> u32 {
    label.trim_start_matches(super::TAB_PREFIX).parse().unwrap_or(0)
}

impl Tabs {
    pub fn insert(&mut self, label: &str, window: &str, chat: Option<String>, url: &str) {
        self.tabs.insert(
            label.to_string(),
            Tab { window: window.to_string(), chat, url: url.to_string(), title: String::new(), loading: true },
        );
    }

    /// Forget a page; the visible-page entries that pointed at it go too.
    pub fn remove(&mut self, label: &str) -> Option<Tab> {
        self.shown.remove(label);
        self.driven.remove(label);
        self.announced.remove(label);
        self.staged.remove(label);
        self.visible.retain(|_, tab| tab != label);
        self.tabs.remove(label)
    }

    pub fn contains(&self, label: &str) -> bool {
        self.tabs.contains_key(label)
    }

    pub fn get(&self, label: &str) -> Option<&Tab> {
        self.tabs.get(label)
    }

    pub fn window_of(&self, label: &str) -> Option<&str> {
        self.tabs.get(label).map(|tab| tab.window.as_str())
    }

    /// Labels of the pages a window owns.
    pub fn labels_of_window(&self, window: &str) -> Vec<String> {
        self.tabs.iter().filter(|(_, tab)| tab.window == window).map(|(label, _)| label.clone()).collect()
    }

    /// A window's pages, oldest first.
    pub fn of_window(&self, window: &str) -> Vec<(String, Tab)> {
        let mut list: Vec<(String, Tab)> =
            self.tabs.iter().filter(|(_, tab)| tab.window == window).map(|(label, tab)| (label.clone(), tab.clone())).collect();
        list.sort_by_key(|(label, _)| number(label));
        list
    }

    /// A chat's pages, oldest first.
    pub fn of_chat(&self, chat: &str) -> Vec<(String, Tab)> {
        let mut list: Vec<(String, Tab)> = self
            .tabs
            .iter()
            .filter(|(_, tab)| tab.chat.as_deref() == Some(chat))
            .map(|(label, tab)| (label.clone(), tab.clone()))
            .collect();
        list.sort_by_key(|(label, _)| number(label));
        list
    }

    /// Make `tab` the page `chat` shows. A page of no known chat joins `chat`
    /// (the first chat shown after a reload adopts it); a page of another chat
    /// is refused.
    pub fn select(&mut self, chat: &str, tab: &str) -> bool {
        let Some(entry) = self.tabs.get_mut(tab) else {
            return false;
        };
        match entry.chat.as_deref() {
            Some(owner) if owner != chat => return false,
            Some(_) => {}
            None => entry.chat = Some(chat.to_string()),
        }
        self.visible.insert(chat.to_string(), tab.to_string());
        true
    }

    /// Give a page of no known chat to `chat` (the webview adopted it after a reload). False when the
    /// page is unknown or already belongs to a chat.
    pub fn assign_chat(&mut self, tab: &str, chat: &str) -> bool {
        match self.tabs.get_mut(tab) {
            Some(entry) if entry.chat.is_none() => {
                entry.chat = Some(chat.to_string());
                true
            }
            _ => false,
        }
    }

    /// The page `chat` shows: the one selected while it still exists, else its newest.
    pub fn visible_of(&self, chat: &str) -> Option<String> {
        if let Some(tab) = self.visible.get(chat) {
            if self.tabs.get(tab).is_some_and(|t| t.chat.as_deref() == Some(chat)) {
                return Some(tab.clone());
            }
        }
        self.of_chat(chat).pop().map(|(label, _)| label)
    }

    /// The tab a tool call works on: `tab_id` when it is one of `chat`'s pages
    /// (a page of another chat is "no tab"), else the chat's visible page.
    pub fn resolve(&self, chat: &str, tab_id: Option<&str>) -> Result<String, String> {
        match tab_id.filter(|id| !id.is_empty()) {
            Some(id) => match self.tabs.get(id) {
                Some(tab) if tab.chat.as_deref() == Some(chat) => Ok(id.to_string()),
                _ => Err(format!("No browser tab {id}; see browser_tabs.")),
            },
            None => self.visible_of(chat).ok_or_else(|| "No browser tab is open. Use browser_navigate first.".to_string()),
        }
    }

    pub fn set_loading(&mut self, label: &str, loading: bool) {
        if let Some(tab) = self.tabs.get_mut(label) {
            tab.loading = loading;
        }
    }

    pub fn set_url(&mut self, label: &str, url: &str) {
        if let Some(tab) = self.tabs.get_mut(label) {
            tab.url = url.to_string();
        }
    }

    pub fn set_title(&mut self, label: &str, title: &str) {
        if let Some(tab) = self.tabs.get_mut(label) {
            tab.title = title.to_string();
        }
    }

    pub fn set_shown(&mut self, label: &str, shown: bool) {
        if shown {
            if self.tabs.contains_key(label) {
                self.shown.insert(label.to_string());
            }
        } else {
            self.shown.remove(label);
        }
    }

    pub fn is_shown(&self, label: &str) -> bool {
        self.shown.contains(label)
    }

    /// Whether the panel shows any page of `chat`.
    pub fn any_shown_of(&self, chat: &str) -> bool {
        self.shown.iter().any(|label| self.tabs.get(label).is_some_and(|t| t.chat.as_deref() == Some(chat)))
    }

    pub fn set_bounds(&mut self, bounds: Bounds) {
        self.bounds = Some(bounds);
        self.bounds_seq += 1;
    }

    pub fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    pub fn bounds_seq(&self) -> u64 {
        self.bounds_seq
    }

    /// A tool used `tab` at `now_ms`. True the first time for a page: the
    /// window learns that an agent is involved with it (`browser://driven`).
    pub fn mark_driven(&mut self, tab: &str, now_ms: u64) -> bool {
        if !self.tabs.contains_key(tab) {
            return false;
        }
        self.driven.insert(tab.to_string(), now_ms);
        self.announced.insert(tab.to_string())
    }

    /// The chat a page belongs to.
    pub fn chat_of(&self, tab: &str) -> Option<&str> {
        self.tabs.get(tab).and_then(|t| t.chat.as_deref())
    }

    pub fn set_staged(&mut self, tab: &str, staged: bool) {
        if staged && self.tabs.contains_key(tab) {
            self.staged.insert(tab.to_string());
        } else {
            self.staged.remove(tab);
        }
    }

    pub fn is_staged(&self, tab: &str) -> bool {
        self.staged.contains(tab)
    }

    /// Whether a tool used `tab` within [`DRIVEN_MS`] of `now_ms`.
    pub fn is_driven(&self, tab: &str, now_ms: u64) -> bool {
        self.driven.get(tab).is_some_and(|at| now_ms.saturating_sub(*at) < DRIVEN_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs() -> Tabs {
        let mut tabs = Tabs::default();
        tabs.insert("browser-1", "main", Some("chat-a".into()), "https://a.test/");
        tabs.insert("browser-2", "main", Some("chat-b".into()), "https://b.test/");
        tabs.insert("browser-10", "main", Some("chat-a".into()), "https://a2.test/");
        tabs
    }

    #[test]
    fn pages_of_a_chat_come_oldest_first_by_number_not_by_text() {
        let tabs = tabs();
        let labels: Vec<String> = tabs.of_chat("chat-a").into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels, ["browser-1", "browser-10"]);
        assert!(tabs.of_chat("nobody").is_empty());
        assert_eq!(tabs.of_window("main").len(), 3);
        assert!(tabs.of_window("other").is_empty());
    }

    #[test]
    fn the_visible_page_defaults_to_the_newest_and_follows_the_selection() {
        let mut tabs = tabs();
        assert_eq!(tabs.visible_of("chat-a").as_deref(), Some("browser-10"));
        assert!(tabs.select("chat-a", "browser-1"));
        assert_eq!(tabs.visible_of("chat-a").as_deref(), Some("browser-1"));
        assert_eq!(tabs.visible_of("chat-b").as_deref(), Some("browser-2"), "each chat has its own");
        // A page of another chat cannot be made visible, an unknown page neither.
        assert!(!tabs.select("chat-a", "browser-2"));
        assert!(!tabs.select("chat-a", "browser-99"));
        assert_eq!(tabs.visible_of("chat-a").as_deref(), Some("browser-1"));
    }

    #[test]
    fn closing_the_visible_page_falls_back_to_the_newest_left() {
        let mut tabs = tabs();
        tabs.select("chat-a", "browser-10");
        assert!(tabs.remove("browser-10").is_some());
        assert_eq!(tabs.visible_of("chat-a").as_deref(), Some("browser-1"));
        tabs.remove("browser-1");
        assert_eq!(tabs.visible_of("chat-a"), None);
        assert!(tabs.remove("browser-1").is_none());
    }

    #[test]
    fn a_page_of_unknown_chat_joins_the_first_chat_that_selects_it() {
        let mut tabs = Tabs::default();
        tabs.insert("browser-3", "main", None, "https://x.test/");
        assert_eq!(tabs.visible_of("chat-a"), None, "a page of no chat is nobody's");
        assert!(tabs.resolve("chat-a", Some("browser-3")).is_err());
        assert!(tabs.select("chat-a", "browser-3"));
        assert_eq!(tabs.get("browser-3").unwrap().chat.as_deref(), Some("chat-a"));
        assert_eq!(tabs.resolve("chat-a", None).unwrap(), "browser-3");
        assert!(!tabs.select("chat-b", "browser-3"), "now it is chat-a's");
    }

    #[test]
    fn only_a_page_of_unknown_chat_can_be_assigned_one() {
        let mut tabs = tabs();
        tabs.insert("browser-5", "main", None, "https://x.test/");
        assert!(tabs.assign_chat("browser-5", "chat-a"));
        assert!(!tabs.assign_chat("browser-5", "chat-b"), "it has a chat now");
        assert!(!tabs.assign_chat("browser-1", "chat-b"), "so did this one");
        assert!(!tabs.assign_chat("browser-99", "chat-a"));
        assert_eq!(tabs.of_chat("chat-a").len(), 3);
        assert_eq!(tabs.visible_of("chat-a").as_deref(), Some("browser-10"), "assigning does not change what is visible");
    }

    #[test]
    fn a_tab_id_resolves_for_its_own_chat_only() {
        let tabs = tabs();
        assert_eq!(tabs.resolve("chat-a", Some("browser-1")).unwrap(), "browser-1");
        assert_eq!(tabs.resolve("chat-a", None).unwrap(), "browser-10");
        assert_eq!(tabs.resolve("chat-a", Some("")).unwrap(), "browser-10", "an empty id means the visible page");
        assert_eq!(tabs.resolve("chat-a", Some("browser-2")).unwrap_err(), "No browser tab browser-2; see browser_tabs.");
        assert_eq!(tabs.resolve("chat-a", Some("browser-77")).unwrap_err(), "No browser tab browser-77; see browser_tabs.");
        assert_eq!(tabs.resolve("chat-c", None).unwrap_err(), "No browser tab is open. Use browser_navigate first.");
    }

    #[test]
    fn what_the_panel_shows_is_tracked_per_page_and_per_chat() {
        let mut tabs = tabs();
        assert!(!tabs.any_shown_of("chat-a"));
        tabs.set_shown("browser-1", true);
        tabs.set_shown("browser-99", true);
        assert!(tabs.is_shown("browser-1"));
        assert!(!tabs.is_shown("browser-99"), "an unknown page is never shown");
        assert!(tabs.any_shown_of("chat-a"));
        assert!(!tabs.any_shown_of("chat-b"));
        tabs.remove("browser-1");
        assert!(!tabs.any_shown_of("chat-a"));
        tabs.set_shown("browser-10", true);
        tabs.set_shown("browser-10", false);
        assert!(!tabs.any_shown_of("chat-a"));
    }

    #[test]
    fn bounds_are_remembered_and_every_report_is_counted() {
        let mut tabs = Tabs::default();
        assert_eq!((tabs.bounds(), tabs.bounds_seq()), (None, 0));
        let one = Bounds { x: 10.0, y: 20.0, width: 300.0, height: 400.0 };
        tabs.set_bounds(one);
        tabs.set_bounds(one);
        assert_eq!((tabs.bounds(), tabs.bounds_seq()), (Some(one), 2), "an identical report still counts: it says the panel is there");
    }

    #[test]
    fn the_window_hears_once_that_an_agent_is_involved_with_a_page() {
        let mut tabs = Tabs::default();
        tabs.insert("browser-1", "main", Some("chat-a".into()), "about:blank");
        assert!(tabs.mark_driven("browser-1", 10), "the first use is announced");
        assert!(!tabs.mark_driven("browser-1", 20), "the second is not");
        assert!(!tabs.mark_driven("browser-9", 30), "an unknown page is not");
        assert_eq!(tabs.chat_of("browser-1"), Some("chat-a"));
        assert_eq!(tabs.chat_of("browser-9"), None);
        // A page that closed and a new one with the same label would be announced again.
        tabs.remove("browser-1");
        tabs.insert("browser-1", "main", None, "about:blank");
        assert!(tabs.mark_driven("browser-1", 40));
    }

    #[test]
    fn a_staged_page_is_remembered_until_it_closes() {
        let mut tabs = Tabs::default();
        tabs.insert("browser-1", "main", None, "about:blank");
        assert!(!tabs.is_staged("browser-1"));
        tabs.set_staged("browser-1", true);
        assert!(tabs.is_staged("browser-1"));
        tabs.set_staged("browser-9", true);
        assert!(!tabs.is_staged("browser-9"), "an unknown page cannot be staged");
        tabs.set_staged("browser-1", false);
        assert!(!tabs.is_staged("browser-1"));
        tabs.set_staged("browser-1", true);
        tabs.remove("browser-1");
        assert!(!tabs.is_staged("browser-1"));
    }

    #[test]
    fn a_page_is_driven_for_two_minutes_after_a_tool_used_it() {
        let mut tabs = tabs();
        assert!(!tabs.is_driven("browser-1", 5_000));
        tabs.mark_driven("browser-1", 5_000);
        assert!(tabs.is_driven("browser-1", 5_000));
        assert!(tabs.is_driven("browser-1", 5_000 + DRIVEN_MS - 1));
        assert!(!tabs.is_driven("browser-1", 5_000 + DRIVEN_MS));
        assert!(!tabs.is_driven("browser-2", 5_000), "another page is not");
        // Using it again restarts the two minutes.
        tabs.mark_driven("browser-1", 100_000);
        assert!(tabs.is_driven("browser-1", 100_000 + DRIVEN_MS - 1));
        // A closed page forgets it; an unknown one is never marked.
        tabs.remove("browser-1");
        assert!(!tabs.is_driven("browser-1", 100_001));
        tabs.mark_driven("browser-99", 1);
        assert!(!tabs.is_driven("browser-99", 1));
    }

    #[test]
    fn load_state_url_and_title_follow_the_page_events() {
        let mut tabs = tabs();
        assert!(tabs.get("browser-1").unwrap().loading, "a new page is loading");
        tabs.set_loading("browser-1", false);
        tabs.set_url("browser-1", "https://a.test/next");
        tabs.set_title("browser-1", "Next");
        let tab = tabs.get("browser-1").unwrap();
        assert_eq!((tab.loading, tab.url.as_str(), tab.title.as_str()), (false, "https://a.test/next", "Next"));
        tabs.set_loading("browser-99", false);
    }

    #[test]
    fn a_window_close_can_list_its_pages() {
        let mut tabs = tabs();
        tabs.insert("browser-4", "chat-window", Some("chat-c".into()), "https://c.test/");
        assert_eq!(tabs.labels_of_window("chat-window"), ["browser-4"]);
        assert_eq!(tabs.window_of("browser-4"), Some("chat-window"));
        assert_eq!(tabs.window_of("browser-99"), None);
    }
}
