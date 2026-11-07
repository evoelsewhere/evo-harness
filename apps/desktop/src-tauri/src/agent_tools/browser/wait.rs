//! `browser_wait` (docs/features/agent-browser FR-2, NFR-3): wait for a ref or
//! text to appear or go away, for the network to be quiet, or for a time.
//!
//! The page is polled from here with one small script per round. What decides
//! "matched" or "timed out" is a plain state machine ([`Waiter`]) that takes the
//! time as an argument, so it is tested without a page or a clock. A stopped
//! turn ends the wait at once; so does a dialog opening (the page is held by it).
//!
//! The scan of the page for the element behind a ref ([`ELEMENT_SCAN`]) is also
//! used by `upload.rs` and `pointer.rs`.

use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};

use super::driver::{cut_chars, finish, is_dialog_error, js, resolve_page, Page, Stop, ToolResult};
use crate::agent_tools::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};

// ── Schema ──────────────────────────────────────────────────────────────────

const WAIT: &str = "Wait for something on the page, up to timeoutMs (default 10000, at most 60000): a ref or the text to appear (state \"visible\", the default) or go away (state \"gone\"); or networkIdleMs, until the page has loaded and made no new requests for that long; or timeMs, a fixed pause. Returns as soon as it happens and says what happened and how long it took, or that it timed out. Ends at once when the turn is stopped.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ref": { "type": "string", "description": "Wait for this element (a ref from browser_read_page / browser_find)." },
            "text": { "type": "string", "description": "Wait for this text on the page (case-insensitive)." },
            "state": { "type": "string", "enum": ["visible", "gone"], "description": "With ref or text: wait for it to be visible (default) or to be gone." },
            "networkIdleMs": { "type": "number", "description": "Wait until the page is loaded and no new request finished for this many ms." },
            "timeMs": { "type": "number", "description": "Just wait this long (at most 60000)." },
            "timeoutMs": { "type": "number", "description": "Give up after this long. Default 10000, at most 60000." },
            "tabId": { "type": "string", "description": "A tab id from browser_tabs; default: the active tab." },
        },
        "additionalProperties": false,
    })
}

fn run(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(async move { finish(wait(&ctx, &args).await) })
}

/// The tool of this module.
pub fn tools() -> Vec<Tool> {
    vec![Tool { name: "browser_wait", description: WAIT, schema, class: Class::Read, run }]
}

// ── Arguments ───────────────────────────────────────────────────────────────

pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;
pub const MAX_TIMEOUT_MS: u64 = 60_000;

/// What to wait for.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Ref { reference: String, gone: bool },
    Text { text: String, gone: bool },
    NetworkIdle { ms: u64 },
    Time { ms: u64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub mode: Mode,
    pub timeout_ms: u64,
}

fn text_arg(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(format!("{key} must be a string.")),
    }
}

fn ms_arg(args: &Value, key: &str) -> Result<Option<u64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => match value.as_f64() {
            Some(n) if n.is_finite() && n >= 0.0 => Ok(Some(n.round() as u64)),
            _ => Err(format!("{key} must be a number of milliseconds (0 or more).")),
        },
    }
}

/// Validate the arguments of `browser_wait`.
pub fn parse_wait(args: &Value) -> Result<Spec, String> {
    let reference = text_arg(args, "ref")?;
    let text = text_arg(args, "text")?;
    let idle = ms_arg(args, "networkIdleMs")?;
    let time = ms_arg(args, "timeMs")?;
    let given = [reference.is_some(), text.is_some(), idle.is_some(), time.is_some()].iter().filter(|g| **g).count();
    if given == 0 {
        return Err("browser_wait needs one of ref, text, networkIdleMs or timeMs.".into());
    }
    if given > 1 {
        return Err("browser_wait takes only one of ref, text, networkIdleMs and timeMs.".into());
    }
    let gone = match args.get("state") {
        None | Some(Value::Null) => false,
        Some(Value::String(state)) if state == "visible" => false,
        Some(Value::String(state)) if state == "gone" => true,
        Some(_) => return Err("state must be \"visible\" or \"gone\".".into()),
    };
    let timeout_ms = ms_arg(args, "timeoutMs")?.map_or(DEFAULT_TIMEOUT_MS, |ms| ms.clamp(1, MAX_TIMEOUT_MS));
    let mode = if let Some(reference) = reference {
        Mode::Ref { reference: reference.trim().to_string(), gone }
    } else if let Some(text) = text {
        Mode::Text { text, gone }
    } else if let Some(ms) = idle {
        if ms == 0 {
            return Err("networkIdleMs must be more than 0.".into());
        }
        if ms > timeout_ms {
            return Err(format!("networkIdleMs ({ms}) is longer than timeoutMs ({timeout_ms}); raise timeoutMs (at most {MAX_TIMEOUT_MS})."));
        }
        Mode::NetworkIdle { ms }
    } else {
        let ms = time.unwrap_or(0);
        if ms > MAX_TIMEOUT_MS {
            return Err(format!("timeMs is at most {MAX_TIMEOUT_MS}."));
        }
        Mode::Time { ms }
    };
    Ok(Spec { mode, timeout_ms })
}

// ── The state machine ───────────────────────────────────────────────────────

/// What one look at the page saw.
#[derive(Debug, Clone, PartialEq)]
pub enum Probe {
    Element { present: bool, visible: bool },
    Text { found: bool },
    /// `count` and `last_end` identify the newest resource entry; `loading` is true while the page loads.
    Network { count: u64, last_end: f64, loading: bool },
}

/// What the waiter decides after a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pending,
    Matched,
    TimedOut,
}

/// Tracks one wait. All times are milliseconds on any monotonic scale.
#[derive(Debug)]
pub struct Waiter {
    spec: Spec,
    started: u64,
    last_activity: u64,
    last_seen: Option<(u64, i64)>,
}

impl Waiter {
    pub fn new(spec: Spec, now: u64) -> Self {
        Self { spec, started: now, last_activity: now, last_seen: None }
    }

    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    /// Milliseconds since the wait began.
    pub fn elapsed(&self, now: u64) -> u64 {
        now.saturating_sub(self.started)
    }

    /// A look at the page at `now` (`None` when the page could not be read this time).
    pub fn step(&mut self, now: u64, probe: Option<&Probe>) -> Verdict {
        let elapsed = self.elapsed(now);
        let matched = match (&self.spec.mode, probe) {
            (Mode::Time { ms }, _) => elapsed >= *ms,
            (Mode::Ref { gone, .. }, Some(Probe::Element { present, visible })) => {
                if *gone {
                    !present || !visible
                } else {
                    *present && *visible
                }
            }
            (Mode::Text { gone, .. }, Some(Probe::Text { found })) => found != gone,
            (Mode::NetworkIdle { ms }, Some(Probe::Network { count, last_end, loading })) => {
                let key = (*count, (*last_end * 1000.0).round() as i64);
                if *loading || self.last_seen != Some(key) {
                    // Activity (or the first look): the quiet period starts over.
                    self.last_activity = now;
                    self.last_seen = Some(key);
                }
                now.saturating_sub(self.last_activity) >= *ms
            }
            _ => false,
        };
        if matched {
            Verdict::Matched
        } else if !matches!(self.spec.mode, Mode::Time { .. }) && elapsed >= self.spec.timeout_ms {
            Verdict::TimedOut
        } else {
            Verdict::Pending
        }
    }
}

/// `1.6 s`, `10 s`, `250 ms`.
pub fn fmt_duration(ms: u64) -> String {
    if ms < 1000 {
        return format!("{ms} ms");
    }
    let tenths = (ms + 50) / 100;
    let (whole, part) = (tenths / 10, tenths % 10);
    if part == 0 {
        format!("{whole} s")
    } else {
        format!("{whole}.{part} s")
    }
}

fn quoted(text: &str) -> String {
    let short = cut_chars(text, 80);
    js(&if short.len() < text.len() { format!("{short}…") } else { short })
}

/// "text "Saved" to appear" — what the wait is for.
pub fn waiting_for(mode: &Mode) -> String {
    match mode {
        Mode::Ref { reference, gone: false } => format!("{reference} to be visible"),
        Mode::Ref { reference, gone: true } => format!("{reference} to be gone"),
        Mode::Text { text, gone: false } => format!("text {} to appear", quoted(text)),
        Mode::Text { text, gone: true } => format!("text {} to disappear", quoted(text)),
        Mode::NetworkIdle { ms } => format!("the network to be idle for {}", fmt_duration(*ms)),
        Mode::Time { ms } => fmt_duration(*ms),
    }
}

/// "text "Saved" appeared" — what happened when it matched.
pub fn what_matched(mode: &Mode) -> String {
    match mode {
        Mode::Ref { reference, gone: false } => format!("{reference} is visible"),
        Mode::Ref { reference, gone: true } => format!("{reference} is gone"),
        Mode::Text { text, gone: false } => format!("text {} appeared", quoted(text)),
        Mode::Text { text, gone: true } => format!("text {} disappeared", quoted(text)),
        Mode::NetworkIdle { ms } => format!("the network was idle for {}", fmt_duration(*ms)),
        Mode::Time { ms } => format!("waited {}", fmt_duration(*ms)),
    }
}

/// The result line of a finished wait.
pub fn outcome_line(mode: &Mode, verdict: Verdict, elapsed_ms: u64) -> String {
    match (verdict, mode) {
        (Verdict::Matched, Mode::Time { .. }) => format!("Waited {}.", fmt_duration(elapsed_ms)),
        (Verdict::Matched, _) => format!("Matched after {}: {}.", fmt_duration(elapsed_ms), what_matched(mode)),
        _ => format!("Timed out after {} waiting for {}.", fmt_duration(elapsed_ms), waiting_for(mode)),
    }
}

// ── Looking at the page ─────────────────────────────────────────────────────

/// Page script: `evoElement(ref)`, the element a ref names (also inside same-origin frames), or null.
/// A ref is the `__evoRef` the page helper put on the element; scanning for it reads nothing else
/// and, unlike `window.__evo.point`, scrolls nothing.
pub const ELEMENT_SCAN: &str = r#"const evoElement = (ref) => {
  const walk = (doc, depth) => {
    for (const el of doc.querySelectorAll("*")) if (el.__evoRef === ref) return el;
    if (depth >= 6) return null;
    for (const frame of doc.querySelectorAll("iframe,frame")) {
      let inner = null;
      try { inner = frame.contentDocument; } catch {}
      const found = inner ? walk(inner, depth + 1) : null;
      if (found) return found;
    }
    return null;
  };
  return walk(document, 0);
};"#;

/// `ELEMENT_SCAN`, then `body` with `el` bound to the element of `reference` (null when gone), as one expression.
pub fn with_element(reference: &str, body: &str) -> String {
    format!("(() => {{ {ELEMENT_SCAN} const el = evoElement({}); {body} }})()", js(reference))
}

const REF_PROBE: &str = r#"if (!el || !el.isConnected) return { present: false, visible: false };
const style = (el.ownerDocument.defaultView || window).getComputedStyle(el);
let visible = !(style.display === "none" || style.visibility === "hidden" || style.opacity === "0");
if (visible && style.display !== "contents") {
  const rect = el.getBoundingClientRect();
  visible = rect.width > 0 || rect.height > 0 || (style.overflow === "visible" && !!el.firstElementChild);
}
return { present: true, visible };"#;

const NETWORK_PROBE: &str = r#"(() => {
  const entries = performance.getEntriesByType("resource");
  let last = 0;
  for (const entry of entries.slice(-30)) last = Math.max(last, entry.responseEnd || 0);
  return { count: entries.length, lastEnd: last, loading: document.readyState !== "complete" };
})()"#;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ElementProbe {
    present: bool,
    visible: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkProbe {
    count: u64,
    last_end: f64,
    loading: bool,
}

/// The script of one look for `mode`; `None` for a plain pause.
fn probe_script(mode: &Mode) -> Option<String> {
    match mode {
        Mode::Ref { reference, .. } => Some(with_element(reference, REF_PROBE)),
        Mode::Text { text, .. } => Some(format!("(document.body?.innerText || \"\").toLowerCase().includes(String({}).toLowerCase())", js(text))),
        Mode::NetworkIdle { .. } => Some(NETWORK_PROBE.to_string()),
        Mode::Time { .. } => None,
    }
}

fn read_probe(mode: &Mode, value: Value, page_loading: bool) -> Option<Probe> {
    match mode {
        Mode::Ref { .. } => serde_json::from_value::<ElementProbe>(value).ok().map(|p| Probe::Element { present: p.present, visible: p.visible }),
        Mode::Text { .. } => value.as_bool().map(|found| Probe::Text { found }),
        Mode::NetworkIdle { .. } => serde_json::from_value::<NetworkProbe>(value)
            .ok()
            .map(|p| Probe::Network { count: p.count, last_end: p.last_end, loading: p.loading || page_loading }),
        Mode::Time { .. } => None,
    }
}

/// Sleep `ms`, or until the call is dropped; true when the call was dropped.
pub async fn sleep_or_stop(page: &Page, ms: u64) -> bool {
    match &page.cancel {
        Some(cancel) => tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(ms)) => cancel.is_cancelled(),
            _ = cancel.cancelled() => true,
        },
        None => {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            false
        }
    }
}

const POLL_MS: u64 = 150;

async fn wait(ctx: &ToolCtx, args: &Value) -> ToolResult {
    let spec = parse_wait(args)?;
    let page = resolve_page(ctx, args)?;
    let started = Instant::now();
    let now = || started.elapsed().as_millis() as u64;
    let mut waiter = Waiter::new(spec, 0);
    let script = probe_script(&waiter.spec().mode);
    let mut last_error: Option<String> = None;
    loop {
        if page.cancelled() {
            return Ok(ToolOutput::text("Wait stopped: the turn was stopped."));
        }
        if !page.exists() {
            return Err("The tab closed.".into());
        }
        let probe = match &script {
            Some(script) => match page.eval(script, false).await {
                Ok(value) => read_probe(&waiter.spec().mode, value, page.is_loading()),
                // A dialog holds the page: the wait cannot see it any more, so report it.
                Err(error) if is_dialog_error(&error) => return Err(Stop::from(error)),
                // A page in the middle of a navigation cannot be read for a moment.
                Err(error) => {
                    last_error = Some(error);
                    None
                }
            },
            None => None,
        };
        let verdict = waiter.step(now(), probe.as_ref());
        if verdict != Verdict::Pending {
            let elapsed = waiter.elapsed(now());
            let mut lines = vec![outcome_line(&waiter.spec().mode, verdict, elapsed)];
            if verdict == Verdict::TimedOut {
                if let Some(error) = last_error {
                    lines.push(format!("The page could not be read: {error}"));
                }
            }
            lines.push(format!("Page: {}", page.summary().await));
            lines.extend(page.dialog_note());
            let out = lines.join("\n");
            return Ok(if verdict == Verdict::TimedOut { ToolOutput::error(out) } else { ToolOutput::text(out) });
        }
        // Sleep until the next look (a pause sleeps to its end).
        let pause = match &waiter.spec().mode {
            Mode::Time { ms } => ms.saturating_sub(waiter.elapsed(now())).clamp(1, 500),
            _ => POLL_MS,
        };
        if sleep_or_stop(&page, pause).await {
            return Ok(ToolOutput::text("Wait stopped: the turn was stopped."));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: Value) -> Result<Spec, String> {
        parse_wait(&args)
    }

    #[test]
    fn one_mode_is_required_and_only_one() {
        assert!(spec(json!({})).unwrap_err().contains("needs one of"));
        assert!(spec(json!({ "ref": "ref_1", "text": "x" })).unwrap_err().contains("only one"));
        assert!(spec(json!({ "text": "   " })).unwrap_err().contains("needs one of"), "blank text is no text");
        assert!(spec(json!({ "timeMs": 100, "networkIdleMs": 100 })).unwrap_err().contains("only one"));
    }

    #[test]
    fn arguments_are_checked_for_type_and_range() {
        assert!(spec(json!({ "ref": 4 })).unwrap_err().contains("must be a string"));
        assert!(spec(json!({ "timeMs": -1 })).unwrap_err().contains("timeMs"));
        assert!(spec(json!({ "timeMs": "soon" })).unwrap_err().contains("timeMs"));
        assert!(spec(json!({ "timeMs": 61_000 })).unwrap_err().contains("at most 60000"));
        assert!(spec(json!({ "text": "x", "state": "hidden" })).unwrap_err().contains("state"));
        assert!(spec(json!({ "networkIdleMs": 0 })).unwrap_err().contains("more than 0"));
        assert!(spec(json!({ "networkIdleMs": 20_000 })).unwrap_err().contains("longer than timeoutMs"));
    }

    #[test]
    fn modes_state_and_timeout_defaults() {
        let s = spec(json!({ "text": "Saved" })).unwrap();
        assert_eq!(s, Spec { mode: Mode::Text { text: "Saved".into(), gone: false }, timeout_ms: 10_000 });
        let s = spec(json!({ "ref": " ref_3 ", "state": "gone", "timeoutMs": 90_000 })).unwrap();
        assert_eq!(s, Spec { mode: Mode::Ref { reference: "ref_3".into(), gone: true }, timeout_ms: 60_000 });
        let s = spec(json!({ "networkIdleMs": 500, "timeoutMs": 2000 })).unwrap();
        assert_eq!(s.mode, Mode::NetworkIdle { ms: 500 });
        assert_eq!(s.timeout_ms, 2000);
        assert_eq!(spec(json!({ "timeMs": 1500 })).unwrap().mode, Mode::Time { ms: 1500 });
        assert_eq!(spec(json!({ "text": "x", "timeoutMs": 0 })).unwrap().timeout_ms, 1, "a zero timeout still looks once");
    }

    fn text_spec(gone: bool, timeout_ms: u64) -> Spec {
        Spec { mode: Mode::Text { text: "Saved".into(), gone }, timeout_ms }
    }

    #[test]
    fn a_text_wait_matches_when_it_appears_and_times_out_otherwise() {
        let mut w = Waiter::new(text_spec(false, 1000), 0);
        assert_eq!(w.step(0, Some(&Probe::Text { found: false })), Verdict::Pending);
        assert_eq!(w.step(600, Some(&Probe::Text { found: false })), Verdict::Pending);
        assert_eq!(w.step(750, Some(&Probe::Text { found: true })), Verdict::Matched);
        assert_eq!(w.elapsed(750), 750);

        let mut w = Waiter::new(text_spec(false, 1000), 5000);
        assert_eq!(w.step(5999, Some(&Probe::Text { found: false })), Verdict::Pending);
        assert_eq!(w.step(6000, Some(&Probe::Text { found: false })), Verdict::TimedOut);
    }

    #[test]
    fn a_match_at_the_deadline_counts() {
        let mut w = Waiter::new(text_spec(false, 1000), 0);
        assert_eq!(w.step(1000, Some(&Probe::Text { found: true })), Verdict::Matched);
    }

    #[test]
    fn waiting_for_text_to_go_away() {
        let mut w = Waiter::new(text_spec(true, 1000), 0);
        assert_eq!(w.step(0, Some(&Probe::Text { found: true })), Verdict::Pending);
        assert_eq!(w.step(300, Some(&Probe::Text { found: false })), Verdict::Matched);
    }

    #[test]
    fn an_unreadable_page_never_counts_as_gone() {
        let mut w = Waiter::new(text_spec(true, 500), 0);
        assert_eq!(w.step(100, None), Verdict::Pending);
        assert_eq!(w.step(500, None), Verdict::TimedOut);
    }

    #[test]
    fn a_ref_must_be_present_and_visible_or_absent() {
        let visible = Probe::Element { present: true, visible: true };
        let hidden = Probe::Element { present: true, visible: false };
        let absent = Probe::Element { present: false, visible: false };
        let mode = |gone| Spec { mode: Mode::Ref { reference: "ref_2".into(), gone }, timeout_ms: 1000 };
        let mut w = Waiter::new(mode(false), 0);
        assert_eq!(w.step(0, Some(&absent)), Verdict::Pending);
        assert_eq!(w.step(100, Some(&hidden)), Verdict::Pending);
        assert_eq!(w.step(200, Some(&visible)), Verdict::Matched);
        let mut w = Waiter::new(mode(true), 0);
        assert_eq!(w.step(0, Some(&visible)), Verdict::Pending);
        assert_eq!(w.step(100, Some(&hidden)), Verdict::Matched, "hidden counts as gone");
        let mut w = Waiter::new(mode(true), 0);
        assert_eq!(w.step(100, Some(&absent)), Verdict::Matched);
    }

    fn net(count: u64, last_end: f64, loading: bool) -> Probe {
        Probe::Network { count, last_end, loading }
    }

    #[test]
    fn network_idle_needs_a_quiet_stretch_without_loading() {
        let spec = Spec { mode: Mode::NetworkIdle { ms: 500 }, timeout_ms: 5000 };
        let mut w = Waiter::new(spec, 0);
        assert_eq!(w.step(0, Some(&net(3, 100.0, true))), Verdict::Pending);
        assert_eq!(w.step(200, Some(&net(3, 100.0, true))), Verdict::Pending, "still loading");
        assert_eq!(w.step(400, Some(&net(4, 380.0, false))), Verdict::Pending, "a new request finished");
        assert_eq!(w.step(800, Some(&net(4, 380.0, false))), Verdict::Pending, "400 ms quiet");
        assert_eq!(w.step(900, Some(&net(4, 380.0, false))), Verdict::Matched, "500 ms quiet");
    }

    #[test]
    fn network_that_never_settles_times_out() {
        let spec = Spec { mode: Mode::NetworkIdle { ms: 500 }, timeout_ms: 1000 };
        let mut w = Waiter::new(spec, 0);
        let mut count = 0;
        let mut verdict = Verdict::Pending;
        for now in (0..=1000).step_by(100) {
            count += 1;
            verdict = w.step(now, Some(&net(count, now as f64, false)));
        }
        assert_eq!(verdict, Verdict::TimedOut);
    }

    #[test]
    fn a_pause_ends_after_its_time_whatever_the_timeout() {
        let mut w = Waiter::new(Spec { mode: Mode::Time { ms: 2000 }, timeout_ms: 500 }, 100);
        assert_eq!(w.step(1000, None), Verdict::Pending);
        assert_eq!(w.step(2100, None), Verdict::Matched);
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(fmt_duration(250), "250 ms");
        assert_eq!(fmt_duration(1000), "1 s");
        assert_eq!(fmt_duration(1640), "1.6 s");
        assert_eq!(fmt_duration(10_020), "10 s");
        assert_eq!(fmt_duration(59_960), "60 s");
    }

    #[test]
    fn result_lines_say_what_happened() {
        let text = Mode::Text { text: "Saved".into(), gone: false };
        assert_eq!(outcome_line(&text, Verdict::Matched, 1640), "Matched after 1.6 s: text \"Saved\" appeared.");
        assert_eq!(outcome_line(&text, Verdict::TimedOut, 10_000), "Timed out after 10 s waiting for text \"Saved\" to appear.");
        let gone = Mode::Ref { reference: "ref_9".into(), gone: true };
        assert_eq!(outcome_line(&gone, Verdict::Matched, 300), "Matched after 300 ms: ref_9 is gone.");
        assert_eq!(outcome_line(&gone, Verdict::TimedOut, 2000), "Timed out after 2 s waiting for ref_9 to be gone.");
        assert_eq!(outcome_line(&Mode::NetworkIdle { ms: 500 }, Verdict::TimedOut, 3000), "Timed out after 3 s waiting for the network to be idle for 500 ms.");
        assert_eq!(outcome_line(&Mode::Time { ms: 2000 }, Verdict::Matched, 2004), "Waited 2 s.");
        let long = Mode::Text { text: "x".repeat(200), gone: false };
        assert!(waiting_for(&long).len() < 120);
    }

    #[test]
    fn probe_scripts_name_the_ref_and_the_text_as_literals() {
        let script = probe_script(&Mode::Ref { reference: "f1:ref_3\"".into(), gone: false }).unwrap();
        assert!(script.contains("evoElement(\"f1:ref_3\\\"\")"));
        let script = probe_script(&Mode::Text { text: "a'b".into(), gone: false }).unwrap();
        assert!(script.contains("String(\"a'b\")"));
        assert!(probe_script(&Mode::Time { ms: 5 }).is_none());
    }

    #[test]
    fn probes_are_read_from_the_page_values() {
        let mode = Mode::Ref { reference: "r".into(), gone: false };
        assert_eq!(read_probe(&mode, json!({ "present": true, "visible": false }), false), Some(Probe::Element { present: true, visible: false }));
        assert_eq!(read_probe(&mode, json!("nonsense"), false), None);
        let text = Mode::Text { text: "t".into(), gone: false };
        assert_eq!(read_probe(&text, json!(true), false), Some(Probe::Text { found: true }));
        let net_mode = Mode::NetworkIdle { ms: 100 };
        assert_eq!(
            read_probe(&net_mode, json!({ "count": 2, "lastEnd": 12.5, "loading": false }), true),
            Some(Probe::Network { count: 2, last_end: 12.5, loading: true }),
            "the shell's load events count too"
        );
    }
}
