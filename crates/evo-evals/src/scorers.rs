//! Assertions over a step's frozen evidence: one verdict each, and a
//! failing message that names the assertion's kind
//! (docs/features/learning-layers/spec.md, FR-27).
//!
//! Kinds: `toolsOffered`, `toolCalled`, `routedTo`, `handoff`, `notice`,
//! `memoryHas`, `memoryLacks`, `skillHas`, `promptContains`, `promptLacks`,
//! `journalHas`, and the generic `path` + `equals` / `contains`.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verdict {
    pub kind: String,
    pub ok: bool,
    pub message: String,
}

fn verdict(kind: &str, ok: bool, message: impl Into<String>) -> Verdict {
    Verdict {
        kind: kind.into(),
        ok,
        message: if ok {
            String::new()
        } else {
            format!("{kind}: {}", message.into())
        },
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// `want` is contained in `have`: every key of an object, equal leaves.
pub fn subset(want: &Value, have: &Value) -> bool {
    match (want, have) {
        (Value::Object(want), Value::Object(have)) => want
            .iter()
            .all(|(key, value)| have.get(key).is_some_and(|h| subset(value, h))),
        _ => want == have,
    }
}

/// The value at a dotted `path` of `evidence`.
pub fn at<'a>(evidence: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .filter(|part| !part.is_empty())
        .try_fold(evidence, |value, part| match part.parse::<usize>() {
            Ok(index) => value.get(index),
            Err(_) => value.get(part),
        })
}

/// Score one assertion against a step's evidence.
pub fn score(assertion: &Value, evidence: &Value) -> Verdict {
    let Some((kind, want)) = assertion
        .as_object()
        .and_then(|object| object.iter().find(|(key, _)| *key != "path"))
    else {
        return verdict("assertion", false, "an assertion names one kind");
    };
    let kind = kind.as_str();
    match kind {
        "toolsOffered" => {
            let offered: Vec<Vec<String>> = evidence
                .get("toolsOffered")
                .and_then(Value::as_array)
                .map(|calls| calls.iter().map(strings).collect())
                .unwrap_or_default();
            let want = strings(want);
            let ok = !offered.is_empty() && offered.iter().all(|tools| *tools == want);
            verdict(
                kind,
                ok,
                format!("wanted every call to offer {want:?}, saw {offered:?}"),
            )
        }
        "toolCalled" => {
            let calls = evidence
                .get("toolCalls")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let name = want.get("name").and_then(Value::as_str).unwrap_or("");
            let args = want
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            let ok = calls.iter().any(|call| {
                call.get("name").and_then(Value::as_str) == Some(name)
                    && subset(&args, call.get("arguments").unwrap_or(&Value::Null))
            });
            verdict(
                kind,
                ok,
                format!(
                    "no call of `{name}` with {args}; calls were {}",
                    Value::Array(calls)
                ),
            )
        }
        "routedTo" => {
            let have = evidence.get("routedTo").map(strings).unwrap_or_default();
            verdict(
                kind,
                have == strings(want),
                format!(
                    "wanted {:?}, routed to {have:?} ({})",
                    strings(want),
                    evidence.get("reason").cloned().unwrap_or(Value::Null)
                ),
            )
        }
        "handoff" => {
            let handoffs = evidence
                .get("handoffs")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let ok = match want {
                Value::Number(count) => handoffs.len() as u64 == count.as_u64().unwrap_or(0),
                _ => handoffs.iter().any(|h| subset(want, h)),
            };
            verdict(
                kind,
                ok,
                format!("wanted {want}, hand-offs were {}", Value::Array(handoffs)),
            )
        }
        "notice" => {
            let have = evidence.get("notice").cloned().unwrap_or(Value::Null);
            verdict(
                kind,
                &have == want,
                format!("wanted {want}, the notice was {have}"),
            )
        }
        "memoryHas" | "memoryLacks" => {
            let text = want
                .get("text")
                .and_then(Value::as_str)
                .or(want.as_str())
                .unwrap_or("");
            let topics = evidence
                .get("topics")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let topic = want.get("topic").and_then(Value::as_str);
            let found = topics.iter().any(|(name, body)| {
                topic.is_none_or(|t| t == name) && body.as_str().is_some_and(|b| b.contains(text))
            });
            let ok = (kind == "memoryHas") == found;
            verdict(
                kind,
                ok,
                format!(
                    "`{text}` {} in what reaches a turn: {}",
                    if found { "is" } else { "is not" },
                    Value::Object(topics)
                ),
            )
        }
        "skillHas" => {
            let skill = want.get("skill").and_then(Value::as_str).unwrap_or("");
            let text = want.get("text").and_then(Value::as_str).unwrap_or("");
            let body = evidence
                .pointer(&format!("/skills/{skill}"))
                .and_then(Value::as_str)
                .unwrap_or("");
            verdict(
                kind,
                !body.is_empty() && body.contains(text),
                format!("learned skill `{skill}` does not hold `{text}`"),
            )
        }
        "promptContains" | "promptLacks" => {
            let text = want.as_str().unwrap_or("");
            let prompts = evidence.get("prompts").map(strings).unwrap_or_default();
            let found = prompts.iter().any(|prompt| prompt.contains(text));
            verdict(
                kind,
                (kind == "promptContains") == found,
                format!(
                    "`{text}` {} in the prompts",
                    if found { "is" } else { "is not" }
                ),
            )
        }
        "journalHas" => {
            let rows = evidence
                .get("journal")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            verdict(
                kind,
                rows.iter().any(|row| subset(want, row)),
                format!(
                    "no journal row like {want}; rows were {}",
                    Value::Array(rows)
                ),
            )
        }
        "equals" | "contains" => {
            let path = assertion.get("path").and_then(Value::as_str).unwrap_or("");
            let have = at(evidence, path).cloned().unwrap_or(Value::Null);
            let ok = if kind == "equals" {
                &have == want
            } else {
                have.as_str()
                    .zip(want.as_str())
                    .is_some_and(|(h, w)| h.contains(w))
                    || have.as_array().is_some_and(|list| list.contains(want))
            };
            verdict(
                kind,
                ok,
                format!("`{path}` is {have}, wanted {kind} {want}"),
            )
        }
        other => verdict(other, false, "unknown assertion kind"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // FR-27: each kind passes on matching evidence and fails with a message naming it.
    #[test]
    fn scorers_name_the_failing_assertion() {
        let evidence = json!({
            "toolsOffered": [["memory", "set_summary"]],
            "toolCalls": [{ "name": "memory", "arguments": { "action": "save", "name": "x" } }],
            "routedTo": ["w1"], "reason": "Mentioned",
            "handoffs": [{ "from": "a", "to": "b", "kind": "asks" }],
            "notice": "limit",
            "topics": { "style": "Short drafts." },
            "skills": { "reports": "Summary first." },
            "prompts": ["You keep memory."],
            "journal": [{ "by": "dream", "action": "added", "name": "style" }],
            "inner": "hello"
        });
        let cases = [
            (
                json!({ "toolsOffered": ["memory", "set_summary"] }),
                json!({ "toolsOffered": ["memory"] }),
            ),
            (
                json!({ "toolCalled": { "name": "memory", "arguments": { "action": "save" } } }),
                json!({ "toolCalled": { "name": "bash" } }),
            ),
            (
                json!({ "routedTo": ["w1"] }),
                json!({ "routedTo": ["lead"] }),
            ),
            (
                json!({ "handoff": { "kind": "asks", "to": "b" } }),
                json!({ "handoff": 2 }),
            ),
            (json!({ "notice": "limit" }), json!({ "notice": null })),
            (
                json!({ "memoryHas": { "topic": "style", "text": "Short" } }),
                json!({ "memoryHas": "Long" }),
            ),
            (
                json!({ "memoryLacks": "Long" }),
                json!({ "memoryLacks": "Short" }),
            ),
            (
                json!({ "skillHas": { "skill": "reports", "text": "Summary" } }),
                json!({ "skillHas": { "skill": "reports", "text": "Table" } }),
            ),
            (
                json!({ "promptContains": "keep memory" }),
                json!({ "promptContains": "never" }),
            ),
            (
                json!({ "promptLacks": "never" }),
                json!({ "promptLacks": "memory" }),
            ),
            (
                json!({ "journalHas": { "by": "dream" } }),
                json!({ "journalHas": { "by": "tidy" } }),
            ),
            (
                json!({ "path": "inner", "equals": "hello" }),
                json!({ "path": "inner", "equals": "bye" }),
            ),
        ];
        for (pass, fail) in cases {
            let good = score(&pass, &evidence);
            assert!(good.ok, "{pass}: {}", good.message);
            let bad = score(&fail, &evidence);
            assert!(!bad.ok, "{fail} should fail");
            assert!(
                bad.message.starts_with(&format!("{}:", bad.kind)),
                "{}",
                bad.message
            );
        }
        assert!(!score(&json!({ "nonsense": 1 }), &evidence).ok);
    }
}
