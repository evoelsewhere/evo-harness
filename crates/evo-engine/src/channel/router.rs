//! The `auto` rule's router (docs/features/evo-mates/spec.md, FR-16, FR-28,
//! FR-53): one tool-free question to the channel's model, naming each
//! member with its role, the recent lines and the message, answered as
//! `{"pick": "<member id>" | "several", "confidence": 0..1}`. No answer
//! within [`BUDGET`], an error or an answer that cannot be read leaves the
//! pick to the lead ([`super::route::route`]).

use std::time::Duration;

use evo_protocol::mesh::{ChannelRole, RouterModel};
use serde_json::Value;

use super::route::RouterAnswer;

/// How long the router may take (FR-53).
pub const BUDGET: Duration = Duration::from_secs(6);

/// The model that routes (FR-28): the person's choice when it is in
/// `catalog`, else the cheapest priced one. The catalogue holds only
/// connected providers' models that models.dev marks fit for tool use.
/// `None` when it is empty: `auto` then leaves the pick to the lead.
pub fn router_model<'a>(
    catalog: &'a [evo_ai::Model],
    chosen: Option<&RouterModel>,
) -> Option<&'a evo_ai::Model> {
    if let Some(chosen) = chosen {
        let found = catalog.iter().find(|m| {
            m.id.eq_ignore_ascii_case(&chosen.model)
                && m.provider.eq_ignore_ascii_case(&chosen.provider)
        });
        if found.is_some() {
            return found;
        }
    }
    let price = |m: &evo_ai::Model| m.cost.rates.input + m.cost.rates.output;
    catalog
        .iter()
        .filter(|m| m.has_pricing())
        .min_by(|a, b| price(a).total_cmp(&price(b)).then_with(|| a.id.cmp(&b.id)))
        .or_else(|| catalog.first())
}

/// The question for the router.
pub fn question(role: &ChannelRole, recent: &[(String, String)], text: &str) -> String {
    let mut out = String::from(
        "You route one message in a team chat to the member who should answer it. Members:\n",
    );
    for member in &role.members {
        let what = if member.role.trim().is_empty() {
            "no role given"
        } else {
            member.role.trim()
        };
        out.push_str(&format!(
            "- id {}: {} ({what})\n",
            member.mate_id, member.name
        ));
    }
    if !role.instructions.trim().is_empty() {
        out.push_str(&format!(
            "The chat is about: {}\n",
            role.instructions.trim()
        ));
    }
    if !recent.is_empty() {
        out.push_str("Recent lines:\n");
        for (author, line) in recent.iter().rev().take(8).rev() {
            let line: String = line.chars().take(300).collect();
            out.push_str(&format!("{author}: {line}\n"));
        }
    }
    out.push_str(&format!("The message:\n{}\n", text.trim()));
    out.push_str(
        "Answer with JSON only, no other words: {\"pick\": \"<member id>\", \"confidence\": <0 to 1>}, or \
{\"pick\": \"several\", \"confidence\": <0 to 1>} when more than one member should answer.",
    );
    out
}

/// The router's answer, read from its reply; `None` when it cannot be read.
pub fn parse(reply: &str) -> Option<RouterAnswer> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let value: Value = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let pick = value.get("pick")?.as_str()?.trim().to_string();
    let confidence = value
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    if pick.eq_ignore_ascii_case("several") {
        return Some(RouterAnswer {
            pick: None,
            several: true,
            confidence,
        });
    }
    Some(RouterAnswer {
        pick: Some(pick),
        several: false,
        confidence,
    })
}

/// Ask the channel's model, within the budget. `Err`: why there is no pick.
pub async fn ask(
    harness: &evo_harness::agent_harness::AgentHarness,
    role: &ChannelRole,
    recent: &[(String, String)],
    text: &str,
) -> Result<RouterAnswer, String> {
    let question = question(role, recent, text);
    match tokio::time::timeout(BUDGET, crate::side_questions::ask_once(harness, &question)).await {
        Err(_) => Err("no answer in 6 s".into()),
        Ok(Err(error)) => Err(format!("router failed: {error}")),
        Ok(Ok(reply)) => parse(&reply).ok_or_else(|| "router answer unreadable".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_protocol::mesh::ChannelMember;

    #[test]
    fn the_question_names_members_lines_and_the_message() {
        let role = ChannelRole {
            members: vec![
                ChannelMember {
                    mate_id: "p".into(),
                    name: "Planner".into(),
                    role: "plans work".into(),
                    ..Default::default()
                },
                ChannelMember {
                    mate_id: "w".into(),
                    name: "Writer".into(),
                    ..Default::default()
                },
            ],
            instructions: "Ship by Friday".into(),
            ..ChannelRole::default()
        };
        let q = question(
            &role,
            &[("The person".into(), "hi".into())],
            "Draft the intro",
        );
        assert!(
            q.contains("- id p: Planner (plans work)\n- id w: Writer (no role given)\n"),
            "{q}"
        );
        assert!(q.contains("The chat is about: Ship by Friday"));
        assert!(q.contains("Recent lines:\nThe person: hi\n"));
        assert!(q.contains("The message:\nDraft the intro\n"));
        assert!(q.contains("\"several\""));
    }

    #[test]
    fn the_router_is_the_chosen_model_else_the_cheapest() {
        let model = |provider: &str, id: &str, price: Option<f64>| {
            let mut m = evo_ai::Model::new(
                id,
                id,
                evo_ai::Api::OpenaiCompletions,
                provider,
                "https://example.invalid",
            );
            if let Some(price) = price {
                m.cost.rates.input = price;
                m.cost.rates.output = price * 4.0;
                m.cost_configured = true;
            }
            m
        };
        let catalog = vec![
            model("acme", "big", Some(3.0)),
            model("acme", "unpriced", None),
            model("other", "small", Some(0.1)),
            model("acme", "mid", Some(1.0)),
        ];
        assert_eq!(
            router_model(&catalog, None).map(|m| m.id.as_str()),
            Some("small")
        );
        let chosen = RouterModel {
            provider: "acme".into(),
            model: "big".into(),
        };
        assert_eq!(
            router_model(&catalog, Some(&chosen)).map(|m| m.id.as_str()),
            Some("big")
        );
        let gone = RouterModel {
            provider: "acme".into(),
            model: "retired".into(),
        };
        assert_eq!(
            router_model(&catalog, Some(&gone)).map(|m| m.id.as_str()),
            Some("small"),
            "a choice no longer connected falls back to the default"
        );
        assert!(
            router_model(&[], None).is_none(),
            "nothing connected: the lead answers"
        );
    }

    #[test]
    fn answers_are_read_from_json_anywhere_in_the_reply() {
        assert_eq!(
            parse("{\"pick\":\"w\",\"confidence\":0.82}"),
            Some(RouterAnswer {
                pick: Some("w".into()),
                several: false,
                confidence: 0.82
            })
        );
        assert_eq!(
            parse("Sure: {\"pick\": \"several\", \"confidence\": 0.7}."),
            Some(RouterAnswer {
                pick: None,
                several: true,
                confidence: 0.7
            })
        );
        assert_eq!(
            parse("{\"pick\":\"w\",\"confidence\":3}")
                .unwrap()
                .confidence,
            1.0
        );
        assert_eq!(
            parse("{\"pick\":\"w\"}").unwrap().confidence,
            0.0,
            "no confidence: not sure"
        );
        assert_eq!(parse("Writer"), None);
        assert_eq!(parse("{not json}"), None);
    }
}
