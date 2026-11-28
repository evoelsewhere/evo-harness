//! Scenarios as data files: a world, steps that drive the real engine code,
//! and per step the assertions on what happened. A run freezes each step's
//! evidence, scores it, and gives the same verdicts every time
//! (docs/features/learning-layers/spec.md, FR-26, FR-27; plan 3.9).
//!
//! ```json
//! { "name": "mention-loop", "world": "channel", "about": "…", "fault": "…",
//!   "channel": { "members": [{ "id": "w1", "name": "Writer" }], "rule": "everyone", "lead": "w1" },
//!   "steps": [{ "do": "reply", "from": "w1", "text": "Thanks @Planner", "expect": [{ "handoff": 0 }] }] }
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::scorers::{score, Verdict};
use crate::worlds;

#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    pub name: String,
    pub world: String,
    #[serde(default)]
    pub about: String,
    /// The fault this scenario keeps fixed, when it is one.
    #[serde(default)]
    pub fault: Option<String>,
    #[serde(default)]
    pub channel: Value,
    /// The local day the memory world runs on.
    #[serde(default)]
    pub today: Option<String>,
    pub steps: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepResult {
    pub step: usize,
    pub verdicts: Vec<Verdict>,
    /// What happened, as the assertions read it.
    pub evidence: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub name: String,
    pub world: String,
    pub passed: bool,
    pub error: Option<String>,
    pub steps: Vec<StepResult>,
}

impl Outcome {
    /// The failing assertions' messages.
    pub fn failures(&self) -> Vec<String> {
        let mut out: Vec<String> = self.error.iter().cloned().collect();
        for step in &self.steps {
            for verdict in step.verdicts.iter().filter(|v| !v.ok) {
                out.push(format!("step {}: {}", step.step + 1, verdict.message));
            }
        }
        out
    }
}

/// Every scenario under `dir`, by file name.
pub fn load_dir(dir: &Path) -> Result<Vec<(PathBuf, Scenario)>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let scenario: Scenario =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok((path, scenario))
        })
        .collect()
}

/// Run one scenario.
pub async fn run(scenario: &Scenario) -> Outcome {
    let today = scenario
        .today
        .clone()
        .unwrap_or_else(|| "2026-11-28".into());
    let mut steps = Vec::new();
    let mut error = None;
    let world = match scenario.world.as_str() {
        "memory" | "chat" => match worlds::memory::World::new() {
            Ok(world) => Some(world),
            Err(e) => {
                error = Some(e);
                None
            }
        },
        _ => None,
    };
    let role = worlds::channel::role_of(&scenario.channel);
    for (index, step) in scenario.steps.iter().enumerate() {
        if error.is_some() {
            break;
        }
        let evidence = match (scenario.world.as_str(), &world) {
            ("channel", _) => worlds::channel::step(&role, step),
            ("memory", Some(world)) => worlds::memory::step(world, step, &today).await,
            ("chat", Some(world)) => worlds::chat::step(world, step, &today).await,
            (other, _) => Err(format!("unknown world `{other}`")),
        };
        match evidence {
            Ok(evidence) => {
                let verdicts = step
                    .get("expect")
                    .and_then(Value::as_array)
                    .map(|list| list.iter().map(|a| score(a, &evidence)).collect())
                    .unwrap_or_default();
                steps.push(StepResult {
                    step: index,
                    verdicts,
                    evidence,
                });
            }
            Err(e) => error = Some(format!("step {}: {e}", index + 1)),
        }
    }
    let passed = error.is_none()
        && steps.iter().all(|s| s.verdicts.iter().all(|v| v.ok))
        && steps.iter().any(|s| !s.verdicts.is_empty());
    Outcome {
        name: scenario.name.clone(),
        world: scenario.world.clone(),
        passed,
        error,
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn a_scenario_without_assertions_does_not_pass() {
        let scenario: Scenario = serde_json::from_value(json!({
            "name": "empty", "world": "channel",
            "channel": { "members": [{ "id": "w1", "name": "Writer" }], "rule": "everyone" },
            "steps": [{ "do": "message", "text": "Hi" }]
        }))
        .unwrap();
        assert!(!run(&scenario).await.passed);
        let unknown: Scenario = serde_json::from_value(
            json!({ "name": "x", "world": "moon", "steps": [{ "do": "x" }] }),
        )
        .unwrap();
        let outcome = run(&unknown).await;
        assert!(!outcome.passed && outcome.failures()[0].contains("unknown world"));
    }
}
