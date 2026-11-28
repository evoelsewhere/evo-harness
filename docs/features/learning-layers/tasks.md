# Learning layers: memory upkeep, dream, retro and evals — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-28 |

## 1. Phases

### Phase 1 — Journal, entries and redaction

Done when: every memory change in both apps is a journal row with its
author and can be undone; expired and struck entries no longer reach a
turn; secrets never reach disk.

- [x] T1 [FR-1, FR-2, NFR-4, NFR-6] Failing store tests: `memory_changes` rows migrate into `journal` with `by='index'`; rows keep 500 per owner and subject; usage accepts the four new sources — `crates/evo-store/src/journal.rs`, `crates/evo-store/tests/` — check: `cargo test -p evo-store journal` fails for the right reason
- [x] T2 [FR-1, FR-2, NFR-4, NFR-6] Migration `0018_learning.sql` and `journal.rs`, `learning.rs`; remove `memory_log.rs` users — `crates/evo-store/src/` — check: T1 passes; an old `evo.db` copy upgrades
- [x] T3 [P] [FR-9] Failing tests then `learning/redact.rs`: each secret shape is replaced, ordinary text and code are not — `crates/evo-engine/src/learning/redact.rs` — check: `cargo test -p evo-engine redact`
- [x] T4 [P] [FR-4, FR-5] Failing tests then `learning/entries.rs`: parse and write the grammar, `visible(text, today)`, archive move with `· moved`, yearly dates never expire, unparsed lines kept — `crates/evo-engine/src/learning/entries.rs` — check: `cargo test -p evo-engine entries`
- [x] T5 [FR-1 – FR-5, FR-9] Every topic write goes redact → write → journal with its author (agent, person, index, undo); `read` returns visible text; `until` on save/update; undo writes a row; hash-checked panel save — `crates/evo-engine/src/memory.rs`, `tools/memory.rs`, `modes.rs` — check: engine tests `every_memory_write_is_journaled`, `an_expired_entry_is_not_read`, `a_stale_panel_save_is_refused`
- [ ] T6 [FR-1 – FR-3] Memory panel: journal list with source, chat link and Undo; conflict dialog keeping the draft (**Reload**, **Overwrite with mine**); mate panel Memory tab the same — `apps/desktop/src/features/memory/`, `features/mates/mate-panel.tsx` — check: manual in the running app

### Phase 2 — Background model, learn mode and tidy-up

Done when: Tidy now archives, dedupes and strikes contradictions with
journal rows, in both apps, on the Background model.

- [x] T7 [FR-24] Background model: `learning.backgroundModel` copied once from `memory.indexModel`; indexing reads it; Settings › Memory renames Index model — `apps/desktop/src-tauri/src/memory_index.rs`, `features/settings/sections/memory-settings.tsx`, `stores/memory-store.ts` — check: shell test `the_index_model_becomes_the_background_model`
- [x] T8 [NFR-1, NFR-4, NFR-5] `--mode learn` skeleton: args, owner resolution (dirs per owner), model resolution (Background model, else the owner's), tool-free request, 3-minute limit, JSON result line, usage source per task — `crates/evo-engine/src/learning/mod.rs`, `modes.rs`, `args.rs` — check: engine test `a_learn_run_sends_no_tools` with faux
- [x] T9 [FR-6, FR-7] Failing tests then `learning/tidy.rs`: expiry → archive, exact duplicates, contradiction prompt and parse, protected lines never offered, a fifth at most, none under five, still-true part kept — `crates/evo-engine/src/learning/tidy.rs` — check: `cargo test -p evo-engine tidy`
- [x] T10 [FR-6, NFR-2, NFR-5] Shell clock loop: nightly due per owner after 03:00 local and idle, one process per owner, failure counts; `learning_tidy_now` — `apps/desktop/src-tauri/src/learning/{mod,clock,jobs,commands}.rs`, `lib.rs` — check: shell tests `tidy_is_due_once_a_night_after_three`, `a_working_owner_is_not_tidied`
- [ ] T11 [FR-6, FR-23, FR-25] Settings switches (Harness off, Mates on), per-mate switches, **Tidy now** in the Memory panel and mate panel — `features/settings/sections/{memory,mates}-settings.tsx`, `features/mates/mate-panel.tsx` — check: manual

### Phase 3 — Quote checks and the dream

Done when: a nightly dream saves only cross-chat patterns backed by real
quotes, supersedes outdated facts and offers ideas the person can accept.

- [x] T12 [P] [FR-8] Failing tests then `learning/quotes.rs`: a quote must be in its chat (folded), numbers, paths and URLs must be in the quotes, any language — `crates/evo-engine/src/learning/quotes.rs` — check: `cargo test -p evo-engine quotes`
- [ ] T13 [FR-8] The index run's memory tool takes `evidence` for `user` facts and refuses without a passing check — `crates/evo-engine/src/memory_index.rs`, `tools/memory.rs` — check: engine test `an_index_fact_without_a_real_quote_is_refused`
- [ ] T14 [FR-10 – FR-13] Failing tests then `learning/dream.rs`: chat selection (owner's lines only, no scheduled, mail or heartbeat), cuts, prompt, parse with limits and confidence, quote checks, apply learned and outdated with journal rows — `crates/evo-engine/src/learning/dream.rs` — check: `cargo test -p evo-engine dream`
- [ ] T15 [FR-14] Suggestions: store ideas; Harness Memory panel **Suggestions**; mate conversation card with **Set it up** (opens the scheduled-task or skill editor filled in) and **No thanks** (never again) — `crates/evo-store/src/learning.rs`, `learning/commands.rs`, `features/memory/`, `features/mates/conversation.tsx` — check: manual
- [ ] T16 [FR-22] Learn from channels: the dream and a capture pass read a mate's channel stretches under channel rules, sourced to the channel — `crates/evo-engine/src/learning/dream.rs`, `learning/channels.rs`, `mates-settings.tsx` — check: engine test `another_mates_claim_is_not_learned`
- [ ] T17 [FR-10] Dream after tidy in the clock loop — `apps/desktop/src-tauri/src/learning/clock.rs` — check: shell test `the_dream_runs_after_the_tidy_with_two_chats`

### Phase 4 — Learned skills and the review

Done when: a correction in a conversation with real tool work becomes a
learned skill change, shown under the reply and undoable.

- [ ] T18 [FR-15] Learned-skills folders in the search order, `learnedSkillsDir` in a mate's role, Customize and the mate's Skills tab list them as learned with switch, Undo last change, Delete — `crates/evo-engine/src/resource_dirs.rs`, `customize.rs`, `crates/evo-mesh/src/wire.rs`, `src-tauri/src/mates/mod.rs`, `features/customize/`, `features/mates/skills-tab.tsx` — check: engine test `a_learned_skill_never_shadows_an_installed_one`; manual
- [ ] T19 [FR-16, FR-17] Failing tests then `learning/review.rs`: prompt (lesson, do-not-capture and authoring blocks), parse, at most 3 changes, update only shown learned skills keeping every line, names free and class-level, installed skills never changed — `crates/evo-engine/src/learning/review.rs` — check: `cargo test -p evo-engine review`
- [ ] T20 [FR-16] Review due: step count per chat since the last review, started on `agent_settled` — `apps/desktop/src-tauri/src/learning/jobs.rs` — check: shell test `a_review_starts_after_ten_tool_steps`
- [ ] T21 [FR-18] "Learned · <skill>" under the reply, from the journal, opening the change with Undo — `features/chat/`, `features/mates/conversation.tsx`, `learning/commands.rs` — check: manual

### Phase 5 — The retro

Done when: a channel job that ended gets a retro card with lessons in each
mate's learned skills, and Run retro works.

- [ ] T22 [FR-19] The coordinator writes a `job` entry at every end, with status and hand-off count — `crates/evo-engine/src/channel/coordinator.rs` — check: engine test `every_job_end_writes_a_job_entry`
- [ ] T23 [FR-20] Failing tests then `learning/retro.rs`: job lines with authors and kinds, prompt, parse, at most 2 lessons per mate, lessons applied through the review rules, team hand-off lessons to the lead — `crates/evo-engine/src/learning/retro.rs` — check: `cargo test -p evo-engine retro`
- [ ] T24 [FR-19, FR-21] Retro due after 2 minutes of quiet, `channel_post_retro` RPC posts the card, notification only after a limit, stop or failure, **Run retro** — `learning/jobs.rs`, `crates/evo-engine/src/modes.rs`, `channel/coordinator.rs` — check: shell test `a_retro_waits_for_two_quiet_minutes`
- [ ] T25 [FR-21] Retro card in the channel with lessons and Undo; wakes nobody — `lib/mates.ts`, `features/mates/channel-rows.ts`, `channel-view.tsx` — check: manual

### Phase 6 — Evals

Done when: `cargo run -p evo-evals -- offline` passes in CI with the named
scenarios, golden replay and offline case scoring; the live tier skips
without its switch.

- [ ] T26 [FR-26, FR-27] `crates/evo-evals`: scenario format, runner, frozen evidence, scorers with one failing message per assertion kind, report — `crates/evo-evals/`, `evals/` — check: `cargo test -p evo-evals scorers`
- [ ] T27 [FR-26] Worlds: chat on faux, channel (hub + coordinator + faux seats), memory in a temp agent dir — `crates/evo-evals/src/worlds/` — check: a smoke scenario per world passes twice with the network off
- [ ] T28 [FR-28] Scenarios for the 2026-11 channel faults, the hand-off limit, the router fall-through, journal and undo, expiry, tidy protected lines, dream and review checks — `evals/scenarios/` — check: all pass on the current code; each fails when its fix is reverted (spot check two)
- [ ] T29 [FR-29] Golden: redaction with leak scan, trace extraction, replay against baselines; one redacted channel from the test data — `crates/evo-evals/src/golden/`, `evals/golden/` — check: `cargo test -p evo-evals golden`
- [ ] T30 [FR-30] Case sets dream, router, tool choice with offline scoring of saved answers — `evals/cases/`, `crates/evo-evals/src/cases/` — check: `cargo test -p evo-evals cases`
- [ ] T31 [FR-31] Live tier behind `EVO_EVAL_LIVE=1`: real model, judge prompts pinned by sha256, baseline and drift — `crates/evo-evals/src/live/`, `evals/live/` — check: without the switch it prints the skip and passes; one live run on the owner's machine
- [ ] T32 [FR-32, NFR-7] CI job and report; time the offline tier — `.github/workflows/ci.yml` — check: CI run green; under 5 minutes

### Phase 7 — Docs and wrap-up

- [ ] T33 [A-4] NOTICE credits; `apps/desktop/README.md` UI map; `evals/README.md` — check: review
- [ ] T34 Run every check in section 2 and record the results — check: table filled

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `every_memory_write_is_journaled`; store `journal` tests | not run |
| FR-2 | engine undo test; manual Undo in the panel | not run |
| FR-3 | `a_stale_panel_save_is_refused`; manual | not run |
| FR-4 | `entries` tests; `an_expired_entry_is_not_read` | not run |
| FR-5 | `entries` tests | not run |
| FR-6 | `tidy` tests; `tidy_is_due_once_a_night_after_three`; manual Tidy now | not run |
| FR-7 | `tidy` tests (protected lines) | not run |
| FR-8 | `quotes` tests; `an_index_fact_without_a_real_quote_is_refused` | not run |
| FR-9 | `redact` tests | not run |
| FR-10 | `dream` tests; `the_dream_runs_after_the_tidy_with_two_chats` | not run |
| FR-11 | `dream` tests (kinds and limits) | not run |
| FR-12 | `dream` tests; eval case set `dream` | not run |
| FR-13 | `dream` tests (journal rows) | not run |
| FR-14 | manual: Set it up, No thanks | not run |
| FR-15 | `a_learned_skill_never_shadows_an_installed_one`; manual | not run |
| FR-16 | `review` tests; `a_review_starts_after_ten_tool_steps` | not run |
| FR-17 | `review` tests | not run |
| FR-18 | manual: Learned line and Undo | not run |
| FR-19 | `every_job_end_writes_a_job_entry`; `a_retro_waits_for_two_quiet_minutes` | not run |
| FR-20 | `retro` tests | not run |
| FR-21 | manual: retro card wakes nobody | not run |
| FR-22 | `another_mates_claim_is_not_learned` | not run |
| FR-23 | manual: switches off stop every step | not run |
| FR-24 | `the_index_model_becomes_the_background_model`; `a_learn_run_sends_no_tools` | not run |
| FR-25 | manual: one mate switched off | not run |
| FR-26 | `evo-evals` runs twice offline with the same verdicts | not run |
| FR-27 | `cargo test -p evo-evals scorers` | not run |
| FR-28 | `evals/scenarios/` all pass | not run |
| FR-29 | `cargo test -p evo-evals golden` | not run |
| FR-30 | `cargo test -p evo-evals cases` | not run |
| FR-31 | live tier skip without the switch; one live run | not run |
| FR-32 | CI job | not run |
| NFR-1 | `a_learn_run_sends_no_tools` | not run |
| NFR-2 | manual: a message during a dream starts at once | not run |
| NFR-3 | review of the settings text | not run |
| NFR-4 | store usage test; manual Usage page | not run |
| NFR-5 | engine test with a failing faux model | not run |
| NFR-6 | store `journal` keep test | not run |
| NFR-7 | timed CI run | not run |
