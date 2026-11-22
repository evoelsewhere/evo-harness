# Auto-compact threshold: the person chooses when a chat compacts — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-23 |
| ADD sections | [Engine and shell](../../architecture/README.md) |
| ADRs | none |

## 1. Context and constraints

- `crates/evo-harness/src/compaction/settings.rs`: `should_compact(tokens, window, settings)` is `settings.enabled && tokens > window - settings.reserve_tokens`. `CompactionSettings { enabled, reserve_tokens, keep_recent_tokens }` (`types.rs`, `Copy`) is built from `settings.json` once, in `crates/evo-engine/src/session.rs` (`saved_settings.compaction_settings()`), and the harness has no setter. A run copies it at its start (`agent_harness.rs`: the pre-run check and the `after_tool_results` hook), so a change takes effect from the next run.
- `reserve_tokens` also sizes the summary (`summary_max_tokens` = 0.8 × reserve), so it stays.
- `crates/evo-engine/src/context_breakdown.rs` draws "Autocompact buffer" as `reserve_tokens` and `untilCompact` as `window - reserve - total`; `modes.rs` (`context_state`) reports `threshold`, `reserveTokens`.
- Precedent for an app setting that every engine follows live: `experimental.codeMode` — saved in the app settings (`evo-store`), passed at start (`--code-mode`, `src-tauri/src/engine.rs`) and fanned out by `src-tauri/src/supervisor/settings.rs` (`Change::CodeMode` → `set_code_mode`; a starting engine gets it once set up). This feature copies that path.
- `components/ui/` has no slider; `@base-ui/react/slider` is installed.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | The threshold rule lives in `evo-harness`; the shell only stores the setting and relays it. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | `evo-harness` gains a field and a setter; no upward dependency. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | ok | New command `set_compaction` (engine `modes.rs`, desktop `engine/protocol.ts`); `get_context_breakdown` and the state line gain `triggerPercent`/`threshold`. |
| Models come from models.dev; no hardcoded model facts | ok | The window comes from the model; only the percent is configured. |
| Real engine in the app; settings and credentials reload live | ok | Live through the supervisor fan-out. |
| Untrusted input stays untrusted | ok | The value is clamped in the engine; nothing is executed. |
| Desktop UI structure and `DESIGN.md` | ok | A `Slider` in `components/ui/` (generated style), a store, two rows. |

## 3. Design

### 3.1 Overview

`CompactionSettings` gains `trigger_percent: Option<u8>`. `None` keeps the old rule (window − reserve). `Some(p)` triggers at `max(window × p / 100, 2 × keep_recent_tokens)`, capped at the window. One method, `CompactionSettings::threshold(window)`, is the single source of the number: `should_compact`, the context breakdown and the state line all call it.

The app saves `compaction = { enabled, triggerPercent }`. The shell passes it at an engine's start (`--compaction-config <json>`) and, when it changes, sends `set_compaction` to every set-up engine (a starting one gets it once set up). The engine applies it to the harness (`AgentHarness::set_compaction`), so the next run uses it.

### 3.2 Components

| Layer | Path | Change | Serves |
| --- | --- | --- | --- |
| harness | `crates/evo-harness/src/types.rs`, `compaction/settings.rs`, `agent_harness.rs` | `trigger_percent`; `threshold(window)`; `should_compact` uses it; `set_compaction`. Test literals get `trigger_percent: None`. | FR-2, FR-3 |
| engine | `crates/evo-engine/src/settings.rs`, `args.rs`, `host.rs`/`session.rs` | `compaction.triggerPercent` in `settings.json`; default 90 unless `reserveTokens` is set without it; `--compaction-config` overrides. | FR-1, FR-6 |
| engine | `crates/evo-engine/src/modes.rs`, `context_breakdown.rs` | `set_compaction` command; buffer and `untilCompact` from `threshold`; `triggerPercent` and `threshold` in the state and breakdown. | FR-3, FR-5 |
| shell | `src-tauri/src/engine.rs`, `supervisor/settings.rs` | `COMPACTION_KEY`; `compaction_config(&db)`; start arg; `Change::Compaction` and `Pending.compaction`; `on_setting`. | FR-3 |
| UI | `components/ui/slider.tsx` | A Base UI slider in the shadcn style. | FR-4, FR-5, NFR-1 |
| UI | `stores/compaction-store.ts`, `engine/protocol.ts` | The saved value, loaded at start, saved through `db.setSetting`; protocol types. | FR-1 |
| UI | `features/settings/sections/general-settings.tsx`, `features/chat/context-panel.tsx`, `features/chat/compaction-control.tsx` | One shared control (switch + slider + value), used in both places; the panel reloads its breakdown after a save. | FR-4, FR-5 |

### 3.3 Interfaces

- App setting `compaction`: `{ "enabled": bool, "triggerPercent": 30..=95 }`.
- Engine arg `--compaction-config '{"enabled":true,"triggerPercent":90}'`.
- RPC `set_compaction { enabled?: bool, triggerPercent?: number }` → `{ enabled, triggerPercent, threshold }`.
- `get_context_breakdown` adds `triggerPercent` (number or null) and `threshold`.

### 3.4 Data and storage

No schema change: the value is one more key in the app settings table. `settings.json` keeps its `compaction` object and gains `triggerPercent`.

### 3.5 Behaviour

- Threshold: `Some(p)` → `min(window, max(window × p / 100, 2 × keep_recent))`; `None` → `window − reserve`. Compact when `enabled` and `tokens > threshold`.
- Default for an engine with neither the arg nor `settings.json`: 90 %. `reserveTokens` in `settings.json` without `triggerPercent` → `None` (old rule).
- The slider saves on release (and per key press); the panel reloads its breakdown shortly after.

### 3.6 Errors and edge cases

An out-of-range percent is clamped to 30–95 by the engine; a non-number is refused with a message. An engine without `set_compaction` (older sidecar) logs it and takes the value at its next start.

### 3.7 Security and trust

n/a — a number saved by the person; no new capability, no command or path.

### 3.8 Configuration

New keys: app setting `compaction`; `settings.json` `compaction.triggerPercent`. No flag.

## 4. Alternatives considered

- Computing the percent into `reserve_tokens` in the shell: the window is per model and changes with the model, so the engine must do it.
- A per-chat value stored in the chat row: not asked for (spec 2.2), needs a migration.

## 5. As-built notes

- The save is debounced: the slider writes on release, on a key press, or after a 250 ms pause (and when the control goes away mid-drag), because the context panel is a hover card that can close under a drag. Spec NFR-2 says "settled value"; the debounce keeps that.
- The context breakdown gained a small pure helper, `compaction_budget`, so the buffer and the threshold are tested without an engine.
- An engine started by an older sidecar rejects `--compaction-config` ("Unknown option") once the person has saved the setting: rebuild the sidecar (`bun run engine:sidecar`) with the shell.
- Not checked in the running app: the person's own app was running, and the app is single-instance.
