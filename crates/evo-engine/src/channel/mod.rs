//! Evo Mates' channels in the engine (docs/features/evo-mates/spec.md,
//! FR-14 – FR-19, FR-53 – FR-57; plan 3.5): an engine whose role has
//! `channel` runs a coordinator instead of a model loop. It routes each of
//! the person's messages to its members' seats, delivers it to them, and
//! writes their replies, hand-offs and notices into the channel.

pub mod coordinator;
pub mod route;
