//! Per-tab ring of DevTools protocol events (agent browser, FR-4, FR-13,
//! FR-14; docs/features/agent-browser). Every event gets a sequence number
//! that only grows, so a reader asks for "everything since N" and learns how
//! many events the ring lost in the meantime.

use std::collections::VecDeque;

use serde::Serialize;
use serde_json::Value;

/// Events kept per tab.
pub const RING_CAP: usize = 1000;
/// Events returned by one read unless the caller asks for fewer or more.
pub const DEFAULT_LIMIT: usize = 200;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StoredEvent {
    pub seq: u64,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Page {
    pub events: Vec<StoredEvent>,
    /// The `since` to pass next time.
    pub next: u64,
    /// Events the caller wanted (seq >= since) that the ring no longer has.
    pub dropped: u64,
}

#[derive(Debug)]
pub struct EventRing {
    cap: usize,
    events: VecDeque<StoredEvent>,
    /// Sequence number of the next event; the first one is 1.
    next_seq: u64,
}

impl EventRing {
    pub fn new(cap: usize) -> Self {
        Self { cap: cap.max(1), events: VecDeque::new(), next_seq: 1 }
    }

    /// Store an event; returns its sequence number.
    pub fn push(&mut self, method: String, params: Value) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        if self.events.len() == self.cap {
            self.events.pop_front();
        }
        self.events.push_back(StoredEvent { seq, method, params });
        seq
    }

    /// Events with `seq >= since`, optionally only those named in `names`, at
    /// most `limit` of them.
    pub fn read(&self, since: u64, names: Option<&[String]>, limit: usize) -> Page {
        let wanted = since.max(1);
        let first = self.events.front().map_or(self.next_seq, |e| e.seq);
        let dropped = first.saturating_sub(wanted);
        let limit = limit.max(1);
        let mut events = Vec::new();
        let mut next = self.next_seq;
        for event in self.events.iter().filter(|e| e.seq >= wanted) {
            if names.is_some_and(|names| !names.iter().any(|n| n == &event.method)) {
                continue;
            }
            if events.len() == limit {
                // More is waiting: resume at the first event not returned.
                next = event.seq;
                break;
            }
            events.push(event.clone());
        }
        Page { events, next, dropped }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ring_with(cap: usize, methods: &[&str]) -> EventRing {
        let mut ring = EventRing::new(cap);
        for method in methods {
            ring.push((*method).to_string(), json!({ "m": method }));
        }
        ring
    }

    #[test]
    fn sequence_numbers_start_at_one_and_only_grow() {
        let mut ring = EventRing::new(3);
        assert_eq!(ring.push("a".into(), Value::Null), 1);
        assert_eq!(ring.push("b".into(), Value::Null), 2);
        for _ in 0..10 {
            ring.push("c".into(), Value::Null);
        }
        assert_eq!(ring.push("d".into(), Value::Null), 13);
    }

    #[test]
    fn reads_everything_since_a_sequence_number() {
        let ring = ring_with(10, &["a", "b", "c"]);
        let page = ring.read(0, None, 200);
        assert_eq!(page.events.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!((page.next, page.dropped), (4, 0));
        let page = ring.read(3, None, 200);
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].method, "c");
        let page = ring.read(4, None, 200);
        assert!(page.events.is_empty());
        assert_eq!((page.next, page.dropped), (4, 0));
    }

    #[test]
    fn overflow_drops_the_oldest_and_counts_them() {
        let ring = ring_with(3, &["a", "b", "c", "d", "e"]);
        // seq 1 and 2 are gone; 3, 4, 5 remain.
        let page = ring.read(1, None, 200);
        assert_eq!(page.events.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![3, 4, 5]);
        assert_eq!(page.dropped, 2);
        assert_eq!(page.next, 6);
        // A reader that kept up loses nothing.
        let page = ring.read(4, None, 200);
        assert_eq!(page.dropped, 0);
        assert_eq!(page.events.len(), 2);
        // Asking from 0 counts the same as from 1.
        assert_eq!(ring.read(0, None, 200).dropped, 2);
    }

    #[test]
    fn an_empty_ring_still_reports_what_was_lost() {
        let ring = EventRing::new(5);
        let page = ring.read(0, None, 200);
        assert_eq!((page.events.len(), page.next, page.dropped), (0, 1, 0));
    }

    #[test]
    fn filters_by_name_and_resumes_after_the_scanned_events() {
        let ring = ring_with(10, &["Network.requestWillBeSent", "Page.frameNavigated", "Network.responseReceived"]);
        let names = vec!["Network.responseReceived".to_string()];
        let page = ring.read(1, Some(&names), 200);
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].seq, 3);
        assert_eq!(page.next, 4);
    }

    #[test]
    fn a_limit_resumes_at_the_first_event_not_returned() {
        let ring = ring_with(10, &["a", "b", "c", "d"]);
        let page = ring.read(1, None, 2);
        assert_eq!(page.events.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(page.next, 3);
        let page = ring.read(page.next, None, 2);
        assert_eq!(page.events.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![3, 4]);
        assert_eq!(page.next, 5);
        // The limit counts only events that match the filter.
        let names = vec!["d".to_string()];
        let page = ring.read(1, Some(&names), 1);
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.next, 5);
    }
}
