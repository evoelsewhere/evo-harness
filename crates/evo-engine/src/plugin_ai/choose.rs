//! Choosing a model for a plugin's run (docs/features/plugin-ai/spec.md,
//! FR-6 – FR-9; plan 3.5 "Choosing"): from the person's connected, allowed
//! models, by what the run needs, never by a model id written in code.
//!
//! Tiers come from the catalogue's facts: `fast` is the cheapest by output
//! price; `best` puts reasoning models first, then the largest context, then
//! the highest price (a proxy for capability); `balanced` is the middle of
//! the price order. A model the person pinned for a tier wins. Ties keep the
//! order the models came in (the person's provider order).

use evo_protocol::plugin_ai::{ModelInfo, ModelRef, Need, Pins, Tier};

/// What a run asked for, beside its need.
#[derive(Debug, Clone)]
pub struct Ask<'a> {
    pub need: &'a Need,
    pub named: Option<&'a ModelRef>,
    pub default_model: Option<&'a ModelRef>,
    pub pins: &'a Pins,
    /// A schema needs a model that calls tools.
    pub schema: bool,
}

impl<'a> Ask<'a> {
    pub fn new(need: &'a Need, pins: &'a Pins) -> Self {
        Ask {
            need,
            named: None,
            default_model: None,
            pins,
            schema: false,
        }
    }
}

fn key(model: &ModelInfo) -> ModelRef {
    ModelRef {
        provider: model.provider.clone(),
        model: model.model.clone(),
    }
}

fn is(model: &ModelInfo, wanted: &ModelRef) -> bool {
    model.provider == wanted.provider && model.model.eq_ignore_ascii_case(&wanted.model)
}

fn output_price(model: &ModelInfo) -> f64 {
    model
        .cost
        .map(|c| c.output)
        .filter(|p| *p > 0.0)
        .unwrap_or(f64::INFINITY)
}

fn wants_reasoning(need: &Need) -> bool {
    need.reasoning
        .as_deref()
        .is_some_and(|level| level != "off")
}

/// Why no model fits, in the person's words.
fn missing(models: &[ModelInfo], need: &Need, tools: bool) -> String {
    if models.is_empty() {
        return "no AI model is connected or allowed for this plugin".into();
    }
    let mut lacks = Vec::new();
    if need.vision && !models.iter().any(|m| m.vision) {
        lacks.push("reads images");
    }
    if tools && !models.iter().any(|m| m.tools) {
        lacks.push("calls tools");
    }
    if wants_reasoning(need) && !models.iter().any(|m| m.reasoning) {
        lacks.push("reasons");
    }
    if lacks.is_empty() {
        "no allowed model has everything this run needs".into()
    } else {
        format!("no connected model {}", lacks.join(" and "))
    }
}

/// Models in the order a run tries them; the first is used, the next one
/// after a failure that may pass.
pub fn choose(models: &[ModelInfo], ask: &Ask) -> Result<Vec<ModelRef>, String> {
    let tools = ask.need.tools || ask.schema;
    let fits = |m: &&ModelInfo| {
        (!ask.need.vision || m.vision)
            && (!tools || m.tools)
            && (!wants_reasoning(ask.need) || m.reasoning)
    };
    let pool: Vec<&ModelInfo> = models.iter().filter(fits).collect();
    if let Some(named) = ask.named {
        let Some(found) = models.iter().find(|m| is(m, named)) else {
            return Err(format!(
                "{}/{} is not connected or not allowed for this plugin",
                named.provider, named.model
            ));
        };
        if !fits(&found) {
            return Err(format!(
                "{}/{} cannot do what this run needs",
                named.provider, named.model
            ));
        }
        let mut order = vec![key(found)];
        order.extend(
            rank(&pool, Tier::Balanced)
                .into_iter()
                .filter(|m| !is(m, named))
                .map(key),
        );
        return Ok(order);
    }
    if pool.is_empty() {
        return Err(missing(models, ask.need, tools));
    }
    let tier = ask.need.tier.unwrap_or(Tier::Balanced);
    let mut order: Vec<ModelRef> = rank(&pool, tier).into_iter().map(key).collect();
    let first = if ask.need.tier.is_none() {
        ask.default_model.or(ask.pins.get(tier))
    } else {
        ask.pins.get(tier)
    };
    if let Some(first) = first {
        if let Some(at) = order.iter().position(|m| {
            m.provider == first.provider && m.model.eq_ignore_ascii_case(&first.model)
        }) {
            let chosen = order.remove(at);
            order.insert(0, chosen);
        }
    }
    Ok(order)
}

/// `pool` ordered for `tier` (stable: ties keep the given order).
fn rank<'a>(pool: &[&'a ModelInfo], tier: Tier) -> Vec<&'a ModelInfo> {
    let mut order: Vec<&ModelInfo> = pool.to_vec();
    match tier {
        Tier::Fast => order.sort_by(|a, b| output_price(a).total_cmp(&output_price(b))),
        Tier::Best => order.sort_by(|a, b| {
            b.reasoning
                .cmp(&a.reasoning)
                .then(b.context.cmp(&a.context))
                .then(
                    output_price(b)
                        .min(f64::MAX)
                        .total_cmp(&output_price(a).min(f64::MAX)),
                )
        }),
        Tier::Balanced => {
            let mut prices: Vec<f64> = order
                .iter()
                .map(|m| output_price(m))
                .filter(|p| p.is_finite())
                .collect();
            prices.sort_by(f64::total_cmp);
            let median = prices.get(prices.len() / 2).copied().unwrap_or(0.0);
            order.sort_by(|a, b| {
                let distance = |m: &ModelInfo| {
                    let p = output_price(m);
                    if p.is_finite() {
                        (p - median).abs()
                    } else {
                        f64::INFINITY
                    }
                };
                distance(a)
                    .total_cmp(&distance(b))
                    .then(output_price(a).total_cmp(&output_price(b)))
            });
        }
    }
    order
}

/// Which tiers each model leads in: the first third of each tier's order.
pub fn tiers_of(models: &mut [ModelInfo]) {
    let pool: Vec<ModelInfo> = models.to_vec();
    let refs: Vec<&ModelInfo> = pool.iter().collect();
    let third = refs.len().div_ceil(3).max(1);
    for tier in [Tier::Fast, Tier::Balanced, Tier::Best] {
        for leader in rank(&refs, tier).into_iter().take(third) {
            if let Some(model) = models
                .iter_mut()
                .find(|m| m.provider == leader.provider && m.model == leader.model)
            {
                model.tiers.push(tier);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_protocol::plugin_ai::Price;

    fn model(
        provider: &str,
        id: &str,
        output: f64,
        context: u64,
        vision: bool,
        reasoning: bool,
    ) -> ModelInfo {
        ModelInfo {
            provider: provider.into(),
            model: id.into(),
            name: id.into(),
            vision,
            tools: true,
            reasoning,
            context,
            max_output: 8_000,
            cost: Some(Price {
                input: output / 5.0,
                output,
            }),
            tiers: Vec::new(),
        }
    }

    fn catalog() -> Vec<ModelInfo> {
        vec![
            model("a", "small", 1.0, 128_000, false, false),
            model("a", "mid", 8.0, 200_000, true, false),
            model("b", "large", 40.0, 200_000, true, true),
            model("b", "mini", 2.0, 400_000, true, true),
        ]
    }

    fn ids(order: &[ModelRef]) -> Vec<&str> {
        order.iter().map(|m| m.model.as_str()).collect()
    }

    fn ask(need: &Need) -> Ask<'_> {
        static PINS: std::sync::OnceLock<Pins> = std::sync::OnceLock::new();
        Ask::new(need, PINS.get_or_init(Pins::default))
    }

    #[test]
    fn tiers_order_by_price_and_ability() {
        let models = catalog();
        let fast = Need {
            tier: Some(Tier::Fast),
            ..Default::default()
        };
        assert_eq!(
            ids(&choose(&models, &ask(&fast)).unwrap()),
            ["small", "mini", "mid", "large"]
        );
        let best = Need {
            tier: Some(Tier::Best),
            ..Default::default()
        };
        assert_eq!(
            ids(&choose(&models, &ask(&best)).unwrap())[0],
            "mini",
            "reasoning first, then the largest context"
        );
        let balanced = Need::default();
        assert_eq!(
            ids(&choose(&models, &ask(&balanced)).unwrap())[0],
            "mid",
            "the middle of the price order"
        );
    }

    #[test]
    fn abilities_filter_and_say_what_is_missing() {
        let models = catalog();
        let vision_fast = Need {
            tier: Some(Tier::Fast),
            vision: true,
            ..Default::default()
        };
        assert_eq!(
            ids(&choose(&models, &ask(&vision_fast)).unwrap()),
            ["mini", "mid", "large"]
        );
        let reasoning = Need {
            reasoning: Some("high".into()),
            ..Default::default()
        };
        assert!(ids(&choose(&models, &ask(&reasoning)).unwrap())
            .iter()
            .all(|m| *m == "mini" || *m == "large"));
        let text_only = vec![model("a", "small", 1.0, 1000, false, false)];
        assert_eq!(
            choose(
                &text_only,
                &ask(&Need {
                    vision: true,
                    ..Default::default()
                })
            )
            .unwrap_err(),
            "no connected model reads images"
        );
        assert!(choose(&[], &ask(&Need::default()))
            .unwrap_err()
            .contains("no AI model"));
        let mut no_tools = catalog();
        no_tools.iter_mut().for_each(|m| m.tools = false);
        let plain = Need::default();
        let schema = Ask {
            schema: true,
            ..ask(&plain)
        };
        assert_eq!(
            choose(&no_tools, &schema).unwrap_err(),
            "no connected model calls tools"
        );
    }

    #[test]
    fn a_named_model_must_be_allowed_and_fit() {
        let models = catalog();
        let need = Need::default();
        let named = ModelRef {
            provider: "b".into(),
            model: "large".into(),
        };
        let order = choose(
            &models,
            &Ask {
                named: Some(&named),
                ..ask(&need)
            },
        )
        .unwrap();
        assert_eq!(order[0], named);
        assert_eq!(order.len(), 4, "the others stay as fallbacks");
        let other = ModelRef {
            provider: "c".into(),
            model: "x".into(),
        };
        assert!(choose(
            &models,
            &Ask {
                named: Some(&other),
                ..ask(&need)
            }
        )
        .unwrap_err()
        .contains("not connected or not allowed"));
        let small = ModelRef {
            provider: "a".into(),
            model: "small".into(),
        };
        let vision = Need {
            vision: true,
            ..Default::default()
        };
        assert!(choose(
            &models,
            &Ask {
                named: Some(&small),
                ..ask(&vision)
            }
        )
        .unwrap_err()
        .contains("cannot do"));
    }

    #[test]
    fn the_persons_pins_and_default_come_first() {
        let models = catalog();
        let pins = Pins {
            fast: Some(ModelRef {
                provider: "a".into(),
                model: "mid".into(),
            }),
            ..Default::default()
        };
        let fast = Need {
            tier: Some(Tier::Fast),
            ..Default::default()
        };
        let order = choose(&models, &Ask::new(&fast, &pins)).unwrap();
        assert_eq!(ids(&order)[0], "mid");
        let none = Need::default();
        let default = ModelRef {
            provider: "b".into(),
            model: "large".into(),
        };
        let no_pins = Pins::default();
        let order = choose(
            &models,
            &Ask {
                default_model: Some(&default),
                ..Ask::new(&none, &no_pins)
            },
        )
        .unwrap();
        assert_eq!(
            ids(&order)[0],
            "large",
            "no tier asked: the person's default"
        );
        let with_tier = choose(
            &models,
            &Ask {
                default_model: Some(&default),
                ..Ask::new(&fast, &no_pins)
            },
        )
        .unwrap();
        assert_eq!(
            ids(&with_tier)[0],
            "small",
            "a tier asked: the tier's order"
        );
    }

    #[test]
    fn models_are_marked_with_the_tiers_they_lead() {
        let mut models = catalog();
        tiers_of(&mut models);
        let of = |id: &str| models.iter().find(|m| m.model == id).unwrap().tiers.clone();
        assert!(of("small").contains(&Tier::Fast));
        assert!(of("mini").contains(&Tier::Best));
        assert!(of("mid").contains(&Tier::Balanced));
    }
}
