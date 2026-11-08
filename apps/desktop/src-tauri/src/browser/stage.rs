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
/// The page's own frame rate, which `everyNthFrame` divides.
const PAGE_FPS: u32 = 60;
/// The most frames per second the window may ask for.
pub const MAX_FPS: u32 = 30;

/// `everyNthFrame` for about `fps` frames per second.
pub fn every_nth(fps: u32) -> u32 {
    let fps = fps.clamp(1, MAX_FPS);
    (PAGE_FPS / fps).max(1)
}

/// The parameters of `Page.startScreencast` at about `fps`.
pub fn screencast_params(fps: u32) -> Value {
    json!({
        "format": "jpeg",
        "quality": 60,
        "maxWidth": WIDTH as u32,
        "maxHeight": HEIGHT as u32,
        "everyNthFrame": every_nth(fps),
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
#[derive(Default)]
pub struct Streams {
    fps: HashMap<String, u32>,
    seq: HashMap<String, u64>,
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

    /// The page closed.
    pub fn remove(&mut self, tab: &str) {
        self.fps.remove(tab);
        self.seq.remove(tab);
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
    fn a_frame_rate_becomes_a_frame_divider_within_limits() {
        assert_eq!(every_nth(60), 2, "60 is above the cap of 30 frames per second");
        assert_eq!(every_nth(30), 2);
        assert_eq!(every_nth(12), 5);
        assert_eq!(every_nth(0), 60, "zero counts as the lowest rate, 1 frame per second");
        assert_eq!(every_nth(1), 60);
        assert_eq!(every_nth(1000), 2, "capped at the most frames per second");
        let params = screencast_params(12);
        assert_eq!(params["everyNthFrame"], 5);
        assert_eq!(params["format"], "jpeg");
        assert_eq!((params["maxWidth"].as_u64(), params["maxHeight"].as_u64()), (Some(1280), Some(800)));
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
