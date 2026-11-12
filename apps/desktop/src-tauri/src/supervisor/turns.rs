//! Turn records: every finished run is counted in its chat's row by the
//! shell, whether or not a window watched it
//! (docs/features/shell-hosted-engines/spec.md, FR-6, NFR-1; plan 3.3
//! "Engine protocol", 3.5 "Turn records").
//!
//! The engine says what a run used on its `agent_settled` line (`turn`):
//! its own responses' tokens and cost, and the usage of its subagents and
//! plugin children that settled since the last turn (subagents FR-6). The
//! shell adds the two and calls `chats::record_turn` once per run id. The
//! chat is unread unless a focused window shows it. A compaction run has no
//! `turn` and is not a turn.
//!
//! An engine whose `ready` does not advertise `turnTotals` (a stale dev
//! sidecar) has no totals: the shell records nothing for it, says so once in
//! the log, and the window that shows the chat records as it did before.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use evo_store::chats::TurnRecord;
use serde::Deserialize;
use serde_json::Value;

/// Logged once when an engine has no turn totals.
pub const NO_TOTALS: &str = "engine has no turn totals: rebuild the sidecar";
/// Run ids remembered per chat, for one record per run.
const RECORDED: usize = 64;

/// An `agent_settled` line's `turn` (plan 3.3).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Totals {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    /// The run's own responses.
    pub cost_usd: f64,
    /// Subagents and plugin children settled since the last turn.
    pub child_cost_usd: f64,
    pub child_tokens: i64,
    /// The start of the final text; absent for an incognito chat.
    pub preview: Option<String>,
}

impl Totals {
    /// The chat row's record of this turn.
    pub fn record(&self, unread: bool) -> TurnRecord {
        TurnRecord {
            preview: self.preview.clone().filter(|text| !text.trim().is_empty()),
            cost_usd: self.cost_usd + self.child_cost_usd,
            tokens: self.input + self.output + self.cache_read + self.cache_write + self.child_tokens,
            provider: self.provider.clone(),
            model: self.model.clone(),
            unread,
        }
    }
}

/// What a settled run asks of the shell.
#[derive(Debug, Clone)]
pub enum Settled {
    /// Record this turn.
    Record(TurnRecord),
    /// The engine has no turn totals: the window records; `first` says to log it.
    NoTotals { first: bool },
    /// Nothing: a compaction, or a run already recorded.
    Skip,
}

/// Which engines give turn totals, and the runs already recorded.
#[derive(Default)]
pub struct Turns {
    /// Per chat: the engine generation and whether its `ready` said `turnTotals`.
    engines: Mutex<HashMap<String, (u64, bool)>>,
    /// Recorded run ids per chat, newest last.
    recorded: Mutex<HashMap<String, VecDeque<String>>>,
    warned: AtomicBool,
}

impl Turns {
    /// `chat`'s engine of `generation` said `ready` with `line`.
    pub fn on_ready(&self, chat: &str, generation: u64, line: &Value) {
        let capable = line.get("turnTotals") == Some(&Value::Bool(true));
        self.engines.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(chat.to_string(), (generation, capable));
    }

    /// `chat`'s engine of `generation` settled a run with `line`; `unread`:
    /// no focused window shows the chat.
    pub fn on_settled(&self, chat: &str, generation: u64, line: &Value, unread: bool) -> Settled {
        // A compaction is not a turn.
        if line.get("compact") == Some(&Value::Bool(true)) {
            return Settled::Skip;
        }
        let capable = self.engines.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(chat) == Some(&(generation, true));
        if !capable {
            return Settled::NoTotals { first: !self.warned.swap(true, Ordering::AcqRel) };
        }
        let Some(turn) = line.get("turn").filter(|turn| turn.is_object()) else {
            return Settled::Skip;
        };
        if let Some(run) = line.get("runId").and_then(Value::as_str) {
            let mut recorded = self.recorded.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let runs = recorded.entry(chat.to_string()).or_default();
            if runs.iter().any(|seen| seen == run) {
                return Settled::Skip;
            }
            runs.push_back(run.to_string());
            while runs.len() > RECORDED {
                runs.pop_front();
            }
        }
        let totals: Totals = serde_json::from_value(turn.clone()).unwrap_or_default();
        Settled::Record(totals.record(unread))
    }
}

// docs/features/shell-hosted-engines/spec.md, FR-6 (T21).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::tests::{set_up, top, viewer, Fake};
    use crate::supervisor::{EngineSupervisor, EnsureRequest};
    use serde_json::json;
    use std::time::Instant;

    fn turn() -> Value {
        json!({
            "provider": "acme", "model": "acme-large",
            "input": 100, "output": 20, "cacheRead": 300, "cacheWrite": 5,
            "costUsd": 0.25, "childCostUsd": 0.5, "childTokens": 75,
            "preview": "Done: the build passes.",
        })
    }

    fn settled(run: &str, turn: Option<Value>) -> Value {
        let mut line = json!({ "type": "agent_settled", "runId": run, "outcome": "completed", "compact": false });
        if let Some(turn) = turn {
            line["turn"] = turn;
        }
        line
    }

    #[test]
    fn a_turns_cost_and_tokens_include_its_children() {
        let totals: Totals = serde_json::from_value(turn()).unwrap();
        let record = totals.record(true);
        assert_eq!(record.cost_usd, 0.75);
        assert_eq!(record.tokens, 100 + 20 + 300 + 5 + 75);
        assert_eq!((record.provider.as_deref(), record.model.as_deref()), (Some("acme"), Some("acme-large")));
        assert_eq!(record.preview.as_deref(), Some("Done: the build passes."));
        assert!(record.unread);
    }

    #[tokio::test]
    async fn one_record_per_run_unread_unless_a_focused_window_shows_it() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let generation = set_up(&fake, &supervisor, "c").await;
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "ready", "turnTotals": true }), 1);
        // No window shows it: unread.
        supervisor.follow_line(&fake, "c", generation, &settled("r1", Some(turn())), 1);
        // The same run again (a replay): no second record.
        supervisor.follow_line(&fake, "c", generation, &settled("r1", Some(turn())), 1);
        assert_eq!(fake.lines("record c"), ["record c cost=0.75 tokens=500 unread=true preview=Done: the build passes."]);
        // A window shows it, but is not focused: still unread.
        supervisor.attach("chat-c", "c", Instant::now());
        supervisor.follow_line(&fake, "c", generation, &settled("r2", Some(turn())), 1);
        assert!(fake.lines("record c").last().unwrap().contains("unread=true"));
        // That window is focused: read.
        fake.focus("chat-c");
        supervisor.follow_line(&fake, "c", generation, &settled("r3", Some(turn())), 1);
        assert!(fake.lines("record c").last().unwrap().contains("unread=false"));
        // A focused window that shows another chat does not read this one.
        supervisor.attach("chat-c", "d", Instant::now());
        supervisor.follow_line(&fake, "c", generation, &settled("r4", Some(turn())), 1);
        assert!(fake.lines("record c").last().unwrap().contains("unread=true"));
        assert_eq!(fake.lines("record c").len(), 4);
        // A compaction is no turn.
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "agent_settled", "runId": "r5", "outcome": "completed", "compact": true }), 1);
        assert_eq!(fake.lines("record c").len(), 4);
    }

    #[tokio::test]
    async fn an_incognito_turn_is_recorded_without_a_preview() {
        let fake = Fake::default().with_row("c", top(None));
        let supervisor = EngineSupervisor::default();
        let generation = set_up(&fake, &supervisor, "c").await;
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "ready", "turnTotals": true }), 1);
        // The engine of an incognito chat leaves the preview out.
        let mut incognito = turn();
        incognito.as_object_mut().unwrap().remove("preview");
        supervisor.follow_line(&fake, "c", generation, &settled("r1", Some(incognito)), 1);
        assert_eq!(fake.lines("record c"), ["record c cost=0.75 tokens=500 unread=true preview=-"]);
    }

    #[tokio::test]
    async fn an_engine_without_turn_totals_is_not_recorded_and_logged_once() {
        let fake = Fake::default().with_row("c", top(None)).with_row("d", top(None));
        let supervisor = EngineSupervisor::default();
        let c = set_up(&fake, &supervisor, "c").await;
        let d = set_up(&fake, &supervisor, "d").await;
        // A stale sidecar: its `ready` has no `turnTotals`.
        supervisor.follow_line(&fake, "c", c, &json!({ "type": "ready" }), 1);
        supervisor.follow_line(&fake, "d", d, &json!({ "type": "ready" }), 1);
        supervisor.follow_line(&fake, "c", c, &settled("r1", None), 1);
        supervisor.follow_line(&fake, "d", d, &settled("r2", None), 1);
        supervisor.follow_line(&fake, "c", c, &settled("r3", None), 1);
        assert!(fake.lines("record").is_empty(), "{:?}", fake.log());
        assert_eq!(fake.log().iter().filter(|line| line.ends_with(NO_TOTALS)).count(), 1);
        // The chat's engine is rebuilt and started again: it has them.
        let request = EnsureRequest { restart: true, ..viewer("c", Some("/w"), true) };
        let generation = supervisor.ensure(&fake, request).await.unwrap().generation;
        supervisor.on_ready(&fake, "c", generation).await;
        supervisor.follow_line(&fake, "c", generation, &json!({ "type": "ready", "turnTotals": true }), 1);
        // A late line of the old engine is not this one's.
        supervisor.follow_line(&fake, "c", c, &settled("r4", Some(turn())), 1);
        assert!(fake.lines("record").is_empty());
        supervisor.follow_line(&fake, "c", generation, &settled("r5", Some(turn())), 1);
        assert_eq!(fake.lines("record c").len(), 1);
    }
}
