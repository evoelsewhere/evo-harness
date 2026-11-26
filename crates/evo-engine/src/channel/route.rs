//! Who answers a message in a channel (docs/features/evo-mates/spec.md,
//! FR-15, FR-16, FR-53): `@everyone`, then mentions, then the channel's
//! reply rule. Under `auto` a router model may pick; its answer is weighed
//! here, so the rule is testable without a model.

use evo_protocol::mesh::{ChannelMember, ChannelRole};

/// The confidence an `auto` pick needs (FR-53).
pub const CONFIDENCE: f64 = 0.6;

/// Why the members were picked; shown in the route line above the reply.
#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    /// `@everyone` in the message.
    EveryoneMentioned,
    /// The message mentions these members.
    Mentioned,
    /// The rule is `everyone`.
    Everyone,
    /// The rule is `lead`, or `auto` fell back to the lead (with why).
    Lead { fallback: Option<String> },
    /// The router picked one member.
    Router { confidence: f64 },
    /// The router said several members should answer.
    RouterSeveral { confidence: f64 },
    /// Nobody: the rule is `mentioned` and no one was mentioned.
    NeedsMention,
    /// Nobody: there is no lead to fall back to.
    NoLead,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// Mate ids, in member order.
    pub picked: Vec<String>,
    pub reason: Reason,
}

/// A router model's answer (FR-53): one member, or "several".
#[derive(Debug, Clone, PartialEq)]
pub struct RouterAnswer {
    pub pick: Option<String>,
    pub several: bool,
    pub confidence: f64,
}

fn fold(text: &str) -> String {
    text.to_lowercase()
}

/// The members `text` mentions (`@Name`, case-insensitive, names may hold
/// spaces: the longest member name after `@` wins), in member order, and
/// whether it says `@everyone` (or `@all`).
pub fn mentions(text: &str, members: &[ChannelMember]) -> (Vec<String>, bool) {
    let lower = fold(text);
    let mut found: Vec<&str> = Vec::new();
    let mut everyone = false;
    let mut by_length: Vec<&ChannelMember> = members.iter().collect();
    by_length.sort_by_key(|m| std::cmp::Reverse(m.name.chars().count()));
    for (at, _) in lower.match_indices('@') {
        // An `@` inside a word (an email address) is not a mention.
        if lower[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric())
        {
            continue;
        }
        let rest = &lower[at + 1..];
        let ends_word = |len: usize| {
            rest[len..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric())
        };
        if ["everyone", "all"]
            .iter()
            .any(|word| rest.starts_with(word) && ends_word(word.len()))
        {
            everyone = true;
            continue;
        }
        if let Some(member) = by_length.iter().find(|m| {
            let name = fold(m.name.trim());
            !name.is_empty() && rest.starts_with(&name) && ends_word(name.len())
        }) {
            if !found.contains(&member.mate_id.as_str()) {
                found.push(&member.mate_id);
            }
        }
    }
    let ordered = members
        .iter()
        .filter(|m| found.contains(&m.mate_id.as_str()))
        .map(|m| m.mate_id.clone())
        .collect();
    (ordered, everyone)
}

fn everyone_of(channel: &ChannelRole) -> Vec<String> {
    channel.members.iter().map(|m| m.mate_id.clone()).collect()
}

fn lead_of(channel: &ChannelRole, fallback: Option<String>) -> Route {
    let lead = channel
        .lead
        .clone()
        .filter(|lead| channel.members.iter().any(|m| &m.mate_id == lead));
    match lead {
        Some(lead) => Route {
            picked: vec![lead],
            reason: Reason::Lead { fallback },
        },
        None => Route {
            picked: Vec::new(),
            reason: Reason::NoLead,
        },
    }
}

/// Whether `text` needs the router: no mention and the rule is `auto` with
/// more than one member.
pub fn needs_router(text: &str, channel: &ChannelRole) -> bool {
    let (named, everyone) = mentions(text, &channel.members);
    named.is_empty() && !everyone && channel.rule == "auto" && channel.members.len() > 1
}

/// Who answers `text`. `router` is the router's answer under `auto`: `None`
/// when there is no router, it failed or took too long (`router_error` says
/// which).
pub fn route(
    text: &str,
    channel: &ChannelRole,
    router: Option<RouterAnswer>,
    router_error: Option<&str>,
) -> Route {
    let (named, everyone) = mentions(text, &channel.members);
    if everyone {
        return Route {
            picked: everyone_of(channel),
            reason: Reason::EveryoneMentioned,
        };
    }
    if !named.is_empty() {
        return Route {
            picked: named,
            reason: Reason::Mentioned,
        };
    }
    match channel.rule.as_str() {
        "everyone" => Route {
            picked: everyone_of(channel),
            reason: Reason::Everyone,
        },
        "mentioned" => Route {
            picked: Vec::new(),
            reason: Reason::NeedsMention,
        },
        "lead" => lead_of(channel, None),
        _ => {
            if channel.members.len() == 1 {
                return Route {
                    picked: everyone_of(channel),
                    reason: Reason::Everyone,
                };
            }
            let Some(answer) = router else {
                return lead_of(
                    channel,
                    Some(router_error.unwrap_or("no router").to_string()),
                );
            };
            if answer.several && answer.confidence >= CONFIDENCE {
                return Route {
                    picked: everyone_of(channel),
                    reason: Reason::RouterSeveral {
                        confidence: answer.confidence,
                    },
                };
            }
            let known = answer
                .pick
                .filter(|pick| channel.members.iter().any(|m| &m.mate_id == pick));
            match known {
                Some(pick) if answer.confidence >= CONFIDENCE => Route {
                    picked: vec![pick],
                    reason: Reason::Router {
                        confidence: answer.confidence,
                    },
                },
                _ => lead_of(
                    channel,
                    Some(format!("router unsure ({:.2})", answer.confidence)),
                ),
            }
        }
    }
}

/// The route line above the reply (FR-53): who and why, in plain words.
pub fn line(route: &Route, channel: &ChannelRole) -> String {
    let name = |id: &str| {
        channel
            .members
            .iter()
            .find(|m| m.mate_id == id)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let names = route
        .picked
        .iter()
        .map(|id| name(id))
        .collect::<Vec<_>>()
        .join(", ");
    match &route.reason {
        Reason::EveryoneMentioned => "Everyone, as you asked".into(),
        Reason::Mentioned => format!("{names}, as you mentioned"),
        Reason::Everyone => "Everyone answers in this channel".into(),
        Reason::Lead { fallback: None } => format!("{names}, the lead"),
        Reason::Lead {
            fallback: Some(why),
        } => format!("{names}, the lead ({why})"),
        Reason::Router { confidence } => format!("{names} · picked at {:.0}%", confidence * 100.0),
        Reason::RouterSeveral { confidence } => {
            format!("Everyone · picked at {:.0}%", confidence * 100.0)
        }
        Reason::NeedsMention => "Mention a mate with @ to get an answer in this channel".into(),
        Reason::NoLead => "This channel has no lead: mention a mate with @".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, name: &str) -> ChannelMember {
        ChannelMember {
            mate_id: id.into(),
            name: name.into(),
            role: format!("{name}'s role"),
            ..ChannelMember::default()
        }
    }

    fn channel(rule: &str) -> ChannelRole {
        ChannelRole {
            channel_id: "ch".into(),
            name: "Launch room".into(),
            rule: rule.into(),
            lead: Some("p".into()),
            members: vec![
                member("p", "Planner"),
                member("w", "Writer"),
                member("n", "New mate"),
            ],
            ..ChannelRole::default()
        }
    }

    fn sure(pick: &str, confidence: f64) -> Option<RouterAnswer> {
        Some(RouterAnswer {
            pick: Some(pick.into()),
            several: false,
            confidence,
        })
    }

    #[test]
    fn mentions_find_members_by_name() {
        let ch = channel("auto");
        assert_eq!(
            mentions("@writer draft it, @Planner check", &ch.members),
            (vec!["p".into(), "w".into()], false)
        );
        assert_eq!(
            mentions("ask @new mate please", &ch.members),
            (vec!["n".into()], false)
        );
        assert_eq!(
            mentions("mail me at x@writer.com", &ch.members),
            (vec![], false)
        );
        assert_eq!(
            mentions("@writers unite", &ch.members),
            (vec![], false),
            "a longer word is no mention"
        );
        assert_eq!(mentions("@everyone hi", &ch.members), (vec![], true));
        assert_eq!(mentions("@all hi", &ch.members), (vec![], true));
    }

    #[test]
    fn mentions_and_everyone_come_before_the_rule() {
        for rule in ["auto", "lead", "everyone", "mentioned"] {
            let ch = channel(rule);
            assert_eq!(
                route("@Writer go", &ch, None, None),
                Route {
                    picked: vec!["w".into()],
                    reason: Reason::Mentioned
                }
            );
            assert_eq!(
                route("@everyone go", &ch, None, None).picked,
                ["p", "w", "n"]
            );
        }
    }

    #[test]
    fn each_rule_without_a_mention() {
        assert_eq!(
            route("go", &channel("everyone"), None, None).picked,
            ["p", "w", "n"]
        );
        assert_eq!(
            route("go", &channel("lead"), None, None),
            Route {
                picked: vec!["p".into()],
                reason: Reason::Lead { fallback: None }
            }
        );
        assert_eq!(
            route("go", &channel("mentioned"), None, None),
            Route {
                picked: vec![],
                reason: Reason::NeedsMention
            }
        );
        let no_lead = ChannelRole {
            lead: None,
            ..channel("lead")
        };
        assert_eq!(route("go", &no_lead, None, None).reason, Reason::NoLead);
    }

    #[test]
    fn auto_takes_a_sure_pick_else_the_lead() {
        let ch = channel("auto");
        assert!(needs_router("go", &ch));
        assert!(!needs_router("@Writer go", &ch));
        assert_eq!(
            route("go", &ch, sure("w", 0.8), None),
            Route {
                picked: vec!["w".into()],
                reason: Reason::Router { confidence: 0.8 }
            }
        );
        assert_eq!(
            route("go", &ch, sure("w", 0.6), None).picked,
            ["w"],
            "0.6 is enough"
        );
        assert_eq!(
            route("go", &ch, sure("w", 0.59), None).picked,
            ["p"],
            "below 0.6: the lead"
        );
        assert_eq!(
            route("go", &ch, sure("ghost", 0.9), None).picked,
            ["p"],
            "an unknown pick: the lead"
        );
        let several = Some(RouterAnswer {
            pick: None,
            several: true,
            confidence: 0.7,
        });
        assert_eq!(route("go", &ch, several, None).picked, ["p", "w", "n"]);
        let timeout = route("go", &ch, None, Some("no answer in 1.5 s"));
        assert_eq!(
            timeout,
            Route {
                picked: vec!["p".into()],
                reason: Reason::Lead {
                    fallback: Some("no answer in 1.5 s".into())
                }
            }
        );
        assert_eq!(
            route("go", &ch, None, None).reason,
            Reason::Lead {
                fallback: Some("no router".into())
            }
        );
    }

    #[test]
    fn a_single_member_answers_everything() {
        let ch = ChannelRole {
            members: vec![member("w", "Writer")],
            ..channel("auto")
        };
        assert!(!needs_router("go", &ch));
        assert_eq!(route("go", &ch, None, None).picked, ["w"]);
    }

    #[test]
    fn route_lines_read_plainly() {
        let ch = channel("auto");
        assert_eq!(
            line(&route("go", &ch, sure("w", 0.82), None), &ch),
            "Writer · picked at 82%"
        );
        assert_eq!(
            line(&route("@Writer @Planner", &ch, None, None), &ch),
            "Planner, Writer, as you mentioned"
        );
        assert_eq!(
            line(&route("go", &ch, None, None), &ch),
            "Planner, the lead (no router)"
        );
    }
}
