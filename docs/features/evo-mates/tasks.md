# Evo Mates: AI teammates in a second app on the same core — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-28 |

## 1. Phases

### Phase 1 — Two apps, one Evo

Done when: from one install, Evo Mates opens as its own window from its own
launcher entry, from the tray and from **Switch to Evo Mates**, in the
running Evo, with its own taskbar entry on Windows; the window shows an
empty roster.

- [x] T1 [FR-2, FR-3] Write tests for the launch target: `--app` parsing, default `harness`, cold start with `mates` builds only that window — `apps/desktop/src-tauri/src/app_identity.rs` (tests module) — check: `bun run test:shell` fails for the right reason
- [x] T2 [FR-2, FR-3, FR-4] Implement `app_identity::open` and route the single-instance callback through it; build and show the `mates` window; keep the app alive while any window or the tray is on — `app_identity.rs`, `lib.rs`, `window.rs`, `system.rs` — check: T1 passes
- [x] T3 [P] [FR-1, FR-3] Add the `mates.html` page, `src/mates/main.tsx` bootstrap with `appKind = "mates"`, and the Vite input; guard main-window-only work as chat windows do — `apps/desktop/mates.html`, `src/mates/`, `vite.config.ts`, `src/app/use-app-bootstrap.ts` — check: `bun run build`; the page loads in the running app
- [x] T4 [P] [FR-3] **Switch to Evo Mates** in Harness and **Switch to Evo Harness** in Mates, calling `app_open` — `src/features/shell/*`, `src/features/mates/mates-header.tsx`, `src/lib/mates.ts` — check: manual, both directions, other window unchanged
- [ ] T5 [FR-5] Tray menu entries **Open Evo Harness** and **Open Evo Mates** — `system.rs` — check: manual on Windows
- [ ] T6 [FR-1, NFR-4] Windows: AppUserModelID per window, and the Evo Mates Start menu shortcut with the same ID and `--app mates`; Mates icon — `app_identity.rs`, `src-tauri/windows/` NSIS hooks, `icons/` — check: manual, installer build shows two Start entries and two taskbar groups
- [ ] T7 [P] [FR-1, NFR-4] macOS launcher bundle "Evo Mates.app" that opens the main app with `--app mates`; Linux second desktop entry for `.deb` and `.rpm` — `scripts/mates-launcher/`, `tauri.linux.conf.json`, `.github/workflows/desktop-packages.yml` — check: CI package job lists the extra files; manual open on each platform
- [x] T8 [NFR-1] Measure switch and cold-start times — check: median of 10 runs recorded in the verification table

### Phase 2 — Mates and their conversations

Done when: a person picks a role in the gallery, gets a mate with its own
folder, memory, model and approval mode, talks to it in one lasting
conversation, edits its profile, and Harness shows none of it except in
Needs you and Usage.

- [x] T9 [FR-6, FR-13, FR-24, NFR-8] Write migration tests: upgrade from schema 14 keeps every chat as `kind = 'chat'`; mates, channels, members round-trip; Harness chat queries skip other kinds — `crates/evo-store/tests/` — check: `cargo test -p evo-store` fails for the right reason
- [x] T10 [FR-6, FR-13, FR-22, FR-24, NFR-8] Migration `0015_mates.sql` (tables, chat kinds, usage source `mates` by table rebuild) and `evo-store` `mates.rs`; `chats.rs` filters by kind — `crates/evo-store/src/` — check: T9 passes
- [x] T11 [FR-9, FR-10, FR-11] `SessionRole` gains `mate`, `memory_dir`, `channel` with serde defaults; a test that an old role file still parses — `crates/evo-mesh/src/wire.rs` — check: `cargo test -p evo-mesh`
- [x] T12 [FR-11, FR-26] Tests, then code: with `memory_dir`, the `project` scope resolves to it whatever the folder; prompt names it the mate's memory; Harness project topics unreachable — `crates/evo-engine/src/memory.rs`, `session_roles.rs` — check: `cargo test -p evo-engine memory`
- [x] T13 [FR-7, FR-10, FR-13, FR-25] Shell `mates` commands: create (folder under `mates/<id>/work`, direct chat of kind `mate`, role written), update (role rewritten, reconfigure at settle), duplicate, pin, hide, delete (stop sessions, confirm, remove) — `src-tauri/src/mates/mod.rs`, `supervisor.rs` — check: shell tests for each command
- [x] T14 [NFR-5] Per-chat approval mode: mate chats get the mate's mode at `ready`; app tools read the chat's mode — `supervisor.rs`, `supervisor/settings.rs`, `agent_tools/mod.rs` — check: shell test; manual: a mate on "ask" asks while Harness is on "auto"
- [x] T15 [P] [FR-8] Built-in role templates in the team file format, and `mates_templates` — `src-tauri/src/mates/templates/*.md`, `mates/mod.rs` — check: test that every template parses and creates a mate
- [x] T16 [P] [FR-6, FR-7, FR-13] `mates-store` and the roster with states, row menu, gallery and **Start from scratch** — `src/stores/mates-store.ts`, `src/features/mates/roster*.tsx`, `gallery.tsx` — check: `bun run typecheck`; manual
- [ ] T17 [FR-9, FR-10] Direct conversation view on the shared chat components, with the mate's name and avatar on replies, and search in the conversation — `src/features/mates/mate-chat.tsx` — check: manual; NFR-2 measured with a generated 10,000-message transcript
- [ ] T18 [FR-7, FR-10, FR-25, FR-26] Profile: identity, instructions, model and thinking from models.dev, tools, connectors, skills, approval mode, folders, memory list with edit and forget — `src/features/mates/profile*.tsx` — check: manual; forgetting a memory changes the next answer
- [ ] T19 [FR-12, FR-24] Harness: sidebar, history and search skip mate kinds; Needs you lists waiting mates with avatars and opens them in Mates; notices of mate chats go to the Mates window — `src/stores/session-store.ts`, `src/features/shell/*`, `supervisor/notices.rs` — check: manual with a mate waiting and the Mates window closed
- [x] T20 [P] [FR-22] Usage grouped by mate — `crates/evo-store/src/usage.rs`, `src/features/usage/*` — check: `cargo test -p evo-store usage`; manual
- [x] T21 [FR-4] A mate keeps working with its window closed — check: manual: send, close Mates, reopen after the reply
- [x] T54 [FR-58, FR-59, FR-60, FR-61, FR-62] Messaging thread for mates: new `thread.tsx`, `composer.tsx`, `conversation.tsx`, `message-markup.ts` (reply and reactions note in the sent text), migration 0016 and `mates_react` / `mates_reactions` / `mates_take_reactions`; Harness's chat view is no longer used in Evo Mates — `src/features/mates/`, `crates/evo-store` — check: markup and store tests, the app on a copy of real data
- [ ] T41 [FR-39] Tests, then code: a mate's Apps narrow the connector and plugin tools its engines get, with the chosen account — `src-tauri/src/supervisor.rs` — check: shell test with All, Only these and None
- [x] T42 [FR-40] `connect_app` app tool and the Connect / Reconnect card; expired sign-in results point to it — `src-tauri/src/agent_tools/mates.rs`, `src/features/mates/connect-card.tsx` — check: shell test; manual with a missing mail connector
- [x] T43 [FR-41, FR-42] Tool classes from MCP annotations and the catalogue (unknown counts as sending); per-mate Always allow for read-only tools in `mate_grants`; the sending gate — `src-tauri/src/mates/grants.rs`, `agent_tools/mod.rs` — check: shell tests for each class and setting
- [x] T44 [FR-43] Per-mate vault, `request_secret` and the secure card; references resolved only inside the tool call; masking of echoed values — `crates/evo-vault`, `src-tauri/src/agent_tools/mates.rs`, `src/features/mates/secret-card.tsx` — check: test that no value reaches the transcript or a provider request
- [x] T45 [P] [FR-44] Skills switches per mate: the role's `skills` (the names a session may use, `null` = every skill; a nested session keeps its mate's limit), the engine drops the rest (`customize::split_by_role`), and a Skills section in the mate panel's Apps tab — `crates/evo-mesh/src/wire.rs`, `crates/evo-engine/src/customize.rs`, `src/features/mates/skills-tab.tsx`, `src-tauri/src/mates/mod.rs` — check: `cargo test -p evo-engine a_role_keeps_only_the_skills_it_allows`, `skills-tab.test.ts`, manual
- [ ] T55 [FR-44] Skills flagged by Evo's skill check arrive off for every mate, with the reason — needs a skill check, which Evo does not have yet — check: manual with a flagged skill
- [x] T46 [FR-45] Pause / Resume / Pause all through the supervisor hold; paused line — `src-tauri/src/mates/mod.rs`, `supervisor.rs`, `system.rs`, `src/features/mates/pause.tsx` — check: shell test; manual
- [x] T47 [FR-46, FR-47, FR-48, FR-51] Mate conduct block in the role prompt; a fixed test set of 10 ambiguous tasks and 10 task replies — `crates/evo-engine/src/session_roles.rs`, `crates/evo-engine/tests/mates_conduct.rs` — check: test set results recorded
- [x] T48 [FR-49] **Send as next** and "Seen while working" in the Mates composer on the engine's steering and follow-up queues — `src/features/mates/mate-chat.tsx` — check: manual
- [x] T49 [FR-50] Recent-work brief and search limited to the mate's own chats — `crates/evo-engine/src/mates/brief.rs` — check: `cargo test -p evo-engine mates_brief`
- [x] T50 [FR-52] Notice rules for mates and the per-mate switch — `src-tauri/src/supervisor/notices.rs` — check: shell test for each case
- [ ] T35 [P] [FR-33, NFR-7] Mate faces on `open-bot-avatar` (exact version, `bun add`): Evo's own definitions, palette, state/mood/gaze/still mapping, SVG for lists and live mount for header and panel, reduced motion; MIT notice in `NOTICE` — `src/components/mate-face/`, `package.json`, `bun.lock`, `NOTICE` — check: visual review in light and dark; reduced motion stops all loops; `bun run build`
- [ ] T36 [P] [FR-29, FR-32] Sidebar with pinned tiles, roster rows with state words, header pill, mate panel tabs, folding rules — `src/features/mates/{sidebar,roster-row,pinned-tiles,header-pill,mate-panel}.tsx` — check: design review against ux.md 2 at 1440×900 and 900×700
- [ ] T37 [FR-30, FR-31, FR-35] Bubbles with joined runs and tint; plain-word activity chips; Saved to memory chip — `src/features/mates/{bubbles,activity-chips,memory-chip}.tsx` — check: `bun run typecheck`; manual on a turn that reads, searches and saves to memory
- [x] T38 [FR-34] First-turn setup block and the `mate_profile` app tool — `src-tauri/src/mates/mod.rs`, `src-tauri/src/agent_tools/` — check: shell test for the tool; manual: answers land in the profile
- [x] T39 [P] [FR-37, FR-38] Gallery categories and search; keyboard shortcuts — `src/features/mates/{gallery,shortcuts}.tsx` — check: manual
- [x] T40 [NFR-7] DESIGN.md "Evo Mates layer" section; UI string check for banned words — `DESIGN.md` — check: review; `rg -i "session|engine|agent type" src/features/mates` finds no UI string

### Phase 3 — Channels

Done when: a channel of three mates follows each reply rule, mates hand off
in view, the hand-off limit and **Stop** work, and the running limit
queues extra work.

- [x] T22 [FR-15, FR-16, FR-28] Tests for routing: mentions, `mentioned` with none, `lead`, `auto` with a faux router (pick, low confidence, failure, none configured) — `crates/evo-engine/src/channel/tests.rs` — check: `cargo test -p evo-engine channel` fails for the right reason
- [x] T23 [FR-14, FR-15, FR-16] Channel coordinator: run instead of the agent loop for a `channel` role; deliveries with `<channel>` blocks; seats started on first delivery and reused; transcript entries — `crates/evo-engine/src/channel/`, `host.rs` — check: T22 passes
- [x] T24 [FR-17, FR-18, FR-19] Hand-off observation and counting, stop at the limit with a notice; stop one seat or all — `channel/` — check: engine test with two faux mates handing back and forth stops at the limit
- [x] T25 [FR-28] Router default from models.dev (`tool_call`, lowest cost among connected), its picks recorded under usage source `mates` (added by T10) — `crates/evo-engine/src/channel/router.rs` — check: unit test on a fixture catalogue
- [x] T26 [FR-14, FR-25] Shell `channels` commands; seat chats of kind `seat`; channel folder through project trust — `src-tauri/src/mates/mod.rs` — check: shell tests; [FR-23] manual with an untrusted folder
- [x] T27 [NFR-6] Running limit across mate, seat and channel engines with a queue and the **Queued** state — `supervisor.rs` — check: shell test with limit 1
- [x] T51 [FR-15, FR-16, FR-53] Routing with `everyone` and `@everyone`, router inputs, 0.6 thresholds, 1.5 s budget, pick line — `crates/evo-engine/src/channel/` — check: `cargo test -p evo-engine channel`
- [x] T52 [FR-54, FR-55] Hand-off kinds and labels; lead assignments and wrap-up; members cannot assign the lead — `crates/evo-engine/src/channel/` — check: engine tests with faux mates
- [x] T53 [FR-18, FR-56, FR-57] Per-seat queue with steering for the person's single-mention messages, `reply_to`, delivery contents, per-pair hourly limit — `crates/evo-engine/src/channel/` — check: engine tests
- [ ] T28 [P] [FR-14 – FR-19, FR-36] Channel view: messages by author, member avatars and rule chip in the header, `@` member list, route lines, hand-off lines, live seat activity, **Stop**; channel settings dialog — `src/features/mates/channel*.tsx`, `mention-menu.tsx`, `route-line.tsx`, `handoff-line.tsx` — check: manual
- [x] T29 [P] [NFR-6, FR-18, FR-28] Settings › Evo Mates — `src/features/settings/sections/mates-settings.tsx` — check: manual; values apply without restart
- [ ] T30 [NFR-5] Trust check: a mate asked by another mate to run a command still asks the person under "ask" — check: manual, recorded
- [x] T64 [FR-72] A seat asks in its reply: the seat role names its channel (`mate.channel`), its engine drops `ask_user` and gets the channel rules; a reply without a mention goes to the mates who asked — `crates/evo-engine/src/session_roles.rs`, `channel/route.rs`, `channel/coordinator.rs`, `src-tauri/src/mates/channels.rs` — check: `cargo test -p evo-engine channel session_roles`, shell test `a_seat_role_names_its_channel`
- [x] T65 [FR-12, FR-28] Fixes found in the app: a channel's engine keeps the model it picks (the supervisor no longer sets the row's or the default model on it); a chat window opened from **Answer** shows the mate's seat — `src-tauri/src/supervisor.rs`, `src/lib/db.ts`, `src/stores/session-store.ts` — check: shell test `a_channel_keeps_the_model_its_engine_picked`; manual
- [x] T66 [FR-18, FR-28, FR-54] Fixes found in a channel: a mention in a mate's reply asks only from a question (`route::asked_in`); a report back to who asked or assigned is not counted against the message (`coordinator::Owed`); a mail hand-off shows the mate's text without the envelope (`hub::mail_inner`); a seat does not inherit the channel's routing model; a seat started while the channel runs is written into the channel's role, so a restart reuses it (`channels::with_seat`); a refused routing model gives way to the next (`router::router_models`) — `crates/evo-engine/src/channel/`, `crates/evo-mesh/src/hub.rs`, `src-tauri/src/mesh.rs` — check: `cargo test -p evo-engine channel`, `cargo test -p evo-mesh`, shell tests `a_seat_never_runs_on_its_channels_router_model`, `a_new_seat_is_kept_in_the_channels_role`; manual — passed in the app (2026-11-27): a lead assigned three mates by mail (lines read "assigns", no envelope), each answer went back to it as "tells", it summed up, no limit; a mate added to a running channel got a seat on the default model, kept after a restart with its conversation; a mention outside a question woke nobody and the reply without a mention went to "Writer, who asked you"; under Auto the refused cheapest model gave way to the next, which picked at 95%
- [x] T67 [FR-18] The default hand-off limit per message is 12, not 6 (A-4) — `crates/evo-mesh/src/wire.rs`, `src-tauri/src/mates/channels.rs`, `src/features/settings/sections/mates-settings.tsx` — check: `a_channel_allows_twelve_handoffs_by_default`, shell test `the_default_handoff_limit_is_twelve`

### Phase 4 — Team files

Done when: a channel exports to one file with no secret, and that file
imports on another machine after a preview.

- [x] T31 [FR-20, FR-21, FR-27] Tests: export round-trip, no model ID, no secrets or messages, newer version refused, unknown entries listed — `src-tauri/src/mates/team_file.rs` (tests) — check: `bun run test:shell` fails for the right reason
- [x] T32 [FR-20, FR-21, FR-27] `team_file.rs`: parse, validate, preview, write; needs resolved against models.dev — check: T31 passes
- [x] T33 [FR-20, FR-21, FR-44] Export and import UI with the preview, connectors off and skills off (`skills: []`) — `src/features/mates/team-import.tsx`, row menus — check: manual on two agent directories

### Phase 4b — Mates that speak first

Done when: a mate follows up on an event the person mentioned, writes
nothing when nothing is due, keeps to its initiative and Do not disturb,
and its Following up list and check-ins can be edited.

- [x] T56 [FR-63, FR-69, FR-70] Migration 0017 (`mates.initiative`, `mate_agenda`, `mate_initiative`) and store functions with tests — `crates/evo-store/src/migrations/0017_mate_agenda.sql`, `src/mates.rs` — check: `cargo test -p evo-store mates`
- [x] T57 [FR-63, FR-69] `mate_agenda` app tool (add, done, list; due times and repeats parsed) and the conduct rule — `src-tauri/src/agent_tools/mates.rs`, `crates/evo-engine/src/session_roles.rs` — check: shell tests; engine role test
- [x] T58 [FR-64, FR-65, FR-67, FR-71] Heartbeat: due items, budget, back-off, Do not disturb, pause; the prompt; NOTHING_TO_SAY — `src-tauri/src/mates/initiative.rs` — check: shell tests for each skip and for a fire
- [x] T59 [FR-68] Read-only heartbeat turns: approval `ask` and every sending tool asks while one runs — `src-tauri/src/engine.rs`, `agent_tools/mcp.rs` — check: shell test
- [x] T60 [FR-66] Thread: hide heartbeat prompts and NOTHING_TO_SAY turns; a first message shows as the mate's message with its reason — `src/features/mates/thread-rows.ts`, `message-markup.ts` — check: `bun test`
- [x] T61 [FR-65, FR-70] Panel: Initiative and Following up (remove, never, pause, change a check-in) — `src/features/mates/following-up.tsx`, `mates_agenda_*` commands — check: manual
- [x] T62 [FR-53] Router budget 6 s — `crates/evo-engine/src/channel/router.rs` — check: `cargo test -p evo-engine channel`
- [ ] T63 [SC-7, FR-66] Conduct set: a scripted week of five events; review of first messages — `crates/evo-engine/tests/mates_conduct.rs` — check: results recorded in `conduct-results.md`

### Phase 6 — From the market research

Done when: Evo Mates opens on Today with what needs the person, what is
new, what is coming and what each mate did today; new mates start from 16
roles or a ready-made team or a description, with starters to click; the
roster search finds messages; a mate reads its Knowledge files; a check-in
can be added from the panel ([research-2026-11.md](research-2026-11.md),
plan 3.10).

- [x] T68 [FR-73] Tests, then code: `agenda::upcoming` (open items of every mate before a time, in order; done, never and paused left out); a test that a mate's usage since a time counts its seats and nothing older — `crates/evo-store/src/agenda.rs`, `mates.rs` (tests) — check: `cargo test -p evo-store upcoming a_mates_usage_since`
- [x] T69 [FR-73] `mates_today`; `todaySections` with tests (waiting first, unread newest first, hidden mates left out); the Today page and its sidebar row, shown when no conversation is open — `src-tauri/src/mates/today.rs`, `src/features/mates/today*.ts(x)`, `mates-shell.tsx`, `src/mates/mates-app.tsx`, `src/stores/mates-store.ts` — check: `bun test src/features/mates`; manual
- [x] T70 [FR-8, FR-74] Tests, then code: `starter:` lines in role files; every role has 3–4 starters; at least 16 roles in at least 6 categories; eight new roles — `src-tauri/src/mates/template.rs`, `templates/*.md` — check: `bun run test:shell mates::template`
- [x] T71 [FR-74] Starter chips under the greeting; from-scratch starters; choosing one sends it — `src/features/mates/starters.tsx`, `mate-panel.tsx`, `src/lib/mates.ts` — check: `bun test` (`startersFor`); manual
- [x] T72 [FR-75] Tests, then code: every built-in team is made of gallery roles with its lead among them and previews with the app's mode; `mates_teams`, `team_preview_builtin`, `team_import_builtin` over the file's preview and import — `src-tauri/src/mates/teams.rs`, `team_file.rs` — check: `bun run test:shell mates::teams`
- [x] T73 [FR-75] Teams tab in the gallery and Add team through the import preview — `src/features/mates/gallery.tsx`, `team-import.tsx` — check: manual: Cancel creates nothing, Confirm creates the channel
- [x] T74 [FR-76] Tests, then code: `mates::search_messages` finds mates' and channels' chats only, with a snippet from a line that is not an envelope — `crates/evo-store/src/mates.rs` — check: `cargo test -p evo-store search_messages`
- [x] T75 [FR-76] `mates_search` and **Messages** under the roster while searching — `src-tauri/src/mates/mod.rs`, `src/features/mates/message-hits.tsx`, `roster.tsx` — check: manual
- [x] T76 [FR-77] Tests, then code: Knowledge add refuses folders, hidden files, links, files over 25 MB and the 51st file, keeps names unique; the role's instructions list the files; an export does not — `src-tauri/src/mates/knowledge.rs`, `mates/mod.rs` — check: `bun run test:shell mates::knowledge`
- [x] T77 [FR-77] Knowledge in the Files tab: Add files, list, Remove — `src/features/mates/knowledge.tsx`, `mate-panel.tsx` — check: manual; the mate names a knowledge file it read
- [x] T78 [FR-78] Tests, then code: `mate_profile` saves `name` and `role` only while the mate is "New mate"; the conduct rule for a describing first message — `src-tauri/src/agent_tools/mates.rs`, `crates/evo-engine/src/session_roles.rs` — check: `bun run test:shell agent_tools::mates`, `cargo test -p evo-engine session_roles`
- [x] T79 [FR-78] **Describe a mate** in the gallery — `src/features/mates/gallery.tsx` — check: manual: the mate names itself in its first turn
- [x] T80 [FR-79] Tests, then code: `mates_agenda_add_checkin` makes an open check-in whose first time follows its schedule; presets; **Add check-in** in Following up — `src-tauri/src/mates/initiative.rs`, `src/features/mates/following-up.tsx` — check: `bun run test:shell mates::initiative`, `bun test following-up`; manual
- [x] T82 [FR-80] Tests, then code: `spend::over` (mates at or over the limit, not paused, not excused today; none with the limit off); the check on work changes; excuse on Resume; the setting and the paused line — `src-tauri/src/mates/spend.rs`, `system.rs`, `mates/mod.rs`, `src/features/settings/sections/mates-settings.tsx`, `src/features/mates/composer.tsx` — check: `bun run test:shell mates::spend`; manual
- [x] T81 [FR-73 – FR-80] Check each in the app on scratch data, light and dark, screenshots in `docs/features/evo-mates/assets/`; verification rows; `apps/desktop/README.md` UI map — check: rows below

- [x] T83 [FR-81, FR-82, FR-83] Tests, then code: `attachments.ts` (pictures by extension, `send_file` files, tool pictures, files a reply names); thread rows keep the person's files and pasted blocks and add a mate's files row — `src/features/mates/attachments.ts`, `thread-rows.ts` — check: `bun test src/features/mates`
- [ ] T84 [FR-81, FR-82] Thumbnails, file chips, pasted chips, the full-size viewer and the mate's files row in the thread; paste and drop in the mate composer — `attachment-views.tsx`, `thread.tsx`, `composer.tsx` — check: manual in the app
- [ ] T85 [FR-83] The channel composer's +, paste and drop; the person's and mates' lines with their pictures and files — `channel-view.tsx` — check: manual in the app

### Phase 5 — Verify

Done when: every row below is `passed` with its date.

- [ ] T34 Run the verification table, record results, update `apps/desktop/README.md` UI map and the ADD, set ADR 0028 to Accepted if not yet

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Install on Windows, macOS, Linux; both entries open their app (T6, T7) | not run |
| FR-2 | Shell test T1; manual: no second process, provider added in Harness works for a mate | Windows: `--app mates` cold start opens only Evo Mates (main hidden); a second launch hands over and exits 0, one process — passed (2026-11-25). Provider part: not run (no mates yet) |
| FR-3 | Manual switch both ways (T4) | passed (2026-11-25): Switch to Evo Harness shows main, Mates stays; Switch to Evo Mates reopens a closed Mates window |
| FR-4 | Manual T21 | passed (2026-11-26): a mate asked for a note while Evo Mates was open, the window was closed, the turn completed (engine run completed), and reopening Evo Mates from Harness showed the reply; earlier, with the tray off, closing Evo Mates while main is hidden quits the app (exit 0) — passed (2026-11-25) |
| FR-5 | Manual tray menu (T5) | `tray_target` test passed (2026-11-25); menu clicked in the app: not run |
| FR-6 | Manual roster states (T16) | partly (2026-11-25): rows with face, name, last line and time, pinned tiles, unread/needs-you dot; states from the shell's work list. Queued and error words: not run |
| FR-7 | Manual create from template and from scratch | from a template — passed (2026-11-25, Researcher created and opened on its conversation); from scratch: not run |
| FR-8 | Template parse test (T15, T70) | passed (2026-11-25): `mates::template::tests::every_gallery_role_reads`; gallery shows 8 roles; 2026-11-28: `the_gallery_has_sixteen_roles_in_six_categories`, the gallery shows 16 roles in 10 categories |
| FR-9 | Manual: conversation past the context window; search finds the first message | not run |
| FR-10 | Manual: model change applies next turn | partly (2026-11-25): the mate answers as itself from its role ("Hi! I'm Researcher, one of your mates…"); restart on a profile change is coded, not run |
| FR-11 | `cargo test -p evo-engine memory` (T12); manual across DM and channel | `a_mates_prompt_names_its_own_memory` passed (2026-11-25); the role carries the mate's memory folder (checked in the DB); manual: not run |
| FR-12 | Manual T19 | not run |
| FR-13 | Shell tests T13; manual row menu | not run |
| FR-14 | Shell tests T26; manual | passed (2026-11-26): store, hub and shell tests; in the app a channel of two mates was made, a message mentioning both started their seats (kind `seat`, not listed in Harness) and both replies showed in the channel |
| FR-15 | `cargo test -p evo-engine channel` (T22) | passed (2026-11-26) |
| FR-16 | `cargo test -p evo-engine channel` (T22) | passed (2026-11-26) for mentions, `@everyone` and each rule; the `auto` router runs on the model of T25 (with no connected model `auto` falls back to the lead) |
| FR-17 | Engine test T24; manual | passed (2026-11-26): hub copy test; coordinator tests for mention deliveries; in the app a mate asked another by @mention, the answer went back and the asker posted the result |
| FR-18 | Engine test T24 | passed (2026-11-26): `a_chain_stops_at_the_messages_limit` (six hand-offs, the sixth stops every seat), `an_eleventh_hand_off_in_the_hour_is_refused` (across messages, either direction, freed after the hour); the chain itself is counted in the coordinator with these functions; manual: not run; 2026-11-27: `a_report_back_is_not_counted_against_the_message` (T66); default 12: `a_channel_allows_twelve_handoffs_by_default` (T67) |
| FR-19 | Manual: three mates stop within 2 s | partly (2026-11-26): Stop in a channel of two mates stopped the working seat at once (no run after it) and posted "Stopped."; three mates and one-mate Stop: not run |
| FR-20 | Team file tests T31 | passed (2026-11-26): round trip, no ids, no model ID, setup answers stay home; in the app a channel export held no ids or secrets (grep 0) |
| FR-21 | Team file tests T31; manual T33 | passed (2026-11-26): the preview shows the app's mode and missing connectors (test); in the app an import made both mates with the app's mode, sending Ask, skills off, and the channel with them; the native file dialogs: not driven |
| FR-22 | `cargo test -p evo-store usage`; manual | `usage_is_summed_by_mate` passed (2026-11-25); the Usage page's "Evo Mates" panel: not checked in the app |
| FR-23 | Manual with an untrusted project folder | not run; a channel's seats run with the channel's folder as their working folder, so the engine's project trust (`trust.rs`) applies to it as in Harness |
| FR-24 | Manual T19 | partly (2026-11-25): `harness_lists_and_searches_only_its_own_chats` passed; the Harness window does not list a mate's chat. Needs you: not run (T19) |
| FR-25 | Shell tests T13, T26; manual | partly (2026-11-26): seats resolve the channel's folder, else the mate's (`folder_of`); channel settings set and clear it and seats start again there; manual: not run |
| FR-26 | Manual T18 | not run |
| FR-27 | Team file tests T31 | passed (2026-11-26): a newer version and a non-team file are refused, unknown entries are listed |
| FR-28 | Router unit test T25; manual Usage | passed (2026-11-26): `the_router_is_the_chosen_model_else_the_cheapest` on a fixture catalogue (the choice, the cheapest priced, a gone choice falls back, none connected: no router); a channel's calls record under usage source `mates` (engine test of T58); Settings › Evo Mates › Picks who answers; manual Usage: not run; in the app (2026-11-26): with no choice the channel ran on the cheapest connected model, with a choice on it, after a restart of the channel (T65); 2026-11-27: the order models are tried in (`the_router_is_the_chosen_model_else_the_cheapest`), shell test `a_seat_never_runs_on_its_channels_router_model` (T66) |
| FR-29 | Design review T36 | not run |
| FR-30 | Manual T37 | not run |
| FR-31 | Manual T37 | not run |
| FR-32 | Manual T36 | not run |
| FR-33 | Visual review T35 | not run |
| FR-34 | Shell test and manual T38 | passed (2026-11-25): shell tests; in the app a mate saved four answers and they show in its profile |
| FR-35 | Manual T37 | not run |
| FR-36 | Manual T28 | not run |
| FR-37 | Manual T39 | not run |
| FR-38 | Manual T39 | not run |
| FR-39 | Shell test T41 | partly (2026-11-25): the Apps tab switches each connector for the mate's chat (Harness's per-chat switches, `connectors.chatOff`); plugin accounts and template defaults off: not done |
| FR-40 | Shell test and manual T42 | passed (2026-11-25): shell tests; in the app the Connect card showed for a missing mail app, Connect opened Customize → Connectors and the waiting card could be cancelled |
| FR-41 | Shell tests T43 | passed (2026-11-25) |
| FR-42 | Shell tests T43 | passed (2026-11-25) |
| FR-43 | Leak test T44 | resolve and mask tests passed (2026-11-25); in the app a canary value typed into the secure card was found nowhere in the data folder or logs; a check of real provider requests: not run |
| FR-44 | Manual T45; T33 (imported skills off); T55 (flagged skills) | switches passed (2026-11-25): engine and UI tests; in the app two skills switched off showed as off in the mate's engine after its restart; imported and flagged skills: not done |
| FR-45 | Shell test and manual T46 | passed (2026-11-25, in the app): Pause all marks every mate Paused, the box shows "<name> is paused" with Resume, Resume all restores them; engines stop (`stop_mate`) |
| FR-46 | Test set T47 | conduct rule in the role prompt checked by `a_mate_role_gets_the_mate_conduct` (2026-11-26); a new mate's setup card: seen in the app (one card), not part of the set |
| FR-47 | Test set T47 (8 of 10) | passed (2026-11-26): 10 of 10 on the second run, [conduct-results.md](conduct-results.md) |
| FR-48 | Review T47 | passed (2026-11-26): 10 of 10 replies put the result first and name the files, [conduct-results.md](conduct-results.md) |
| FR-49 | Manual T48 | not run |
| FR-50 | `cargo test -p evo-engine mates_brief` (T49) | passed (2026-11-26): the brief lists the other places' lines of the last two days, newest ten, never its own place; place and summary from a turn; the log is cut back; search half: `cargo test -p evo-store a_mate_finds_only_its_own_chats` passed (2026-11-25); manual (ask in the direct conversation about channel work): not run |
| FR-51 | Test set T47; manual | rule in the role prompt (2026-11-26); a fact the person states saved with the chip: seen in the app (2026-11-25); other mates' claims: needs channels (phase 3), not run |
| FR-52 | Shell tests T50 | passed (2026-11-25) |
| FR-53 | `cargo test -p evo-engine channel` (T51) | passed (2026-11-26): thresholds, "several", fallbacks and route lines; in the app a live router never answered within 1.5 s; the budget moves to 6 s (T62) |
| FR-54 | Engine tests T52; manual | passed (2026-11-26): `hand_off_kinds`; in the app a mention hand-off showed "asks" and the answer "tells"; 2026-11-27: `a_mate_asks_the_members_it_mentions_in_a_question`, hub `mail_between_seats_is_copied_to_the_channel` without the envelope (T66) |
| FR-55 | Engine tests T52 | passed (2026-11-26): `members_never_assign_the_lead`; the lead's wrap-up is asked for in its delivery (`a_delivery_carries_the_channel_the_recent_lines_and_the_message`) |
| FR-56 | Engine tests T53 | passed (2026-11-26): `only_a_single_mention_of_a_working_mate_steers`; channel rows test for "Seen by … while working"; replies carry `replyTo`; manual steering in the app: not run |
| FR-57 | Engine tests T53; manual | passed (2026-11-26): delivery contents tests (`a_delivery_carries_the_channel_the_recent_lines_and_the_message`, `a_mention_from_a_mate_is_marked_as_the_mates`); manual day of use: not run |
| FR-58 | Visual review in the app | partly (2026-11-25): blue right bubbles, grey left bubbles with Markdown, tails on the last of a run, a time line, steps in plain words, details behind an info button; dark theme and a 16-minute gap: not run |
| FR-59 | Manual: a running turn | passed (2026-11-25): three-dot typing bubble while the mate works, Stop in the box |
| FR-60 | `bun test src/features/mates` (markup), `cargo test -p evo-store reactions`; manual | tests passed (2026-11-25); reacting in the app: not run |
| FR-61 | `bun test src/features/mates` (markup); manual | markup tests passed (2026-11-25); a reply in the app: not run |
| FR-62 | Manual | partly (2026-11-25): pill box, + attach, send and stop buttons, Enter sends, no inner focus ring; attach and Shift+Enter: not run |
| FR-63 | Store tests T56; tool tests T57 | passed (2026-11-26): store tests; in the app a mate put a call the person mentioned on its agenda by itself |
| FR-64 | Shell tests T58 | passed (2026-11-26): decision tests; in the app the heartbeat woke the mate for its due item and skipped it while it was busy |
| FR-65 | Shell tests T58; manual T61 | passed (2026-11-26): budget and back-off tests; in the app the unanswered count went to 1 after the first message |
| FR-66 | Thread tests T60; review T63 | thread tests passed (2026-11-26); in the app the first message said why first ("I am writing because at 11:56 you had a call…") with no prompt shown, and the mate showed unread; review of ten: not run (T63) |
| FR-67 | Shell tests T58 | passed (2026-11-26) for the skip; sending what is still worth saying after Do not disturb: not run |
| FR-68 | Shell test T59 | passed (2026-11-26): `a_heartbeat_turn_is_read_only` |
| FR-69 | Tool tests T57; manual T61 | check-ins through `mate_agenda` with the Scheduler's schedules; manual: not run |
| FR-70 | Manual T61 | partly (2026-11-26): the list, Initiative and the check-in editor (every day, weekdays, weekly days and time) are in Settings, editor tests passed; a manual pass of each action: not run |
| FR-71 | Shell tests T58; manual Usage | passed (2026-11-26): pause skips; heartbeat turns and a channel's calls record under usage source `mates` (engine test); manual check of Usage: not run |
| FR-72 | `cargo test -p evo-engine channel session_roles` (T64); manual | passed (2026-11-26): `a_reply_goes_to_the_mates_who_asked`, `a_question_to_the_person_is_one_that_mentions_no_member`, `a_seat_asks_in_its_reply_not_with_a_card`; manual in a channel with **Everyone**: the mate asked numbered questions in its reply, the answer without a mention went only to it ("Hạnh Sale, who asked you") and it wrote the draft |
| FR-73 | `cargo test -p evo-store upcoming a_mates_usage_since` (T68); `bun test` (T69); manual T81 | passed (2026-11-28): `upcoming_lists_every_mates_open_items_in_time_order`, `a_mates_usage_since_a_time_counts_its_seats_and_nothing_older`, `the_window_runs_from_this_midnight_to_the_end_of_tomorrow`, `today-sections.test.ts`; in the app (scratch data) Today listed Writer under Needs you, a daily check-in under Coming up (Tomorrow 10:00) and three mates' replies and cost; light and dark ([assets/research-today-light.png](assets/research-today-light.png), [dark](assets/research-today-dark.png)) |
| FR-74 | `bun run test:shell mates::template` (T70); manual T81 | passed (2026-11-28): `every_gallery_role_reads` (3–4 starters each), `starters_are_one_per_line_and_may_hold_commas`, `starters.test.ts`; in the app a starter sent as the person's first message and the mate answered ([assets/research-starters-light.png](assets/research-starters-light.png)) |
| FR-75 | `bun run test:shell mates::teams` (T72); manual T81 | passed (2026-11-28): `every_team_is_made_of_gallery_roles_with_its_lead_among_them`, `a_team_previews_with_the_apps_mode_like_a_file`; in the app Add team showed the preview with Ask every time, and confirming made three mates and #Content studio led by the Editor ([preview](assets/research-team-preview-light.png), [teams](assets/research-teams-light.png), [dark](assets/research-teams-dark.png)) |
| FR-76 | `cargo test -p evo-store search_messages` (T74); manual T81 | passed (2026-11-28): `search_messages_finds_mates_and_channels_only_with_a_snippet`; in the app "team plan" listed the Researcher's conversation with its snippet ([assets/research-search-light.png](assets/research-search-light.png)) |
| FR-77 | `bun run test:shell mates::knowledge` (T76); manual T81 | passed (2026-11-28): knowledge tests (refusals, unique names, 50 files, the role block, stored instructions untouched); in the app a folder was refused with its reason and the mate answered from the added file, naming it ([light](assets/research-knowledge-light.png), [dark](assets/research-knowledge-dark.png)) |
| FR-78 | `bun run test:shell agent_tools::mates` (T78); manual T81 | passed (2026-11-28): `a_mate_names_itself_only_while_it_is_new`, `a_mate_role_gets_the_mate_conduct`; in the app a described mate named itself "Ledger" with a one-line role in its first turn ([assets/research-describe-light.png](assets/research-describe-light.png)); the first try exposed a tools race, fixed in `252fae8e` |
| FR-79 | `bun run test:shell mates::initiative` (T80); manual T81 | passed (2026-11-28): `a_check_in_from_the_panel_comes_round_on_its_schedule`, presets test; in the app Morning brief added "Every weekday at 09:00" to Following up ([assets/research-add-checkin-light.png](assets/research-add-checkin-light.png)) |
| FR-80 | `bun run test:shell mates::spend` (T82); manual T81 | passed (2026-11-28): `mates_at_or_over_the_limit_are_paused_unless_excused_or_paused_already`, `paused-line.test.ts`; in the app with a $0.05 limit, Writer ($0.18) and Researcher ($0.08) paused with the log line and the box naming the limit, Ledger ($0.04) did not, and Resume kept Researcher going ([assets/research-limit-light.png](assets/research-limit-light.png)) |
| FR-81 | `bun test` (T83); manual T84 | not run |
| FR-82 | `bun test` (T83); manual T84 | not run |
| FR-83 | `bun test` (T83); manual T85 | not run |
| NFR-1 | Timing T8 | passed (2026-11-25, Windows debug build): switch to an open window 29 ms; Mates window rebuilt from Harness 248 ms; second launch to Mates shown 602 ms; cold start to Mates page 1.4 s |
| NFR-2 | Timing in T17 | not run |
| NFR-3 | Open ports and outbound hosts before and after | not run |
| NFR-4 | Manual per platform | Windows: two taskbar groups (Evo Harness, Evo Mates) — passed (2026-11-25); Start menu entry from an installed build, macOS launcher, Linux desktop entry: not run |
| NFR-5 | Manual T30 | not run |
| NFR-6 | Shell test T27; manual | queue tests passed (2026-11-26); held: mail that would start a mate's or seat's engine; not held: the person's own messages and mail to an engine that already runs; manual with limit 1: not run |
| NFR-7 | Design review against DESIGN.md and ux.md; string check T40 | string check passed: no session, engine or agent type in Mates UI strings (2026-11-25); design review: not run |
| NFR-8 | Migration test T9; Harness CI checks | `mates_upgrade_keeps_chats_as_harness_chats_and_usage_rows` passed (2026-11-25); a copy of a real schema-14 database upgraded to 15 in the app |
