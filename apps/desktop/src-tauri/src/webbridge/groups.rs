//! Webbridge tabs as the shell names and holds them (docs/features/browser-bridge/spec.md
//! FR-5 – FR-8, FR-17; clarification answers in 6.2).
//!
//! A tab's id is `<browser>:<profile>:<tab>` (`edge:p85ee…:1275259289`); the
//! tab registry (`browser/tabs.rs`) keeps its chat like any page. Here: the
//! id format, how long the debugging API stays on a tab (a lease: held while
//! the chat's turn runs, released 60 s after it ends), and which tabs the
//! person took over.

use std::collections::{HashMap, HashSet};

/// How long after a chat's turn ends its tabs keep the debugging API.
pub const LEASE_MS: u64 = 60_000;

/// The id the agent uses for a tab of a profile.
pub fn tab_id(profile_key: &str, tab: i64) -> String {
    format!("{profile_key}:{tab}")
}

/// A Webbridge tab id's profile key and browser tab number.
pub fn split(id: &str) -> Option<(&str, i64)> {
    if !(id.starts_with("chrome:") || id.starts_with("edge:")) {
        return None;
    }
    let (profile, tab) = id.rsplit_once(':')?;
    if !profile.contains(':') {
        return None;
    }
    Some((profile, tab.parse().ok()?))
}

/// Whether `id` names a Webbridge tab (not a built-in browser page).
pub fn is_tab(id: &str) -> bool {
    split(id).is_some()
}

/// Leases on the debugging API and takeovers, by tab id.
#[derive(Default)]
pub struct Holds {
    /// Attached tabs: `None` while the chat's turn runs, else when to let go (ms).
    leases: HashMap<String, Option<u64>>,
    /// Tabs the person took over (FR-17).
    taken: HashSet<String>,
}

impl Holds {
    /// A tool used `tab`: it stays attached.
    pub fn touch(&mut self, tab: &str) {
        self.leases.insert(tab.to_string(), None);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_attached(&self, tab: &str) -> bool {
        self.leases.contains_key(tab)
    }

    /// A chat's turn ended: its attached tabs are let go after [`LEASE_MS`].
    pub fn turn_ended<'a>(&mut self, tabs: impl IntoIterator<Item = &'a str>, now_ms: u64) {
        for tab in tabs {
            if let Some(until) = self.leases.get_mut(tab) {
                *until = Some(now_ms + LEASE_MS);
            }
        }
    }

    /// Tabs whose lease ran out; they are no longer held.
    pub fn due(&mut self, now_ms: u64) -> Vec<String> {
        let due: Vec<String> = self
            .leases
            .iter()
            .filter(|(_, until)| until.is_some_and(|u| u <= now_ms))
            .map(|(tab, _)| tab.clone())
            .collect();
        for tab in &due {
            self.leases.remove(tab);
        }
        due
    }

    /// The tab let go (detached by the browser, closed, released).
    pub fn forget(&mut self, tab: &str) {
        self.leases.remove(tab);
        self.taken.remove(tab);
    }

    /// Every attached tab of a profile, let go at once (Stop all).
    pub fn release_profile(&mut self, profile_key: &str) -> Vec<String> {
        let tabs: Vec<String> = self
            .leases
            .keys()
            .filter(|tab| split(tab).is_some_and(|(p, _)| p == profile_key))
            .cloned()
            .collect();
        for tab in &tabs {
            self.leases.remove(tab);
            self.taken.remove(tab);
        }
        tabs
    }

    pub fn set_taken(&mut self, tab: &str, taken: bool) {
        if taken {
            self.taken.insert(tab.to_string());
        } else {
            self.taken.remove(tab);
        }
    }

    pub fn is_taken(&self, tab: &str) -> bool {
        self.taken.contains(tab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_carry_the_profile_and_the_tab() {
        let id = tab_id("edge:p85ee319123b3", 1275259289);
        assert_eq!(id, "edge:p85ee319123b3:1275259289");
        assert_eq!(split(&id), Some(("edge:p85ee319123b3", 1275259289)));
        assert!(is_tab(&id));
        // Built-in pages and malformed ids are not Webbridge tabs.
        assert!(!is_tab("browser-3"));
        assert!(!is_tab("chrome:1234"));
        assert!(!is_tab("chrome:p1:abc"));
        assert!(!is_tab("firefox:p1:1"));
    }

    #[test]
    fn a_tab_is_held_during_the_turn_and_let_go_after_the_lease() {
        let mut holds = Holds::default();
        holds.touch("edge:p1:1");
        holds.touch("edge:p1:2");
        assert!(holds.due(1_000_000).is_empty(), "held while the turn runs");
        holds.turn_ended(["edge:p1:1"], 1_000);
        assert!(holds.due(1_000 + LEASE_MS - 1).is_empty());
        assert_eq!(holds.due(1_000 + LEASE_MS), vec!["edge:p1:1".to_string()]);
        assert!(!holds.is_attached("edge:p1:1"));
        assert!(holds.is_attached("edge:p1:2"));
    }

    #[test]
    fn a_new_call_renews_the_lease() {
        let mut holds = Holds::default();
        holds.touch("edge:p1:1");
        holds.turn_ended(["edge:p1:1"], 0);
        holds.touch("edge:p1:1");
        assert!(holds.due(LEASE_MS * 2).is_empty());
    }

    #[test]
    fn stop_all_lets_go_of_one_profile_only() {
        let mut holds = Holds::default();
        holds.touch("edge:p1:1");
        holds.touch("chrome:p2:1");
        holds.set_taken("edge:p1:1", true);
        assert_eq!(
            holds.release_profile("edge:p1"),
            vec!["edge:p1:1".to_string()]
        );
        assert!(!holds.is_taken("edge:p1:1"));
        assert!(holds.is_attached("chrome:p2:1"));
    }
}
