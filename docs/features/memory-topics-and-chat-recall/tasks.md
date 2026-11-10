# Memory topics and chat recall — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-10 |

## 1. Phases

Checks: `cargo test -p <crate> <name>` from the repository root; shell tests
with `bun run test:shell` in `apps/desktop`; UI with `bun run typecheck` and
a manual check in the running app.

### Phase 1 — Store

Done when: `cargo test -p evo-store` passes with the new schema, the chat
flags, ranked search and the shared helpers.

- [x] T1 [FR-6, FR-19, FR-27, FR-29, FR-30] Write the failing upgrade test: a database at migration 10 with chats opens with `memory_off`, `incognito`, `scheduled` at 0 and the tables `chat_index` and `memory_changes` — `crates/evo-store/src/lib.rs` — check: `cargo test -p evo-store upgrade_to_11` fails for the missing migration
- [x] T2 [FR-6, FR-19, FR-30] Add `0011_memory_topics.sql`, register it, add the three fields to `CHAT_COLUMNS`, `chat_row`, `Chat`, `NewChat` (`#[serde(default)]`); a fork copies `memory_off`; declare the new modules; fix the `NewChat` literal in the usage test — `crates/evo-store/src/migrations/0011_memory_topics.sql`, `src/lib.rs`, `src/chats.rs`, `src/usage.rs` — check: T1 passes
- [x] T3 [FR-34] Test, then `0012_usage_memory_source.sql` (rebuild `usage_events` with `'memory'`) and `SOURCES` — `crates/evo-store/src/migrations/0012_usage_memory_source.sql`, `src/lib.rs`, `src/usage.rs` — check: `cargo test -p evo-store memory_source_is_kept` (rows from before survive; a `memory` row is summed under its own source)
- [x] T4 [FR-21, NFR-4] Test, then make `refresh_search` and `search_chats` skip incognito chats — `crates/evo-store/src/chats.rs` — check: `cargo test -p evo-store incognito_chats_are_never_searched`
- [x] T5 [FR-11, FR-12, FR-13, FR-32] Write the failing tests for `find_chats`: words apart still match, the FR-11 ranking example, a summary match above a transcript match, list by time with `after`, `before` and order, one project only (the home group too), the asking chat left out — `crates/evo-store/src/chats.rs` — check: the tests fail for the missing function
- [x] T6 [FR-11, FR-12, FR-13, FR-27, FR-32] Implement `find_chats` and `chat_index` (summary, mark, time, failure) — `crates/evo-store/src/chats.rs`, `src/chat_index.rs` — check: T5 passes; `cargo test -p evo-store chat_index`
- [x] T7 [NFR-2] Speed test: a search over 1,000 chats with 20 kB of text each — `crates/evo-store/tests/search_speed.rs` — check: `cargo test -p evo-store --release search_speed -- --ignored` under 2 s
- [x] T8 [FR-13, FR-27] Move reading a saved transcript as messages into the store, with entry ids and "messages after a mark"; move its tests — `crates/evo-store/src/transcripts.rs` — check: `cargo test -p evo-store transcript_messages`
- [x] T9 [FR-29] `memory_log`: record, list newest first by scope and project, mark undone, clear, keep the newest 200 — `crates/evo-store/src/memory_log.rs` — check: `cargo test -p evo-store memory_log`
- [x] T10 [FR-22, NFR-4] `logs::delete_chat` and `chats::incognito_ids` (with nested chats) — `crates/evo-store/src/logs.rs`, `src/chats.rs` — check: `cargo test -p evo-store incognito_ids`, `delete_chat_logs`
- [x] T11 [FR-26, FR-30, FR-33] `chats::index_candidates` with tests: idle time, `indexSince`, updated after last indexed, never incognito, memory-off or nested, `scheduled` reported — `crates/evo-store/src/chats.rs` — check: `cargo test -p evo-store index_candidates`

### Phase 2 — Engine: topics and switches

Done when: a chat's agent updates a topic in place, the switches change the
prompt and the tool, and an engine started with `--no-memory` has neither.

- [x] T12 [FR-3] Write the failing tests for a topic update: replace text that occurs once, add at the end, absent text fails, text that occurs twice fails, the file is replaced by rename — `crates/evo-engine/src/memory.rs` — check: `cargo test -p evo-engine topic_update` fails for the missing function
- [x] T13 [FR-1, FR-2, FR-3] Implement the update, frontmatter `by` and `sensitive`, writes through a rename; older files read as before — `crates/evo-engine/src/memory.rs` — check: T12 passes; `save_list_rename_and_delete_keep_the_index` still passes
- [x] T14 [FR-3, FR-5, FR-17] `memory` tool: `update`, `old_text`, `new_text`, `sensitive`; result `diff`; refusal when sensitive is off; `update` is an edit — `crates/evo-engine/src/tools/memory.rs`, `src/approval.rs` — check: `cargo test -p evo-engine memory_tool`, `classification_table`
- [x] T15 [FR-4, FR-17, FR-18] Prompt text: update a covering topic, short topics, remember / change / forget at once, the sensitive rule by setting, the never-save list; setting `memorySensitive` — `crates/evo-engine/src/memory.rs`, `src/settings.rs` — check: `cargo test -p evo-engine memory_prompt_follows_settings`
- [x] T16 [FR-7] **Use personal memory** per folder (`memoryPersonalOff`): no personal index in the prompt, the tool refuses the personal scope, `describe` reports it — `crates/evo-engine/src/memory.rs`, `src/tools/memory.rs`, `src/settings.rs` — check: `cargo test -p evo-engine personal_memory_switch`
- [x] T17 [FR-9] Reset: delete every topic and index in every scope and the change list; report the count — `crates/evo-engine/src/memory.rs` — check: `cargo test -p evo-engine reset_leaves_no_topic`
- [x] T18 [FR-6, FR-20, FR-23] `--no-memory` and `--incognito`: parsed; no Memory section at build and at reload; `memory` not in the tool list; usage without a chat id for incognito — `crates/evo-engine/src/args.rs`, `src/session.rs`, `src/host.rs` — check: `cargo test -p evo-engine memory_flags_tests` (prompt and tool list, before and after a reload)
- [x] T19 [FR-7, FR-9, FR-10, FR-17, FR-29] RPC `set_memory_settings`, `reset_memory`, `list_memory_changes`, `undo_memory_change`; new fields from `list_memories` — `crates/evo-engine/src/modes.rs`, `src/rpc/protocol.rs` — check: `cargo test -p evo-engine customization_and_mesh_commands_pass_payload_validation` (payload shapes), `undo_indexing_change`; the handlers by hand from the panel
- [x] T20 [FR-10] Test that tools set with `configure_external_tools` are still offered after `reload_resources`; fix it if they are not — `crates/evo-engine/src/session.rs` (or `apps/desktop/src/stores/engine-store.ts` if the app must send them again) — check: `cargo test -p evo-engine external_tools_survive_reload`

### Phase 3 — Shell: flags, incognito, past-chat tools

Done when: the shell starts engines with a chat's flags, an incognito chat
leaves nothing behind, and the past-chat tools rank, list and stay in one
project.

- [x] T21 [FR-6, FR-19, FR-20] `engine_start` options; flags from the chat row, else from the options; kept with the engine process; passed as `--no-memory` / `--incognito` — `apps/desktop/src-tauri/src/engine.rs` — check: `bun run test:shell` `engine_args_follow_chat_flags`
- [x] T22 [NFR-4] No stderr lines, error strings or event payloads logged for an incognito chat's engine — `apps/desktop/src-tauri/src/engine.rs` — check: `bun run test:shell` `incognito_engines_log_no_text`
- [x] T23 [FR-22] Ending an incognito chat (row, nested chats, transcripts, document versions, log entries): on **End chat**, at exit after engines stopped, at start before the sweep — `apps/desktop/src-tauri/src/db.rs`, `src/lib.rs` — check: `bun run test:shell` `incognito_chats_are_removed`
- [x] T24 [FR-22] A nested chat gets its parent's flags — `apps/desktop/src-tauri/src/mesh.rs` — check: `bun run test:shell` `nested_chats_inherit_flags`
- [x] T25 [FR-11, FR-12, FR-13, FR-14, FR-16, FR-21, FR-32] Tests, then `session_search` over `find_chats` (words, ranking, no query lists by time, summaries, structured results, no `workspaceOnly`) and `session_read` refusing another project's or an incognito chat; use the store's transcript reader — `apps/desktop/src-tauri/src/agent_tools/app/sessions.rs` — check: `bun run test:shell` `sessions`
- [x] T26 [FR-6, FR-15, FR-20] Hide the two tools when **Search past chats** is off or the chat is memory-off or incognito; changing the setting re-offers tools to running chats — `apps/desktop/src-tauri/src/agent_tools/mod.rs`, `src/db.rs` — check: `bun run test:shell` `past_chat_tools_follow_settings`

### Phase 4 — Desktop UI: switches, marks, rows, settings

Done when: the person can start a chat without memory or incognito, sees the
marks and rows, and manages memory in **Settings → Memory**.

State (2026-11-10): done. Checked in the running app on a copy of the data
(a debug build with its own identifier and data folder); see the
verification table.

- [x] T27 [FR-6, FR-19, FR-22] Flags on a chat and a draft; changing one restarts the draft's engine with options; the first prompt saves them; fixed title and no preview for incognito; scheduled chats marked; nested chats inherit; ending an incognito chat — `apps/desktop/src/stores/session-store.ts`, `src/stores/engine-store.ts`, `src/lib/db.ts`, `src/engine/client.ts`, `src/engine/protocol.ts` — check: `bun run typecheck`; manual: a chat started with memory off still has none after restarting evo
- [x] T28 [P] [FR-6, FR-19] **Memory** and **Incognito chat** in the composer's menu, locked after the first message; "memory off" mark — `apps/desktop/src/features/chat/chat-composer.tsx` — check: manual in the running app
- [x] T29 [P] [FR-19, FR-22] Incognito mark in the sidebar and chat header; menu with **End chat** only — `apps/desktop/src/features/shell/sidebar-chats.tsx`, `chat-menu.tsx`, `workspace-header.tsx` — check: manual in the running app
- [x] T30 [P] [FR-5, FR-14, FR-17] Rows: "Updated memory · name" with lines added and removed, the sensitive mark, "Searched past chats" with chats that open, "Read past chat · title" — `apps/desktop/src/components/ai-elements/tool.tsx`, `tool-kinds.ts` — check: manual in the running app
- [x] T31 [P] [FR-1, FR-7, FR-8] Memory panel: who changed a topic last, **Use personal memory**, paused and "off for this chat" states — `apps/desktop/src/features/memory/memory-panel.tsx` — check: manual in the running app
- [x] T32 [FR-8, FR-9, FR-10, FR-15, FR-17] **Settings → Memory**: Pause memory, Search past chats, Include sensitive subjects, Reset memory (confirm with the count), Manage; the store for app-side memory settings; a shared number field and model picker — `apps/desktop/src/features/settings/sections/memory-settings.tsx`, `settings-registry.ts`, `settings-controls.tsx`, `sections/provider-settings.tsx`, `sections/sandbox-settings.tsx`, `apps/desktop/src/stores/memory-store.ts`, `ui-store.ts` — check: manual: each setting applies to the next message of an idle chat

### Phase 5 — Automatic indexing

Done when: with indexing on, an idle chat's new messages update topics and
its summary, every change is listed and undoable, and summaries are found
by search.

State (2026-11-10): done. An index run was made in the running app with a
real model on a copy of the data; see the verification table.

- [x] T33 [FR-27, FR-31] Write the failing tests for the index input: only entries after the mark, no tool results, tool calls by name, the size limit and the mark per call — `crates/evo-engine/src/memory_index.rs` — check: `cargo test -p evo-engine index_input` fails for the missing module
- [x] T34 [FR-25, FR-27, FR-28, FR-30, FR-31, FR-34] The index run: `--mode index`, `--index-after`, `--index-summary-only`; model resolved from the flags; the loop with `memory` and `set_summary` only; changes by `index`; usage source `memory`; the result line; `skipped` when paused or nothing is new — `crates/evo-engine/src/memory_index.rs`, `src/args.rs`, `src/host.rs`, `src/usage_log.rs` — check: T33 passes; `cargo test -p evo-engine index_run` (a scripted `faux` model saves a topic and sets a summary; the tool list is exactly two; a missing model gives `model_unavailable`)
- [x] T35 [FR-29] Record a change made by indexing with the topic file before and after; undo while the file still equals "after", refuse otherwise — `crates/evo-engine/src/memory.rs` — check: `cargo test -p evo-engine undo_indexing_change`
- [x] T36 [FR-24, FR-25, FR-26, FR-28, FR-30, FR-33, NFR-5] The index worker: tick, candidates, which chats are in a run, one process at a time with a time limit, storing the result, `memory://changed` and `memory://index-status`, stopping on `model_unavailable`, retry after an hour, the commands — `apps/desktop/src-tauri/src/memory_index.rs`, `src/lib.rs`, `src/engine.rs` — check: `bun run test:shell` `memory_index` (eligibility, result parsing, back-off)
- [x] T37 [P] [FR-10, FR-24, FR-25, FR-26, FR-33] **Automatic indexing** in **Settings → Memory**: the switch (needs a model), Index model, Index after idle, Index now, Index earlier chats (with the count), the status line — `apps/desktop/src/features/settings/sections/memory-settings.tsx`, `apps/desktop/src/stores/memory-store.ts` — check: manual in the running app
- [x] T38 [P] [FR-29] Memory panel: indexing changes with their chat and undo; reload idle chats on `memory://changed` — `apps/desktop/src/features/memory/memory-panel.tsx`, `apps/desktop/src/stores/engine-store.ts` — check: manual: undo returns a topic to its earlier text
- [x] T39 [FR-34] "Memory indexing" as a usage source — `apps/desktop/src/lib/db.ts`, `apps/desktop/src/features/usage/usage-page.tsx` — check: manual: an index run shows under its own source

### Phase 6 — Documents and verification

Done when: the documents describe what was built and every requirement has
a recorded result.

- [x] T40 [FR-24 – FR-34] The ADD describes the index process, the worker and the new tables — `docs/architecture/README.md` — check: sections 4, 5.1 and 7 match the code
- [x] T41 [FR-3 – FR-8, FR-11 – FR-16] History lines in the specs this feature extends — `docs/features/memory-and-scheduler/spec.md`, `docs/features/agent-browser/spec.md` — check: each names the requirement changed and links here
- [x] T42 [FR-10, FR-19] UI map: Settings → Memory, the composer's menu, incognito chats — `apps/desktop/README.md` — check: review
- [ ] T43 Run every check in the verification table and record the results — this file — check: no row says `not run` (open: FR-8, FR-16, FR-18, NFR-5 and the parts marked not run)

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Manual: a memory saved before this feature is listed with its text unchanged; `cargo test -p evo-engine save_list_rename_and_delete_keep_the_index` | automated passed (2026-11-10): `older_memory_files_read_as_topics`, `save_list_rename_and_delete_keep_the_index`; manual not run |
| FR-2 | `cargo test -p evo-engine memory_tool` (the schema offers exactly the four types) | passed (2026-11-10): `memory_tool_update_changes_one_part_and_reports_the_lines` checks the four types |
| FR-3 | `cargo test -p evo-engine topic_update`, `memory_tool`, `classification_table` | passed (2026-11-10): `topic_update_*`, `memory_tool_*`, `classification_table`; in the app (2026-11-10): the agent changed an existing topic with `update` |
| FR-4 | `cargo test -p evo-engine memory_prompt_follows_settings`; manual: five rules about one subject in five chats leave one topic | automated passed (2026-11-10): `memory_prompt_follows_settings`; in the app (2026-11-10): two rules told in two turns ended in one topic; five rules in five chats not run |
| FR-5 | Manual: the row shows the lines added and removed | passed in the app (2026-11-10): "Updated memory · indentation-style" opens to the added line, also after a restart |
| FR-6 | `cargo test -p evo-engine memory_flags_tests`; `bun run test:shell engine_args_follow_chat_flags`; manual: restart evo, the chat still has no memory | automated passed (2026-11-10): `memory_flags_tests`, `engine_args_follow_chat_flags`; in the app (2026-11-10): a draft switched to memory off had no Memory section, no `memory` tool and no past-chat tools, and got them back when switched on; a saved memory-off chat after a restart not run |
| FR-7 | `cargo test -p evo-engine personal_memory_switch`, `memory_tool_follows_the_personal_switch_and_the_pause` | passed (2026-11-10): `personal_memory_switch_drops_personal_topics`, `memory_tool_follows_the_personal_switch_and_the_pause`; in the app (2026-11-10): the switch in the panel's request round-trips through the engine's settings |
| FR-8 | Manual: with memory paused the panel lists and edits topics | not run |
| FR-9 | `cargo test -p evo-engine reset_leaves_no_topic`; manual: the confirm states the count | automated passed (2026-11-10): `reset_leaves_no_topic`; passed in the app (2026-11-10): the confirm said "1 topic", then no topic, file or change was left |
| FR-10 | Manual: each setting in **Settings → Memory** applies to the next message of an idle chat; `cargo test -p evo-engine external_tools_survive_reload` | automated passed (2026-11-10): `external_tools_survive_reload`; passed in the app (2026-11-10): every control of **Settings → Memory** used; Search past chats changed a running chat's tools at once |
| FR-11 | `cargo test -p evo-store find_chats` (ranking example) | passed (2026-11-10): `find_chats_matches_words_and_ranks`, `search_matches_words_and_ranks`; in the app (2026-11-10): `session_search` for two words found the chat that holds them apart |
| FR-12 | `cargo test -p evo-store find_chats` (by time) | passed (2026-11-10): `find_chats_lists_by_time`, `search_without_a_query_lists_chats_by_time` |
| FR-13 | `cargo test -p evo-store find_chats` (one project); `bun run test:shell sessions` (read refused) | passed (2026-11-10): `find_chats_stays_in_one_project`, `reading_stays_in_the_askers_project_and_skips_incognito_chats`, `search_skips_the_asking_chat_other_projects_and_incognito_chats`; in the app (2026-11-10): with 59 chats of other projects in the database, a search returned only the project's chat |
| FR-14 | Manual: selecting a chat in "Searched past chats" opens it | passed in the app (2026-11-10): "Searched past chats · indentation tabs" listed the chat; selecting it opened it |
| FR-15 | `bun run test:shell past_chat_tools_follow_settings` | automated passed (2026-11-10): `past_chat_tools_follow_settings`; passed in the app (2026-11-10): with the setting off a running chat was offered neither tool, with it on both |
| FR-16 | Manual: approval mode "ask", a search shows no approval; `bun run test:shell sessions` (untrusted marks) | automated passed (2026-11-10): the `sessions` tests (untrusted marks; both tools are class Read); manual not run |
| FR-17 | `cargo test -p evo-engine memory_tool` (refusal), `memory_prompt_follows_settings`; manual: a health condition is not saved with the setting off | automated passed (2026-11-10): `memory_tool_update_refuses_sensitive_subjects_until_allowed`, `memory_prompt_follows_settings`; manual not run |
| FR-18 | Manual: asked to remember a password, the agent declines and says why | not run (the prompt text is checked by `memory_prompt_follows_settings`, passed 2026-11-10) |
| FR-19 | Manual: an incognito chat is marked in the chat and the sidebar; the choice is locked after the first message | passed in the app (2026-11-10): marks in the header, the composer and the sidebar; fixed title; the choice was locked after the first message |
| FR-20 | `cargo test -p evo-engine memory_flags_tests`; `bun run test:shell past_chat_tools_follow_settings` | passed (2026-11-10): `memory_flags_tests`, `past_chat_tools_follow_settings`; in the app (2026-11-10): an incognito draft had no Memory section, `memory` tool or past-chat tools |
| FR-21 | `cargo test -p evo-store incognito_chats_are_never_searched` | passed (2026-11-10): `incognito_chats_are_never_searched`, `search_skips_the_asking_chat_other_projects_and_incognito_chats`; in the app (2026-11-10): an open incognito chat was not found by chat search |
| FR-22 | `bun run test:shell incognito_chats_are_removed`, `nested_chats_inherit_flags`; manual: kill evo with one open, start it, none is left | automated passed (2026-11-10): `incognito_chats_are_removed`, `nested_chats_inherit_flags`, `incognito_ids_include_nested_chats`; passed in the app (2026-11-10): End chat, a normal quit and a kill followed by a start each left no row, no nested chat and no transcript |
| FR-23 | Manual: after an incognito chat, Usage totals include it and no row opens it | in the app (2026-11-10): the incognito chat's response is in `usage_events` with no chat id; the Usage page's row not looked at |
| FR-24 | Manual: the switch asks for a model first; the note about the provider and tokens shows | passed in the app (2026-11-10): the switch stayed off without a model; the note about the provider and tokens shows |
| FR-25 | `cargo test -p evo-engine index_run` (`model_unavailable`); manual: the setting shows the reason | automated passed (2026-11-10): `index_run_stops_at_a_model_error_and_keeps_the_parts_read`, `index_run_result_lines_and_the_chosen_model`, `memory_index_results_are_read_from_the_last_result_line`; manual not run |
| FR-26 | `cargo test -p evo-store index_candidates`; manual: an idle chat is indexed once after the set time | automated passed (2026-11-10): `index_candidates_are_idle_chats_with_new_messages`, `memory_index_passes_follow_the_settings`; passed in the app (2026-11-10): with one minute set, an idle chat was indexed once, 70 s after its last message |
| FR-27 | `cargo test -p evo-engine index_input`, `index_run` | passed (2026-11-10): `index_input_*`, `index_run_saves_a_topic_and_the_summary_with_two_tools`; in the app (2026-11-10): an index run with a real model added a topic from a preference stated without asking, and wrote the chat's summary |
| FR-28 | `cargo test -p evo-engine index_run` (paused); `personal_memory_switch` | automated passed (2026-11-10): `personal_memory_switch_drops_personal_topics`; a paused memory in an index run not run |
| FR-29 | `cargo test -p evo-store memory_log`; `cargo test -p evo-engine undo_indexing_change`; manual: undo in the panel | automated passed (2026-11-10): `memory_log_*`, `undo_indexing_change_restores_the_topic`, `memory_tool_tells_the_recorder_what_changed`; passed in the app (2026-11-10): the change was listed with its chat; Undo removed the topic and marked the change undone |
| FR-30 | `cargo test -p evo-store index_candidates`; `cargo test -p evo-engine index_run` (summary only) | passed (2026-11-10): `index_candidates_are_idle_chats_with_new_messages`, `index_run_writes_the_summary_only_when_memory_is_not_offered`, `memory_index_arguments_name_the_chat_the_model_and_the_mark` |
| FR-31 | `cargo test -p evo-engine index_input` (no tool results), `index_run` (two tools) | passed (2026-11-10): `index_input_is_the_new_words_of_the_person_and_the_assistant`, `index_run_saves_a_topic_and_the_summary_with_two_tools` |
| FR-32 | `cargo test -p evo-store find_chats` (summary match) | passed (2026-11-10): `find_chats_ranks_a_summary_match_above_a_transcript_match`, `search_matches_and_shows_summaries` |
| FR-33 | `cargo test -p evo-store index_candidates` (`indexSince`); manual: **Index earlier chats** shows the count first | automated passed (2026-11-10): `index_candidates_are_idle_chats_with_new_messages`, `memory_index_passes_follow_the_settings`; passed in the app (2026-11-10): 50 earlier chats were counted in the confirm and none was read without it |
| FR-34 | `cargo test -p evo-store memory_source_is_kept`; manual: the Usage page shows memory indexing | automated passed (2026-11-10): `memory_source_is_kept`; passed in the app (2026-11-10): three index responses show under "Memory indexing" on the Usage page |
| NFR-1 | Review: `find_chats` and the tools make no network request; with indexing off no index process starts | passed (2026-11-10): review of `find_chats` and `sessions.rs` (SQL and local files only); `memory_index_passes_follow_the_settings` (no pass with indexing off) |
| NFR-2 | `cargo test -p evo-store --release search_speed -- --ignored` | passed (2026-11-10): 116 ms over 1,000 chats, release build |
| NFR-3 | `cargo test -p evo-engine save_list_rename_and_delete_keep_the_index`; manual: an older memory folder lists every memory | automated passed (2026-11-10): `older_memory_files_read_as_topics`; manual not run |
| NFR-4 | `bun run test:shell incognito_engines_log_no_text`; manual: after an incognito chat with a distinctive phrase ends, search evo's data folder for it | automated passed (2026-11-10): `incognito_engines_log_no_text`, `incognito_chats_are_never_searched`; passed in the app (2026-11-10): after the incognito chat ended, its phrase was in no file under the data folder |
| NFR-5 | Manual: send a message to a chat while it is indexed; the answer starts at once | not run |
