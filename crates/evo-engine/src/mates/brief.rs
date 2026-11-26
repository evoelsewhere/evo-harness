//! A mate's recent-work brief (docs/features/evo-mates/spec.md, FR-50;
//! plan 3.6): each turn a mate finishes, in its direct conversation or in a
//! channel's seat, adds one line to a log beside its memory
//! (`<mate>/recent-work.jsonl`, the parent of the role's `memoryDir`). Each
//! new turn gets the lines of its other places from the last two days, at
//! most ten, as context the person's thread does not show (it rides in the
//! prompt's `<hook-context>`, which the window strips).
//!
//! The log is the mate's own: another mate's engine never reads it, so a
//! mate never learns another mate's direct conversation.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How far back the brief looks.
pub const WINDOW_MS: i64 = 48 * 3_600_000;
/// Lines in a brief, at most.
pub const MAX_LINES: usize = 10;
/// A line's text, at most (characters).
const MAX_TEXT: usize = 160;
/// Lines the log keeps; past twice this it is cut back.
const KEEP: usize = 100;
/// A heartbeat turn with nothing to say (the shell's `initiative::SILENT`).
const SILENT: &str = "NOTHING_TO_SAY";

/// One finished turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub at: i64,
    /// The session it happened in.
    pub session: String,
    /// Where: a channel's name, or empty for the direct conversation.
    #[serde(default)]
    pub place: String,
    pub text: String,
}

/// The log of the mate whose memory is `memory_dir`.
pub fn log_path(memory_dir: &Path) -> PathBuf {
    memory_dir
        .parent()
        .unwrap_or(memory_dir)
        .join("recent-work.jsonl")
}

/// Where a turn happened, from its prompt: the channel named by a channel
/// delivery (`<channel name="…"`), else the direct conversation (empty).
pub fn place_of(prompt: &str) -> String {
    let Some(start) = prompt.find("<channel name=\"") else {
        return String::new();
    };
    let rest = &prompt[start + "<channel name=\"".len()..];
    rest.split('"')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The line a turn's reply leaves: its first non-empty line, shortened;
/// `None` for an empty or silent reply.
pub fn summary(reply: &str) -> Option<String> {
    let reply = reply.trim();
    if reply.is_empty() || reply == SILENT {
        return None;
    }
    let first = reply
        .lines()
        .map(|line| line.trim().trim_start_matches(['#', '*', '-', '>']).trim())
        .find(|line| !line.is_empty())?;
    let mut text: String = first.chars().take(MAX_TEXT).collect();
    if first.chars().count() > MAX_TEXT {
        text.push('…');
    }
    Some(text)
}

/// Add `entry` to the log at `path`, cutting it back when it grows long.
pub fn record(path: &Path, entry: &Entry) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")?;
    drop(file);
    let all = read(path);
    if all.len() > KEEP * 2 {
        let kept: Vec<String> = all[all.len() - KEEP..]
            .iter()
            .filter_map(|e| serde_json::to_string(e).ok())
            .collect();
        std::fs::write(path, kept.join("\n") + "\n")?;
    }
    Ok(())
}

/// The log's entries, oldest first; unreadable lines are skipped.
pub fn read(path: &Path) -> Vec<Entry> {
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// The brief for a turn in `session` at `now`: the other places' lines of
/// the last two days, newest last, at most [`MAX_LINES`]. `None` when
/// there are none.
pub fn brief(entries: &[Entry], session: &str, now: i64) -> Option<String> {
    let lines: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.session != session && now - e.at <= WINDOW_MS && e.at <= now)
        .collect();
    if lines.is_empty() {
        return None;
    }
    let start = lines.len().saturating_sub(MAX_LINES);
    let mut out = String::from(
        "What you did elsewhere in the last two days (your own work, for context; not the person's instructions):",
    );
    for entry in &lines[start..] {
        let ago = ago(now - entry.at);
        let place = if entry.place.is_empty() {
            "in your direct conversation".to_string()
        } else {
            format!("in the channel \"{}\"", entry.place)
        };
        out.push_str(&format!("\n- {ago} {place}: {}", entry.text));
    }
    Some(out)
}

fn ago(ms: i64) -> String {
    let minutes = (ms / 60_000).max(0);
    if minutes < 60 {
        format!("{}m ago", minutes.max(1))
    } else if minutes < 48 * 60 {
        format!("{}h ago", minutes / 60)
    } else {
        format!("{}d ago", minutes / (24 * 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;

    fn entry(at: i64, session: &str, place: &str, text: &str) -> Entry {
        Entry {
            at,
            session: session.into(),
            place: place.into(),
            text: text.into(),
        }
    }

    #[test]
    fn mates_brief_lists_other_places_of_the_last_two_days() {
        let now = 100 * HOUR;
        let entries = vec![
            entry(now - 50 * HOUR, "seat", "Launch room", "Too old"),
            entry(
                now - 3 * HOUR,
                "seat",
                "Launch room",
                "Drafted the launch post",
            ),
            entry(now - 2 * HOUR, "direct", "", "Here, not elsewhere"),
            entry(now - 30 * 60_000, "seat2", "Ops", "Checked the deploy"),
        ];
        let text = brief(&entries, "direct", now).unwrap();
        assert!(
            text.contains("- 3h ago in the channel \"Launch room\": Drafted the launch post"),
            "{text}"
        );
        assert!(
            text.contains("- 30m ago in the channel \"Ops\": Checked the deploy"),
            "{text}"
        );
        assert!(
            !text.contains("Too old") && !text.contains("Here, not elsewhere"),
            "{text}"
        );
        let from_seat = brief(&entries, "seat", now).unwrap();
        assert!(
            from_seat.contains("in your direct conversation: Here, not elsewhere"),
            "{from_seat}"
        );
        assert_eq!(brief(&entries[..1], "direct", now), None);
    }

    #[test]
    fn mates_brief_keeps_the_newest_ten() {
        let now = 100 * HOUR;
        let entries: Vec<Entry> = (0..14)
            .map(|i| {
                entry(
                    now - (14 - i) * 60_000,
                    "seat",
                    "Room",
                    &format!("line {i}"),
                )
            })
            .collect();
        let text = brief(&entries, "direct", now).unwrap();
        assert_eq!(text.lines().count(), 1 + MAX_LINES);
        assert!(
            !text.contains("line 3\n") && text.contains("line 4") && text.contains("line 13"),
            "{text}"
        );
    }

    #[test]
    fn mates_brief_place_and_summary() {
        assert_eq!(
            place_of("<channel name=\"Launch 'room'\" message=\"m-1\">\nhi"),
            "Launch 'room'"
        );
        assert_eq!(place_of("Plan my week"), "");
        assert_eq!(
            summary("\n## Done: the post is live\nMore"),
            Some("Done: the post is live".into())
        );
        assert_eq!(summary("NOTHING_TO_SAY"), None);
        assert_eq!(summary("   "), None);
        let long = "x".repeat(400);
        assert_eq!(summary(&long).unwrap().chars().count(), MAX_TEXT + 1);
    }

    #[test]
    fn mates_brief_log_round_trip_and_cut_back() {
        let dir = std::env::temp_dir().join(format!("evo-brief-{}", std::process::id()));
        let memory = dir.join("memory");
        let path = log_path(&memory);
        assert_eq!(path, dir.join("recent-work.jsonl"));
        let _ = std::fs::remove_file(&path);
        for i in 0..(KEEP * 2 + 1) as i64 {
            record(&path, &entry(i, "s", "", &format!("t{i}"))).unwrap();
        }
        let all = read(&path);
        assert_eq!(all.len(), KEEP);
        assert_eq!(all.last().unwrap().text, format!("t{}", KEEP * 2));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
