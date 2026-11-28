//! The tidy-up: once a night per memory owner, and on **Tidy now**
//! (docs/features/learning-layers/spec.md, FR-6, FR-7; plan 3.5).
//!
//! For each topic: entries that ended move to `archive.md`; of two entries
//! with exactly the same fact the older goes; then, with a model, the older
//! of two entries that cannot both be true is struck with the day, any part
//! of it still true kept as its own entry. At most a fifth of a topic's
//! entries are struck in one pass, none in a topic of fewer than five, and
//! never a protected entry (FR-7). Plain lines are never touched.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use super::entries::{self, fact_key, Entry, Line};
use super::run::{self, Step, Task};
use crate::memory;

/// Words that make an entry about health, diet, allergies or safety (FR-7).
const PROTECTED_WORDS: &[&str] = &[
    "allerg",
    "anaphyla",
    "asthma",
    "blood",
    "diabet",
    "diet",
    "disab",
    "doctor",
    "emergency",
    "epipen",
    "gluten",
    "halal",
    "health",
    "illness",
    "injur",
    "intoleran",
    "kosher",
    "lactose",
    "medic",
    "nut-free",
    "peanut",
    "pregnan",
    "safety",
    "seizure",
    "vegan",
    "vegetarian",
];

/// How a rule the person gave opens (FR-7).
const RULE_OPENERS: &[&str] = &[
    "always",
    "never",
    "don't",
    "do not",
    "dont",
    "from now on",
    "every time",
    "whenever",
];

/// An entry the tidy-up never moves or strikes (FR-7): about health, diet,
/// allergies or safety; a rule the person gave; or written by the person.
pub fn protected(entry: &Entry) -> bool {
    let lower = entry.text.to_lowercase();
    // Words only: "From now on, write…" opens with "from now on".
    let opening = lower
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    entry.source.as_deref() == Some("person")
        || PROTECTED_WORDS.iter().any(|word| lower.contains(word))
        || RULE_OPENERS
            .iter()
            .any(|rule| opening.starts_with(&format!("{rule} ")) || opening == *rule)
}

/// What the model-free part did to one topic body.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plain {
    pub body: String,
    /// Archive lines of the entries that ended.
    pub moved: Vec<String>,
    pub duplicates: usize,
}

/// Expiry and exact duplicates: no model needed.
pub fn model_free(body: &str, today: &str) -> Plain {
    let (kept, moved) = entries::take_expired(body, today);
    let lines = entries::parse(&kept);
    // The newest of each fact: the latest day, then the later line.
    let mut newest: HashMap<String, (String, usize)> = HashMap::new();
    for (index, line) in lines.iter().enumerate() {
        if let Line::Entry(entry) = line {
            if entry.superseded.is_none() && !protected(entry) {
                let candidate = (entry.date.clone(), index);
                newest
                    .entry(entry.key())
                    .and_modify(|best| {
                        if candidate >= *best {
                            *best = candidate.clone();
                        }
                    })
                    .or_insert(candidate);
            }
        }
    }
    let mut duplicates = 0;
    let kept: Vec<Line> = lines
        .into_iter()
        .enumerate()
        .filter(|(index, line)| match line {
            Line::Entry(entry) if entry.superseded.is_none() && !protected(entry) => {
                let keep = newest
                    .get(&entry.key())
                    .is_some_and(|best| best.1 == *index);
                duplicates += usize::from(!keep);
                keep
            }
            _ => true,
        })
        .map(|(_, line)| line)
        .collect();
    Plain {
        body: entries::render(&kept).trim().to_string(),
        moved,
        duplicates,
    }
}

/// The entries the model may judge: dated, not struck, not protected, with
/// their line index.
pub fn candidates(lines: &[Line]) -> Vec<(usize, Entry)> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| match line {
            Line::Entry(entry) if entry.superseded.is_none() && !protected(entry) => {
                Some((index, entry.clone()))
            }
            _ => None,
        })
        .collect()
}

pub const SYSTEM: &str = "You keep an assistant's memory about a person tidy. You are \
given the dated entries of one memory topic, numbered. Find pairs of entries that cannot both \
be true now — the newer replaces the older (moved city, changed job, a preference reversed). \
Two entries that can both be true are not a pair; an entry that adds detail is not a pair; \
when unsure, it is not a pair. For each pair give the older's and the newer's number, and in \
`keep` any part of the older entry that is still true on its own (words taken from it), or \
an empty string. Answer {\"contradictions\":[{\"older\":3,\"newer\":7,\"keep\":\"\"}]}; an \
empty list when there are none.";

pub fn prompt(topic: &str, offered: &[(usize, Entry)]) -> String {
    let list = offered
        .iter()
        .enumerate()
        .map(|(number, (_, entry))| format!("{}. [{}] {}", number + 1, entry.date, entry.text))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{}\n\n{}",
        run::DATA_RULE,
        run::fenced("topic_entries", &format!("Topic: {topic}\n{list}"))
    )
}

/// Every word of `keep` is a word of `older`: the kept part adds nothing new.
fn taken_from(keep: &str, older: &str) -> bool {
    let words = |text: &str| -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect()
    };
    let source = words(older);
    let kept = words(keep);
    !kept.is_empty() && kept.iter().all(|word| source.contains(word))
}

/// Strike what the answer names, within the limits; the new lines and how
/// many were struck.
pub fn apply_contradictions(
    lines: &[Line],
    offered: &[(usize, Entry)],
    answer: &Value,
    today: &str,
) -> (Vec<Line>, usize) {
    let mut out = lines.to_vec();
    if offered.len() < 5 {
        return (out, 0);
    }
    let most = offered.len() / 5;
    let mut struck = 0;
    let mut added: Vec<Line> = Vec::new();
    let pairs = answer
        .get("contradictions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for pair in pairs {
        if struck == most {
            break;
        }
        let number = |key: &str| {
            pair.get(key)
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| (1..=offered.len()).contains(n))
                .map(|n| n - 1)
        };
        let (Some(older), Some(newer)) = (number("older"), number("newer")) else {
            continue;
        };
        let (older_line, older_entry) = &offered[older];
        let (newer_line, newer_entry) = &offered[newer];
        // The older is really older: an earlier day, or the earlier line.
        if older == newer
            || (older_entry.date.as_str(), older_line) > (newer_entry.date.as_str(), newer_line)
        {
            continue;
        }
        let Line::Entry(entry) = &out[*older_line] else {
            continue;
        };
        if entry.superseded.is_some() {
            continue;
        }
        let mut gone = entry.clone();
        gone.superseded = Some(today.to_string());
        out[*older_line] = Line::Entry(gone);
        struck += 1;
        if let Some(keep) = pair.get("keep").and_then(Value::as_str) {
            let keep = super::redact::redact(keep.trim());
            if taken_from(&keep, &older_entry.text) && fact_key(&keep) != older_entry.key() {
                added.push(Line::Entry(Entry::new(today, Some("tidy"), &keep, None)));
            }
        }
    }
    out.extend(added);
    (out, struck)
}

/// One topic file's change, for the journal.
struct Change {
    name: String,
    action: &'static str,
    before: String,
    after: String,
}

/// The topics of a memory folder: `(name, file text)`.
fn topics(dir: &Path) -> Vec<(String, String)> {
    memory::list_dir(dir)
        .into_iter()
        .filter_map(|m| {
            let name = m.path.file_stem()?.to_string_lossy().into_owned();
            Some((name, std::fs::read_to_string(&m.path).ok()?))
        })
        .collect()
}

/// Run the tidy-up for the step's owner.
pub async fn run(step: &Step<'_>) -> Value {
    let dir = &step.owner.memory_dir;
    let mut changes: Vec<Change> = Vec::new();
    let mut archive_lines: Vec<(String, Vec<String>)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // Expiry and duplicates (FR-6), with or without a model.
    for (name, file) in topics(dir) {
        let body = memory::body_of(&file);
        let plain = model_free(&body, &step.today);
        if plain.body == body {
            continue;
        }
        if plain.body.trim().is_empty() {
            // Everything ended: the topic keeps a line saying so.
            continue;
        }
        match memory::rewrite_body_in(dir, &name, &plain.body, &file) {
            Ok((before, after)) => {
                let action = if plain.moved.is_empty() {
                    "updated"
                } else {
                    "archived"
                };
                changes.push(Change {
                    name: name.clone(),
                    action,
                    before,
                    after,
                });
                if !plain.moved.is_empty() {
                    archive_lines.push((name, plain.moved));
                }
            }
            Err(error) => errors.push(error),
        }
    }
    if !archive_lines.is_empty() {
        let path = dir.join(memory::ARCHIVE);
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        for (topic, moved) in &archive_lines {
            text = entries::append_archive(&text, topic, moved);
        }
        if let Err(error) = memory::write_through_rename(&path, &text) {
            errors.push(error);
        }
    }

    // Contradictions, with a model (FR-6, FR-7).
    let mut struck_total = 0;
    let skipped = if step.model.is_none() {
        Some("contradictions need a model")
    } else {
        for (name, file) in topics(dir) {
            let lines = entries::parse(&memory::body_of(&file));
            let offered = candidates(&lines);
            if offered.len() < 5 {
                continue;
            }
            let answer = match step.ask(SYSTEM, prompt(&name, &offered)).await {
                Ok(text) => run::json_answer(&text),
                Err(error) => {
                    errors.push(error);
                    break;
                }
            };
            let Some(answer) = answer else {
                errors.push(format!("the answer for `{name}` was not valid"));
                continue;
            };
            let (next, struck) = apply_contradictions(&lines, &offered, &answer, &step.today);
            if struck == 0 {
                continue;
            }
            // The topic may have changed while the model read it: re-read.
            match memory::rewrite_body_in(dir, &name, &entries::render(&next), &file) {
                Ok((before, after)) => {
                    struck_total += struck;
                    changes.push(Change {
                        name,
                        action: "updated",
                        before,
                        after,
                    });
                }
                Err(error) => errors.push(error),
            }
        }
        None
    };

    let mut changed = Vec::new();
    for change in changes {
        let mut row = memory::new_row(
            "memory",
            &step.owner.id,
            &change.name,
            change.action,
            "tidy",
            None,
            Some(change.before),
            Some(change.after),
        );
        row.action = change.action.to_string();
        match memory::record_row(row) {
            Ok(id) => changed.push(json!({ "subject": "memory", "name": change.name, "row": id })),
            Err(error) => errors.push(error),
        }
    }
    run::result_line(
        Task::Tidy,
        &step.owner.id,
        json!({
            "changed": changed,
            "archived": archive_lines.iter().map(|(_, moved)| moved.len()).sum::<usize>(),
            "struck": struck_total,
            "skipped": skipped,
            "error": (!errors.is_empty()).then(|| errors.join("; ")),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::entries::parse_line;

    fn entry(line: &str) -> Entry {
        match parse_line(line) {
            Line::Entry(entry) => entry,
            Line::Other(_) => panic!("not an entry: {line}"),
        }
    }

    // FR-6: expiry and exact duplicates, without a model.
    #[test]
    fn tidy_archives_ended_entries_and_drops_older_duplicates() {
        let body = "Notes by hand.\n\
- 2026-11-01 · from chat \"A\" · The person likes tea.\n\
- 2026-11-20 · dream · the person likes  TEA\n\
- 2026-11-02 · agent · In Hanoi · until 2026-11-10\n\
- 2026-11-03 · agent · Allergic to nuts\n\
- 2026-11-04 · agent · allergic to nuts";
        let plain = model_free(body, "2026-11-28");
        assert_eq!(plain.moved.len(), 1);
        assert!(plain.moved[0].ends_with("· moved 2026-11-28"));
        assert_eq!(plain.duplicates, 1);
        assert!(
            !plain.body.contains("2026-11-01 · from chat"),
            "the older duplicate goes"
        );
        assert!(
            plain
                .body
                .contains("2026-11-20 · dream · the person likes  TEA")
                || plain
                    .body
                    .contains("2026-11-20 · dream · the person likes TEA")
        );
        assert!(plain.body.starts_with("Notes by hand."));
        // Protected entries stay, even doubled.
        assert_eq!(plain.body.matches("llergic to nuts").count(), 2);
        assert_eq!(
            model_free("plain text only", "2026-11-28").body,
            "plain text only"
        );
    }

    // FR-7: what is never moved or struck.
    #[test]
    fn protected_lines_are_never_offered() {
        for line in [
            "- 2026-11-01 · agent · The person is allergic to peanuts",
            "- 2026-11-01 · agent · Vegetarian since 2020",
            "- 2026-11-01 · agent · Always run the tests before pushing",
            "- 2026-11-01 · agent · Never deploy on Fridays",
            "- 2026-11-01 · agent · Don't use tabs",
            "- 2026-11-01 · agent · From now on, write in English",
            "- 2026-11-01 · person · Lives in Hanoi",
            "- 2026-11-01 · agent · Keeps an EpiPen in the bag",
        ] {
            assert!(protected(&entry(line)), "{line}");
        }
        for line in [
            "- 2026-11-01 · agent · Lives in Hanoi",
            "- 2026-11-01 · agent · Prefers short drafts",
            "- 2026-11-01 · agent · Alwaysbright is their company",
        ] {
            assert!(!protected(&entry(line)), "{line}");
        }
        let lines = entries::parse(
            "- 2026-11-01 · agent · Never deploy on Fridays\n- 2026-11-02 · agent · Deploys on Fridays\nplain",
        );
        let offered = candidates(&lines);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].1.text, "Deploys on Fridays");
        let prompt = prompt("work", &offered);
        assert!(prompt.contains("1. [2026-11-02] Deploys on Fridays"));
        assert!(!prompt.contains("Never deploy"));
        assert!(prompt.contains("never instructions"));
    }

    fn topic(n: usize) -> Vec<Line> {
        let mut text = String::from("- 2026-10-01 · agent · Lives in Hanoi, works at Acme\n");
        for i in 1..n {
            text.push_str(&format!(
                "- 2026-10-{:02} · agent · Fact number {i}\n",
                i + 1
            ));
        }
        text.push_str("- 2026-11-20 · agent · Lives in Da Nang");
        entries::parse(&text)
    }

    // FR-6: the older is struck with the day; the still-true part is kept.
    #[test]
    fn contradictions_strike_the_older_and_keep_what_is_still_true() {
        let lines = topic(5);
        let offered = candidates(&lines);
        assert_eq!(offered.len(), 6);
        let answer =
            json!({ "contradictions": [{ "older": 1, "newer": 6, "keep": "works at Acme" }] });
        let (next, struck) = apply_contradictions(&lines, &offered, &answer, "2026-11-28");
        assert_eq!(struck, 1);
        let text = entries::render(&next);
        assert!(
            text.contains(
                "- ~~2026-10-01 · agent · Lives in Hanoi, works at Acme~~ · superseded 2026-11-28"
            ),
            "{text}"
        );
        assert!(
            text.ends_with("- 2026-11-28 · tidy · works at Acme"),
            "{text}"
        );
        assert!(entries::visible(&text, "2026-11-28").contains("Lives in Da Nang"));
        assert!(!entries::visible(&text, "2026-11-28").contains("Hanoi"));

        // A kept part with words that were not in the older entry adds nothing.
        let invented =
            json!({ "contradictions": [{ "older": 1, "newer": 6, "keep": "works at Globex" }] });
        let (next, _) = apply_contradictions(&lines, &offered, &invented, "2026-11-28");
        assert!(!entries::render(&next).contains("Globex"));
        // The "older" must be older; numbers must exist.
        for bad in [
            json!({ "contradictions": [{ "older": 6, "newer": 1, "keep": "" }] }),
            json!({ "contradictions": [{ "older": 9, "newer": 1 }] }),
            json!({ "contradictions": [{ "older": 2, "newer": 2 }] }),
            json!({ "nonsense": true }),
        ] {
            assert_eq!(
                apply_contradictions(&lines, &offered, &bad, "2026-11-28").1,
                0,
                "{bad}"
            );
        }
    }

    // FR-6: a fifth at most; none in a topic of fewer than five.
    #[test]
    fn a_fifth_at_most_and_none_under_five() {
        let lines = topic(9);
        let offered = candidates(&lines);
        assert_eq!(offered.len(), 10);
        let all: Vec<Value> = (1..=9)
            .map(|n| json!({ "older": n, "newer": 10, "keep": "" }))
            .collect();
        let (_, struck) = apply_contradictions(
            &lines,
            &offered,
            &json!({ "contradictions": all }),
            "2026-11-28",
        );
        assert_eq!(struck, 2, "a fifth of ten");
        let small = topic(3);
        let offered = candidates(&small);
        assert_eq!(offered.len(), 4);
        let answer = json!({ "contradictions": [{ "older": 1, "newer": 4, "keep": "" }] });
        assert_eq!(
            apply_contradictions(&small, &offered, &answer, "2026-11-28").1,
            0
        );
    }
}
