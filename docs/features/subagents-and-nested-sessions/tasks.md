# Subagents and nested sessions — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-11 |

## 1. Phases

Phases 1 and 3 are independent and were built in parallel; phase 2 builds on
phase 1, phase 4 on phases 1 and 3. Engine tests below run with
`cargo test -p evo-engine <name>` (faux provider); `evo-store` and
`evo-worktrees` tests with `cargo test -p <crate> <name>`.

### Phase 1 — Foreground subagents

Done when: a model call runs an `explore` child, the UI shows its steps live
and its report, the cost includes the child, and tests with the faux
provider pass.

- [x] T1 [FR-1] [FR-2] [NFR-3] Agent type loader beside `load_skills`, public `parse_frontmatter`, built-in `general` / `explore` / `plan` with their prompts and report formats — `crates/evo-engine/src/agents.rs`, `crates/evo-harness/src/frontmatter.rs`, `crates/evo-harness/src/skills.rs` — check: `files_define_types_and_project_wins`, `access_thinking_and_turns_are_read_and_checked`, `explore_and_plan_are_read_only_and_general_is_not`, `a_file_can_replace_a_builtin`, `an_agent_file_added_later_is_usable_at_once`
- [x] T2 [FR-3] Read-only guard (`ChildToolGuard`) before the approval prompt and before the call runs; shell command allow-lists — `crates/evo-engine/src/read_only.rs`, `crates/evo-harness/src/agent_harness/child.rs` — check: `bash_reading_commands_pass`, `bash_writing_commands_are_refused`, `powershell_reading_commands_pass`, `powershell_writing_commands_are_refused`, `bash_commands_that_hide_a_write_are_refused`, `powershell_commands_that_hide_a_write_are_refused`, `guard_passes_reading_tools_and_refuses_the_rest`, `explore_agents_cannot_write`, `read_only_agents_cannot_write_through_the_shell_or_their_tools`, `read_only_agents_cannot_write_with_commands_the_guard_misread`
- [x] T3 [FR-4] [FR-7] [FR-8] Built-in `agent` tool built in `build_tools` with the late-bound harness handle (also on `/reload`); per-type setup; foreground run; limits; report cut at 16 KiB — `crates/evo-engine/src/tools/agent.rs`, `crates/evo-engine/src/session.rs` — check: `a_subagent_works_and_reports_back`, `unknown_types_and_models_are_refused`, `long_reports_are_cut_with_a_way_to_the_rest`
- [x] T4 [FR-5] [NFR-1] Cumulative `details.child` progress snapshots, throttled, 50 rows / 4 KB — `crates/evo-engine/src/tools/agent.rs` — check: `a_subagent_is_a_mesh_node_with_live_summary_events`; manual check in the running app
- [x] T5 [FR-6] Child usage in `AgentToolResult.usage`; the desktop adds tool-result usage to the turn's cost — `crates/evo-engine/src/tools/agent.rs`, `apps/desktop/src/features/chat/transcript.ts` — check: manual check in the running app
- [x] T6 [FR-5] Desktop `agent` tool kind: nested trace, latest text, footer, report — `apps/desktop/src/components/ai-elements/agent-run.tsx`, `apps/desktop/src/components/ai-elements/tool.tsx`, `apps/desktop/src/features/chat/chat-turn.tsx` — check: `bun run typecheck` and a preview with sample data
- [x] T7 [NFR-2] Plugin child agents keep their action API and share limits with the `agent` tool — `crates/evo-engine/src/child_agents.rs` — check: `child_limits_reject_wrong_types_and_out_of_range_values`, `capacity_is_bounded_even_while_disposed_children_are_cleaning_up`, and the other `child_agents` tests

### Phase 2 — Background subagents

Done when: a background child finishes while the parent is idle and the
parent continues with its report.

- [x] T8 [FR-9] `background: true`: session-lifetime scope, immediate `{agentId, status}`, report to a running or idle parent, cancellation — `crates/evo-engine/src/tools/agent.rs`, `crates/evo-engine/src/modes.rs`, `crates/evo-engine/src/rpc/protocol.rs` — check: `a_background_agent_reports_to_the_idle_parent_on_its_next_run`, `a_background_agent_can_be_cancelled`
- [x] T9 [FR-10] `message_agent` resumes a kept child — `crates/evo-engine/src/tools/agent.rs` — check: `a_finished_agent_continues_with_its_context`
- [x] T10 [FR-9] Desktop wake-ups, "Background agents" chip and event lines (first by polling `get_background_agents`, now by mesh push events) — `apps/desktop/src/stores/engine-store.ts`, `apps/desktop/src/features/chat/chat-turn.tsx` — check: `bun run typecheck` and a preview with sample data
- [x] T11 [FR-12] Lost background agents reported once on engine start; `ready.lostBackgroundAgents`; the desktop settles unreported calls — `crates/evo-engine/src/tools/agent.rs`, `apps/desktop/src/features/chat/transcript.ts` — check: `background_agents_without_a_report_are_found`, `lost_background_agents_are_reported_once_to_the_next_run`
- [x] T12 [FR-11] A steer releases waiting foreground calls (`tools::agent::steered`, `ATTACHED → FINISHED | DETACHED`) — `crates/evo-engine/src/tools/agent.rs` — check: `steering_releases_a_waiting_foreground_agent_into_the_background`, `without_steering_a_foreground_agent_reports_to_its_call`

### Phase 3 — Nested sessions

Done when: a chat proposes a session, the person starts it, it appears
nested, and its finished turn reaches the parent.

- [x] T13 [FR-14] Migration `0004_chat_parents.sql`; `Chat` / `NewChat` gain `parent_chat_id` and `origin` — `crates/evo-store/src/migrations/0004_chat_parents.sql`, `crates/evo-store/src/chats.rs` — check: `nested_sessions_link_to_their_parent`
- [x] T14 [FR-15] [FR-17] [FR-18] `start_session` / `message_session` / `list_sessions` and `startChildChat` (now defined and answered by the mesh hub) — `apps/desktop/src-tauri/src/mesh.rs`, `apps/desktop/src/stores/engine-store.ts`, `apps/desktop/src/stores/session-store.ts` — check: manual check in the running app
- [x] T15 [FR-16] Proposal card and **Start sessions without asking** — `apps/desktop/src/components/ai-elements/session-proposal.tsx`, `apps/desktop/src/features/settings/sections/sandbox-settings.tsx` — check: `bun run typecheck` and a preview with sample data
- [x] T16 [FR-19] Notes to the parent: read at the next step when busy, coalesced automated wake when idle, guard rails — `apps/desktop/src-tauri/src/mesh.rs`, `apps/desktop/src/stores/engine-store.ts` — check: manual check in the running app
- [x] T17 [FR-20] Sidebar nesting: children under the parent, footer, status dots, roll-up, header and menu — `apps/desktop/src/features/shell/sidebar-chats.tsx`, `apps/desktop/src/features/shell/app-sidebar.tsx` — check: `bun run typecheck` and a preview with sample data

### Phase 4 — Worktree review

Done when: a kept worktree can be reviewed and applied or discarded from the
UI.

- [x] T18 [FR-13] [FR-22] [NFR-4] Worktree manager in its own crate, shared by engine and shell — `crates/evo-worktrees/src/lib.rs`, `crates/evo-engine/src/worktrees.rs` — check: `cargo test -p evo-worktrees` (`changes_are_measured_from_the_fork_and_applied_to_the_working_folder`, `a_conflicting_apply_changes_nothing`, `only_managed_worktrees_are_removed`, `both_locations_are_listed_and_removable`, `outside_a_repository_is_an_error`, …)
- [x] T19 [FR-21] Migration `0005_chat_worktrees.sql` and `start_session`'s `worktree` option — `crates/evo-store/src/migrations/0005_chat_worktrees.sql`, `crates/evo-store/src/chats.rs`, `apps/desktop/src-tauri/src/mesh.rs` — check: manual check in the running app
- [x] T20 [FR-13] [FR-22] Shell worktree commands and the review dialog, reached from a subagent's kept worktree, a worktree session's toolbar chip and **Settings → Sandbox** — `apps/desktop/src-tauri/src/worktrees.rs`, `apps/desktop/src/components/ai-elements/worktree-review.tsx` — check: `bun run typecheck` and a preview with sample data

### Phase 5 — Follow-ups

Done when: the designed settings that are not built yet exist.

- ~~T21~~ built by [session roles](../session-roles-and-mail/tasks.md) T16 – T17 (`agentDirs`, FR-11).
- ~~T22~~ built by [session roles](../session-roles-and-mail/tasks.md) T13 – T15 (settable limits, FR-5; nesting stays one level).
- [ ] T29 [FR-15] `start_session` argument `workspace` (its `model` part was built by [session roles](../session-roles-and-mail/tasks.md) T5 – T12) — `crates/evo-mesh/src/hub.rs`, `apps/desktop/src-tauri/src/mesh.rs`, `apps/desktop/src/stores/engine-store.ts` — check: a hub test and a run in the app
- ~~T30~~ dropped 2026-11-11: no operating-system write restriction (ADR 0020).
- [ ] T31 [FR-20] Open the parent (or a started session) in the main window from a separate chat window — `apps/desktop/src-tauri/src/windows.rs`, `apps/desktop/src/stores/engine-store.ts` — check: a run in the app

### Phase 6 — Audit fixes (2026-11-11)

Done when: every defect the audit of 2026-11-11 found (code review, probes
and runs in the app) is fixed with a test where one can be written.

- [x] T23 [FR-3] Read-only guard reads a command no more generously than the shell — `crates/evo-engine/src/read_only.rs` — check: `bash_commands_that_hide_a_write_are_refused`, `powershell_commands_that_hide_a_write_are_refused`, `read_only_agents_cannot_write_with_commands_the_guard_misread`, `commands_the_checker_misread_are_commands`
- [x] T24 [FR-13] [FR-22] [NFR-4] Apply into the folder the worktree was cut from, from the recorded fork commit, as bytes, all or nothing, only what was reviewed; tolerant removal; leaks closed; a plugin child reports its kept worktree — `crates/evo-worktrees/src/lib.rs`, `crates/evo-worktrees/src/snapshot.rs`, `crates/evo-engine/src/child_agents.rs`, `apps/desktop/src-tauri/src/worktrees.rs`, `apps/desktop/src/components/ai-elements/worktree-review.tsx`, `apps/desktop/src/lib/worktrees.ts` — check: `cargo test -p evo-worktrees` (`apply_lands_in_the_checkout_the_worktree_was_cut_from`, `the_fork_point_is_the_one_recorded_at_creation`, `apply_keeps_text_that_is_not_utf8`, `a_failure_part_way_puts_the_workspace_back`, `a_locked_file_does_not_leave_the_workspace_half_changed`, `a_folder_in_use_is_left_and_cleaned_up_later`, `apply_refuses_changes_that_were_not_reviewed`, …), `a_cancelled_child_still_reports_its_kept_worktree`, `bun test src/lib/worktrees.test.ts`
- [x] T25 [FR-4] [FR-5] [FR-6] [FR-7] [FR-8] [FR-10] The `agent` tool: id in a foreground result and id forms for `message_agent`; a stopped child keeps its view, cost and last words; foreground calls wait for a place; per-run start time; note tags made inert; setup failures clean their worktree — `crates/evo-engine/src/tools/agent.rs`, `crates/evo-engine/src/agents.rs` — check: `cargo test -p evo-engine --lib tools::agent` (`a_foreground_result_names_the_agent_and_message_agent_finds_it`, `a_stopped_foreground_agent_keeps_its_view_and_usage`, `an_agent_that_runs_out_of_turns_hands_over_what_it_said`, `six_foreground_agents_in_one_message_all_run_four_at_a_time`, `a_foreground_call_waits_for_a_slot_and_a_background_one_is_refused`, `a_report_cannot_pass_for_another_agents_note`, …)
- [x] T26 [FR-15] [FR-16] [FR-17] [FR-18] [FR-19] [FR-21] The hub: the limit counts starting children; proposals recorded, started once, dismissed with a note; cut notes say so; a refused prompt leaves the chat idle; a child follows its parent's model; a failed start removes its worktree — `crates/evo-mesh/src/hub.rs`, `apps/desktop/src-tauri/src/mesh.rs`, `crates/evo-store/src/chats.rs` — check: `cargo test -p evo-mesh` (`sessions_just_started_count_against_the_limit_and_list_as_starting`, `starts_at_the_same_moment_share_the_limit`, `a_proposal_starts_once_and_the_parent_is_told`, `a_cut_report_says_so_and_how_to_read_the_rest`, `a_prompt_refused_for_another_reason_leaves_the_chat_idle`, …), `bun run test:shell mesh::`
- [x] T27 [FR-5] [FR-9] [FR-12] [FR-13] [FR-19] The transcript: a queue of approval requests; background reports, failed children and `message_agent` runs on their calls, live and reopened; agents settled when the engine exits; automated messages recognised by their whole structure; the badge's actions — `apps/desktop/src/features/chat/transcript.ts`, `chat-turn.tsx`, `summary-card.tsx`, `approval-panel.tsx`, `apps/desktop/src/components/ai-elements/agent-run.tsx`, `tool.tsx`, `apps/desktop/src/engine/mesh.ts` — check: `bun test src/features/chat`
- [x] T32 [FR-13] [FR-22] A folded turn keeps a kept worktree's badge in sight; the review dialog fits its width; a failed external tool call reads as its text — `apps/desktop/src/components/ai-elements/agent-run.tsx`, `apps/desktop/src/features/chat/chat-turn.tsx`, `apps/desktop/src/components/ai-elements/worktree-review.tsx`, `crates/evo-engine/src/rpc/external_tools.rs` — check: `error_keeps_complete_raw_content_and_structured_result`, runs in the app
- [x] T28 [FR-6] [FR-16] [FR-19] [FR-20] [FR-22] The stores and the shell UI: wakes held for a draft, an open approval or an archived chat; usage counted when an agent ends; engines with working subagents left alone; proposals started once; the sidebar's dots, roll-up and footer; header and menu; a session's engine stopped around Apply; a deleted chat's unchanged worktree removed — `apps/desktop/src/stores/engine-store.ts`, `session-store.ts`, `ui-store.ts`, `apps/desktop/src/features/shell/`, `apps/desktop/src/components/ai-elements/session-proposal.tsx`, `apps/desktop/src/features/chat/chat-composer.tsx`, `apps/desktop/src/features/settings/sections/sandbox-settings.tsx` — check: `bun run typecheck`; a run in the app

## 2. Verification

Results are from 2026-11-11 (`cargo test -p evo-engine --lib`: 761 passed;
the one failure, `external_transport_rejects_invalid_utf8_and_oversized_input`,
fails the same without these changes). The desktop's transcript reducer and worktree
helpers have `bun test` suites; the stores, the sidebar and the dialogs have
no automated tests, so their rows name a run in the app.

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `cargo test -p evo-engine -- files_define_types_and_project_wins access_thinking_and_turns_are_read_and_checked` | passed (2026-11-11) |
| FR-2 | `cargo test -p evo-engine -- explore_and_plan_are_read_only_and_general_is_not a_file_can_replace_a_builtin` | passed (2026-11-11) |
| FR-3 | `cargo test -p evo-engine read_only` and `explore_agents_cannot_write`, `read_only_agents_cannot_write_through_the_shell_or_their_tools` | passed (2026-11-11) |
| FR-4 | `cargo test -p evo-engine -- a_subagent_works_and_reports_back unknown_types_and_models_are_refused` | passed (2026-11-11) |
| FR-5 | Manual check in the running app; `bun run typecheck` | passed (2026-11-11): `bun test src/features/chat`, typecheck; run in the app (2026-11-11): a stopped child kept its 21 steps live and reopened, a background report showed on its call after reopening  |
| FR-6 | Manual check in the running app: the turn's cost includes the child | passed (2026-11-11): `a_stopped_foreground_agent_keeps_its_view_and_usage`; the store's counting: run in the app (2026-11-11): turn costs matched the parent's calls plus each child run (foreground, continued, cancelled, background), none twice  |
| FR-7 | `cargo test -p evo-engine child_limits_reject_wrong_types_and_out_of_range_values`; manual recursion check | passed (2026-11-11): `six_foreground_agents_in_one_message_all_run_four_at_a_time`, `child_limits_reject_wrong_types_and_out_of_range_values` |
| FR-8 | `cargo test -p evo-engine long_reports_are_cut_with_a_way_to_the_rest` | passed (2026-11-11) |
| FR-9 | `cargo test -p evo-engine a_background_agent_reports_to_the_idle_parent_on_its_next_run` | passed (2026-11-11) |
| FR-10 | `cargo test -p evo-engine -- a_finished_agent_continues_with_its_context a_background_agent_can_be_cancelled` | passed (2026-11-11): also `a_foreground_result_names_the_agent_and_message_agent_finds_it` |
| FR-11 | `cargo test -p evo-engine -- steering_releases_a_waiting_foreground_agent_into_the_background without_steering_a_foreground_agent_reports_to_its_call` | passed (2026-11-11) |
| FR-12 | `cargo test -p evo-engine -- background_agents_without_a_report_are_found lost_background_agents_are_reported_once_to_the_next_run` | passed (2026-11-11); the desktop side by `bun test` |
| FR-13 | `cargo test -p evo-worktrees`; manual check of the badge in the running app | passed (2026-11-11): `cargo test -p evo-worktrees`, `a_cancelled_child_still_reports_its_kept_worktree`; run in the app (2026-11-11): Open in…, Show changes, Discard and Apply from the badge, also with the turn folded |
| FR-14 | `cargo test -p evo-store nested_sessions_link_to_their_parent` | passed (2026-11-11) |
| FR-15 | Manual check in the running app: an allowed `start_session` creates and runs a child | passed (2026-11-11): hub tests (`a_fresh_session_follows_its_parents_model_and_flags`); run in the app (2026-11-11): a session started on its parent's model  |
| FR-16 | Manual check: default proposal card; setting on starts at once | passed (2026-11-11): `a_proposal_starts_once_and_the_parent_is_told`, `a_dismissed_proposal_cannot_be_started_and_the_parent_is_told`; the card run in the app (2026-11-11): two Starts gave one session; the parent got the started note  |
| FR-17 | Manual check: `message_session` reaches the child; `list_sessions` lists it | passed (2026-11-11): `sessions_just_started_count_against_the_limit_and_list_as_starting`; run in the app (2026-11-11): `list_sessions` said `starting` right after the starts  |
| FR-18 | Manual check: a child session has no `start_session` | passed (2026-11-11): `starts_at_the_same_moment_share_the_limit`, `the_cards_start_goes_through_the_limit_too` |
| FR-19 | Manual check: an idle parent is woken once with a **Nested sessions update** chip | passed (2026-11-11): `a_cut_report_says_so_and_how_to_read_the_rest`; the wake guards run in the app (2026-11-11): a draft held the wake; clearing it woke the parent after about 2 s  |
| FR-20 | `bun run typecheck` and a preview with sample data | typecheck; seen in the app (2026-11-11): children under their parent with the footer |
| FR-21 | Manual check: `worktree: true` runs the child in a new worktree | passed (2026-11-11): `an_incognito_parent_has_incognito_sessions_and_no_copy`; run in the app (2026-11-11)  |
| FR-22 | `cargo test -p evo-worktrees -- a_conflicting_apply_changes_nothing changes_are_measured_from_the_fork_and_applied_to_the_working_folder`; manual check of the dialog | passed (2026-11-11): `cargo test -p evo-worktrees` (24 tests), `bun test src/lib/worktrees.test.ts`; the dialog run in the app (2026-11-11): Apply from a running session's chip applied, removed the worktree and its branch, and restarted the session in the workspace  |
| NFR-1 | Code review of the throttle and caps in `tools/agent.rs`; `a_subagent_is_a_mesh_node_with_live_summary_events` | passed (2026-11-11): `a_subagent_is_a_mesh_node_with_live_summary_events` |
| NFR-2 | `cargo test -p evo-engine child_agents` | passed (2026-11-11) |
| NFR-3 | `cargo test -p evo-engine an_agent_file_added_later_is_usable_at_once` | passed (2026-11-11) |
| NFR-4 | `cargo test -p evo-worktrees -- only_managed_worktrees_are_removed a_conflicting_apply_changes_nothing` | passed (2026-11-11) |
