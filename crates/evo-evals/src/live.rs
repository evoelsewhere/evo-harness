//! The live tier: real models, only with `EVO_EVAL_LIVE=1`, never in CI
//! (docs/features/learning-layers/spec.md, FR-31; plan 3.9).
//!
//! * The model: `EVO_EVAL_MODEL=provider/model`, resolved through the
//!   engine's provider settings (models come from models.dev).
//! * The dream and router case sets run on it, with time and cost per case.
//! * A judge reads one reply against a learned skill. Its prompts are pinned:
//!   `evals/live/judge/manifest.json` holds each file's sha256, and a prompt
//!   that does not match is refused.
//! * A run is compared with the last saved one
//!   (`evals/live/baselines/last.json`, not committed) and fails when its pass
//!   rate drops by more than `evals/live/config.json`'s `maxDrift`.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use evo_ai::types::AssistantMessage;
use evo_engine::learning::{dream, run};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::cases::{self, Graded};

pub const SWITCH: &str = "EVO_EVAL_LIVE";

pub fn enabled() -> bool {
    std::env::var(SWITCH).is_ok_and(|v| v == "1")
}

/// A judge prompt, checked against its pinned hash.
pub fn judge_prompt(dir: &Path, name: &str) -> Result<String, String> {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(dir.join(name)).map_err(|e| e.to_string())?;
    let hash = format!(
        "{:x}",
        Sha256::digest(text.replace("\r\n", "\n").as_bytes())
    );
    match manifest.get(name).and_then(Value::as_str) {
        Some(pinned) if pinned == hash => Ok(text),
        Some(_) => Err(format!(
            "judge prompt `{name}` changed: update its version and hash in manifest.json"
        )),
        None => Err(format!(
            "judge prompt `{name}` is not pinned in manifest.json"
        )),
    }
}

/// The pass rate dropped more than `max_drift` from the baseline.
pub fn drifted(rate: f64, baseline: Option<f64>, max_drift: f64) -> bool {
    baseline.is_some_and(|before| before - rate > max_drift)
}

/// Run the live tier from `evals/`. `Ok(None)` when it was skipped.
pub async fn run(evals: &Path) -> Result<Option<(Vec<Graded>, bool)>, String> {
    if !enabled() {
        println!("live tier skipped: set {SWITCH}=1 and EVO_EVAL_MODEL=provider/model to run it");
        return Ok(None);
    }
    let named = std::env::var("EVO_EVAL_MODEL")
        .map_err(|_| "EVO_EVAL_MODEL=provider/model is required".to_string())?;
    let (provider, model) = named
        .split_once('/')
        .ok_or("EVO_EVAL_MODEL is provider/model")?;
    let resolved = evo_engine::provider::resolve(Some(provider), Some(model), None, None, None)
        .map_err(|e| e.to_string())?;
    let (provider, model) = (resolved.provider.clone(), resolved.model.clone());
    let cost = Arc::new(Mutex::new(0.0_f64));
    let mut graded = Vec::new();

    let dream_cases: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(evals.join("cases/dream.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    for case in &dream_cases {
        let chats = cases::dream_chats(case);
        let started = Instant::now();
        *cost.lock().unwrap() = 0.0;
        let sink = cost.clone();
        let count =
            move |message: &AssistantMessage| *sink.lock().unwrap() += message.usage.cost.total;
        let answer = run::ask(
            &provider,
            &model,
            dream::SYSTEM,
            dream::prompt(&chats, &[]),
            &count,
        )
        .await;
        let misses = match answer.map(|text| run::json_answer(&text)) {
            Ok(Some(answer)) => cases::grade_dream(case, &answer),
            Ok(None) => vec!["the answer was not JSON".into()],
            Err(error) => vec![error],
        };
        graded.push(Graded {
            set: "dream".into(),
            case: case["name"].as_str().unwrap_or("").into(),
            pass: misses.is_empty(),
            misses,
            ms: Some(started.elapsed().as_millis()),
            cost_usd: Some(*cost.lock().unwrap()),
        });
    }

    // The judge: does a reply follow the learned skill it was given?
    let judge = judge_prompt(&evals.join("live/judge"), "follows-skill.md")?;
    let started = Instant::now();
    *cost.lock().unwrap() = 0.0;
    let sink = cost.clone();
    let count = move |message: &AssistantMessage| *sink.lock().unwrap() += message.usage.cost.total;
    let skill = "Weekly reports: put a one-paragraph summary first, then the table.";
    let reply = run::ask(
        &provider,
        &model,
        &format!("Follow this skill exactly.\n{skill}"),
        "Write this week's report: 3 deploys, 1 incident, 2 new hires.".into(),
        &count,
    )
    .await?;
    let verdict = run::ask(
        &provider,
        &model,
        &judge,
        format!("<skill>\n{skill}\n</skill>\n<reply>\n{reply}\n</reply>"),
        &count,
    )
    .await?;
    let pass = run::json_answer(&verdict)
        .and_then(|v| v.get("follows").and_then(Value::as_bool))
        .unwrap_or(false);
    graded.push(Graded {
        set: "judge".into(),
        case: "follows-skill".into(),
        pass,
        misses: if pass {
            vec![]
        } else {
            vec![format!("judge: {verdict}")]
        },
        ms: Some(started.elapsed().as_millis()),
        cost_usd: Some(*cost.lock().unwrap()),
    });

    let rate = graded.iter().filter(|g| g.pass).count() as f64 / graded.len().max(1) as f64;
    let config: Value = serde_json::from_str(
        &std::fs::read_to_string(evals.join("live/config.json")).unwrap_or_else(|_| "{}".into()),
    )
    .unwrap_or(json!({}));
    let max_drift = config
        .get("maxDrift")
        .and_then(Value::as_f64)
        .unwrap_or(0.2);
    let baseline_path = evals.join("live/baselines/last.json");
    let baseline = std::fs::read_to_string(&baseline_path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("passRate").and_then(Value::as_f64));
    let failed = drifted(rate, baseline, max_drift);
    let _ = std::fs::create_dir_all(evals.join("live/baselines"));
    let _ = std::fs::write(
        &baseline_path,
        json!({ "passRate": rate, "model": named }).to_string(),
    );
    println!(
        "live tier: {:.0}% of {} cases passed (baseline {:?}, max drift {max_drift})",
        rate * 100.0,
        graded.len(),
        baseline
    );
    Ok(Some((graded, !failed)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // FR-31: without the switch it skips; a changed judge prompt is refused.
    #[tokio::test]
    async fn the_live_tier_skips_without_its_switch_and_pins_its_judge() {
        if !enabled() {
            assert!(run(Path::new("/nowhere")).await.unwrap().is_none());
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("j.md"), "Judge.\n").unwrap();
        let hash = format!("{:x}", Sha256::digest(b"Judge.\n"));
        std::fs::write(
            dir.path().join("manifest.json"),
            json!({ "j.md": hash }).to_string(),
        )
        .unwrap();
        assert_eq!(judge_prompt(dir.path(), "j.md").unwrap(), "Judge.\n");
        std::fs::write(dir.path().join("j.md"), "Judge, changed.\n").unwrap();
        assert!(judge_prompt(dir.path(), "j.md")
            .unwrap_err()
            .contains("changed"));
        assert!(drifted(0.5, Some(0.8), 0.2));
        assert!(!drifted(0.7, Some(0.8), 0.2));
        assert!(!drifted(0.1, None, 0.2), "the first run sets the baseline");
    }
}
