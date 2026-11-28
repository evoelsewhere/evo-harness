//! The learning layers: memory upkeep, the dream, the skill review and the
//! retro (docs/features/learning-layers/spec.md; plan 3.1).
//!
//! Every model step runs as `evo-engine --mode learn` with no tools; the
//! text it reads is data. What it saves goes through [`redact`] and, for a
//! fact about the person, the quote checks, then into the journal.

pub mod entries;
pub mod redact;
