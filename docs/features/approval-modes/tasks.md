# Approval modes enforced in the engine — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-07 |

## 1. Phases

### Phase 1 — Engine gate and app setting

Engine gate, classification, argument, RPC commands, protocol; app setting,
broadcast, panel options, hints, grant chips; one-time move and upgrade note;
ADD §9 and §8 lines.

Done when: FR-1–FR-10 and FR-12–FR-15 pass.

- [x] T1 [FR-4, NFR-1] Write the classification tests (classification table, shell commands, unknown tools, a timing check under 1 ms) — `crates/evo-engine/src/approval.rs` — check: `cargo test -p evo-engine approval::` fails for the right reason
- [x] T2 [FR-4] Expose the per-command check for reuse, with no behaviour change for read-only agents — `crates/evo-engine/src/read_only.rs` — check: existing read-only agent tests pass
- [x] T3 [FR-4, FR-5, NFR-1] Implement `ApprovalMode`, `ApprovalState` and `classify` — `crates/evo-engine/src/approval.rs` — check: T1 passes
- [x] T4 [FR-3, FR-5, FR-7, FR-8, FR-12, FR-13, NFR-3] Write the RPC tests: each mode × kind, grants, hook allow/block ordering, `set_approval_mode` mid-run, headless refusal, stop with an open prompt — `crates/evo-engine` tests — check: `cargo test -p evo-engine` fails for the right reason
- [x] T5 [FR-5, FR-6, FR-8, FR-9, FR-13, NFR-3] Implement the `before_tool_call` link and prompt text; put it last in the chain and pass the subagent name — `crates/evo-engine/src/approval.rs`, `crates/evo-engine/src/session.rs` — check: T4 mode, ordering and stop tests pass
- [x] T6 [FR-2, FR-12] Add `--approval-mode` with its defaults — `crates/evo-engine/src/args.rs` — check: T4 headless refusal test passes
- [x] T7 [FR-3, FR-7, NFR-2] Add the three commands and the `approval` state field to the protocol — `crates/evo-engine/src/rpc/protocol.rs` (the wire contract lives there, not in `evo-protocol`) — check: `cargo test -p evo-engine rpc::protocol`
- [x] T8 [FR-3, FR-7] Handle `set_approval_mode`, `get_approval_state`, `clear_approval_grants`; include the mode and grants in `get_state` — `crates/evo-engine/src/modes.rs` — check: T4 passes
- [x] T9 [FR-1, FR-2] Read `approvalMode` from the app database and pass `--approval-mode` to every engine — `apps/desktop/src-tauri/src/engine.rs` — check: `cargo check` in `src-tauri`; a nested session asks before a command in `ask`
- [x] T10 [FR-1, FR-15] Drop the local `approval` value; read and write the app setting; one-time move of the stored local value — `apps/desktop/src/stores/ui-store.ts` — check: change the mode, restart, the menu shows the same mode
- [x] T11 [FR-3, FR-7, NFR-2] Broadcast `set_approval_mode` on change, read grants from state, add `clearApprovalGrants(chat)`, log an unknown command once — `apps/desktop/src/stores/engine-store.ts` — check: `bun run typecheck`; manual mode switch mid-run
- [x] T12 [P] [FR-6, FR-9, FR-10] Show the summary line, the third option and the subagent label — `apps/desktop/src/features/chat/approval-panel.tsx` — check: manual prompts in the running app
- [x] T13 [P] [FR-7, FR-14, FR-15] New mode hints, grant chips and the one-time upgrade note (`approvalNoteSeen`) — `apps/desktop/src/features/chat/chat-composer.tsx` — check: manual in the running app
- [x] T14 [FR-3, FR-13] Add the approval gate and its commands to ADD §9 and §8 — `docs/architecture/README.md` — check: review
- [ ] T15 [FR-1, FR-2, FR-6, FR-9, FR-10, FR-14, FR-15] Check the running app over WebView2 DevTools: menu, restart, prompts, subagent label, sidebar dot — check: manual
- [ ] T16 [NFR-2] Run the app with an older engine binary — check: manual

### Phase 2 — Scheduled task setting

Done when: FR-11 passes.

- [ ] T17 [FR-11] Add "When it needs approval" (wait for me / run without asking) to the task editor and the task record; a "run without asking" run starts its engine with `--approval-mode auto` and gets no later mode changes — `apps/desktop/src/features/scheduler/`, scheduler store — check: a task set to run without asking edits a file with nobody at the computer

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Running app over WebView2 DevTools: change the mode, restart, menu shows the same mode | not run (needs the running app; `bun run typecheck` passes) |
| FR-2 | Running app: a nested session started by the mesh hub asks before a command in `ask` | not run (needs the running app; `cargo check` in `src-tauri` passes, every engine starts through `engine_start`) |
| FR-3 | `cargo test -p evo-engine` RPC test: `set_approval_mode` mid-run | passed (2026-11-07) — `rpc_changes_and_reports_the_approval_mode`, `a_mode_change_applies_from_the_next_call`; real engine over stdin: `set_approval_mode` then `get_approval_state` |
| FR-4 | `cargo test -p evo-engine approval::` (classification table, shell commands, unknown tools) | passed (2026-11-07) — `approval::tests` classification |
| FR-5 | `cargo test -p evo-engine` RPC tests: each mode × kind | passed (2026-11-07) — `each_mode_asks_for_the_kinds_it_says` and the gate tests per mode |
| FR-6 | Running app: edit and command prompts show the summary and three options | not run in the app; engine prompt shape passed (`ask_mode_asks_for_a_command_and_runs_it_when_allowed`, `prompts_summarise_the_call`) |
| FR-7 | `cargo test -p evo-engine` RPC tests: grants | passed (2026-11-07) — `allow_for_this_chat_stops_that_kind_until_cleared` |
| FR-8 | `cargo test -p evo-engine` RPC tests: denied call result | passed (2026-11-07) — `a_denied_call_is_blocked_with_the_reason` |
| FR-9 | Running app: subagent prompt label | not done — the prompt reaches the parent chat, but the engine does not send the subagent name yet (`approval.agent`); the panel already shows it when set |
| FR-10 | Running app: sidebar dot and notification for a background chat | not run |
| FR-11 | Scheduled task set to run without asking | not run |
| FR-12 | `cargo test -p evo-engine` RPC test: headless refusal | passed (2026-11-07) — `without_a_ui_a_call_that_needs_approval_fails_clearly`, `engines_start_in_the_mode_they_were_given` |
| FR-13 | `cargo test -p evo-engine` RPC tests: hook allow/block ordering | passed (2026-11-07) — `a_block_from_an_earlier_link_wins_even_in_auto`, `a_call_a_hook_allowed_does_not_ask_again`, `rewritten_arguments_are_what_gets_classified` |
| FR-14 | Running app: menu hints | not run |
| FR-15 | Running app: upgrade with `auto` picked, note shown once | not run |
| NFR-1 | `cargo test -p evo-engine approval::` timing check | passed (2026-11-07) — `classifying_takes_well_under_a_millisecond` |
| NFR-2 | App with an older engine binary | not run |
| NFR-3 | `cargo test -p evo-engine` RPC test: stop with an open prompt | passed (2026-11-07) — `stopping_the_turn_closes_the_prompt` |
