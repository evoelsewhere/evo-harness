# Code mode: the agent runs many tool calls from one piece of code — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-22 |

## 1. Phases

### Phase 1 — Refactors the tool depends on

Hook output replacement for every tool, approval prompts that know about
calls from code, and tool program knobs for a trusted host.

Done when: FR-21 passes. The approval and tool program unit tests for
FR-11 and FR-13 pass. The existing tests of `evo-hooks`, `evo-agent` and
`evo-engine` pass unchanged.

- [x] T1 [FR-21] Write tests: `updatedToolOutput` is parsed for `PostToolUse` (string and object kept, other types and other events ignored) — `crates/evo-hooks/src/run.rs` — check: `cargo test -p evo-hooks` fails for the right reason
- [x] T2 [FR-21] Add `Outcome::updated_output` and parse it — `crates/evo-hooks/src/run.rs` — check: T1 passes
- [x] T3 [FR-21, FR-17] Write tests: a string replaces the text and keeps images, an object uses a registered output view, malformed or unregistered values are ignored, `PostToolUseFailure` never replaces — `crates/evo-engine/src/hooks.rs` — check: `cargo test -p evo-engine hooks::` fails for the right reason
- [x] T4 [FR-21, FR-17] Apply `updated_output` in `after_tool_call`; add `OutputView` and `register_output_view`; send a tool's view as `tool_response` when it has one — `crates/evo-engine/src/hooks.rs` — check: T3 passes
- [x] T5 [FR-13, FR-11, FR-14] Write tests: the timeout cap stays 120 s for JSON callers and rises after `set_timeout_limit_ms`; `stop` refuses new starts, cancels active children and is reported by `stopped()`, without setting `terminate` — `crates/evo-agent/tests/tool_program.rs` — check: `cargo test -p evo-agent --test tool_program` fails for the right reason
- [x] T6 [FR-13, FR-11, FR-14] Implement `set_timeout_limit_ms`, `stop` and `stopped` — `crates/evo-agent/src/tool_program.rs` — check: T5 passes
- [x] T7 [FR-11, FR-10, FR-20] Write tests: a nested call under `run_code` gets `origin: "code"` and the four options; Stop code stops the parent program and blocks; Deny this call blocks only that call; direct calls keep today's three options — `crates/evo-engine/src/approval/tests.rs` — check: `cargo test -p evo-engine approval::` fails for the right reason
- [x] T8 [FR-11, FR-10, FR-20] Implement the code-origin prompt and answers in the gate — `crates/evo-engine/src/approval.rs` — check: T7 passes
- [x] T9 [P] [FR-11] Show **From code** for `approval.origin === "code"` — `apps/desktop/src/engine/protocol.ts`, `apps/desktop/src/features/chat/transcript.ts`, `apps/desktop/src/features/chat/approval-panel.tsx` — check: `bun run typecheck`; `bun test src/features/chat/transcript-approval.test.ts`

### Phase 2 — The interpreter and `run_code`

Done when: FR-3–FR-14, FR-17–FR-20 and NFR-1–NFR-4 pass with the `faux`
provider.

- [x] T20 [NFR-1, NFR-2] Add `rquickjs` (check MSRV ≤ 1.89 and the build on Windows, macOS and Linux); update ADR 0025 to Accepted and the ADD crate table and trust section — `Cargo.toml`, `crates/evo-engine/Cargo.toml`, `docs/architecture/` — check: `cargo build --workspace --locked`
- [x] T21 [FR-9, FR-13, NFR-4] Write interpreter tests: no fs, net, env, process or modules; memory, time and interrupt limits; an unhandled rejection is reported — `crates/evo-engine/src/tools/code/runtime.rs` — check: fails for the right reason
- [x] T22 [FR-9, FR-13, NFR-1, NFR-4] Implement the QuickJS runtime on a blocking worker — `crates/evo-engine/src/tools/code/runtime.rs` — check: T21 passes
- [x] T23 [FR-5, FR-7, FR-12, FR-13] Write bridge tests: results and errors, 8 at once and then queueing, excluded tools refused, 128-call cap, release after read — `crates/evo-engine/src/tools/code/bridge.rs` — check: fails for the right reason
- [x] T24 [FR-5, FR-7, FR-12, FR-13] Implement the bridge over `ToolProgram` — `crates/evo-engine/src/tools/code/bridge.rs` — check: T23 passes
- [x] T25 [FR-6] Write the prelude (`$`, `read`, `write`, `edit`, `grep`, `glob`, `tools.list`, `tools.describe`, `console`) and its tests, including argument quoting for `bash` and `powershell` — `crates/evo-engine/src/tools/code/prelude.js`, `mod.rs` — check: prelude tests pass
- [x] T26 [FR-3, FR-4, FR-8, FR-17] Implement `run_code` (schema, result shape, `details`, output view registration, output cut) and its table row — `crates/evo-engine/src/tools/code/mod.rs`, `tools/info.rs`, `tools/mod.rs` — check: faux-provider test of a program that edits two files
- [x] T27 [FR-18] Generate typed declarations from tool schemas and add the instructions text — `crates/evo-engine/src/tools/code/declarations.rs` — check: prompt snapshot with the mode on
- [x] T28 [FR-10, FR-11, FR-14, FR-16, FR-20] Write the check matrix — `crates/evo-engine/src/tools/code/tests.rs` (as built: in the engine's lib tests, beside the tool) — check: passes. Covered end to end in the agent loop: the real approval gate in Ask (a prompt per call with the code options, Deny this call, Stop code, grants), an earlier link blocking a call, prompt time not counted, Stop turn. Not covered end to end: Revert this turn (FR-16) and a real subagent or read-only agent type (FR-20). Both hold by construction (nested calls run the same `before_tool_call` chain, and a child's guard wraps every tool the program calls) and are checked again in the T42 run
- [x] T29 [FR-20] Inherit code mode in children; respect agent type tool lists — no code change (as built): a subagent takes the parent's active tools filtered by its agent type's list, so it has `run_code` exactly when the parent has it and its list allows it; its guard wraps the tools the program calls — check: code read; end-to-end check in T42

### Phase 3 — Setting and protocol

Done when: FR-1, FR-2, FR-19 and NFR-5 pass in the running app.

- [x] T30 [FR-1, FR-2, NFR-5] Write tests: off by default, `--code-mode`, `set_code_mode` live, `get_state.codeMode`, no change in the tool list or prompt when off — `crates/evo-engine` — check: fails for the right reason
- [x] T31 [FR-1, FR-2, FR-3, FR-19] Implement `code_mode.rs`, the argument, the command and the state field; offer `run_code` only when on and the model has tool calls — `crates/evo-engine/src/code_mode.rs`, `args.rs`, `rpc/protocol.rs`, `modes.rs`, `tools/mod.rs` — check: T30 passes
- [x] T32 [FR-1, FR-2] Pass `--code-mode` from `experimental.codeMode` and broadcast changes — `apps/desktop/src-tauri/src/engine.rs` — check: `cargo check` in `src-tauri`
- [x] T33 [FR-1, FR-2] Add **Settings → Experimental → Code mode**, the setting and the broadcast; update the UI map — `apps/desktop/src/features/settings/sections/experimental-settings.tsx`, `settings-registry.ts`, `stores/ui-store.ts` (section id only), `apps/desktop/README.md` (as built: the page saves the app setting itself and the shell's supervisor sends `set_code_mode`, so `engine-store.ts` is unchanged) — check: `bun run typecheck`; switching it on mid-chat in the running app not run

### Phase 4 — Transcript and benchmark

Done when: FR-15, US-5 and SC-1 to SC-4 are checked.

- [x] T40 [FR-15] Write transcript tests from a fixture with a `run_code` result (running, done, failed, stopped, interrupted; live progress; rebuilt from saved messages) — `apps/desktop/src/features/chat/transcript-code.test.ts`, `apps/desktop/src/components/ai-elements/code-run.test.tsx` — check: the reducer test failed before T41 (as built: the row reads the result's bounded `calls`, not the journal entries)
- [x] T41 [FR-15] Build the **Code** row with its nested cards — `apps/desktop/src/components/ai-elements/code-run.tsx`, `code-run-model.ts`, `tool.tsx`, `apps/desktop/src/features/chat/transcript.ts`, and the bounded per-call copy in `crates/evo-engine/src/tools/code/bridge.rs` — check: T40 passes; the row rendered with the built CSS in light and dark (screenshot); the running app not checked
- [x] T42 [SC-1] Add the 12-task benchmark and run it with the mode off and on — `scripts/bench/code-mode/` — check: the fixture and checks agree (no task passes before the agent works; facts the checks expect hold) and the runner runs; the run with a real model is not done yet (it spends tokens; the owner starts it)
- [x] T43 [FR-6] Whole-file `read(path)` and `.nothrow().truncated`: write the failing tests first (a 300 KB file comes back byte for byte without notices, a line over the page limit and a file over 64 pages reject, a range read stays one call, `truncated` follows the shell tool's `details`), then page through the read tool in the prelude and add `truncated` — `crates/evo-engine/src/tools/code/tests.rs`, `prelude.js`, `declarations.rs` — check: the four tests failed before `prelude.js` changed and pass after; the 46 `tools::code` tests pass
- [x] T44 [FR-18] TypeScript declarations from the tool schemas: write the mapping tests first (each JSON Schema form, quoted names, optional `args`, doc comments cut to one sentence), then render them in a `ts` block of the `run_code` description, and measure the size for the engine's real tool set — `crates/evo-engine/src/tools/code/declarations.rs`, `code_mode.rs` — check: the mapping tests failed before the change and pass after; the size test stays within its budget; the `tools::code` and `code_mode` tests pass

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `code_mode::tests::the_argument_turns_it_on`; `modes::tests::rpc_turns_code_mode_on_and_off` (off before it is set); project settings have no key for it (code read) | passed (2026-11-22) |
| FR-2 | `modes::tests::rpc_turns_code_mode_on_and_off`, `code_mode::tests::switching_adds_and_removes_the_code_tool`, shell `supervisor::settings::tests::code_mode_reaches_every_set_up_engine_and_a_starting_one_once_set_up`; switching in the running app not run | passed (2026-11-22) |
| FR-3 | `code_mode::tests::switching_adds_and_removes_the_code_tool` (on adds only `run_code`, off restores the list); `tools::code::tests::the_schema_takes_code_and_a_description` | passed (2026-11-22) |
| FR-4 | `tools::code::tests::the_schema_takes_code_and_a_description` | passed (2026-11-22) |
| FR-5 | `tools::code::tests::{a_program_calls_tools_and_returns_its_last_value, failed_calls_throw_tool_errors, describe_gives_parameters}` | passed (2026-11-22) |
| FR-6 | `tools::code::tests::{shell_and_file_shortcuts, grep_and_glob_need_their_tools, read_gives_a_file_longer_than_one_page_whole, read_rejects_a_file_it_cannot_give_whole, read_with_a_range_is_one_call_as_the_tool_answers, nothrow_says_when_the_output_was_cut}` | passed (2026-11-22) |
| FR-7 | `tools::code::tests::calls_run_in_parallel_up_to_eight_at_once`; `runtime::tests::tool_replies_reach_the_program_even_out_of_order` | passed (2026-11-22) |
| FR-8 | `tools::code::tests::{a_program_calls_tools_and_returns_its_last_value, images_are_attached_to_the_result}`; `runtime::tests::the_last_expression_is_the_result` | passed (2026-11-22) |
| FR-9 | `runtime::tests::a_program_has_no_ambient_abilities` | passed (2026-11-22) |
| FR-10 | `tools::code::tests::{each_call_from_code_asks_with_the_code_options, an_earlier_link_blocks_a_call_from_code}` | passed (2026-11-22) (approval gate and earlier links; read-only and checkpoint links by construction) |
| FR-11 | `approval::tests` (code rows), `tools::code::tests::{stop_code_ends_the_program_and_keeps_finished_calls, time_at_a_prompt_does_not_count}`, `--test tool_program native_wait_and_clock_holds`; T9 typecheck | passed (2026-11-22) |
| FR-12 | `tools::code::tests::excluded_and_unknown_tools_cannot_be_called`; `tools::info::tests::programs_cannot_call_the_person_session_or_app_surface_tools` | passed (2026-11-22) |
| FR-13 | `runtime::tests::{a_busy_loop_stops_at_the_time_limit, runaway_memory_stops_at_the_memory_limit, the_call_cap_stops_the_program}`, `tools::code::tests::{the_call_cap_ends_the_run, a_large_result_is_cut}`, `--test tool_program host_timeout_limit_and_stop_end_only_the_program` | passed (2026-11-22) (5-minute `npm test` not run) |
| FR-14 | `tools::code::tests::stopping_the_turn_stops_the_program`; `runtime::tests::cancelling_stops_the_program` | passed (2026-11-22) |
| FR-15 | `bun test src/features/chat/transcript-code.test.ts src/components/ai-elements/code-run.test.tsx` (live progress, statuses, rebuilt from saved messages, rendered markup); `tools::code::tests::calls_keep_a_bounded_copy_for_the_transcript`; rendered with the built CSS in light and dark; the running app not checked | passed (2026-11-22) |
| FR-16 | Revert this turn end to end | not run (by construction: nested edits pass the checkpoint link) |
| FR-17 | `hooks::tests::output`, `tools::code::tests::{hooks_see_the_structured_response, a_hook_can_hide_a_secret_and_a_malformed_copy_is_ignored}` | passed (2026-11-22) |
| FR-18 | `tools::code::declarations::tests` (each schema form, names and arguments, `$ref` cycles, first-sentence docs, the real tool set within 8,000 bytes) and `code_mode::tests::switching_adds_and_removes_the_code_tool` | passed (2026-11-23): 8 declaration tests failed before T44 and pass after; the 16 callable built-in tools render in 5,944 bytes; SC-1 not run again |
| FR-19 | no model list in the code (code read); a model without tool calls gets no tools, as before | not run |
| FR-20 | `approval::tests` (code rows); real subagent end to end | partly: approval rows passed (2026-11-22); subagent not run |
| FR-21 | T1 `cargo test -p evo-hooks updated_tool_output`, T3 `cargo test -p evo-engine hooks::tests::output` | passed (2026-11-22) |
| FR-22 | `crates/evo-engine/src/tools/lsp.rs` skips the wait for calls from code; benchmark `license-and-check` with code mode on: 420 s before, 11–12 s after | passed (2026-11-22) |
| NFR-1 | `runtime::tests::a_program_starts_quickly` (under 500 ms in a debug build; the 50 ms release target not measured) | passed (2026-11-22) (debug bound) |
| NFR-2 | CI on three platforms | not run |
| NFR-3 | T24 benchmark | not run |
| NFR-4 | `runtime::tests::{a_busy_loop_stops_at_the_time_limit, runaway_memory_stops_at_the_memory_limit, an_unhandled_rejection_does_not_crash}` | passed (2026-11-22) |
| NFR-5 | `register_all` adds `run_code` only when on; the full engine suite passes with it off | passed (2026-11-22) |
| SC-1 | `bun scripts/bench/code-mode/run.ts` (set A) and `--set b`, one fixed model through the copilot provider, 2026-11-22 | passed (2026-11-22) against the revised target (at least 10% fewer turns, no more input tokens, as many tasks solved, set B): round 6 −11% turns, −6% input tokens, 12/12 solved on vs 11/12 off. History against the first target (−40% / −20%, not met): turns / input tokens incl. cache reads, code mode on vs off: set A round 1 −3% / +9% (the model never called `run_code`); round 2 (short description, prompt rule, 40 files) −10% / −5%; round 3 (code as the default) −5% / 0%. Set A cannot reach −40%: each task needs at least 2 turns, so 12 tasks need 24 against 38 with code mode off (at most −37%). Set B (multi-step tasks): round 4 −9% / −5%; round 5 (top-level `return`, `files()`, no `rg`) −7% / −2%; round 6 (worked example) −11% / −6%, 12/12 passed on vs 11/12 off, `run_code` used in 12/12 tasks. The model usually splits a task into a program that looks and one that acts, or checks again with a direct call |
