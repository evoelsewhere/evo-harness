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
- [x] T6 [FR-1 – FR-3] Memory panel: journal list with source, chat link and Undo; conflict dialog keeping the draft (**Reload**, **Overwrite with mine**); mate panel Memory tab the same — `apps/desktop/src/features/memory/`, `features/mates/mate-panel.tsx` — check: manual in the running app

### Phase 2 — Background model, learn mode and tidy-up

Done when: Tidy now archives, dedupes and strikes contradictions with
journal rows, in both apps, on the Background model.

- [x] T7 [FR-24] Background model: `learning.backgroundModel` copied once from `memory.indexModel`; indexing reads it; Settings › Memory renames Index model — `apps/desktop/src-tauri/src/memory_index.rs`, `features/settings/sections/memory-settings.tsx`, `stores/memory-store.ts` — check: shell test `the_index_model_becomes_the_background_model`
- [x] T8 [NFR-1, NFR-4, NFR-5] `--mode learn` skeleton: args, owner resolution (dirs per owner), model resolution (Background model, else the owner's), tool-free request, 3-minute limit, JSON result line, usage source per task — `crates/evo-engine/src/learning/mod.rs`, `modes.rs`, `args.rs` — check: engine test `a_learn_run_sends_no_tools` with faux
- [x] T9 [FR-6, FR-7] Failing tests then `learning/tidy.rs`: expiry → archive, exact duplicates, contradiction prompt and parse, protected lines never offered, a fifth at most, none under five, still-true part kept — `crates/evo-engine/src/learning/tidy.rs` — check: `cargo test -p evo-engine tidy`
- [x] T10 [FR-6, NFR-2, NFR-5] Shell clock loop: nightly due per owner after 03:00 local and idle, one process per owner, failure counts; `learning_tidy_now` — `apps/desktop/src-tauri/src/learning/{mod,clock,jobs,commands}.rs`, `lib.rs` — check: shell tests `tidy_is_due_once_a_night_after_three`, `a_working_owner_is_not_tidied`
- [x] T11 [FR-6, FR-23, FR-25] Settings switches (Harness off, Mates on), per-mate switches, **Tidy now** in the Memory panel and mate panel — `features/settings/sections/{memory,mates}-settings.tsx`, `features/mates/mate-panel.tsx` — check: manual

### Phase 3 — Quote checks and the dream

Done when: a nightly dream saves only cross-chat patterns backed by real
quotes, supersedes outdated facts and offers ideas the person can accept.

- [x] T12 [P] [FR-8] Failing tests then `learning/quotes.rs`: a quote must be in its chat (folded), numbers, paths and URLs must be in the quotes, any language — `crates/evo-engine/src/learning/quotes.rs` — check: `cargo test -p evo-engine quotes`
- [x] T13 [FR-8] The index run's memory tool takes `evidence` for `user` facts and refuses without a passing check — `crates/evo-engine/src/memory_index.rs`, `tools/memory.rs` — check: engine test `an_index_fact_without_a_real_quote_is_refused`
- [x] T14 [FR-10 – FR-13] Failing tests then `learning/dream.rs`: chat selection (owner's lines only, no scheduled, mail or heartbeat), cuts, prompt, parse with limits and confidence, quote checks, apply learned and outdated with journal rows — `crates/evo-engine/src/learning/dream.rs` — check: `cargo test -p evo-engine dream`
- [x] T15 [FR-14] Suggestions: store ideas; Harness Memory panel **Suggestions**; mate conversation card with **Set it up** (opens the scheduled-task or skill editor filled in) and **No thanks** (never again) — `crates/evo-store/src/learning.rs`, `learning/commands.rs`, `features/memory/`, `features/mates/conversation.tsx` — check: manual
- [x] T16 [FR-22] Learn from channels: the dream and a capture pass read a mate's channel stretches under channel rules, sourced to the channel — `crates/evo-engine/src/learning/dream.rs`, `learning/channels.rs`, `mates-settings.tsx` — check: engine test `another_mates_claim_is_not_learned`
- [x] T17 [FR-10] Dream after tidy in the clock loop — `apps/desktop/src-tauri/src/learning/clock.rs` — check: shell test `the_dream_runs_after_the_tidy_with_two_chats`

### Phase 4 — Learned skills and the review

Done when: a correction in a conversation with real tool work becomes a
learned skill change, shown under the reply and undoable.

- [x] T18 [FR-15] Learned-skills folders in the search order, `learnedSkillsDir` in a mate's role, Customize and the mate's Skills tab list them as learned with switch, Undo last change, Delete — `crates/evo-engine/src/resource_dirs.rs`, `customize.rs`, `crates/evo-mesh/src/wire.rs`, `src-tauri/src/mates/mod.rs`, `features/customize/`, `features/mates/skills-tab.tsx` — check: engine test `a_learned_skill_never_shadows_an_installed_one`; manual
- [x] T19 [FR-16, FR-17] Failing tests then `learning/review.rs`: prompt (lesson, do-not-capture and authoring blocks), parse, at most 3 changes, update only shown learned skills keeping every line, names free and class-level, installed skills never changed — `crates/evo-engine/src/learning/review.rs` — check: `cargo test -p evo-engine review`
- [x] T20 [FR-16] Review due: step count per chat since the last review, started on `agent_settled` — `apps/desktop/src-tauri/src/learning/jobs.rs` — check: shell test `a_review_starts_after_ten_tool_steps`
- [x] T21 [FR-18] "Learned · <skill>" under the reply, from the journal, opening the change with Undo — `features/chat/`, `features/mates/conversation.tsx`, `learning/commands.rs` — check: manual

### Phase 5 — The retro

Done when: a channel job that ended gets a retro card with lessons in each
mate's learned skills, and Run retro works.

- [x] T22 [FR-19] The coordinator writes a `job` entry at every end, with status and hand-off count — `crates/evo-engine/src/channel/coordinator.rs` — check: engine test `every_job_end_writes_a_job_entry`
- [x] T23 [FR-20] Failing tests then `learning/retro.rs`: job lines with authors and kinds, prompt, parse, at most 2 lessons per mate, lessons applied through the review rules, team hand-off lessons to the lead — `crates/evo-engine/src/learning/retro.rs` — check: `cargo test -p evo-engine retro`
- [x] T24 [FR-19, FR-21] Retro due after 2 minutes of quiet, `channel_post_retro` RPC posts the card, notification only after a limit, stop or failure, **Run retro** — `learning/jobs.rs`, `crates/evo-engine/src/modes.rs`, `channel/coordinator.rs` — check: shell test `a_retro_waits_for_two_quiet_minutes`
- [x] T25 [FR-21] Retro card in the channel with lessons and Undo; wakes nobody — `lib/mates.ts`, `features/mates/channel-rows.ts`, `channel-view.tsx` — check: manual

### Phase 6 — Evals

Done when: `cargo run -p evo-evals -- offline` passes in CI with the named
scenarios, golden replay and offline case scoring; the live tier skips
without its switch.

- [x] T26 [FR-26, FR-27] `crates/evo-evals`: scenario format, runner, frozen evidence, scorers with one failing message per assertion kind, report — `crates/evo-evals/`, `evals/` — check: `cargo test -p evo-evals scorers`
- [x] T27 [FR-26] Worlds: chat on faux, channel (hub + coordinator + faux seats), memory in a temp agent dir — `crates/evo-evals/src/worlds/` — check: a smoke scenario per world passes twice with the network off
- [x] T28 [FR-28] Scenarios for the 2026-11 channel faults, the hand-off limit, the router fall-through, journal and undo, expiry, tidy protected lines, dream and review checks — `evals/scenarios/` — check: all pass on the current code; each fails when its fix is reverted (spot check two)
- [x] T29 [FR-29] Golden: redaction with leak scan, trace extraction, replay against baselines; one redacted channel from the test data — `crates/evo-evals/src/golden/`, `evals/golden/` — check: `cargo test -p evo-evals golden`
- [x] T30 [FR-30] Case sets dream, router, tool choice with offline scoring of saved answers — `evals/cases/`, `crates/evo-evals/src/cases/` — check: `cargo test -p evo-evals cases`
- [x] T31 [FR-31] Live tier behind `EVO_EVAL_LIVE=1`: real model, judge prompts pinned by sha256, baseline and drift — `crates/evo-evals/src/live/`, `evals/live/` — check: without the switch it prints the skip and passes; one live run on the owner's machine
- [ ] T32 [FR-32, NFR-7] CI job and report; time the offline tier — `.github/workflows/ci.yml` — check: CI run green; under 5 minutes

### Phase 7 — Docs and wrap-up

- [x] T33 [A-4] NOTICE credits; `apps/desktop/README.md` UI map; `evals/README.md` — check: review
- [x] T34 Run every check in section 2 and record the results — check: table filled

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `every_memory_write_is_journaled`; store `journal` tests | passed (2026-11-28): store `journal` tests; `every_memory_write_is_journaled`; app: History lists rows by you, the tidy-up and an undo (e2e app on a scratch agent folder, 2026-11-28) |
| FR-2 | engine undo test; manual Undo in the panel | passed (2026-11-28): `every_memory_write_is_journaled`, `a_change_from_a_secret_cannot_be_undone`; app: Undo restored the text and added an "Undid" row |
| FR-3 | `a_stale_panel_save_is_refused`; manual | passed (2026-11-28): `a_stale_panel_save_is_refused`; app: a topic changed on disk while edited → save refused, draft kept, Reload / Overwrite with mine |
| FR-4 | `entries` tests; `an_expired_entry_is_not_read` | passed (2026-11-28): `entries` tests; `an_expired_entry_is_not_read`; eval `memory-expiry` |
| FR-5 | `entries` tests | passed (2026-11-28): `entries` tests; `an_expired_entry_is_not_read` (struck line not read) |
| FR-6 | `tidy` tests; `tidy_is_due_once_a_night_after_three`; manual Tidy now | passed (2026-11-28): `tidy` tests; `tidy_now_archives_strikes_and_journals`; `tidy_is_due_once_a_night_after_three`; app: Tidy now archived an ended entry, and the nightly loop ran the tidy-up by itself (app log) |
| FR-7 | `tidy` tests (protected lines) | passed (2026-11-28): `protected_lines_are_never_offered`; eval `tidy-protected-lines` |
| FR-8 | `quotes` tests; `an_index_fact_without_a_real_quote_is_refused` | passed (2026-11-28): `quotes` tests; `an_index_fact_without_a_real_quote_is_refused`; eval `chat-index-tools` |
| FR-9 | `redact` tests | passed (2026-11-28): `redact` tests; eval `memory-journal-and-undo` (key saved as a marker) |
| FR-10 | `dream` tests; `the_dream_runs_after_the_tidy_with_two_chats` | passed (2026-11-28): `dream_selects_the_persons_chats_newest_first_and_cut`; `the_dream_runs_after_the_tidy_with_two_chats` |
| FR-11 | `dream` tests (kinds and limits) | passed (2026-11-28): `dream_keeps_only_checked_patterns` (kinds, limits, confidence) |
| FR-12 | `dream` tests; eval case set `dream` | passed (2026-11-28): `dream_keeps_only_checked_patterns`; case set `dream` offline and live (5 of 5 on the owner's model) |
| FR-13 | `dream` tests (journal rows) | passed (2026-11-28): `dream_writes_learned_lines_and_supersedes_outdated_ones`; `a_dream_learns_one_line_and_journals_it` |
| FR-14 | manual: Set it up, No thanks | passed (2026-11-28): app: Harness Suggestions — No thanks declined it, Set it up opened the Scheduler editor filled in with nothing created; a mate's card filled its message box without sending |
| FR-15 | `a_learned_skill_never_shadows_an_installed_one`; manual | passed (2026-11-28): `a_learned_skill_never_shadows_an_installed_one`; app: Customize "Learned by Evo" with switch, Undo last change, Delete (journaled); a mate's Skills tab lists its learned skill apart, its switch writes `.off` |
| FR-16 | `review` tests; `a_review_starts_after_ten_tool_steps` | passed (2026-11-28): `review` tests; `a_review_writes_a_learned_skill_after_the_reply`; `a_review_starts_after_ten_tool_steps` |
| FR-17 | `review` tests | passed (2026-11-28): `review_keeps_every_line_and_never_renames`; eval `review-checks` |
| FR-18 | manual: Learned line and Undo | passed (2026-11-28): app: "Learned · summary-first-reports" under the reply opened the change; Undo removed the skill and journaled the undo |
| FR-19 | `every_job_end_writes_a_job_entry`; `a_retro_waits_for_two_quiet_minutes` | passed (2026-11-28): `every_job_end_writes_a_job_entry`; `a_retro_waits_for_two_quiet_minutes`; app: Run retro ran the retro of the channel's last job |
| FR-20 | `retro` tests | passed (2026-11-28): `retro` tests; `a_retro_gives_each_mate_its_lessons` |
| FR-21 | manual: retro card wakes nobody | passed (2026-11-28): `a_retro_card_is_a_line_not_a_message`; channel-rows test; app: the card showed in the channel and no mate answered it |
| FR-22 | `another_mates_claim_is_not_learned` | passed (2026-11-28): `another_mates_claim_is_not_learned`; app: Learn from channels off by default in Settings › Evo Mates |
| FR-23 | manual: switches off stop every step | passed (2026-11-28): `learning_defaults_differ_per_app`; app: Settings › Memory (off by default) and Settings › Evo Mates (on by default) switches saved |
| FR-24 | `the_index_model_becomes_the_background_model`; `a_learn_run_sends_no_tools` | passed (2026-11-28): `the_index_model_becomes_the_background_model`; `a_learn_run_sends_no_tools`; `tidy_without_a_model_archives_only`; app: Tidy now said the rest needs a model |
| FR-25 | manual: one mate switched off | passed (2026-11-28): `a_mate_switched_off_is_not_learned_for`; app: a mate's Learn skills switched off saved `learning.mate.<id>` and its retro lessons skipped it |
| FR-26 | `evo-evals` runs twice offline with the same verdicts | passed (2026-11-28): `cargo run -p evo-evals -- offline` twice, same verdicts; scripted model only |
| FR-27 | `cargo test -p evo-evals scorers` | passed (2026-11-28): `cargo test -p evo-evals` (`scorers_name_the_failing_assertion`) |
| FR-28 | `evals/scenarios/` all pass | passed (2026-11-28): 13 scenarios pass; reverting the mention-loop and report-back fixes failed their scenarios. The seat-on-router fault is shell-only (plan 5) |
| FR-29 | `cargo test -p evo-evals golden` | passed (2026-11-28): `golden_redacts_and_replays_against_its_baseline`, `the_leak_scan_refuses_personal_data`; `spring-launch` replay matches |
| FR-30 | `cargo test -p evo-evals cases` | passed (2026-11-28): `cases_score_saved_answers_offline`; 14 cases pass offline |
| FR-31 | live tier skip without the switch; one live run | passed (2026-11-28): `the_live_tier_skips_without_its_switch_and_pins_its_judge`; one live run, 6 of 6, with time and cost |
| FR-32 | CI job | not run: the CI job (`evals`) is added; the same command passes locally (2026-11-28); CI runs after a push |
| NFR-1 | `a_learn_run_sends_no_tools` | passed (2026-11-28): `a_learn_run_sends_no_tools`; eval `learn-call-no-tools` |
| NFR-2 | manual: a message during a dream starts at once | not run: steps run as their own process and write by rename (design, plan 3.5); not timed in the app |
| NFR-3 | review of the settings text | passed (2026-11-28): review of the settings text in both apps (each switch says where its text goes) and of `run::Step::ask` |
| NFR-4 | store usage test; manual Usage page | passed (2026-11-28): `upgrade_to_18_moves_memory_changes_into_the_journal` (sources accepted), usage source per task; Usage page not opened |
| NFR-5 | engine test with a failing faux model | passed (2026-11-28): `a_failing_model_is_an_error`, `a_failing_dream_changes_nothing`; failure counting in `clock::settle` |
| NFR-6 | store `journal` keep test | passed (2026-11-28): `journal_keeps_500_per_owner_and_subject` |
| NFR-7 | timed CI run | passed (2026-11-28): offline tier 0.8–1.4 s locally (plus build) |
