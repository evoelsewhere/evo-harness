//! The learning layers: memory upkeep, the dream, the skill review and the
//! retro (docs/features/learning-layers/spec.md; plan 3.1).
//!
//! Every model step runs as `evo-engine --mode learn` with no tools; the
//! text it reads is data. What it saves goes through [`redact`] and, for a
//! fact about the person, the quote checks, then into the journal.

pub mod channels;
pub mod dream;
pub mod entries;
pub mod quotes;
pub mod redact;
pub mod run;
pub mod skills;
pub mod tidy;

use serde_json::Value;

/// Run one step (`--mode learn`).
pub async fn dispatch(step: &run::Step<'_>) -> Value {
    match step.task {
        run::Task::Tidy => tidy::run(step).await,
        run::Task::Dream => dream::run(step).await,
        _ => run::skipped(step.task, &step.owner.id, "not built yet"),
    }
}

#[cfg(test)]
mod memory_tests;
