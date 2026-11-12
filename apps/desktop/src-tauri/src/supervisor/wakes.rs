//! Wakes: an idle chat is started again for the reports, notes and mail that
//! wait for it (docs/features/shell-hosted-engines/spec.md, FR-5, NFR-1;
//! plan 3.5 "Wakes"; the guard rails of
//! docs/features/subagents-and-nested-sessions/spec.md, FR-19, and
//! docs/features/session-mesh-protocol/spec.md, FR-28).
//!
//! The engine lists what waits for a chat on its session node (`meta.wake`,
//! with `wakeTitles` and `wakeReasons`): a background agent's report, a
//! nested session's note, a message from another agent. The shell follows
//! that node through its mesh subscription. When it changes, a run settles,
//! a draft is cleared or a question closes, a wake is gathered for
//! [`WAKE_DELAY`] (later events start the wait again, so several reports
//! make one wake), then checked:
//!
//! - not while the chat runs, its engine is not set up, a window's composer
//!   holds a draft for it, a question waits in it, or it is archived;
//! - at most [`MAX_AUTO_WAKES`] in a row without the person: then reports
//!   wait for the person's next prompt (a window's `prompt` resets the count).
//!
//! A wake is one automated turn with a fixed text ([`text`]), sent as hub
//! mail with `deliver: prompt` so it is durable and shown as an event chip,
//! never as the person's message. One shell decides it, whatever number of
//! windows are open (NFR-1).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde_json::Value;

/// How long a wake gathers what arrives before it is checked (FR-19).
pub const WAKE_DELAY: Duration = Duration::from_secs(2);
/// Automated wakes in a row without a person's prompt, at most (FR-19).
pub const MAX_AUTO_WAKES: u32 = 5;

/// The chat's session node as the shell last saw it (`node_state`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionNode {
    /// The seq of that state (replays are older).
    pub seq: u64,
    /// `idle`, `running`, …
    pub state: String,
    /// Addresses whose reports or mail wait to be read (`meta.wake`).
    pub wake: Vec<String>,
    /// Titles of what waits, by address (`meta.wakeTitles`).
    pub titles: HashMap<String, String>,
    /// Why each waits, by address (`meta.wakeReasons`: `mail`, …).
    pub reasons: HashMap<String, String>,
}

impl SessionNode {
    /// The session node of a `node_state` event's `node`, at `seq`.
    pub fn of(seq: u64, node: &Value) -> Self {
        let strings = |key: &str| -> HashMap<String, String> {
            node.pointer(&format!("/meta/{key}"))
                .and_then(Value::as_object)
                .map(|map| map.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
                .unwrap_or_default()
        };
        SessionNode {
            seq,
            state: node.get("state").and_then(Value::as_str).unwrap_or_default().to_string(),
            wake: node
                .pointer("/meta/wake")
                .and_then(Value::as_array)
                .map(|wake| wake.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default(),
            titles: strings("wakeTitles"),
            reasons: strings("wakeReasons"),
        }
    }

    /// Reports or mail wait to be read.
    pub fn waiting(&self) -> bool {
        !self.wake.is_empty()
    }
}

/// A chat's wake bookkeeping.
#[derive(Debug, Default)]
pub struct WakeState {
    /// What waits that a wake already acted on (or set aside) since the
    /// chat's last run started.
    pub handled: HashSet<String>,
    /// Automated wakes since the person's last prompt.
    pub in_a_row: u32,
    /// Bumped by each event that (re)starts the gather; a check of an older
    /// tick is stale.
    pub tick: u64,
}

impl WakeState {
    /// A run started: it read every report and note that waited.
    pub fn run_started(&mut self) {
        self.handled.clear();
    }

    /// The person sent a prompt: automated wakes may go on.
    pub fn prompted(&mut self) {
        self.in_a_row = 0;
    }

    /// Count one automated wake; false when the limit is reached.
    pub fn take_one(&mut self) -> bool {
        if self.in_a_row >= MAX_AUTO_WAKES {
            return false;
        }
        self.in_a_row += 1;
        true
    }
}

/// What a wake answers: titles of finished background agents, of nested
/// sessions that finished a turn, and of mail senders.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Woken {
    pub agents: Vec<String>,
    pub sessions: Vec<String>,
    pub mails: Vec<String>,
}

impl Woken {
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty() && self.sessions.is_empty() && self.mails.is_empty()
    }
}

/// What waits on `session` that no wake acted on yet, marked as acted on.
/// `agents` are the titles of the chat's agent nodes by address. Only an
/// idle session is woken.
pub fn gather(session: &SessionNode, agents: &HashMap<String, String>, handled: &mut HashSet<String>) -> Woken {
    let mut woken = Woken::default();
    if session.state != "idle" {
        return woken;
    }
    for address in &session.wake {
        if handled.contains(address) {
            continue;
        }
        let title = session.titles.get(address);
        let agent = agents.get(address);
        if session.reasons.get(address).map(String::as_str) == Some("mail") {
            // A message or question from another agent (crates/evo-engine/src/tools/mesh.rs).
            woken.mails.push(title.or(agent).cloned().unwrap_or_else(|| "an agent".to_string()));
        } else if let Some(agent) = agent {
            woken.agents.push(agent.clone());
        } else if !address.contains('/') {
            // Mail from another session: a nested session's note, via the hub.
            woken.sessions.push(title.cloned().unwrap_or_else(|| "a nested session".to_string()));
        } else {
            // An agent this shell has not seen yet: a later event names it.
            continue;
        }
        handled.insert(address.clone());
    }
    woken
}

/// `names`, quoted and joined (a quote inside a name becomes `'`).
fn quoted(names: &[String]) -> String {
    names.iter().map(|name| format!("\"{}\"", name.replace('"', "'"))).collect::<Vec<_>>().join(", ")
}

/// The automated prompt of a wake. Windows show it as an event chip by its
/// tag (`chat-turn.tsx`); the texts are pinned by tests.
pub fn text(woken: &Woken) -> String {
    let Woken { agents, sessions, mails } = woken;
    let also_mail = if mails.is_empty() {
        String::new()
    } else {
        format!(" Messages also arrived from {}; answer questions with the reply tool.", quoted(mails))
    };
    if agents.is_empty() && sessions.is_empty() {
        let what = if mails.len() == 1 { "a message" } else { "messages" };
        return format!(
            "<mail-arrived>\n{} sent you {what} above. If one asks a question, answer it with the reply tool (the asker is waiting); then continue the work.\n</mail-arrived>",
            quoted(mails)
        );
    }
    if !agents.is_empty() {
        let one = agents.len() == 1;
        let also_sessions = if sessions.is_empty() {
            String::new()
        } else {
            format!(" Nested sessions also reported: {}.", quoted(sessions))
        };
        return format!(
            "<background-agents-done>\n{} finished: {}. {} in the message{} above.{also_sessions}{also_mail} Continue the work with {}.\n</background-agents-done>",
            if one { "A background agent" } else { "Background agents" },
            quoted(agents),
            if one { "Its report is" } else { "Their reports are" },
            if one { "" } else { "s" },
            if agents.len() + sessions.len() == 1 { "it" } else { "them" },
        );
    }
    let one = sessions.len() == 1;
    format!(
        "<nested-sessions-done>\n{} finished a turn: {}. {} in the message{} above.{also_mail} Continue the work with {}.\n</nested-sessions-done>",
        if one { "A nested session" } else { "Nested sessions" },
        quoted(sessions),
        if one { "Its note is" } else { "Their notes are" },
        if one { "" } else { "s" },
        if one { "it" } else { "them" },
    )
}

// docs/features/shell-hosted-engines/spec.md, FR-5 (T16).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::tests::{set_up, top, Fake};
    use crate::supervisor::{EngineSupervisor, Row};
    use serde_json::json;

    /// A mesh `node_state` of `chat`'s session node with `wake` waiting.
    fn session(chat: &str, seq: u64, state: &str, wake: &[&str]) -> Value {
        json!({
            "type": "mesh", "v": 1, "id": format!("e{seq}"), "kind": "evt", "op": "node_state",
            "from": format!("s_{chat}"), "to": "*", "seq": seq, "ts": 0,
            "body": { "node": { "id": format!("s_{chat}"), "kind": "session", "state": state, "title": "",
                "meta": { "wake": wake, "wakeTitles": { "s_n": "Writer" } } } },
        })
    }

    /// A mesh `node_state` of agent `id` titled `title`.
    fn agent(seq: u64, id: &str, title: &str, state: &str) -> Value {
        json!({
            "type": "mesh", "v": 1, "id": format!("e{seq}"), "kind": "evt", "op": "node_state",
            "from": id, "to": "*", "seq": seq, "ts": 0,
            "body": { "node": { "id": id, "kind": "agent", "state": state, "title": title, "meta": { "callId": "call_1" } } },
        })
    }

    /// Check the wake of `chat` gathered last.
    async fn check(fake: &Fake, supervisor: &EngineSupervisor, chat: &str) -> bool {
        let tick = fake.last_tick(chat).expect("a wake was gathered");
        supervisor.check_wake(fake, chat, tick).await
    }

    #[tokio::test]
    async fn an_idle_chat_is_woken_once_when_the_gather_ends() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let generation = set_up(&fake, &supervisor, "c").await;
        let follow = |line: Value| supervisor.follow_line(&fake, "c", generation, &line, 1);
        follow(agent(2, "s_c/a_1", "Find call sites", "running"));
        follow(agent(3, "s_c/a_1", "Find call sites", "settled"));
        follow(session("c", 4, "idle", &["s_c/a_1"]));
        let first = fake.last_tick("c").unwrap();
        // A note arrives within the gather: the wait starts again.
        follow(session("c", 5, "idle", &["s_c/a_1", "s_n"]));
        assert!(fake.last_tick("c").unwrap() > first);
        assert!(!supervisor.check_wake(&fake, "c", first).await, "an older gather is stale");
        assert!(check(&fake, &supervisor, "c").await);
        assert_eq!(
            fake.lines("wake c"),
            ["wake c <background-agents-done>\nA background agent finished: \"Find call sites\". Its report is in the message above. Nested sessions also reported: \"Writer\". Continue the work with them.\n</background-agents-done>"],
        );
        // The same reports do not wake it again.
        follow(session("c", 6, "idle", &["s_c/a_1", "s_n"]));
        assert!(!check(&fake, &supervisor, "c").await);
        assert_eq!(fake.lines("wake c").len(), 1);
    }

    #[tokio::test]
    async fn no_wake_in_a_run_with_a_draft_an_open_question_or_archived() {
        let mut fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let generation = set_up(&fake, &supervisor, "c").await;
        let wake_for = |seq: u64, from: &str| session("c", seq, "idle", &[from]);
        // A run is active.
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "agent_start" }), 1);
        supervisor.follow_line(&fake, "c", generation, &wake_for(2, "s_a"), 1);
        assert!(!check(&fake, &supervisor, "c").await);
        // It settles: the gather starts again and wakes it.
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "agent_settled", "outcome": "completed" }), 1);
        assert!(check(&fake, &supervisor, "c").await);
        supervisor.follow_line(&fake, "c", generation, &session("c", 3, "running", &[]), 1);
        supervisor.follow_line(&fake, "c", generation, &session("c", 4, "idle", &[]), 1);

        // A draft in a window's composer holds it; clearing the draft wakes it.
        supervisor.set_composing(&fake, "main", "c", true);
        supervisor.set_composing(&fake, "chat-c", "c", true);
        supervisor.follow_line(&fake, "c", generation, &wake_for(5, "s_b"), 1);
        assert!(!check(&fake, &supervisor, "c").await);
        supervisor.set_composing(&fake, "main", "c", false);
        assert!(!check(&fake, &supervisor, "c").await, "another window still has a draft");
        // That window closes: its draft goes with it.
        supervisor.window_gone("chat-c", std::time::Instant::now());
        supervisor.drop_drafts(&fake, "chat-c");
        assert!(check(&fake, &supervisor, "c").await);
        supervisor.follow_line(&fake, "c", generation, &session("c", 6, "running", &[]), 1);

        // An open question holds it; its answer wakes it.
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "ui_dialog_request", "requestId": "approve:1", "ui": { "kind": "confirm", "title": "Run it?" } }), 1);
        supervisor.follow_line(&fake, "c", generation, &wake_for(7, "s_c2"), 1);
        assert!(!check(&fake, &supervisor, "c").await);
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "ui_dialog_closed", "requestId": "approve:1" }), 1);
        assert!(check(&fake, &supervisor, "c").await);
        supervisor.follow_line(&fake, "c", generation, &session("c", 8, "running", &[]), 1);
        assert_eq!(fake.lines("wake c").len(), 3);

        // An archived chat is never woken.
        fake.rows.insert("c".into(), Row { archived: true, ..top(None) });
        supervisor.follow_line(&fake, "c", generation, &wake_for(9, "s_d"), 1);
        assert!(!check(&fake, &supervisor, "c").await);
        assert_eq!(fake.lines("wake c").len(), 3);
    }

    #[tokio::test]
    async fn at_most_five_wakes_in_a_row_until_the_person_prompts() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let generation = set_up(&fake, &supervisor, "c").await;
        let mut seq = 1;
        let mut note = |supervisor: &EngineSupervisor, from: String| {
            seq += 2;
            supervisor.follow_line(&fake, "c", generation, &session("c", seq, "running", &[]), 1);
            supervisor.follow_line(&fake, "c", generation, &session("c", seq + 1, "idle", &[&from]), 1);
        };
        for n in 0..MAX_AUTO_WAKES {
            note(&supervisor, format!("s_n{n}"));
            assert!(check(&fake, &supervisor, "c").await, "wake {n}");
        }
        note(&supervisor, "s_n5".into());
        assert!(!check(&fake, &supervisor, "c").await, "the sixth waits");
        assert_eq!(fake.lines("log c info a wake waits").len(), 1);
        // The person's prompt (written by a window) resets the count; its run reads the note.
        supervisor.prompted("c");
        note(&supervisor, "s_n6".into());
        assert!(check(&fake, &supervisor, "c").await);
        assert_eq!(fake.lines("wake c").len(), 6);
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_guard_rails_are_a_two_second_gather_and_five_in_a_row() {
        assert_eq!(WAKE_DELAY, Duration::from_secs(2));
        assert_eq!(MAX_AUTO_WAKES, 5);
        let mut state = WakeState::default();
        for _ in 0..MAX_AUTO_WAKES {
            assert!(state.take_one());
        }
        assert!(!state.take_one(), "the sixth waits for the person");
        state.prompted();
        assert!(state.take_one());
    }

    #[test]
    fn a_session_node_says_what_waits() {
        let node = json!({
            "id": "s_c", "kind": "session", "state": "idle",
            "meta": { "wake": ["s_c/a_1", "s_n"], "wakeTitles": { "s_n": "Writer" }, "wakeReasons": { "s_c/a_1": "report" } },
        });
        let session = SessionNode::of(4, &node);
        assert_eq!((session.seq, session.state.as_str(), session.waiting()), (4, "idle", true));
        assert_eq!(session.wake, ["s_c/a_1", "s_n"]);
        assert_eq!(session.titles["s_n"], "Writer");
        assert!(!SessionNode::of(5, &json!({ "state": "running", "meta": { "wake": [] } })).waiting());
    }

    #[test]
    fn a_wake_gathers_agents_sessions_and_mail_once() {
        let session = SessionNode {
            seq: 1,
            state: "idle".into(),
            wake: names(&["s_c/a_1", "s_n", "s_c/a_2", "s_c/a_9", "s_other"]),
            titles: HashMap::from([("s_n".into(), "Writer".into()), ("s_c/a_2".into(), "Reviewer".into())]),
            reasons: HashMap::from([("s_c/a_2".into(), "mail".into())]),
        };
        let agents = HashMap::from([("s_c/a_1".to_string(), "Find call sites".to_string())]);
        let mut handled = HashSet::new();
        let woken = gather(&session, &agents, &mut handled);
        assert_eq!(woken.agents, ["Find call sites"]);
        assert_eq!(woken.sessions, ["Writer", "a nested session"]);
        assert_eq!(woken.mails, ["Reviewer"]);
        // An agent the shell has not seen is left for a later look.
        assert!(!handled.contains("s_c/a_9"));
        // Acted on: the same reports do not wake again.
        assert!(gather(&session, &agents, &mut handled).is_empty());
        // A running session is not woken.
        let mut fresh = HashSet::new();
        assert!(gather(&SessionNode { state: "running".into(), ..session.clone() }, &agents, &mut fresh).is_empty());
        // A run read them all: the next reports wake again.
        let mut state = WakeState { handled, ..Default::default() };
        state.run_started();
        assert!(!gather(&session, &agents, &mut state.handled).is_empty());
    }

    // The texts the webview's wake chips are drawn from (chat-turn.tsx), unchanged from the window's.
    #[test]
    fn the_wake_texts_are_pinned() {
        let woken = |agents: &[&str], sessions: &[&str], mails: &[&str]| Woken { agents: names(agents), sessions: names(sessions), mails: names(mails) };
        assert_eq!(
            text(&woken(&[], &[], &["Reviewer"])),
            "<mail-arrived>\n\"Reviewer\" sent you a message above. If one asks a question, answer it with the reply tool (the asker is waiting); then continue the work.\n</mail-arrived>"
        );
        assert_eq!(
            text(&woken(&[], &[], &["A", "B"])),
            "<mail-arrived>\n\"A\", \"B\" sent you messages above. If one asks a question, answer it with the reply tool (the asker is waiting); then continue the work.\n</mail-arrived>"
        );
        assert_eq!(
            text(&woken(&["Find \"x\""], &[], &[])),
            "<background-agents-done>\nA background agent finished: \"Find 'x'\". Its report is in the message above. Continue the work with it.\n</background-agents-done>"
        );
        assert_eq!(
            text(&woken(&["One", "Two"], &["Writer"], &["Reviewer"])),
            "<background-agents-done>\nBackground agents finished: \"One\", \"Two\". Their reports are in the messages above. Nested sessions also reported: \"Writer\". Messages also arrived from \"Reviewer\"; answer questions with the reply tool. Continue the work with them.\n</background-agents-done>"
        );
        assert_eq!(
            text(&woken(&[], &["Writer"], &[])),
            "<nested-sessions-done>\nA nested session finished a turn: \"Writer\". Its note is in the message above. Continue the work with it.\n</nested-sessions-done>"
        );
        assert_eq!(
            text(&woken(&[], &["Writer", "Tester"], &["Reviewer"])),
            "<nested-sessions-done>\nNested sessions finished a turn: \"Writer\", \"Tester\". Their notes are in the messages above. Messages also arrived from \"Reviewer\"; answer questions with the reply tool. Continue the work with them.\n</nested-sessions-done>"
        );
    }
}
