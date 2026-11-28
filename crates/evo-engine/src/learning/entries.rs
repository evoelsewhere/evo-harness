//! Dated entries inside a memory topic: when a fact was saved, where it came
//! from, the day it ends and whether a newer fact replaced it
//! (docs/features/learning-layers/spec.md, FR-4, FR-5; plan 3.4).
//!
//! ```text
//! - 2026-11-28 · from chat "Plans" · The person wants drafts under 150 words
//! - 2026-11-28 · from chat "Trip" · The person is in Hanoi · until 2026-12-15
//! - ~~2026-10-02 · lives in Hanoi~~ · superseded 2026-11-28
//! - 2026-11-28 · dream · The person usually asks for numbers on Mondays
//! ```
//!
//! Any other line of a topic is kept exactly as written: topics stay
//! person-readable and hand-editable.

/// One dated entry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    /// The day it was saved, `YYYY-MM-DD`.
    pub date: String,
    /// Where it came from: `from chat "…"`, `from channel "…"`, `dream`,
    /// `tidy`, `person`, `agent` or `index`.
    pub source: Option<String>,
    pub text: String,
    /// The last day it holds.
    pub until: Option<String>,
    /// The day a newer fact replaced it.
    pub superseded: Option<String>,
}

/// A line of a topic body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Entry(Entry),
    Other(String),
}

const SEP: &str = " · ";
const SOURCES: &[&str] = &[
    "dream", "tidy", "person", "agent", "index", "review", "retro",
];

/// `YYYY-MM-DD`, a real calendar day.
pub fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| text[range].parse::<u32>().ok();
    matches!(
        (number(0..4), number(5..7), number(8..10)),
        (Some(_), Some(1..=12), Some(1..=31))
    )
}

fn is_source(segment: &str) -> bool {
    SOURCES.contains(&segment) || segment.starts_with("from ")
}

/// `date · [source ·] text [· until date]`.
fn parse_inner(inner: &str) -> Option<Entry> {
    let mut parts: Vec<&str> = inner.split(SEP).collect();
    if parts.len() < 2 || !is_date(parts[0].trim()) {
        return None;
    }
    let date = parts.remove(0).trim().to_string();
    let until = match parts
        .last()
        .and_then(|last| last.trim().strip_prefix("until "))
    {
        Some(day) if is_date(day.trim()) && parts.len() > 1 => {
            let day = day.trim().to_string();
            parts.pop();
            Some(day)
        }
        _ => None,
    };
    let source =
        (parts.len() > 1 && is_source(parts[0].trim())).then(|| parts.remove(0).trim().to_string());
    let text = parts.join(SEP).trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(Entry {
        date,
        source,
        text,
        until,
        superseded: None,
    })
}

/// Read one line of a topic body.
pub fn parse_line(line: &str) -> Line {
    let other = || Line::Other(line.to_string());
    let Some(rest) = line.trim_start().strip_prefix("- ") else {
        return other();
    };
    let rest = rest.trim_end();
    if let Some(struck) = rest.strip_prefix("~~") {
        let Some(close) = struck.find("~~") else {
            return other();
        };
        let after = struck[close + 2..].trim();
        let superseded = after
            .strip_prefix("·")
            .map(str::trim)
            .and_then(|tail| tail.strip_prefix("superseded "))
            .map(str::trim)
            .filter(|day| is_date(day));
        return match (parse_inner(&struck[..close]), superseded) {
            (Some(mut entry), Some(day)) => {
                entry.superseded = Some(day.to_string());
                Line::Entry(entry)
            }
            _ => other(),
        };
    }
    parse_inner(rest).map_or_else(other, Line::Entry)
}

/// Read a topic body.
pub fn parse(body: &str) -> Vec<Line> {
    body.lines().map(parse_line).collect()
}

/// A yearly date (a birthday, an anniversary) never ends (FR-4).
pub fn is_yearly(text: &str) -> bool {
    let lower = text.to_lowercase();
    ["birthday", "anniversary", "every year", "yearly", "annual"]
        .iter()
        .any(|word| lower.contains(word))
}

impl Entry {
    pub fn new(date: &str, source: Option<&str>, text: &str, until: Option<&str>) -> Self {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let until = until
            .filter(|day| is_date(day) && !is_yearly(&text))
            .map(str::to_string);
        Entry {
            date: date.to_string(),
            source: source.map(str::to_string),
            text,
            until,
            superseded: None,
        }
    }

    fn inner(&self) -> String {
        let mut out = self.date.clone();
        if let Some(source) = &self.source {
            out.push_str(SEP);
            out.push_str(source);
        }
        out.push_str(SEP);
        out.push_str(&self.text);
        if let Some(until) = &self.until {
            out.push_str(&format!("{SEP}until {until}"));
        }
        out
    }

    /// The entry as a line of a topic body.
    pub fn render(&self) -> String {
        match &self.superseded {
            Some(day) => format!("- ~~{}~~{SEP}superseded {day}", self.inner()),
            None => format!("- {}", self.inner()),
        }
    }

    /// It ended before `today` (`YYYY-MM-DD`; days compare as text).
    pub fn expired(&self, today: &str) -> bool {
        self.until
            .as_deref()
            .is_some_and(|until| until < today && !is_yearly(&self.text))
    }

    /// The same fact, however it is spaced, cased or ended.
    pub fn key(&self) -> String {
        fact_key(&self.text)
    }
}

/// A fact compared without spacing, case, bullets or a final stop.
pub fn fact_key(text: &str) -> String {
    text.trim()
        .trim_start_matches(['-', '*', ' '])
        .trim_end_matches(['.', '!', ' '])
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Write lines back as a body.
pub fn render(lines: &[Line]) -> String {
    lines
        .iter()
        .map(|line| match line {
            Line::Entry(entry) => entry.render(),
            Line::Other(text) => text.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What reaches a turn: the body without entries that ended before `today`
/// and without struck ones (FR-4, FR-5). Other lines stay.
pub fn visible(body: &str, today: &str) -> String {
    let kept: Vec<Line> = parse(body)
        .into_iter()
        .filter(|line| match line {
            Line::Entry(entry) => entry.superseded.is_none() && !entry.expired(today),
            Line::Other(_) => true,
        })
        .collect();
    render(&kept).trim().to_string()
}

/// Take the entries that ended before `today` out of `body`: the body left,
/// and the archive lines (each `· moved <today>`).
pub fn take_expired(body: &str, today: &str) -> (String, Vec<String>) {
    let mut kept = Vec::new();
    let mut moved = Vec::new();
    for line in parse(body) {
        match line {
            Line::Entry(entry) if entry.expired(today) => {
                moved.push(format!("{}{SEP}moved {today}", entry.render()));
            }
            other => kept.push(other),
        }
    }
    (render(&kept).trim().to_string(), moved)
}

/// Add archive lines to an archive file's text, under a heading per topic.
pub fn append_archive(archive: &str, topic: &str, moved: &[String]) -> String {
    let mut out = if archive.trim().is_empty() {
        "# Archive\n\nEntries that ended or were moved out of a topic.\n".to_string()
    } else {
        archive.trim_end().to_string() + "\n"
    };
    out.push_str(&format!("\n## {topic}\n"));
    for line in moved {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Today's local date, `YYYY-MM-DD`.
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "Notes the person wrote by hand.\n\
- 2026-11-28 · from chat \"Plans\" · The person wants drafts under 150 words\n\
- 2026-11-28 · from chat \"Trip\" · The person is in Hanoi · until 2026-12-15\n\
- ~~2026-10-02 · lives in Hanoi~~ · superseded 2026-11-28\n\
- 2026-11-28 · dream · The person usually asks for numbers on Mondays\n\
- a plain bullet · with a dot";

    // FR-4, FR-5: the grammar reads and writes back unchanged.
    #[test]
    fn entries_parse_and_write_back() {
        let lines = parse(BODY);
        assert_eq!(render(&lines), BODY, "every line round-trips");
        let Line::Entry(trip) = &lines[2] else {
            panic!("an entry")
        };
        assert_eq!(
            trip,
            &Entry {
                date: "2026-11-28".into(),
                source: Some("from chat \"Trip\"".into()),
                text: "The person is in Hanoi".into(),
                until: Some("2026-12-15".into()),
                superseded: None,
            }
        );
        let Line::Entry(old) = &lines[3] else {
            panic!("a struck entry")
        };
        assert_eq!(
            (
                old.text.as_str(),
                old.source.as_deref(),
                old.superseded.as_deref()
            ),
            ("lives in Hanoi", None, Some("2026-11-28"))
        );
        let Line::Entry(dream) = &lines[4] else {
            panic!("an entry")
        };
        assert_eq!(dream.source.as_deref(), Some("dream"));
        // Unparsed lines are kept as written.
        assert!(
            matches!(&lines[0], Line::Other(text) if text == "Notes the person wrote by hand.")
        );
        assert!(matches!(&lines[5], Line::Other(_)));
        assert!(matches!(
            parse_line("- 2026-13-01 · not a date"),
            Line::Other(_)
        ));
        assert!(matches!(
            parse_line("- ~~2026-10-02 · x~~ trailing"),
            Line::Other(_)
        ));
        // A text with the separator in it stays whole.
        let Line::Entry(dotted) = parse_line("- 2026-11-28 · a · b") else {
            panic!("an entry")
        };
        assert_eq!((dotted.source, dotted.text.as_str()), (None, "a · b"));
    }

    // FR-4, FR-5: what reaches a turn.
    #[test]
    fn visible_hides_expired_and_struck_entries() {
        let before_end = visible(BODY, "2026-12-15");
        assert!(
            before_end.contains("in Hanoi · until 2026-12-15"),
            "the last day still holds"
        );
        assert!(
            !before_end.contains("lives in Hanoi"),
            "struck lines never reach a turn"
        );
        let after_end = visible(BODY, "2026-12-16");
        assert!(!after_end.contains("The person is in Hanoi"), "{after_end}");
        assert!(after_end.contains("drafts under 150 words"));
        assert!(after_end.starts_with("Notes the person wrote by hand."));
    }

    // FR-4, FR-6: expired entries move to the archive, marked with the day.
    #[test]
    fn expired_entries_move_with_the_day() {
        let (kept, moved) = take_expired(BODY, "2026-12-16");
        assert!(!kept.contains("The person is in Hanoi"));
        assert!(
            kept.contains("lives in Hanoi"),
            "struck lines stay in the file"
        );
        assert_eq!(
            moved,
            ["- 2026-11-28 · from chat \"Trip\" · The person is in Hanoi · until 2026-12-15 · moved 2026-12-16"]
        );
        let archive = append_archive("", "travel", &moved);
        assert!(archive.starts_with("# Archive"));
        assert!(archive.contains("## travel\n- 2026-11-28"));
        let more = append_archive(&archive, "work", &["- x · moved 2026-12-17".into()]);
        assert!(more.contains("## travel") && more.contains("## work\n- x"));
        let (same, none) = take_expired(BODY, "2026-12-01");
        assert!(none.is_empty());
        assert_eq!(same, BODY);
    }

    // FR-4: a yearly date never gets an end date.
    #[test]
    fn yearly_dates_never_expire() {
        let birthday = Entry::new(
            "2026-11-28",
            None,
            "The person's birthday is 3 March",
            Some("2027-03-03"),
        );
        assert_eq!(birthday.until, None);
        let written = parse_line("- 2026-01-01 · Anniversary dinner every year · until 2026-02-01");
        let Line::Entry(entry) = written else {
            panic!("an entry")
        };
        assert!(!entry.expired("2027-01-01"));
        let trip = Entry::new("2026-11-28", Some("agent"), "In  Hanoi", Some("2026-12-15"));
        assert_eq!(
            trip.render(),
            "- 2026-11-28 · agent · In Hanoi · until 2026-12-15"
        );
        assert_eq!(
            fact_key("- The person likes tea. "),
            fact_key("the person  likes TEA")
        );
        assert_eq!(today().len(), 10);
        assert!(is_date(&today()));
    }
}
