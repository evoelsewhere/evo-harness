//! The stage: where a page the agent drives lives (docs/features/floating-cards,
//! FR-9, FR-10).
//!
//! A page on the stage is a visible webview of the desktop size, parked outside
//! the window. A hidden webview is throttled (its animations stop), a visible
//! one that lies off-screen renders normally, and its viewport is the webview's
//! own size, so the page lays out at 1280×800 whatever card or panel shows it.
//! While something shows the page, DevTools `Page.startScreencast` streams its
//! frames, which the window paints on a canvas. The pure parts live here: the
//! constants, the screencast parameters, which pages stream, and how a frame
//! becomes an event. The wiring (webview calls, events) is in `browser.rs`.

use std::collections::HashMap;

use serde_json::{json, Value};

/// The layout viewport of a staged page, in CSS pixels.
pub const WIDTH: f64 = 1280.0;
pub const HEIGHT: f64 = 800.0;
/// Where a staged page is parked (window logical pixels, far outside the window).
pub const PARK_X: f64 = -10_000.0;
pub const PARK_Y: f64 = -10_000.0;
/// The most frames per second the window may ask for.
pub const MAX_FPS: u32 = 30;

/// The shortest time between two frames sent to the window at about `fps` (ms).
pub fn min_interval_ms(fps: u32) -> u64 {
    1000 / u64::from(fps.clamp(1, MAX_FPS))
}

/// The parameters of `Page.startScreencast`. Every frame of the page is taken (`everyNthFrame`
/// 1): dividing them would drop the one frame of a page that changes only now and then, and a
/// still page after a change would stay as it was. The rate is kept by [`Streams::offer`].
pub fn screencast_params() -> Value {
    json!({
        "format": "jpeg",
        "quality": 60,
        "maxWidth": WIDTH as u32,
        "maxHeight": HEIGHT as u32,
        "everyNthFrame": 1,
    })
}

/// What a `browser_stage` call asks of a page's stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Start (or restart at another rate).
    Start(u32),
    /// Stop the stream.
    Stop,
    /// Nothing to do.
    Keep,
}

/// Which staged pages stream, at what rate, and the sequence number of the
/// next frame of each.
/// What to do with a frame that arrived.
#[derive(Debug, PartialEq)]
pub enum Offer {
    /// Send it now.
    Emit(Value),
    /// Too soon after the last one: it waits, and a timer of this many ms sends the newest then.
    Later(u64),
    /// Too soon, and a timer is already waiting (or nothing streams): nothing more to do.
    Held,
}

#[derive(Default)]
pub struct Streams {
    fps: HashMap<String, u32>,
    seq: HashMap<String, u64>,
    /// When the last frame went to the window (ms on the caller's clock).
    last: HashMap<String, u64>,
    /// The newest frame that was too soon, waiting for its timer.
    pending: HashMap<String, Value>,
    /// A timer is waiting for the page.
    timer: std::collections::HashSet<String>,
}

impl Streams {
    /// The page should stream at `fps` (`None`: not at all). What to do about it.
    pub fn want(&mut self, tab: &str, fps: Option<u32>) -> Change {
        let fps = fps.filter(|f| *f > 0).map(|f| f.min(MAX_FPS));
        match (self.fps.get(tab).copied(), fps) {
            (Some(now), Some(next)) if now == next => Change::Keep,
            (_, Some(next)) => {
                self.fps.insert(tab.to_string(), next);
                Change::Start(next)
            }
            (Some(_), None) => {
                self.fps.remove(tab);
                self.pending.remove(tab);
                Change::Stop
            }
            (None, None) => Change::Keep,
        }
    }

    /// Whether frames of `tab` are wanted.
    pub fn streaming(&self, tab: &str) -> bool {
        self.fps.contains_key(tab)
    }

    /// The sequence number of the next frame of `tab`.
    pub fn next_seq(&mut self, tab: &str) -> u64 {
        let seq = self.seq.entry(tab.to_string()).or_insert(0);
        *seq += 1;
        *seq
    }

    /// A frame of `tab` arrived at `now_ms`: send it, or keep the newest for a timer, so the window
    /// gets at most the asked rate and the last change of a page is never lost.
    pub fn offer(&mut self, tab: &str, now_ms: u64, frame: Value) -> Offer {
        let Some(fps) = self.fps.get(tab).copied() else {
            return Offer::Held;
        };
        let interval = min_interval_ms(fps);
        match self.last.get(tab).copied() {
            Some(last) if now_ms.saturating_sub(last) < interval => {
                self.pending.insert(tab.to_string(), frame);
                if self.timer.insert(tab.to_string()) {
                    Offer::Later(interval - now_ms.saturating_sub(last))
                } else {
                    Offer::Held
                }
            }
            _ => {
                self.pending.remove(tab);
                self.last.insert(tab.to_string(), now_ms);
                Offer::Emit(frame)
            }
        }
    }

    /// The timer of `tab` fired at `now_ms`: the frame it waited for, if the page still streams.
    pub fn flush(&mut self, tab: &str, now_ms: u64) -> Option<Value> {
        self.timer.remove(tab);
        let frame = self.pending.remove(tab)?;
        if !self.fps.contains_key(tab) {
            return None;
        }
        self.last.insert(tab.to_string(), now_ms);
        Some(frame)
    }

    /// The page closed.
    pub fn remove(&mut self, tab: &str) {
        self.fps.remove(tab);
        self.seq.remove(tab);
        self.last.remove(tab);
        self.pending.remove(tab);
        self.timer.remove(tab);
    }
}

/// The parameters of the `Page.screencastFrameAck` for a frame, `None` when
/// the frame has no session id.
pub fn ack_params(frame: &Value) -> Option<Value> {
    frame.get("sessionId").and_then(Value::as_i64).map(|id| json!({ "sessionId": id }))
}

/// The `browser://frame` payload of a `Page.screencastFrame` event: the JPEG
/// (base64) and its size, `None` when the frame has no picture.
pub fn frame_payload(tab: &str, seq: u64, frame: &Value) -> Option<Value> {
    let data = frame.get("data").and_then(Value::as_str).filter(|d| !d.is_empty())?;
    let meta = frame.get("metadata");
    let size = |key: &str, fallback: f64| meta.and_then(|m| m.get(key)).and_then(Value::as_f64).filter(|v| *v > 0.0).unwrap_or(fallback);
    Some(json!({
        "tab": tab,
        "seq": seq,
        "data": data,
        "width": size("deviceWidth", WIDTH),
        "height": size("deviceHeight", HEIGHT),
    }))
}

/// The `browser://pointer` payload for a DevTools call of an agent tool: where its virtual
/// cursor goes (docs/features/floating-cards, FR-17). Only mouse events have one: a move, a
/// press, a release or a wheel turn at a point of the page.
pub fn pointer_payload(tab: &str, chat: Option<&str>, method: &str, params: &Value) -> Option<Value> {
    if method != "Input.dispatchMouseEvent" {
        return None;
    }
    let kind = match params.get("type").and_then(Value::as_str)? {
        "mouseMoved" => "move",
        "mousePressed" => "down",
        "mouseReleased" => "up",
        "mouseWheel" => "wheel",
        _ => return None,
    };
    let (x, y) = (params.get("x").and_then(Value::as_f64)?, params.get("y").and_then(Value::as_f64)?);
    Some(json!({ "tab": tab, "chat": chat, "kind": kind, "x": x, "y": y }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stage_is_a_desktop_viewport_parked_outside_the_window() {
        assert_eq!((WIDTH, HEIGHT), (1280.0, 800.0));
        assert!(PARK_X < -(WIDTH * 2.0) && PARK_Y < -(HEIGHT * 2.0), "no part of the page can show in the window");
    }

    #[test]
    fn a_frame_rate_becomes_a_minimum_gap_within_limits() {
        assert_eq!(min_interval_ms(10), 100);
        assert_eq!(min_interval_ms(30), 33);
        assert_eq!(min_interval_ms(60), 33, "60 is above the cap of 30 frames per second");
        assert_eq!(min_interval_ms(0), 1000, "zero counts as the lowest rate, 1 frame per second");
        let params = screencast_params();
        assert_eq!(params["everyNthFrame"], 1, "no frame of the page is skipped by the page");
        assert_eq!(params["format"], "jpeg");
        assert_eq!((params["maxWidth"].as_u64(), params["maxHeight"].as_u64()), (Some(1280), Some(800)));
    }

    #[test]
    fn frames_come_at_most_at_the_rate_and_the_last_one_is_never_lost() {
        let mut streams = Streams::default();
        let frame = |n: i64| json!({ "sessionId": n });
        assert_eq!(streams.offer("t", 0, frame(1)), Offer::Held, "nothing streams yet");
        streams.want("t", Some(10)); // one frame per 100 ms
        assert_eq!(streams.offer("t", 1_000, frame(1)), Offer::Emit(frame(1)), "the first goes at once");
        assert_eq!(streams.offer("t", 1_030, frame(2)), Offer::Later(70), "too soon: wait for the rest of the gap");
        assert_eq!(streams.offer("t", 1_060, frame(3)), Offer::Held, "a timer is already waiting");
        // The timer fires: the newest frame goes, not the first that waited.
        assert_eq!(streams.flush("t", 1_100), Some(frame(3)));
        assert_eq!(streams.flush("t", 1_101), None, "nothing is left");
        // A frame after a quiet time goes at once.
        assert_eq!(streams.offer("t", 5_000, frame(4)), Offer::Emit(frame(4)));
        // Stopping or closing drops what waited.
        assert_eq!(streams.offer("t", 5_010, frame(5)), Offer::Later(90));
        streams.want("t", None);
        assert_eq!(streams.flush("t", 5_100), None);
    }

    #[test]
    fn a_stream_starts_once_changes_rate_and_stops() {
        let mut streams = Streams::default();
        assert_eq!(streams.want("browser-1", None), Change::Keep, "nothing streams, nothing to stop");
        assert_eq!(streams.want("browser-1", Some(12)), Change::Start(12));
        assert!(streams.streaming("browser-1"));
        assert_eq!(streams.want("browser-1", Some(12)), Change::Keep, "asking again changes nothing");
        assert_eq!(streams.want("browser-1", Some(24)), Change::Start(24), "a new rate restarts it");
        assert_eq!(streams.want("browser-1", Some(0)), Change::Stop, "zero means stop");
        assert!(!streams.streaming("browser-1"));
        assert_eq!(streams.want("browser-1", Some(500)), Change::Start(MAX_FPS), "capped");
        assert!(!streams.streaming("browser-2"), "another page is unaffected");
    }

    #[test]
    fn frames_are_numbered_per_page_and_forgotten_with_it() {
        let mut streams = Streams::default();
        assert_eq!((streams.next_seq("a"), streams.next_seq("a"), streams.next_seq("b")), (1, 2, 1));
        streams.remove("a");
        assert_eq!(streams.next_seq("a"), 1);
    }

    #[test]
    fn a_mouse_call_of_a_tool_moves_the_virtual_cursor() {
        let press = json!({ "type": "mousePressed", "x": 700, "y": 430.5, "button": "left", "clickCount": 1 });
        assert_eq!(
            pointer_payload("browser-2", Some("chat-a"), "Input.dispatchMouseEvent", &press),
            Some(json!({ "tab": "browser-2", "chat": "chat-a", "kind": "down", "x": 700.0, "y": 430.5 }))
        );
        for (kind, name) in [("mouseMoved", "move"), ("mouseReleased", "up"), ("mouseWheel", "wheel")] {
            let payload = pointer_payload("t", None, "Input.dispatchMouseEvent", &json!({ "type": kind, "x": 1, "y": 2 })).unwrap();
            assert_eq!(payload["kind"], name);
            assert!(payload["chat"].is_null());
        }
    }

    #[test]
    fn other_calls_and_pointless_mouse_events_move_nothing() {
        assert!(pointer_payload("t", None, "Input.dispatchKeyEvent", &json!({ "type": "keyDown", "x": 1, "y": 2 })).is_none());
        assert!(pointer_payload("t", None, "Input.dispatchMouseEvent", &json!({ "type": "mouseMoved" })).is_none(), "no point");
        assert!(pointer_payload("t", None, "Input.dispatchMouseEvent", &json!({ "type": "dragCancelled", "x": 1, "y": 2 })).is_none());
    }

    #[test]
    fn a_frame_is_acknowledged_by_its_session_and_becomes_an_event() {
        let frame = json!({ "data": "/9j/4AAQ", "sessionId": 7, "metadata": { "deviceWidth": 1280, "deviceHeight": 800, "timestamp": 1.0 } });
        assert_eq!(ack_params(&frame), Some(json!({ "sessionId": 7 })));
        let payload = frame_payload("browser-3", 4, &frame).unwrap();
        assert_eq!(payload, json!({ "tab": "browser-3", "seq": 4, "data": "/9j/4AAQ", "width": 1280.0, "height": 800.0 }));
        // No metadata: the stage size stands in; no picture or no session: nothing.
        assert_eq!(frame_payload("t", 1, &json!({ "data": "x" })).unwrap()["width"], 1280.0);
        assert!(frame_payload("t", 1, &json!({ "data": "" })).is_none());
        assert!(frame_payload("t", 1, &json!({})).is_none());
        assert!(ack_params(&json!({ "data": "x" })).is_none());
    }
}
