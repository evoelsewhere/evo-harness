//! Evals of the Evo harness: what the harness does, not what models say
//! (docs/features/learning-layers/spec.md, FR-26 – FR-32; plan 3.9).
//!
//! The offline tier runs scripted scenarios on the engine's real code with
//! a scripted model and no network, replays redacted golden channels, and
//! scores case sets on saved answers. The live tier runs only when asked.

pub mod cases;
pub mod golden;
pub mod live;
pub mod report;
pub mod scenario;
pub mod scorers;
pub mod worlds;
