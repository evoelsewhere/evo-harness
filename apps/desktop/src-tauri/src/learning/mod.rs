//! The learning layers' timing: when the tidy-up, the dream, the skill review
//! and the retro run, for which memory owner, and on which model. The steps
//! themselves are the engine's (`evo-engine --mode learn`, a short-lived
//! process like the index run); this module decides when one starts.
//!
//! Spec: docs/features/learning-layers/spec.md (FR-6, FR-10, FR-16, FR-19,
//! FR-23 – FR-25, NFR-2, NFR-5), plan 3.1, 3.4, 3.5.

use serde_json::{Map, Value};
use tauri::AppHandle;

pub mod clock;
pub mod commands;
pub mod jobs;
pub mod owners;
pub mod skills;

/// The first dream of an owner looks back this far (FR-10).
pub const FIRST_DREAM_LOOKBACK_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Start the learning loop: the Background model copied from the Index
/// model once, then the nightly look every minute.
pub fn setup(app: &AppHandle) {
    use tauri::Manager;
    copy_index_model(&app.state::<crate::db::Db>());
    clock::start(app);
}

/// **Background model**, `{provider, modelId}`: the tidy-up's model step,
/// the dream, the review, the retro and indexing (FR-24).
pub const BACKGROUND_MODEL_KEY: &str = "learning.backgroundModel";
/// **Memory upkeep** and **Learn skills automatically** per app (FR-23).
pub const HARNESS_UPKEEP_KEY: &str = "learning.harness.upkeep";
pub const HARNESS_SKILLS_KEY: &str = "learning.harness.skills";
pub const MATES_UPKEEP_KEY: &str = "learning.mates.upkeep";
pub const MATES_SKILLS_KEY: &str = "learning.mates.skills";
/// Tool steps since the last review that start a new one, 3–100 (FR-16).
pub const REVIEW_STEPS_KEY: &str = "learning.reviewSteps";
/// **Learn from channels** in Settings › Evo Mates (FR-22).
pub const LEARN_FROM_CHANNELS_KEY: &str = "mates.learnFromChannels";

pub const DEFAULT_REVIEW_STEPS: i64 = 10;

/// The learning settings, read when a step is due.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// `(provider, model id)`.
    pub model: Option<(String, String)>,
    pub harness_upkeep: bool,
    pub harness_skills: bool,
    pub mates_upkeep: bool,
    pub mates_skills: bool,
    pub review_steps: i64,
    pub learn_from_channels: bool,
}

impl Default for Settings {
    fn default() -> Self {
        settings_from(&Map::new())
    }
}

fn model_of(value: Option<&Value>) -> Option<(String, String)> {
    let model = value?;
    let text = |key: &str| model.get(key).and_then(Value::as_str).filter(|text| !text.is_empty()).map(str::to_string);
    Some((text("provider")?, text("modelId")?))
}

/// The learning settings out of the app settings: upkeep and skill learning
/// on in Evo Mates and off in Evo Harness unless set (FR-23).
pub fn settings_from(all: &Map<String, Value>) -> Settings {
    let flag = |key: &str, default: bool| all.get(key).and_then(Value::as_bool).unwrap_or(default);
    Settings {
        model: model_of(all.get(BACKGROUND_MODEL_KEY)),
        harness_upkeep: flag(HARNESS_UPKEEP_KEY, false),
        harness_skills: flag(HARNESS_SKILLS_KEY, false),
        mates_upkeep: flag(MATES_UPKEEP_KEY, true),
        mates_skills: flag(MATES_SKILLS_KEY, true),
        review_steps: all.get(REVIEW_STEPS_KEY).and_then(Value::as_i64).unwrap_or(DEFAULT_REVIEW_STEPS).clamp(3, 100),
        learn_from_channels: flag(LEARN_FROM_CHANNELS_KEY, false),
    }
}

/// The Background model replaces the Index model: when it is not set and an
/// Index model is, the value to store under [`BACKGROUND_MODEL_KEY`] (FR-24).
pub fn background_model_from_index(all: &Map<String, Value>) -> Option<Value> {
    if all.get(BACKGROUND_MODEL_KEY).is_some_and(|value| !value.is_null()) {
        return None;
    }
    let index = all.get(crate::memory_index::INDEX_MODEL_KEY)?;
    model_of(Some(index)).map(|_| index.clone())
}

/// Copy the Index model to the Background model once.
pub fn copy_index_model(db: &crate::db::Db) {
    let _ = db.with(|conn| {
        let all = evo_store::settings::all(conn)?;
        if let Some(model) = background_model_from_index(&all) {
            evo_store::settings::set(conn, BACKGROUND_MODEL_KEY, &model)?;
        }
        Ok(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn of(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    // FR-24: the Index model becomes the Background model, once.
    #[test]
    fn the_index_model_becomes_the_background_model() {
        let index = json!({ "provider": "p", "modelId": "m" });
        assert_eq!(background_model_from_index(&of(json!({ "memory.indexModel": index }))), Some(index.clone()));
        // Already set: left alone, whatever the Index model says.
        let both = of(json!({ "memory.indexModel": index, "learning.backgroundModel": { "provider": "q", "modelId": "n" } }));
        assert_eq!(background_model_from_index(&both), None);
        // Nothing usable to copy.
        assert_eq!(background_model_from_index(&of(json!({}))), None);
        assert_eq!(background_model_from_index(&of(json!({ "memory.indexModel": { "provider": "p", "modelId": "" } }))), None);
        // Indexing reads the Background model.
        let indexing = crate::memory_index::settings_from(&of(json!({ "memory.autoIndex": true, "learning.backgroundModel": index })));
        assert_eq!(indexing.model, Some(("p".into(), "m".into())));
        assert!(indexing.on);
        assert_eq!(settings_from(&of(json!({ "learning.backgroundModel": index }))).model, Some(("p".into(), "m".into())));
    }

    // FR-23: on in Evo Mates, off in Evo Harness, unless set.
    #[test]
    fn learning_defaults_differ_per_app() {
        let defaults = Settings::default();
        assert_eq!((defaults.harness_upkeep, defaults.harness_skills), (false, false));
        assert_eq!((defaults.mates_upkeep, defaults.mates_skills), (true, true));
        assert_eq!((defaults.review_steps, defaults.learn_from_channels, defaults.model), (10, false, None));
        let set = settings_from(&of(json!({ "learning.harness.upkeep": true, "learning.mates.skills": false, "learning.reviewSteps": 1 })));
        assert!(set.harness_upkeep && !set.mates_skills);
        assert_eq!(set.review_steps, 3, "at least 3");
        assert_eq!(settings_from(&of(json!({ "learning.reviewSteps": 500 }))).review_steps, 100);
    }
}
