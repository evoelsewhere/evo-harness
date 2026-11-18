//! Webbridge tabs as the shell names and holds them (docs/features/browser-bridge/spec.md
//! FR-5 – FR-8, FR-17; clarification answers in 6.2).
//!
//! A tab's id is `<browser>:<profile>:<tab>` (`edge:p85ee…:1275259289`); the
//! tab registry (`browser/tabs.rs`) keeps its chat like any page. Here: the
//! id format, how long the debugging API stays on a tab (a lease: held while
//! the chat's turn runs, released 60 s after it ends), which tabs the
//! person took over, and when the person last used a tab (FR-17: the agent's
//! acts wait until they have been still for a moment).

use std::collections::{HashMap, HashSet};

/// How long after a chat's turn ends its tabs keep the debugging API, unless
/// the app setting `webbridge.leaseSeconds` says otherwise.
pub const LEASE_MS: u64 = 60_000;

/// The lease for a `webbridge.leaseSeconds` value: 60 s when unset or not a
/// number, kept between 5 s and an hour.
pub fn lease_ms(seconds: Option<f64>) -> u64 {
    match seconds {
        Some(s) if s.is_finite() => (s.clamp(5.0, 3600.0) * 1000.0) as u64,
        _ => LEASE_MS,
    }
}

/// How long the person must leave a tab alone before the agent acts in it.
pub const QUIET_MS: u64 = 5_000;
/// Input the page reports this soon after the agent's own input is the agent's.
pub const ECHO_MS: u64 = 300;
/// The longest an act waits for the person to stop before it gives up.
pub const PATIENCE_MS: u64 = 30_000;

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

/// Why tabs went, kept for the latest few so the agent's next call on one says why.
#[derive(Default)]
pub struct Gone {
    reasons: std::collections::VecDeque<(String, &'static str)>,
}

impl Gone {
    const KEEP: usize = 64;

    pub fn note(&mut self, tab: &str, reason: &'static str) {
        self.reasons.retain(|(t, _)| t != tab);
        if self.reasons.len() == Self::KEEP {
            self.reasons.pop_front();
        }
        self.reasons.push_back((tab.to_string(), reason));
    }

    pub fn reason(&self, tab: &str) -> Option<&'static str> {
        self.reasons
            .iter()
            .rev()
            .find(|(t, _)| t == tab)
            .map(|(_, r)| *r)
    }

    /// The tab is back (joined a group again).
    pub fn clear(&mut self, tab: &str) {
        self.reasons.retain(|(t, _)| t != tab);
    }
}

/// The agent's words for a tab that left the chat's group.
pub const TAKEN_BACK: &str = "The person took this tab back.";
/// The agent's words for a tab that was closed.
pub const CLOSED: &str = "The tab closed.";

/// Leases on the debugging API and takeovers, by tab id.
#[derive(Default)]
pub struct Holds {
    /// Attached tabs: `None` while the chat's turn runs, else when to let go (ms).
    leases: HashMap<String, Option<u64>>,
    /// Tabs the person took over (FR-17).
    taken: HashSet<String>,
    /// When the person last used each tab (ms).
    person: HashMap<String, u64>,
    /// When the agent last sent input to each tab (ms).
    agent: HashMap<String, u64>,
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

    /// A chat's turn ended: its attached tabs are let go `lease_ms` later.
    pub fn turn_ended<'a>(
        &mut self,
        tabs: impl IntoIterator<Item = &'a str>,
        now_ms: u64,
        lease_ms: u64,
    ) {
        for tab in tabs {
            if let Some(until) = self.leases.get_mut(tab) {
                *until = Some(now_ms + lease_ms);
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
        self.person.remove(tab);
        self.agent.remove(tab);
    }

    /// The agent sends input to `tab` now.
    pub fn agent_input(&mut self, tab: &str, now_ms: u64) {
        self.agent.insert(tab.to_string(), now_ms);
    }

    /// The page reported input from its user. Input right after the agent's
    /// own is the agent's (the browser reports both alike) and is not kept.
    pub fn person_input(&mut self, tab: &str, now_ms: u64) -> bool {
        if self.agent.get(tab).is_some_and(|at| now_ms <= at + ECHO_MS) {
            return false;
        }
        self.person.insert(tab.to_string(), now_ms);
        true
    }

    /// How long an act on `tab` should wait for the person to stop (0: act now).
    pub fn wait_for_person(&self, tab: &str, now_ms: u64) -> u64 {
        self.person
            .get(tab)
            .map(|at| (at + QUIET_MS).saturating_sub(now_ms))
            .unwrap_or(0)
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
            self.person.remove(tab);
            self.agent.remove(tab);
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
        holds.turn_ended(["edge:p1:1"], 1_000, LEASE_MS);
        assert!(holds.due(1_000 + LEASE_MS - 1).is_empty());
        assert_eq!(holds.due(1_000 + LEASE_MS), vec!["edge:p1:1".to_string()]);
        assert!(!holds.is_attached("edge:p1:1"));
        assert!(holds.is_attached("edge:p1:2"));
    }

    #[test]
    fn a_new_call_renews_the_lease() {
        let mut holds = Holds::default();
        holds.touch("edge:p1:1");
        holds.turn_ended(["edge:p1:1"], 0, LEASE_MS);
        holds.touch("edge:p1:1");
        assert!(holds.due(LEASE_MS * 2).is_empty());
    }

    #[test]
    fn a_tab_not_attached_has_no_lease() {
        let mut holds = Holds::default();
        holds.turn_ended(["edge:p1:9"], 0, LEASE_MS);
        assert!(holds.due(LEASE_MS * 2).is_empty());
        assert!(!holds.is_attached("edge:p1:9"));
    }

    #[test]
    fn a_closed_tab_is_forgotten_with_its_takeover() {
        let mut holds = Holds::default();
        holds.touch("edge:p1:1");
        holds.set_taken("edge:p1:1", true);
        holds.forget("edge:p1:1");
        assert!(!holds.is_attached("edge:p1:1"));
        assert!(!holds.is_taken("edge:p1:1"));
    }

    #[test]
    fn the_lease_setting_is_read_and_kept_in_range() {
        assert_eq!(lease_ms(None), LEASE_MS);
        assert_eq!(lease_ms(Some(f64::NAN)), LEASE_MS);
        assert_eq!(lease_ms(Some(30.0)), 30_000);
        assert_eq!(lease_ms(Some(0.0)), 5_000);
        assert_eq!(lease_ms(Some(1e9)), 3_600_000);
        let mut holds = Holds::default();
        holds.touch("edge:p1:1");
        holds.turn_ended(["edge:p1:1"], 0, lease_ms(Some(10.0)));
        assert_eq!(holds.due(10_000), vec!["edge:p1:1".to_string()]);
    }

    #[test]
    fn acts_wait_until_the_person_has_left_the_tab_alone() {
        let mut holds = Holds::default();
        assert_eq!(holds.wait_for_person("edge:p1:1", 0), 0, "never used");
        assert!(holds.person_input("edge:p1:1", 10_000));
        assert_eq!(holds.wait_for_person("edge:p1:1", 10_000), QUIET_MS);
        assert_eq!(holds.wait_for_person("edge:p1:1", 12_000), 3_000);
        assert_eq!(holds.wait_for_person("edge:p1:1", 10_000 + QUIET_MS), 0);
        assert_eq!(
            holds.wait_for_person("edge:p1:2", 10_000),
            0,
            "other tabs act now"
        );
    }

    #[test]
    fn the_agents_own_input_is_not_the_persons() {
        let mut holds = Holds::default();
        holds.agent_input("edge:p1:1", 1_000);
        assert!(!holds.person_input("edge:p1:1", 1_000 + ECHO_MS));
        assert_eq!(holds.wait_for_person("edge:p1:1", 2_000), 0);
        assert!(holds.person_input("edge:p1:1", 1_000 + ECHO_MS + 1));
        holds.forget("edge:p1:1");
        assert_eq!(holds.wait_for_person("edge:p1:1", 2_100), 0);
    }

    #[test]
    fn a_gone_tab_keeps_its_reason_until_it_comes_back() {
        let mut gone = Gone::default();
        gone.note("edge:p1:1", TAKEN_BACK);
        gone.note("edge:p1:2", CLOSED);
        assert_eq!(gone.reason("edge:p1:1"), Some(TAKEN_BACK));
        assert_eq!(gone.reason("edge:p1:2"), Some(CLOSED));
        gone.clear("edge:p1:1");
        assert_eq!(gone.reason("edge:p1:1"), None);
        for n in 0..100 {
            gone.note(&format!("edge:p1:{}", n + 10), CLOSED);
        }
        assert_eq!(gone.reason("edge:p1:2"), None, "only the latest are kept");
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
