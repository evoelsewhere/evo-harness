//! A terminal tab's output as plain text, for the agent's terminal tools.
//!
//! Raw pty output goes in through [`OutputLog::feed`]; escape sequences are
//! removed (one split across two chunks is carried over), OSC 133 shell
//! integration marks are stored as markers at the current offset, and the OSC
//! title (0/2) is kept. Offsets are running character counts, so a cursor stays
//! valid while the ring drops its oldest text. [`SharedLog`] adds the waits (for
//! a pattern, for quiet, for a marker, for the end of a command) that a tool call
//! makes without blocking anything else.
//!
//! Port of the webview's `output-log.ts` (ADR 0013). Spec:
//! docs/features/agent-terminal (FR-6, FR-8, FR-9, FR-16, NFR-3, NFR-4, NFR-5).

use std::future::Future;
use std::sync::{Arc, Mutex};

use regex::Regex;
use tokio::sync::Notify;
use tokio::time::{sleep_until, Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Plain output kept per tab.
pub const LOG_MAX: usize = 1024 * 1024;
/// The ring is trimmed down this much below the limit, so it is not trimmed per chunk.
const TRIM_SLACK: usize = 64 * 1024;
/// Without seeing the typed command echoed, a pattern wait scans everything after this long.
const ECHO_PATIENCE_MS: u64 = 1500;
/// A pattern wait re-tests this much text before the new data (a match may straddle chunks).
const PATTERN_OVERLAP: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    A,
    B,
    C,
    D,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub kind: MarkerKind,
    /// Offset in the log where it was seen.
    pub offset: usize,
    /// Order of arrival over the tab's life; unlike `offset`, two markers never share one.
    pub seq: u64,
    /// `D` only: the command's exit code, when the shell reports one.
    pub exit: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitReason {
    Matched,
    Idle,
    CommandEnd,
    Marker,
    Exit,
    Timeout,
    Aborted,
}

#[derive(Clone, Debug)]
pub struct WaitResult {
    pub reason: WaitReason,
    /// The text a pattern matched.
    pub matched: Option<String>,
    pub elapsed_ms: u64,
}

pub struct WaitOptions {
    /// Only output after this offset counts (default: now).
    pub from: Option<usize>,
    pub pattern: Option<Regex>,
    /// Return when nothing arrived for this long.
    pub idle_ms: Option<u64>,
    /// For `idle_ms`: wait for some output first (default true).
    pub need_output: bool,
    /// Return when the command typed after `from_seq` ends (see [`OutputLog::command_span`]).
    pub command_end: bool,
    /// Return at the first marker of this kind seen since `from_seq`.
    pub marker: Option<MarkerKind>,
    /// Only markers that arrived at or after this sequence number count (default: now).
    pub from_seq: Option<u64>,
    /// For `pattern`: the command text that was typed. The shell echoes it, and the
    /// echo must not satisfy the pattern, so scanning starts on the line after it.
    pub after_echo: Option<String>,
    pub timeout_ms: u64,
}

impl WaitOptions {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            from: None,
            pattern: None,
            idle_ms: None,
            need_output: true,
            command_end: false,
            marker: None,
            from_seq: None,
            after_echo: None,
            timeout_ms,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReadResult {
    pub text: String,
    /// Characters between the cursor and the oldest kept text (NFR-3).
    pub dropped: usize,
    /// The cursor was beyond the log (the log restarted, e.g. after a reload).
    pub ahead: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Text,
    Esc,
    Csi,
    Osc,
    OscEsc,
    Str,
    StrEsc,
}

pub struct OutputLog {
    text: Vec<char>,
    /// Offset of `text[0]`.
    base: usize,
    markers: Vec<Marker>,
    next_seq: u64,
    /// `None` while running; `Some(code)` once the pty ended.
    exit: Option<Option<i64>>,
    title: String,
    // Parser state, carried across chunks.
    mode: Mode,
    osc: String,
    last_was_cr: bool,
    csi_params: String,
}

impl Default for OutputLog {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputLog {
    pub fn new() -> Self {
        Self {
            text: Vec::new(),
            base: 0,
            markers: Vec::new(),
            next_seq: 0,
            exit: None,
            title: String::new(),
            mode: Mode::Text,
            osc: String::new(),
            last_was_cr: false,
            csi_params: String::new(),
        }
    }

    /// Offset after the last character.
    pub fn end(&self) -> usize {
        self.base + self.text.len()
    }

    /// Offset of the oldest kept character.
    pub fn start(&self) -> usize {
        self.base
    }

    pub fn exited(&self) -> bool {
        self.exit.is_some()
    }

    /// The exit code once `exited` (`None` when the pty gave none).
    pub fn exit_code(&self) -> Option<i64> {
        self.exit.flatten()
    }

    /// The last window title the shell set (OSC 0/2).
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }

    /// The sequence number the next marker will get: a snapshot to tell new markers from old ones.
    pub fn marker_seq(&self) -> u64 {
        self.next_seq
    }

    /// The shell has shown a marker at all: its integration works in this tab.
    pub fn has_markers(&self) -> bool {
        !self.markers.is_empty()
    }

    /// The text in `[from, end)` (clamped to what is kept).
    pub fn slice(&self, from: usize) -> String {
        self.slice_to(from, self.end())
    }

    /// The text in `[from, to)` (clamped to what is kept).
    pub fn slice_to(&self, from: usize, to: usize) -> String {
        let a = from.saturating_sub(self.base).min(self.text.len());
        let b = to.saturating_sub(self.base).min(self.text.len());
        if b <= a {
            return String::new();
        }
        self.text[a..b].iter().collect()
    }

    /// Raw pty output.
    pub fn feed(&mut self, data: &str) {
        if data.is_empty() {
            return;
        }
        let chars: Vec<char> = data.chars().collect();
        let mut out: Vec<char> = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            match self.mode {
                Mode::Text => {
                    if ch == '\x1b' {
                        self.mode = Mode::Esc;
                    } else if ch == '\r' {
                        self.last_was_cr = true;
                    } else {
                        if self.last_was_cr {
                            self.last_was_cr = false;
                            // A bare carriage return redraws the line: keep it as a line break.
                            if ch != '\n' && last_of(&out, &self.text).unwrap_or('\n') != '\n' {
                                out.push('\n');
                            }
                        }
                        if (ch == '\n' || ch == '\t' || ch >= ' ') && ch != '\x7f' {
                            out.push(ch);
                        }
                    }
                }
                Mode::Esc => {
                    if ch == '[' {
                        self.mode = Mode::Csi;
                        self.csi_params.clear();
                    } else if ch == ']' {
                        self.mode = Mode::Osc;
                        self.osc.clear();
                    } else if matches!(ch, 'P' | 'X' | '^' | '_') {
                        self.mode = Mode::Str;
                    } else if (' '..='/').contains(&ch) {
                        self.mode = Mode::Esc; // intermediate bytes (ESC ( B ...)
                    } else {
                        self.mode = Mode::Text;
                    }
                }
                Mode::Csi => {
                    if ('@'..='~').contains(&ch) {
                        self.mode = Mode::Text;
                        // Command Prompt moves to the next row with an absolute position
                        // (`ESC[11;1H`) where other shells send a line break.
                        if (ch == 'H' || ch == 'f') && self.moves_to_new_row() {
                            if let Some(last) = last_of(&out, &self.text) {
                                if last != '\n' {
                                    out.push('\n');
                                }
                            }
                        }
                    } else if self.csi_params.len() < 32 {
                        self.csi_params.push(ch);
                    }
                }
                Mode::Osc => {
                    if ch == '\x07' {
                        self.flush(&mut out);
                        self.finish_osc();
                    } else if ch == '\x1b' {
                        self.mode = Mode::OscEsc;
                    } else if self.osc.chars().count() < 512 {
                        self.osc.push(ch);
                    }
                }
                Mode::OscEsc => {
                    if ch == '\\' {
                        self.flush(&mut out);
                        self.finish_osc();
                    } else {
                        // Not a terminator: the sequence was cut short by a new escape.
                        self.mode = Mode::Esc;
                        continue; // reprocess this character
                    }
                }
                Mode::Str => {
                    if ch == '\x07' {
                        self.mode = Mode::Text;
                    } else if ch == '\x1b' {
                        self.mode = Mode::StrEsc;
                    }
                }
                Mode::StrEsc => {
                    self.mode = if ch == '\\' { Mode::Text } else { Mode::Str };
                }
            }
            i += 1;
        }
        self.flush(&mut out);
        self.trim();
    }

    fn flush(&mut self, out: &mut Vec<char>) {
        if !out.is_empty() {
            self.text.append(out);
        }
    }

    /// The pending cursor position names a row and column 1 (or no column).
    fn moves_to_new_row(&self) -> bool {
        let mut parts = self.csi_params.split(';');
        let row = parts.next().unwrap_or("");
        let column = parts.next();
        !row.is_empty() && matches!(column, None | Some("") | Some("1"))
    }

    fn finish_osc(&mut self) {
        self.mode = Mode::Text;
        let body = std::mem::take(&mut self.osc);
        // Window title: OSC 0 and OSC 2.
        for prefix in ["0;", "2;"] {
            if let Some(title) = body.strip_prefix(prefix) {
                self.title = title.to_string();
                return;
            }
        }
        let Some(rest) = body.strip_prefix("133;") else {
            return;
        };
        let mut chars = rest.chars();
        let kind = match chars.next() {
            Some('A') => MarkerKind::A,
            Some('B') => MarkerKind::B,
            Some('C') => MarkerKind::C,
            Some('D') => MarkerKind::D,
            _ => return,
        };
        let tail = chars.as_str();
        let exit = tail.strip_prefix(';').and_then(|digits| {
            let end = digits
                .char_indices()
                .find(|(i, c)| !(c.is_ascii_digit() || (*i == 0 && *c == '-')))
                .map_or(digits.len(), |(i, _)| i);
            let number = &digits[..end];
            number.parse::<i64>().ok()
        });
        let marker = Marker {
            kind,
            offset: self.end(),
            seq: self.next_seq,
            exit,
        };
        self.next_seq += 1;
        self.markers.push(marker);
        if self.markers.len() > 2000 {
            let drop = self.markers.len() - 1000;
            self.markers.drain(..drop);
        }
    }

    fn trim(&mut self) {
        // Never keep more than LOG_MAX; when over, cut down to a little below it.
        if self.text.len() <= LOG_MAX {
            return;
        }
        let cut = self.text.len() - (LOG_MAX - TRIM_SLACK);
        self.text.drain(..cut);
        self.base += cut;
        let keep_from = self
            .markers
            .iter()
            .position(|m| m.offset >= self.base)
            .unwrap_or(self.markers.len());
        self.markers.drain(..keep_from);
    }

    /// The pty ended.
    pub fn close(&mut self, code: Option<i64>) {
        self.exit = Some(code);
    }

    /// What was written after `since`.
    pub fn read(&self, since: usize) -> ReadResult {
        let ahead = since > self.end();
        let from = if ahead {
            self.base
        } else {
            since.max(self.base)
        };
        ReadResult {
            text: self.text[from - self.base..].iter().collect(),
            dropped: self.base.saturating_sub(since),
            ahead,
        }
    }

    /// First marker of `kind` at or after `offset`.
    #[cfg(test)]
    pub fn marker_after(&self, kind: MarkerKind, offset: usize) -> Option<&Marker> {
        self.markers
            .iter()
            .find(|m| m.kind == kind && m.offset >= offset)
    }

    /// The command typed after the marker snapshot `from_seq`: where its output
    /// starts (`C`) and the `D` that ends it. A shell that reports `C` ends a
    /// command only at a `D` after its own `C`, so the end of an earlier command
    /// that finishes after `from_seq` is not taken for it; Command Prompt reports no
    /// `C`, so any later `D` counts.
    pub fn command_span(&self, from_seq: u64) -> (Option<Marker>, Option<Marker>) {
        let fresh = || self.markers.iter().filter(|m| m.seq >= from_seq);
        let started = fresh().find(|m| m.kind == MarkerKind::C).cloned();
        let reports_start =
            started.is_some() || self.markers.iter().any(|m| m.kind == MarkerKind::C);
        let done = fresh()
            .find(|m| {
                m.kind == MarkerKind::D
                    && (!reports_start || started.as_ref().is_some_and(|s| m.seq > s.seq))
            })
            .cloned();
        (started, done)
    }

    /// Where the pattern may start matching: after the echoed command line; `None`: not yet.
    fn echo_end(&self, from: usize, echo: &[char], waited_ms: u64) -> Option<usize> {
        let begin = from.max(self.base);
        let region = &self.text[(begin - self.base).min(self.text.len())..];
        match find_chars(region, echo, 0) {
            None => (waited_ms > ECHO_PATIENCE_MS).then_some(begin),
            Some(at) => {
                let line_end = find_chars(region, &['\n'], at + echo.len())?;
                Some(begin + line_end + 1)
            }
        }
    }
}

/// The last character of `out`, else of `text`.
fn last_of(out: &[char], text: &[char]) -> Option<char> {
    out.last().or_else(|| text.last()).copied()
}

/// Index of `needle` in `hay` at or after `start`.
fn find_chars(hay: &[char], needle: &[char], start: usize) -> Option<usize> {
    if needle.is_empty() {
        return (start <= hay.len()).then_some(start);
    }
    if hay.len() < needle.len() || start > hay.len() - needle.len() {
        return None;
    }
    (start..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

/// The last `max` characters of `text`, with a note when something was cut (NFR-4).
pub fn tail(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    let cut = chars.len() - max;
    let rest = &chars[cut..];
    // Start on a line when one begins soon, so the first line is whole.
    let kept = match rest.iter().position(|&c| c == '\n') {
        Some(newline) if newline < 200 => &rest[newline + 1..],
        _ => rest,
    };
    let kept_text: String = kept.iter().collect();
    format!(
        "… {} characters cut (showing the last {})\n{}",
        chars.len() - kept.len(),
        kept.len(),
        kept_text
    )
}

/// A log shared between the pty's reader thread (which feeds it) and the tool
/// calls that wait on it.
pub struct SharedLog {
    inner: Mutex<OutputLog>,
    notify: Notify,
}

impl Default for SharedLog {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedLog {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(OutputLog::new()),
            notify: Notify::new(),
        }
    }

    /// Read (or change) the log under its lock; never hold it across an await.
    pub fn with<R>(&self, f: impl FnOnce(&mut OutputLog) -> R) -> R {
        f(&mut self.inner.lock().expect("output log lock"))
    }

    pub fn feed(&self, data: &str) {
        self.with(|log| log.feed(data));
        self.notify.notify_waiters();
    }

    pub fn close(&self, code: Option<i64>) {
        self.with(|log| log.close(code));
        self.notify.notify_waiters();
    }

    /// Wait until the first of: the pattern shows in new output, the output has
    /// been quiet for `idle_ms`, the command ends, a marker shows, the pty exits,
    /// the time runs out, or `cancel` fires. The starting point (`from`,
    /// `from_seq`) is fixed when this is called, not when the future is first
    /// polled. Waiting never blocks anything else (NFR-5).
    pub fn wait(
        self: &Arc<Self>,
        options: WaitOptions,
        cancel: &CancellationToken,
    ) -> impl Future<Output = WaitResult> + Send + 'static {
        let this = Arc::clone(self);
        let cancel = cancel.clone();
        let started = Instant::now();
        let (from, from_seq, mut scanned) = this.with(|log| {
            let from = options.from.unwrap_or(log.end());
            (
                from,
                options.from_seq.unwrap_or(log.marker_seq()),
                from.max(log.start()),
            )
        });
        let echo: Option<Vec<char>> = options.after_echo.as_deref().map(|e| e.chars().collect());
        async move {
            let deadline = started + Duration::from_millis(options.timeout_ms);
            let mut idle_deadline: Option<Instant> = None;
            let mut echo_at: Option<Instant> = echo
                .as_ref()
                .map(|_| started + Duration::from_millis(ECHO_PATIENCE_MS + 50));
            let done = |reason: WaitReason, matched: Option<String>| WaitResult {
                reason,
                matched,
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
            if cancel.is_cancelled() {
                return done(WaitReason::Aborted, None);
            }
            loop {
                let notified = this.notify.notified();
                tokio::pin!(notified);
                // Test the log: output that arrived between `from` and now counts too.
                let verdict = this.with(|log| {
                    if let Some(pattern) = &options.pattern {
                        let mut begin = from
                            .max(log.start())
                            .max(scanned.saturating_sub(PATTERN_OVERLAP));
                        let echoed = match &echo {
                            Some(echo) => {
                                log.echo_end(from, echo, started.elapsed().as_millis() as u64)
                            }
                            None => Some(from),
                        };
                        if let Some(echoed) = echoed {
                            begin = begin.max(echoed);
                            let region = log.slice(begin);
                            scanned = log.end();
                            if let Some(found) = pattern.find(&region) {
                                return Some((
                                    WaitReason::Matched,
                                    Some(found.as_str().to_string()),
                                ));
                            }
                        }
                    }
                    if let Some(kind) = options.marker {
                        if log
                            .markers()
                            .iter()
                            .any(|m| m.kind == kind && m.seq >= from_seq)
                        {
                            return Some((WaitReason::Marker, None));
                        }
                    }
                    if options.command_end && log.command_span(from_seq).1.is_some() {
                        return Some((WaitReason::CommandEnd, None));
                    }
                    if log.exited() {
                        return Some((WaitReason::Exit, None));
                    }
                    // Arm the quiet timer: it restarts at every check.
                    idle_deadline = None;
                    if let Some(idle) = options.idle_ms {
                        if !(options.need_output && log.end() <= from) {
                            idle_deadline = Some(Instant::now() + Duration::from_millis(idle));
                        }
                    }
                    None
                });
                if let Some((reason, matched)) = verdict {
                    return done(reason, matched);
                }
                let mut next = deadline;
                for at in [idle_deadline, echo_at].into_iter().flatten() {
                    next = next.min(at);
                }
                let timer = tokio::select! {
                    _ = &mut notified => false,
                    _ = sleep_until(next) => true,
                    _ = cancel.cancelled() => return done(WaitReason::Aborted, None),
                };
                if timer {
                    let now = Instant::now();
                    if now >= deadline {
                        return done(WaitReason::Timeout, None);
                    }
                    if idle_deadline.is_some_and(|at| now >= at) {
                        return done(WaitReason::Idle, None);
                    }
                    if echo_at.is_some_and(|at| now >= at) {
                        // Re-test once the echo has had its time, in case no more output comes.
                        echo_at = None;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> Arc<SharedLog> {
        Arc::new(SharedLog::new())
    }

    fn token() -> CancellationToken {
        CancellationToken::new()
    }

    fn later(log: &Arc<SharedLog>, ms: u64, data: &'static str) {
        let log = log.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            log.feed(data);
        });
    }

    const C: &str = "\x1b]133;C\x07";
    const PROMPT: &str = "\x1b]133;A\x07$ \x1b]133;B\x07";
    fn d(code: i32) -> String {
        format!("\x1b]133;D;{code}\x07")
    }

    #[test]
    fn strips_escape_sequences_also_when_split_across_chunks() {
        let mut log = OutputLog::new();
        log.feed("\x1b[31mred\x1b[0m \x1b]0;title\x07ok \x1b[");
        log.feed("1;2Hmore\r\nnext\x1b]8;;http://x\x1b\\");
        assert_eq!(log.slice(0), "red ok more\nnext");
        assert_eq!(log.title(), "title");
    }

    #[test]
    fn cursor_returns_only_new_output() {
        let mut log = OutputLog::new();
        log.feed("one\n");
        let first = log.read(0);
        assert_eq!(first.text, "one\n");
        let cursor = log.end();
        log.feed("two\n");
        let second = log.read(cursor);
        assert_eq!(second.text, "two\n");
        assert_eq!(log.read(log.end()).text, "");
    }

    #[test]
    fn ring_drops_old_text_and_reports_it_for_a_stale_cursor() {
        let mut log = OutputLog::new();
        let line = format!("{}\n", "x".repeat(1023));
        for _ in 0..1300 {
            log.feed(&line);
        }
        assert_eq!(log.end(), 1300 * 1024);
        assert!(log.end() - log.start() <= LOG_MAX);
        let read = log.read(0);
        assert_eq!(read.dropped, log.start());
        assert_eq!(read.text.chars().count(), log.end() - log.start());
    }

    #[test]
    fn a_cursor_beyond_the_end_is_flagged() {
        let mut log = OutputLog::new();
        log.feed("abc");
        let read = log.read(999);
        assert!(read.ahead);
        assert_eq!(read.text, "abc");
    }

    #[test]
    fn osc_133_markers_are_stored_at_their_offset() {
        let mut log = OutputLog::new();
        log.feed("\x1b]133;A\x07$ \x1b]133;B\x07");
        let from = log.end();
        log.feed("ls\r\n\x1b]133;C\x07file\r\n\x1b]133;D;3\x07\x1b]133;A\x1b\\");
        let c = log.marker_after(MarkerKind::C, from).unwrap().clone();
        let dm = log.marker_after(MarkerKind::D, from).unwrap().clone();
        assert_eq!(dm.exit, Some(3));
        assert_eq!(log.slice_to(c.offset, dm.offset), "file\n");
    }

    #[test]
    fn a_negative_exit_code_and_a_missing_one() {
        let mut log = OutputLog::new();
        log.feed("\x1b]133;D;-1\x07\x1b]133;D\x07\x1b]133;D;x\x07");
        let exits: Vec<Option<i64>> = log.markers().iter().map(|m| m.exit).collect();
        assert_eq!(exits, vec![Some(-1), None, None]);
    }

    #[tokio::test]
    async fn waits_for_a_pattern_in_new_output_only() {
        let log = shared();
        log.feed("ready on 1\n");
        let waiting = log.wait(
            WaitOptions {
                pattern: Some(Regex::new(r"ready on (\d+)").unwrap()),
                ..WaitOptions::new(1000)
            },
            &token(),
        );
        later(&log, 20, "ready on 3000\n");
        let result = waiting.await;
        assert_eq!(result.reason, WaitReason::Matched);
        assert_eq!(result.matched.as_deref(), Some("ready on 3000"));
    }

    #[tokio::test]
    async fn a_pattern_split_over_two_chunks_matches() {
        let log = shared();
        let waiting = log.wait(
            WaitOptions {
                pattern: Some(Regex::new("ready on").unwrap()),
                ..WaitOptions::new(1000)
            },
            &token(),
        );
        log.feed("rea");
        later(&log, 10, "dy on");
        assert_eq!(waiting.await.reason, WaitReason::Matched);
    }

    #[tokio::test]
    async fn idle_wait_returns_after_quiet_and_timeout_returns_timeout() {
        let log = shared();
        let idle = log.wait(
            WaitOptions {
                idle_ms: Some(30),
                ..WaitOptions::new(1000)
            },
            &token(),
        );
        log.feed("a");
        assert_eq!(idle.await.reason, WaitReason::Idle);
        let never = log.wait(
            WaitOptions {
                pattern: Some(Regex::new("never").unwrap()),
                ..WaitOptions::new(30)
            },
            &token(),
        );
        assert_eq!(never.await.reason, WaitReason::Timeout);
    }

    #[tokio::test]
    async fn exit_and_abort_end_a_wait() {
        let log = shared();
        let waiting = log.wait(
            WaitOptions {
                pattern: Some(Regex::new("x").unwrap()),
                ..WaitOptions::new(1000)
            },
            &token(),
        );
        log.close(Some(2));
        assert_eq!(waiting.await.reason, WaitReason::Exit);
        let cancel = token();
        let other = shared().wait(
            WaitOptions {
                pattern: Some(Regex::new("x").unwrap()),
                ..WaitOptions::new(1000)
            },
            &cancel,
        );
        cancel.cancel();
        assert_eq!(other.await.reason, WaitReason::Aborted);
    }

    #[tokio::test]
    async fn command_end_wait_sees_the_next_d_marker() {
        let log = shared();
        let waiting = log.wait(
            WaitOptions {
                command_end: true,
                ..WaitOptions::new(1000)
            },
            &token(),
        );
        log.feed(&format!("{C}hi\n{}", d(0)));
        assert_eq!(waiting.await.reason, WaitReason::CommandEnd);
    }

    #[tokio::test]
    async fn a_command_does_not_end_at_the_previous_commands_d_at_the_same_offset() {
        let log = shared();
        log.feed(&format!("{PROMPT}{C}one\n{}", d(0)));
        // The first call returned at that D. The next one starts right there.
        let (from, from_seq) = log.with(|l| (l.end(), l.marker_seq()));
        assert!(log.with(|l| l.marker_after(MarkerKind::D, from).is_some())); // an offset-based test would call this "done"
        assert!(log.with(|l| l.command_span(from_seq).1.is_none()));
        let waiting = log.wait(
            WaitOptions {
                from: Some(from),
                from_seq: Some(from_seq),
                command_end: true,
                ..WaitOptions::new(1000)
            },
            &token(),
        );
        log.feed(PROMPT);
        log.feed(&format!("(exit 5)\n{C}{}", d(5)));
        assert_eq!(waiting.await.reason, WaitReason::CommandEnd);
        assert_eq!(
            log.with(|l| l.command_span(from_seq).1.unwrap().exit),
            Some(5)
        );
    }

    #[test]
    fn with_c_markers_only_a_d_after_the_commands_own_c_ends_it() {
        let mut log = OutputLog::new();
        log.feed(&format!("{PROMPT}{C}old\n"));
        let from_seq = log.marker_seq();
        // An earlier command finishes after ours was typed, before ours starts.
        log.feed(&format!("{}{PROMPT}", d(0)));
        assert!(log.command_span(from_seq).1.is_none());
        log.feed(&format!("{C}mine\n{}", d(2)));
        let (started, done) = log.command_span(from_seq);
        let (started, done) = (started.unwrap(), done.unwrap());
        assert_eq!(done.exit, Some(2));
        assert_eq!(log.slice_to(started.offset, done.offset), "mine\n");
    }

    #[test]
    fn a_shell_that_reports_no_c_command_prompt_ends_at_any_later_d() {
        let mut log = OutputLog::new();
        log.feed("\x1b]133;D\x07\x1b]133;A\x07C:\\> \x1b]133;B\x07");
        let from_seq = log.marker_seq();
        log.feed("hi\n\x1b]133;D\x07\x1b]133;A\x07");
        let (started, done) = log.command_span(from_seq);
        let done = done.unwrap();
        assert_eq!(done.exit, None);
        assert!(started.is_none());
    }

    #[test]
    fn command_prompts_move_to_the_next_row_is_a_line_break() {
        let mut log = OutputLog::new();
        log.feed("echo again\r\n");
        log.feed("\x1b[?25lagain\x1b[10;1H\x1b[?25h\x1b]133;D\x1b\\\x1b]133;A\x1b\\C:\\> \x1b]133;B\x1b\\");
        assert_eq!(log.slice(0), "echo again\nagain\nC:\\> ");
        // A move to a column past 1, to the home position, or at a line start adds nothing.
        let mut other = OutputLog::new();
        other.feed("ab\x1b[3;5Hcd\x1b[Hef\ngh\x1b[7;1Hij");
        assert_eq!(other.slice(0), "abcdef\ngh\nij");
    }

    #[tokio::test]
    async fn waits_for_the_first_prompt_end() {
        let log = shared();
        let waiting = log.wait(
            WaitOptions {
                marker: Some(MarkerKind::B),
                from_seq: Some(0),
                ..WaitOptions::new(1000)
            },
            &token(),
        );
        log.feed("banner\n\x1b]133;A\x07");
        later(&log, 20, "$ \x1b]133;B\x07");
        assert_eq!(waiting.await.reason, WaitReason::Marker);
    }

    #[tokio::test]
    async fn the_typed_commands_echo_does_not_satisfy_a_pattern() {
        let log = shared();
        let from = log.with(|l| l.end());
        let waiting = log.wait(
            WaitOptions {
                from: Some(from),
                pattern: Some(Regex::new("ready on").unwrap()),
                after_echo: Some("node server --msg 'ready on'".into()),
                ..WaitOptions::new(1000)
            },
            &token(),
        );
        log.feed("node server --msg 'ready on'");
        log.feed("\r\n");
        later(&log, 30, "starting\nready on 3000\n");
        let result = waiting.await;
        assert_eq!(result.reason, WaitReason::Matched);
        assert!(result.elapsed_ms >= 25, "elapsed {}", result.elapsed_ms);
    }

    #[tokio::test]
    async fn without_a_recognisable_echo_the_pattern_still_matches_after_a_while() {
        let log = shared();
        let waiting = log.wait(
            WaitOptions {
                from: Some(0),
                pattern: Some(Regex::new("done").unwrap()),
                after_echo: Some("echo not-in-output".into()),
                ..WaitOptions::new(4000)
            },
            &token(),
        );
        log.feed("something done\n");
        assert_eq!(waiting.await.reason, WaitReason::Matched);
    }

    #[test]
    fn tail_keeps_short_text_and_cuts_long_text_with_a_note() {
        assert_eq!(tail("short", 20_000), "short");
        let long = (0..5000)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let cut = tail(&long, 20_000);
        assert!(cut.starts_with("… "));
        assert!(cut.chars().count() < 20_200);
        assert!(cut.ends_with("line 4999"));
    }
}
