# Engines run without a window: the shell hosts them — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-11 |

## 1. Phases

Phases run in order. Each moves one responsibility from the webview to the
shell and deletes the TypeScript version in the same phase, so the two never
both act (NFR-1).

Test commands:
- Shell tests: `bun run test:shell <filter>` in `apps/desktop`.
- Engine tests: `cargo test -p evo-engine <name>`.
- Webview tests: `bun test <path>`.

"A run in the app" means the debug app driven over WebView2 DevTools (see
`AGENTS.md`). Supervisor tests use a fake engine binary
(`src-tauri/tests/fixtures`), selected with `EVO_ENGINE_BIN`.

### Phase 1 — Supervisor: ensure and set up

Done when: windows start engines only through `engine_ensure`, and an engine
started by the shell has the same tools and model as one a window set up.

- [x] T1 [FR-1] Tests: `ensure` starts once per chat, picks the folder (worktree, workspace, home) and flags; set-up on `ready` sends `set_model` from the row, then `configure_external_tools` (session tools for top-level chats only), then calls `on_hosted`; a refused set-up during a run retries on `agent_settled` — `apps/desktop/src-tauri/src/supervisor.rs` (tests module), `src-tauri/tests/fixtures/` — check: `bun run test:shell supervisor::` fails for the missing supervisor
- [x] T2 [FR-1] Shell-side RPC to an engine with `shell-` ids and a timeout — `apps/desktop/src-tauri/src/supervisor/rpc.rs` — check: `bun run test:shell supervisor::rpc`
- [x] T3 [FR-1] `EngineSupervisor` with `ensure` and set-up; `engine_start` becomes private `start_process`; `engine_ensure` command; registered in `lib.rs` — `apps/desktop/src-tauri/src/supervisor.rs`, `engine.rs`, `lib.rs` — check: T1 passes
- [x] T4 [FR-1] [NFR-1] `launch()` calls `engine_ensure`; `startOnDefault`'s `set_model`, `configureSessionTools` and `onReadyNested`'s set-up deleted from the store — `apps/desktop/src/stores/engine-store.ts`, `src/engine/client.ts` — check: `bun run typecheck`; a run in the app: a new chat has browser, terminal and connector tools

### Phase 2 — Hosting for the mesh

Done when: a nested session starts, and mail reaches a stopped chat, with
every window closed.

- [x] T5 [FR-4] Tests: `Effect::Host` calls `ensure` with the effect's folder; pending chats are hosted at setup (archived excluded); at most 3 restarts in 120 s for pending work — `apps/desktop/src-tauri/src/supervisor.rs`, `mesh.rs` (tests) — check: fails first
- [x] T6 [FR-4] `mesh::apply` hosts through the supervisor; `supervisor::setup` hosts `mesh_pending_chats`; restart limiter — `apps/desktop/src-tauri/src/mesh.rs`, `supervisor.rs` — check: T5 passes
- [x] T7 [FR-4] [NFR-1] Delete `onHost`, `mayRestartForMail`, the `mesh://host` listener and the start-up `mesh_pending_chats` call — `apps/desktop/src/stores/engine-store.ts` — check: `bun run typecheck`; a run in the app with the main window hidden: a started session runs and its note reaches the parent

### Phase 3 — Viewers, work and idle trim

Done when: closing a chat window leaves its run going, and idle engines are
trimmed app-wide.

- [x] T8 [FR-2] [FR-3] Tests: viewers per chat; work = run, running agents (from the `shell` mesh subscription), pending mail, open question; trim keeps the newest 4 with no work and no viewer; a chat with a running background agent is never stopped — `apps/desktop/src-tauri/src/supervisor.rs` (tests) — check: fails first
- [x] T9 [FR-2] [FR-3] `engine_attach` / `engine_detach`, `engine://state`, the `shell` subscription at level `state`, trim after `agent_settled` and detach — `apps/desktop/src-tauri/src/supervisor.rs`, `engine.rs` — check: T8 passes
- [x] T10 [FR-2] Chat window `Destroyed` detaches instead of stopping the engine — `apps/desktop/src-tauri/src/lib.rs` — check: a run in the app: close a chat window mid-turn; the turn finishes
- [x] T11 [FR-2] [NFR-1] Store attaches on activate and window open, detaches on release; `trimIdle` and `MAX_IDLE_ENGINES` deleted — `apps/desktop/src/stores/engine-store.ts` — check: `bun run typecheck`; `bun test src/stores`

### Phase 4 — Open questions and notifications

Done when: a question in a chat no window shows is notified, and appears
when the chat is opened.

- [x] T12 [FR-9] Tests: `ui_dialog_request` adds, `ui_dialog_closed` and a written `ui_dialog_response` remove; a shell ask registers and clears; `engine_attach` returns every open question — `apps/desktop/src-tauri/src/supervisor/questions.rs` (tests) — check: fails first
- [x] T13 [FR-9] Registry; `ask_person` registers; `engine://question` / `engine://question-closed` — `apps/desktop/src-tauri/src/supervisor/questions.rs`, `agent_tools/mod.rs` — check: T12 passes
- [x] T14 [FR-9] [FR-10] Notices from the shell when no focused window shows the chat: OS notification with an open-chat action (`app://open-chat`, `show_main`), bell entry to `notifications.inbox` or `notify://entry`, mute respected, "A chat" for incognito — `apps/desktop/src-tauri/src/supervisor/turns.rs`, `agent_tools/user.rs` — check: `bun run test:shell supervisor::notices`
- [x] T15 [FR-9] [FR-10] Webview: draw questions from `engine_attach` and the events (replacing `carryPermissions`' window-only memory); drain `notifications_take`; open-chat target; the store's own run notices deleted — `apps/desktop/src/stores/engine-store.ts`, `src/stores/notification-store.ts`, `src/features/notifications/notify.ts`, `src/features/chat/transcript.ts` — check: `bun test src/features/chat`; a run in the app: approval in a hidden chat, notification, open, answer

### Phase 5 — Wakes

Done when: an idle parent with no window is woken once for its children's
turns, within the guard rails.

- [x] T16 [FR-5] Tests: 2 s gather; no wake with a draft, an open question or archived; at most 5 in a row, reset by a person's prompt; the three wake texts pinned — `apps/desktop/src-tauri/src/supervisor/wakes.rs` (tests) — check: fails first
- [x] T17 [FR-5] Wakes in the shell, sent as `Deliver::Prompt` mail; `engine_set_composing` — `apps/desktop/src-tauri/src/supervisor/wakes.rs`, `supervisor.rs`, `mesh.rs` — check: T16 passes
- [x] T18 [FR-5] [NFR-1] Store reports composing; `scheduleWake`, `checkWake`, `startAutomatedRun` and their constants deleted — `apps/desktop/src/stores/engine-store.ts`, `src/features/chat/chat-composer.tsx` — check: `bun run typecheck`; a run in the app: draft holds the wake, clearing it wakes once

### Phase 6 — Turn records

Done when: turns no window watched are recorded with the cost a window would
have shown.

- [x] T19 [FR-6] Engine test: `agent_settled` carries `turn` totals for a run with tools and a foreground subagent; a background child settled later adds to the next turn; no `turn` on compaction; `ready` has `turnTotals` — `crates/evo-engine/src/modes.rs` (tests) — check: `cargo test -p evo-engine turn_totals` fails first
- [x] T20 [FR-6] Engine: compute and emit `turn`; capability flag — `crates/evo-engine/src/modes.rs`, `crates/evo-engine/src/rpc/protocol.rs` — check: T19 passes
- [x] T21 [FR-6] Shell test: one record per run id, unread from viewers and focus, incognito without preview, no record and one log line without the capability — `apps/desktop/src-tauri/src/supervisor/turns.rs` (tests) — check: fails first
- [x] T22 [FR-6] Shell records turns; `chat://updated` after each — `apps/desktop/src-tauri/src/supervisor/turns.rs` — check: T21 passes
- [x] T23 [FR-6] [NFR-1] Store: `recordFinishedTurn`'s record and `countAgent`/`pendingUsage` accounting removed (the transcript keeps showing `extra-usage`); `turn` typed — `apps/desktop/src/stores/engine-store.ts`, `src/engine/protocol.ts` — check: `bun run typecheck`
- [x] T24 [FR-6] Parity check: the same faux run's cost and tokens from the TS reducer and from `turn` match — `apps/desktop/src/features/chat/transcript.test.ts`, `crates/evo-engine/src/modes.rs` fixture — check: `bun test src/features/chat/transcript.test.ts`

### Phase 7 — Settings reach every engine

Done when: approval mode, providers, resources and sandbox changes reach
engines no window holds, and window-stored settings live in the app database.

- [x] T25 [FR-8] [NFR-4] Tests: approval mode to every ready engine; `providers_changed` from one engine reloads the others (busy ones on settle); sandbox restarts engines with no work; `settings_import_window` writes once and never over a stored value — `apps/desktop/src-tauri/src/supervisor/settings.rs` (tests) — check: fails first
- [x] T26 [FR-8] Fan-out from `settings_set`, the reader task, `plugins.rs` and `memory_index.rs`; `mesh.autoStartSessions` and `connectors.chatOff` read by the hub and `agent_mcp_chat_config` — `apps/desktop/src-tauri/src/supervisor/settings.rs`, `db.rs`, `mesh.rs`, `agent_tools/mcp.rs`, `plugins.rs`, `memory_index.rs` — check: T25 passes
- [x] T27 [FR-8] [NFR-4] [NFR-1] Store: `broadcastApprovalMode`, `broadcastProviders`, `restartIdleEngines` and the resource reload listeners deleted; `ui-store` reads and writes the two settings through the shell and imports stored values once — `apps/desktop/src/stores/engine-store.ts`, `src/stores/ui-store.ts`, `src/features/settings/sections/sandbox-settings.tsx` — check: `bun run typecheck`; a run in the app with an old profile: settings kept

### Phase 8 — Scheduler in the shell

Done when: a task due with no window starts within 20 s and is recorded, and
the page and the agent's tools never overwrite each other.

- [x] T28 [FR-7] Tests: missed runs once at setup (also hidden); due tasks start on the tick; a busy last run is skipped; the first prompt is hub mail (kept across a reload); run record, `once` disabled, `nextRunAt`; concurrent page and tool edits both kept — `apps/desktop/src-tauri/src/agent_tools/scheduler/run.rs` (tests) — check: fails first
- [x] T29 [FR-7] Runner and `scheduler_save` / `scheduler_delete` / `scheduler_run_now`, one mutex for every write; tools in `mod.rs` use it — `apps/desktop/src-tauri/src/agent_tools/scheduler/run.rs`, `mod.rs`, `lib.rs` — check: T28 passes
- [x] T30 [FR-7] [NFR-1] Store: `startScheduler`, its timer, `save` and `run` replaced by shell commands; `startTaskChat` and `pendingPrompts` deleted — `apps/desktop/src/stores/scheduler-store.ts`, `src/app/use-app-bootstrap.ts`, `src/stores/engine-store.ts` — check: `bun run typecheck`; a run in the app: task due with the window closed starts and is recorded

### Phase 9 — Lifecycle and tray

Done when: closing every window keeps work going with the tray on, quitting
asks when chats work, and the main window comes back after being destroyed.

- [x] T31 [FR-11] [FR-12] Tests: `engine_working` lists working and waiting chats; quit with work asks, without work quits; tray lines from the list — `apps/desktop/src-tauri/src/system.rs` (tests) — check: fails first
- [x] T32 [FR-11] [FR-12] `ExitRequested` handling, quit question routing, `show_main` rebuilds `main`, tray lines and open-chat on click — `apps/desktop/src-tauri/src/system.rs`, `lib.rs` — check: T31 passes
- [x] T33 [FR-11] Quit dialog and `app://open-chat` handling — `apps/desktop/src/features/shell/quit-dialog.tsx`, `src/app/app.tsx` — check: `bun run typecheck`; a run in the app: quit from the tray with a working chat asks once
- [x] T34 [NFR-5] Sidebar shows chats working with no window as working (from `engine://state` and `engine_working`) — `apps/desktop/src/stores/session-store.ts`, `src/features/shell/sidebar-chats.tsx` — check: a run in the app

### Phase 10 — End-to-end and documents

Done when: the success criteria are shown in the app, and the ADD, ADRs and
amended specs describe what was built.

- [x] T35 [FR-1] [FR-4] [FR-5] [FR-6] [FR-7] End-to-end run with every window closed: a nested-session run, a background agent and a scheduled task finish; on reopening, turns, costs and unread are right — check: a run in the app (SC-1)
- [x] T36 [NFR-1] [NFR-3] Three windows open: one wake and one record per turn; reload a window during a scheduled start: the prompt is sent once — check: a run in the app (SC-2)
- [x] T37 [P] [NFR-2] Review: the shell sends only set-up commands, fixed wake texts and stored scheduled prompts; nothing answers a question — check: code review recorded in plan 5
- [x] T38 [P] Remove TypeScript left unused by phases 1–9 and update `apps/desktop/README.md` (UI map: quit dialog, tray lines, engines without windows) — `apps/desktop/src/`, `apps/desktop/README.md` — check: `bun run build`
- [x] T39 [P] ADD §4 (engines per chat with work or a viewer; the supervisor), §6 (hosting sequence), §9 (questions never auto-answered), §12 (risk: engines working unseen); ADR 0018 `Accepted` and its index row — `docs/architecture/README.md`, `docs/architecture/decisions/0018-the-shell-hosts-engines-windows-attach.md`, `docs/architecture/decisions/README.md` — check: `git diff --check`; links resolve
- [x] T40 [P] Amend specs ("a window" → "the shell"): mesh FR-21 – FR-24; subagents FR-19; Scheduler FR-12 – FR-16 and NFR-2; approval-modes (open questions with no window); each with a History line linking this feature — `docs/features/session-mesh-protocol/spec.md`, `docs/features/subagents-and-nested-sessions/spec.md`, `docs/features/memory-and-scheduler/spec.md`, `docs/features/approval-modes/spec.md` — check: review
- [x] T41 Status `Implemented`, catalogue row, verification table filled — `docs/features/shell-hosted-engines/spec.md`, `tasks.md`, `docs/features/README.md` — check: the review checklist in `docs/README.md`

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `bun run test:shell supervisor::` (T1); a run in the app: a session started with no window has the shell's tools | passed (2026-11-12): `supervisor::` 62/62; run in the app: a new chat and two nested sessions started by the shell had the default model and their session, browser and terminal tools |
| FR-2 | `bun run test:shell supervisor::` (T8); a run in the app: closing a chat window mid-turn (T10) | passed (2026-11-12): `supervisor::` (viewers); run in the app: a chat window closed mid-reply, the engine went on and the main window took it over |
| FR-3 | `bun run test:shell supervisor::` trim tests (T8) | passed (2026-11-12): `supervisor::` trim tests (keeps 4; a chat with a background agent is never stopped) |
| FR-4 | `bun run test:shell supervisor:: mesh::` (T5); a run in the app with the main window hidden (T7) | passed (2026-11-12): `supervisor::`, `mesh::` 6/6; run in the app: mail waiting at quit delivered at the next start before the window loaded; a crashed receiver restarted |
| FR-5 | `bun run test:shell supervisor::wakes` (T16); a run in the app (T18) | passed (2026-11-12): `supervisor::wakes` 7/7; run in the app: an idle parent with no window woken once per child turn (6 turns: 3 prompts, 3 wakes) |
| FR-6 | `cargo test -p evo-engine turn_totals` (T19); `bun run test:shell supervisor::turns` (T21); parity (T24) | passed (2026-11-12): `cargo test -p evo-engine turn_totals` 3/3, `supervisor::turns` 4/4, parity `transcript.test.ts` 1/1; run in the app: three child turns recorded with cost and unread while no window watched |
| FR-7 | `bun run test:shell scheduler::run` (T28); a run in the app (T30) | passed (2026-11-12): `scheduler::run` 9/9 plus the clock tests; run in the app: a missed run at start and a run due 65 s later both started, answered and recorded (after the seconds/milliseconds clock fix) |
| FR-8 | `bun run test:shell supervisor::settings` (T25); a run in the app (T27) | passed (2026-11-12): `supervisor::settings` 8/8 (approval mode, providers, resources, sandbox to every engine). Not run in the app for an engine no window holds |
| FR-9 | `bun run test:shell supervisor::questions` (T12); a run in the app (T15) | passed (2026-11-12): `supervisor::questions` 6/6; run in the app: an approval in a session no window showed was kept, put in the bell and shown on opening; Allow finished the run |
| FR-10 | `bun run test:shell supervisor::notices` (T14) | passed (2026-11-12): `supervisor::notices` 5/5; run in the app: "Approval needed · Writer" bell entry and `shell notice` log lines |
| FR-11 | `bun run test:shell system::` (T31); a run in the app (T33) | passed (2026-11-12): `system::` 7/7; run in the app: tray off, closing the last window mid-run asked once (Cancel kept it, Quit anyway exited with no engine left); a destroyed main window rebuilt by a second launch. The tray's own Quit was not clicked (CDP cannot reach the tray) |
| FR-12 | `bun run test:shell system::` (T31) | passed (2026-11-12): `system::` tray-line tests. The tray menu itself was not seen in the app |
| NFR-1 | A run in the app with three windows (T36) | passed (2026-11-12): run in the app with three windows open: each turn recorded once (turn counts equal the runs in the transcripts: 3 and 2), one wake per child turn |
| NFR-2 | Code review (T37) | passed (2026-11-12): review (T37): the shell sends set-up commands, `set_approval_mode`, `set_limits`, reloads, and prompts only as hub mail (fixed wake texts, stored task prompts); it only reads `prompt` and `ui_dialog_response`; nothing answers a question |
| NFR-3 | A run in the app: reload during a scheduled start (T36) | passed (2026-11-12): run in the app: the main window reloaded ten times across a scheduled start; the task ran once and its prompt was sent once |
| NFR-4 | `bun run test:shell supervisor::settings` import test (T25); a run with an old profile (T27) | passed (2026-11-12): `supervisor::settings` import test; runs in the app on copies of a real profile (migrated database, tasks kept). The local-storage import was not exercised in the app |
| NFR-5 | A run in the app (T34) | passed (2026-11-12): run in the app: a rebuilt main window's sidebar store listed a chat working in another window as working |
