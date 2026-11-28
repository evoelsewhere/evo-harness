//! The channel world: the coordinator's real decisions — who answers a
//! message, which mentions in a reply ask, how hand-offs count toward the
//! limits, what a mail shows, the router models' order and how a job ends —
//! driven step by step with no model and no network.

use evo_engine::channel::coordinator::{self, HandOff, Owed, PairLog};
use evo_engine::channel::{route, router};
use evo_mesh::wire::{ChannelMember, ChannelRole};
use serde_json::{json, Value};

fn text<'a>(step: &'a Value, key: &str) -> &'a str {
    step.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The channel a scenario describes: `{members:[{id,name,role}], rule, lead, limit}`.
pub fn role_of(channel: &Value) -> ChannelRole {
    let members = channel
        .get("members")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|m| ChannelMember {
                    mate_id: text(m, "id").into(),
                    name: text(m, "name").into(),
                    role: text(m, "role").into(),
                    ..Default::default()
                })
                .collect()
        })
        .unwrap_or_default();
    ChannelRole {
        channel_id: "eval".into(),
        name: channel
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Eval")
            .into(),
        rule: channel
            .get("rule")
            .and_then(Value::as_str)
            .unwrap_or("auto")
            .into(),
        lead: channel
            .get("lead")
            .and_then(Value::as_str)
            .map(str::to_string),
        members,
        instructions: String::new(),
        handoff_limit: channel.get("limit").and_then(Value::as_u64).unwrap_or(12) as u32,
        router: None,
    }
}

fn model(provider: &str, id: &str, input: f64, output: f64) -> evo_ai::Model {
    let mut model =
        evo_ai::providers::faux::FauxProvider::new(evo_ai::providers::faux::FauxScript::new())
            .default_model()
            .clone();
    model.provider = provider.into();
    model.id = id.into();
    model.cost = Default::default();
    model.cost_configured = input > 0.0 || output > 0.0;
    model.cost.rates.input = input;
    model.cost.rates.output = output;
    model
}

/// Run one step; its evidence.
pub fn step(role: &ChannelRole, step: &Value) -> Result<Value, String> {
    match text(step, "do") {
        "message" => {
            let asking: Vec<String> = step
                .get("asking")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let router = step.get("router").map(|answer| route::RouterAnswer {
                pick: answer
                    .get("pick")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                several: answer
                    .get("several")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                confidence: answer
                    .get("confidence")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0),
            });
            let message = text(step, "text");
            let route = route::answer_to(message, role, &asking).unwrap_or_else(|| {
                route::route(
                    message,
                    role,
                    router,
                    step.get("routerError").and_then(Value::as_str),
                )
            });
            Ok(
                json!({ "routedTo": route.picked, "reason": format!("{:?}", route.reason), "line": route::line(&route, role), "needsRouter": route::needs_router(message, role) }),
            )
        }
        "reply" => {
            let from = text(step, "from");
            let asks: Vec<String> = route::asked_in(text(step, "text"), &role.members)
                .into_iter()
                .filter(|to| to != from)
                .collect();
            let handoffs: Vec<Value> = asks
                .iter()
                .map(|to| json!({ "from": from, "to": to, "kind": "asks" }))
                .collect();
            Ok(
                json!({ "asks": asks, "handoffs": handoffs, "asksThePerson": route::asks_the_person(text(step, "text"), &role.members) }),
            )
        }
        "handoffs" => {
            let mut count = 0u32;
            let mut pairs = PairLog::default();
            let mut owed = Owed::default();
            let mut out = Vec::new();
            let mut notice = Value::Null;
            for (index, h) in step
                .get("chain")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .iter()
                .enumerate()
            {
                let (from, to, kind) = (text(h, "from"), text(h, "to"), text(h, "kind"));
                let report = kind == "tells" && owed.pays(from, to);
                let verdict = coordinator::hand_off(
                    &mut count,
                    role.handoff_limit,
                    &mut pairs,
                    from,
                    to,
                    1_000 + index as i64,
                    report,
                );
                if verdict != HandOff::PairLimit && kind != "tells" {
                    owed.owe(from, to);
                }
                out.push(json!({ "from": from, "to": to, "kind": kind, "report": report, "verdict": format!("{verdict:?}") }));
                if verdict == HandOff::MessageLimit {
                    notice = json!("limit");
                    break;
                }
            }
            Ok(json!({ "handoffs": out, "notice": notice, "counted": count }))
        }
        "mail" => Ok(json!({ "inner": evo_mesh::hub::mail_inner(text(step, "text")) })),
        "handoffKind" => {
            Ok(json!({ "kind": coordinator::handoff_kind(text(step, "op"), text(step, "text")) }))
        }
        "routerOrder" => {
            let catalog: Vec<evo_ai::Model> = step
                .get("catalog")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .map(|m| {
                            model(
                                text(m, "provider"),
                                text(m, "id"),
                                m.get("input").and_then(Value::as_f64).unwrap_or(0.0),
                                m.get("output").and_then(Value::as_f64).unwrap_or(0.0),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let chosen = step.get("chosen").map(|c| evo_mesh::wire::RouterModel {
                provider: text(c, "provider").into(),
                model: text(c, "model").into(),
            });
            let order: Vec<String> = router::router_models(&catalog, chosen.as_ref())
                .iter()
                .map(|m| format!("{}/{}", m.provider, m.id))
                .collect();
            Ok(json!({ "order": order }))
        }
        "jobEnd" => {
            let track = step.get("track").cloned().unwrap_or(Value::Null);
            let track = coordinator::JobTrack {
                handoffs: track.get("handoffs").and_then(Value::as_u64).unwrap_or(0) as u32,
                failed: track
                    .get("failed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                limit: track.get("limit").and_then(Value::as_bool).unwrap_or(false),
            };
            let job = coordinator::job_entry(
                "j",
                "m",
                &track,
                step.get("result").unwrap_or(&Value::Null),
                1,
            );
            Ok(
                json!({ "job": job, "notice": if job["status"] == "limit" { json!("limit") } else { Value::Null } }),
            )
        }
        other => Err(format!("the channel world has no step `{other}`")),
    }
}
