# Auto-compact threshold: the person chooses when a chat compacts — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-23 |

## 1. Phases

### Phase 1 — The rule and the engine

Done when: the engine compacts at the chosen percent, takes a new value live, and reports it.

- [x] T1 [FR-2] Tests: `threshold` (percent, floor of 2 × keep-recent, cap at the window, `None` = old rule) and `should_compact` with a percent — `crates/evo-harness/src/compaction/settings.rs` — check: `cargo test -p evo-harness compaction::settings` fails before T2
- [x] T2 [FR-2, FR-3] `trigger_percent`, `threshold`, `should_compact`, `AgentHarness::set_compaction`; test literals updated — `crates/evo-harness/src/types.rs`, `compaction/settings.rs`, `agent_harness.rs`, `crates/evo-harness/tests/*.rs` — check: T1 passes; `cargo test -p evo-harness`
- [x] T3 [FR-1, FR-6] Tests then code: `compaction.triggerPercent` in `settings.json`, the default 90 and the `reserveTokens` exception — `crates/evo-engine/src/settings.rs` — check: `cargo test -p evo-engine settings`
- [x] T4 [FR-1, FR-3] `--compaction-config` and the `set_compaction` command (clamping, refusal), applied at session creation — `crates/evo-engine/src/args.rs`, `session.rs`, `modes.rs` — check: an rpc test in `modes.rs`
- [x] T5 [FR-5] The breakdown and the state line use `threshold`; they report `triggerPercent` — `crates/evo-engine/src/context_breakdown.rs`, `modes.rs` — check: breakdown test

### Phase 2 — The shell

Done when: saving the app setting reaches every engine, and a new engine starts with it.

- [x] T6 [FR-3] Tests: a change reaches every set-up engine, a starting one once set up — `apps/desktop/src-tauri/src/supervisor/settings.rs` — check: `bun run test:shell` fails before T7
- [x] T7 [FR-3] `COMPACTION_KEY`, `compaction_config`, the start arg, `Change::Compaction`, `Pending`, `on_setting` — `apps/desktop/src-tauri/src/engine.rs`, `supervisor/settings.rs` — check: T6 passes; `cargo check`

### Phase 3 — The controls

Done when: the slider and switch work in Settings and in the context panel, and both show the same value.

- [x] T8 [FR-4, FR-5, NFR-1] `Slider`; the compaction store; the shared control; Settings row; context panel row; protocol types — `apps/desktop/src/components/ui/slider.tsx`, `stores/compaction-store.ts`, `features/chat/compaction-control.tsx`, `features/settings/sections/general-settings.tsx`, `features/chat/context-panel.tsx`, `engine/protocol.ts` — check: `bun run typecheck`; running app
- [x] T9 UI map and feature catalogue — `apps/desktop/README.md`, `docs/features/README.md` — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `bun test src/stores/compaction-store.test.ts` (4); `cargo test -p evo-engine settings`; saving across a restart in the running app | tests passed (2026-11-23); running app not run |
| FR-2 | `cargo test -p evo-harness compaction::settings` (9) | passed (2026-11-23) |
| FR-3 | `bun run test:shell` (722, with `auto_compact_reaches_every_set_up_engine_and_a_starting_one_once_set_up`); `rpc_changes_the_auto_compact_threshold`; running app with two chats | tests passed (2026-11-23); running app not run |
| FR-4 | `bun run typecheck`; running app: Settings → General | typecheck passed (2026-11-23); running app not run |
| FR-5 | `cargo test -p evo-engine context_breakdown` (3); running app: the panel | tests passed (2026-11-23); running app not run |
| FR-6 | `cargo test -p evo-engine settings` (the percent, the clamp, the `reserveTokens` exception) | passed (2026-11-23) |
| FR-7 | `cargo test -p evo-harness` (455 lib tests and the compaction integration tests, unchanged) | passed (2026-11-23) |
| NFR-1 | Keyboard walkthrough (Base UI slider: arrows, Home, End; labelled) | not run |
| NFR-2 | Code review: saved on release, on a key press or after a 250 ms pause | not run in the app |
| NFR-3 | Light and dark | not run |
