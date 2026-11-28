# 0029. The harness's evals live in a dev-only crate on top of the engine

| | |
| --- | --- |
| Status | Proposed |
| Date | 2026-11-28 |
| ADD sections | [5.1 Crate layers](../README.md#51-crate-layers) |
| Related | [Learning layers](../../features/learning-layers/spec.md) (FR-26 – FR-32) |

## Context

Changes to the channel router, prompts and memory code could make mates
behave worse without any unit test failing, and the 2026-11 channel faults
came back in different shapes. The behaviour lives in Rust (`evo-engine`'s
coordinator, memory and learning code, `evo-mesh`'s hub), and a scripted
model (`evo-ai`'s `faux`) already exists. Evals need to drive that code
end to end, read data files, and run in CI without a network.

## Decision

A new crate, `evo-evals` (`publish = false`, binary `evo-evals`), depends on
`evo-engine`, `evo-ai`, `evo-store` and `evo-mesh` and nothing depends on it.
Its data lives in `evals/` at the root. The offline tier is a CI job; the
live tier runs only with `EVO_EVAL_LIVE=1`.

## Consequences

- The engine exposes the decision functions the evals drive (`channel::route`,
  `channel::coordinator::{hand_off, Owed, PairLog, job_entry}`,
  `evo_mesh::hub::mail_inner`, `learning::*`) as public items.
- A behaviour change that moves a scenario or a golden baseline updates the
  data file in the same commit, reviewed like code.
- Shell-only behaviour (the desktop's own Cargo workspace) is outside the
  suite; it keeps its own tests.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Evals in TypeScript | The coordinator, hub and memory are Rust, and so is the scripted provider. |
| Tests inside `evo-engine` | Scenarios as data files, a report per run and a live tier are a tool of their own, not unit tests. |
