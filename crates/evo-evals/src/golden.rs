//! Golden channels: a real channel turned into a scenario. Names, ids and
//! free text become stable placeholders (mentions and questions kept, so
//! routing still reads them), and a leak scan refuses to write anything
//! personal that is left. Replay runs the coordinator's real routing and
//! hand-off decisions over the redacted lines and compares the trace —
//! turn order, routes, hand-offs — with a saved baseline
//! (docs/features/learning-layers/spec.md, FR-29; plan 3.9).
//!
//! A golden file: `{"channel": {members, rule, lead, limit}, "lines": [channel entries]}`.

use std::collections::BTreeMap;

use evo_engine::channel::route;
use regex::Regex;
use serde_json::{json, Value};

use crate::worlds::channel::role_of;

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Redact a channel: every member id → `mN`, name → `MateN`, the channel's
/// name → `Channel`, free text → `[text N]` with its mentions and its
/// question mark kept. Refused when the leak scan finds anything personal.
pub fn redact(source: &Value) -> Result<Value, String> {
    let channel = source.get("channel").cloned().unwrap_or(Value::Null);
    let members = channel
        .get("members")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut ids: BTreeMap<String, String> = BTreeMap::new();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    for (index, member) in members.iter().enumerate() {
        ids.insert(text(member, "id"), format!("m{}", index + 1));
        names.insert(text(member, "name"), format!("Mate{}", index + 1));
    }
    let id = |value: &str| {
        ids.get(value).cloned().unwrap_or_else(|| {
            if value == "person" {
                value.into()
            } else {
                "unknown".into()
            }
        })
    };
    let mut counter = 0;
    let mut free = |original: &str| -> String {
        counter += 1;
        let mut kept: Vec<String> = Vec::new();
        let lower = original.to_lowercase();
        for (name, placeholder) in &names {
            if !name.is_empty() && lower.contains(&format!("@{}", name.to_lowercase())) {
                kept.push(format!("@{placeholder}"));
            }
        }
        if lower.contains("@everyone") {
            kept.push("@everyone".into());
        }
        let question = if original.trim_end().ends_with('?') || original.contains("?\n") {
            "?"
        } else {
            ""
        };
        format!(
            "[text {counter}]{}{}{question}",
            if kept.is_empty() { "" } else { " " },
            kept.join(" ")
        )
    };
    let mut lines = Vec::new();
    for line in source
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let kind = text(&line, "kind");
        let out = match kind.as_str() {
            "message" => {
                json!({ "kind": "message", "id": text(&line, "id"), "author": id(&text(&line, "author")), "text": free(&text(&line, "text")), "status": line.get("status").cloned().unwrap_or(Value::Null) })
            }
            "route" => {
                json!({ "kind": "route", "message": text(&line, "message"), "picked": line.get("picked").and_then(Value::as_array).map(|p| p.iter().filter_map(Value::as_str).map(&id).collect::<Vec<_>>()).unwrap_or_default() })
            }
            "handoff" => {
                json!({ "kind": "handoff", "handoff": text(&line, "handoff"), "from": id(&text(&line, "from")), "to": id(&text(&line, "to")), "text": free(&text(&line, "text")) })
            }
            "notice" => json!({ "kind": "notice", "notice": text(&line, "notice") }),
            "job" => {
                json!({ "kind": "job", "status": text(&line, "status"), "handoffs": line.get("handoffs").cloned().unwrap_or(json!(0)) })
            }
            _ => continue,
        };
        lines.push(out);
    }
    let redacted = json!({
        "channel": {
            "members": members.iter().map(|m| json!({ "id": id(&text(m, "id")), "name": names.get(&text(m, "name")).cloned().unwrap_or_default() })).collect::<Vec<_>>(),
            "rule": text(&channel, "rule"),
            "lead": channel.get("lead").and_then(Value::as_str).map(&id),
            "limit": channel.get("limit").cloned().unwrap_or(json!(12)),
        },
        "lines": lines,
    });
    let mut known: Vec<String> = names
        .keys()
        .chain(ids.keys())
        .filter(|n| n.len() > 2)
        .cloned()
        .collect();
    known.push(text(&channel, "name"));
    leak_scan(&redacted.to_string(), &known)?;
    Ok(redacted)
}

/// Refuse text that still holds an email, a phone number, a path with a
/// user's name, or one of the source's names.
pub fn leak_scan(text: &str, names: &[String]) -> Result<(), String> {
    let shapes = [
        (
            r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
            "an email address",
        ),
        (r"\+?\d[\d \-().]{7,}\d", "a phone number"),
        (
            r"(?i)([a-z]:\\\\users\\\\|/home/|/users/)[^\\/\s]+",
            "a path with a user name",
        ),
    ];
    for (shape, what) in shapes {
        if Regex::new(shape).expect("shape").is_match(text) {
            return Err(format!("leak scan: {what} is left"));
        }
    }
    let lower = text.to_lowercase();
    for name in names.iter().filter(|n| !n.trim().is_empty()) {
        if lower.contains(&name.to_lowercase()) {
            return Err(format!("leak scan: the name `{name}` is left"));
        }
    }
    Ok(())
}

/// The trace of a channel as recorded: turn order, routes, hand-offs.
pub fn trace(golden: &Value) -> Value {
    let lines = golden
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let order: Vec<String> = lines
        .iter()
        .filter(|l| text(l, "kind") == "message")
        .map(|l| text(l, "author"))
        .collect();
    let routes: Vec<Value> = lines
        .iter()
        .filter(|l| text(l, "kind") == "route")
        .map(|l| l["picked"].clone())
        .collect();
    let handoffs: Vec<Value> = lines
        .iter()
        .filter(|l| text(l, "kind") == "handoff")
        .map(|l| json!([text(l, "from"), text(l, "to"), text(l, "handoff")]))
        .collect();
    json!({ "turnOrder": order, "routes": routes, "handoffs": handoffs })
}

/// Replay: run the real routing and mention decisions over the lines. A
/// person's message is routed again (an `auto` pick the router made is fed
/// back as its answer); a mate's reply gives the asks its mentions make.
pub fn replay(golden: &Value) -> Value {
    let role = role_of(golden.get("channel").unwrap_or(&Value::Null));
    let lines = golden
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut routes = Vec::new();
    let mut asks = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if text(line, "kind") != "message" {
            continue;
        }
        let author = text(line, "author");
        let body = text(line, "text");
        if author == "person" {
            let recorded = lines[index + 1..]
                .iter()
                .find(|l| text(l, "kind") == "route")
                .map(|l| l["picked"].clone())
                .unwrap_or(json!([]));
            let pick = recorded
                .as_array()
                .filter(|p| p.len() == 1)
                .and_then(|p| p[0].as_str())
                .map(str::to_string);
            let answer = route::needs_router(&body, &role).then(|| route::RouterAnswer {
                pick,
                several: false,
                confidence: 1.0,
            });
            routes.push(json!(route::route(&body, &role, answer, None).picked));
        } else {
            for to in route::asked_in(&body, &role.members)
                .into_iter()
                .filter(|to| *to != author)
            {
                asks.push(json!([author, to, "asks"]));
            }
        }
    }
    let recorded = trace(golden);
    // Hand-offs by mail (assigns, tells) are the mates' own; replay checks the mention asks.
    let mention_asks: Vec<Value> = recorded["handoffs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|h| h[2] == "asks")
        .collect();
    json!({ "turnOrder": recorded["turnOrder"], "routes": routes, "handoffs": recorded["handoffs"], "mentionAsks": asks, "recordedAsks": mention_asks })
}

/// Compare a replay with its baseline: the differences, empty when it matches.
pub fn compare(replayed: &Value, baseline: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["turnOrder", "routes", "handoffs", "mentionAsks"] {
        if replayed.get(key) != baseline.get(key) {
            out.push(format!(
                "{key}: replay {} ≠ baseline {}",
                replayed.get(key).unwrap_or(&Value::Null),
                baseline.get(key).unwrap_or(&Value::Null)
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Value {
        json!({
            "channel": { "name": "Launch Room", "members": [{ "id": "mate-a1", "name": "Hana" }, { "id": "mate-b2", "name": "Duc" }], "rule": "auto", "lead": "mate-a1", "limit": 12 },
            "lines": [
                { "kind": "message", "id": "x1", "author": "person", "text": "@Hana can you plan the launch with Duc?" },
                { "kind": "route", "message": "x1", "picked": ["mate-a1"] },
                { "kind": "message", "id": "x2", "author": "mate-a1", "text": "Sure. @Duc what is the date?" },
                { "kind": "handoff", "handoff": "asks", "from": "mate-a1", "to": "mate-b2", "text": "@Duc what is the date?" },
                { "kind": "message", "id": "x3", "author": "mate-b2", "text": "Thanks @Hana, it is the 3rd." },
                { "kind": "job", "status": "completed", "handoffs": 1 }
            ]
        })
    }

    // FR-29: names, ids and text go; mentions and questions stay; replay matches.
    #[test]
    fn golden_redacts_and_replays_against_its_baseline() {
        let golden = redact(&source()).unwrap();
        let text = golden.to_string();
        for gone in ["Hana", "Duc", "mate-a1", "Launch Room", "plan the launch"] {
            assert!(!text.contains(gone), "{gone} left in {text}");
        }
        assert_eq!(golden["lines"][0]["text"], "[text 1] @Mate1?");
        assert_eq!(golden["lines"][2]["text"], "[text 2] @Mate2?");
        let baseline = replay(&golden);
        assert_eq!(baseline["routes"], json!([["m1"]]));
        assert_eq!(
            baseline["mentionAsks"],
            json!([["m1", "m2", "asks"]]),
            "thanks is not an ask"
        );
        assert!(
            compare(&replay(&golden), &baseline).is_empty(),
            "the same every run"
        );
        // A changed hand-off fails the replay.
        let mut changed = golden.clone();
        changed["lines"][4]["text"] = json!("[text 3] @Mate1?");
        assert!(!compare(&replay(&changed), &baseline).is_empty());
    }

    #[test]
    fn the_leak_scan_refuses_personal_data() {
        assert!(leak_scan("write to ana@example.com", &[]).is_err());
        assert!(leak_scan("call +84 912 345 678", &[]).is_err());
        assert!(leak_scan("see /home/ana/notes", &[]).is_err());
        assert!(leak_scan("Hana said", &["Hana".into()]).is_err());
        assert!(leak_scan("[text 1] @Mate1?", &["Hana".into()]).is_ok());
        let mut leaky = source();
        leaky["lines"][0]["author"] = json!("person");
        leaky["channel"]["members"][1]["name"] = json!("ana@example.com");
        // Free text is replaced whole: an address in it never reaches the file.
        assert!(!redact(&leaky).unwrap().to_string().contains("example.com"));
    }
}
