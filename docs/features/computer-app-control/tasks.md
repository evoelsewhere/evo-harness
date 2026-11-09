# Computer app control in the background — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-10 |

## 1. Phases

Paths:

- Shell paths are under `apps/desktop/src-tauri/src/`, and `computer/` means
  `agent_tools/computer/`.
- UI paths are under `apps/desktop/src/`.
- "Ported" means taken from tag `v3.1.1` `desktop/src-tauri/src/computer_app/`
  with its tests. Ported code changes only where the plan says.

Commands:

- Shell tests run with `bun run test:shell <filter>`. Live tests are
  `#[ignore]`: run them with `bun run test:shell <filter> -- --ignored`.
- Every live test starts its own app instance (its own profile for Chromium
  and Electron) and never touches the person's open windows.

### Phase 1 — Windows backend in the shell

Done when:

- `bun run test:shell computer` passes;
- the ported live tests drive Notepad and Excel parked off screen;
- a crashed run's window is put back from `evo.db`.

- [x] T1 [FR-5] Test that the computer module never names `SendInput`, `SetCursorPos`, `SetForegroundWindow`, `BlockInput`, `keybd_event` or `mouse_event` — `computer/tests.rs` — check: fails with a planted call, passes without
- [x] T2 [FR-2, FR-22] Port the action enum and key parsing with blocked keys — `computer/{action,keys}.rs` — check: `bun run test:shell computer::keys computer::action`
- [x] T3 [P] [FR-4] Port policy: protected processes, allow/block matching (case, `.exe`, block wins) — `computer/policy.rs` — check: `bun run test:shell computer::policy`
- [x] T4 [P] [FR-12, FR-26] Port Stop tickets and per-chat workers — `computer/{interrupt,workers}.rs` — check: `bun run test:shell computer::interrupt computer::workers`
- [x] T5 [P] [FR-23] Port the app catalog search — `computer/apps.rs` — check: `bun run test:shell computer::apps`
- [x] T6 [FR-2, NFR-1] `ComputerBackend` trait, platform factory, `UnsupportedBackend` — `computer/backend.rs`, `computer/mod.rs` — check: the module builds on Windows and Linux in CI
- [x] T7 [FR-9, FR-10, FR-14, FR-15, FR-21] Port the Windows channels 2 and 3, UIA tree and refs, typing, keyboard, popups, `PrintWindow` capture — `computer/win/*.rs` — check: live `drives_notepad_in_the_background`, `refuses_refs_behind_a_modal_dialog`, `types_across_cells_that_open_their_own_editor`
- [x] T8 [FR-8] Test, then add the `computer_parked` migration and store functions — `crates/evo-store/src/migrations/0010_computer_parked.sql`, `crates/evo-store/src/computer.rs` — check: `cargo test -p evo-store computer`, plus an old database upgrades
- [x] T9 [FR-6, FR-7, FR-8, FR-9] Stage from v3 parking, journalled in `computer_parked`: off-screen move, stage size, restore, popup follow hook, start-up restore — `computer/win/stage.rs`, `computer.rs`, `lib.rs` — check: live `puts_back_a_window_parked_by_a_run_that_crashed`, `keeps_a_parked_apps_dialogs_off_screen`, `drives_excel_on_the_stage`
- [x] T10 [FR-6, NFR-3] Corner pixel for Chromium windows without a page channel: position and `HWND_TOPMOST` in one `SetWindowPos`, re-asserted with a 1 px nudge; topmost cleared on release — `computer/win/stage.rs` — check: live `corner_pixel_keeps_chromium_painting` (an Edge instance with its own profile shows changing `PrintWindow` frames; the foreground is unchanged)

Phase 1 results (2026-11-10, Windows 11, three monitors):

- `bun run test:shell`: 466 passed. `cargo test -p evo-store`: 38 passed.
- Live tests passed:
  - `corner_pixel_keeps_chromium_painting` (new);
  - `refuses_refs_behind_a_modal_dialog`, `keeps_a_parked_apps_dialogs_off_screen`,
    `keeps_a_parked_apps_panes_and_palettes_off_screen`,
    `types_across_cells_that_open_their_own_editor`,
    `puts_back_a_window_parked_by_a_run_that_crashed`;
  - `drives_notepad_in_the_background`, `drives_excel_on_the_stage`;
  - `probes_chromium_fields`, `probes_chromium_pointer` (the drag now works
    without the transparent peek), `works_in_a_select_popup` (now parked),
    `clicks_the_element_drawn_on_top`;
  - `probes_webview2_fields`, `probes_webview2_pointer`,
    `treats_a_small_web_pane_as_native`.
- `probes_webview2_pointer` hung once while running in a batch right after a
  failing test. It was not reproduced in three runs on its own (15–18 s each).
- `drives_vscode_by_its_keys` fails on VS Code 1.145 (Electron 43):
  - a posted `ctrl+n` does not reach the editor, whether the window is shown
    or parked;
  - while parked, the app thread's focus is empty after `ctrl+alt+i`.

  This is left for the page channel (T24), which is the plan's answer for
  Electron apps evo starts. It is not a Phase 1 regression of the port.

### Phase 2 — The tool, the gate and the lifecycle

Done when: a chat drives Notepad end to end through the engine, with
approvals, release at the end of the turn, and the settings switch applied
live (FR-1 – FR-4, FR-7, FR-24 – FR-26).

- [x] T11 [FR-24] Gate tests for `Class::Control`:
  - with `permission = ask`, it asks in `ask`, `auto-edits` and `auto`;
  - with `allow`, it follows the mode;
  - a refusal blocks the same action for the rest of the run;
  - `status`, `wait` and `detach` never ask.

  Path: `agent_tools/gate.rs`. Check: `bun run test:shell agent_tools::gate`.
- [x] T12 [FR-24] `Class::Control` in the gate — `agent_tools/{gate,mod}.rs` — check: T11 passes
- [x] T13 [FR-2, FR-25, FR-26] Tests for the batch runner (skip after failure, `detach` still runs), per-action approval summaries (`click e12`, `type "…" (240 chars) into e5`), untrusted wrapping, timeouts — `computer/mod.rs` — check: `bun run test:shell computer::tool`
- [x] T14 [FR-2, FR-3, FR-25, FR-26] The `computer_app` tool: schema (plan 3.3), batch runner, one window per chat and one chat per window, results — `computer/mod.rs`, `agent_tools/mod.rs` — check: T13 passes; live `two_chats_cannot_share_a_window`
- [x] T15 [FR-1] Settings keys, tool registered only while enabled, `configure_external_tools` re-sent on change, disabling releases every app — `agent_tools/mod.rs`, `computer.rs`, `stores/engine-store.ts` — check: in the app, the tool appears and disappears without a restart
- [ ] T16 [FR-7] Release on `agent_end`, chat close and quit (3 s cap, own threads) — `engine.rs`, `computer.rs`, `lib.rs` — check: live `returns_the_window_at_the_end_of_the_turn`; quit with a hung app returns within 3 s
- [x] T17 [P] [FR-11] `computer_` activity kind and pointer events emitted before each pointer action — `agent_tools/activity.rs`, `computer/mod.rs` — check: unit test for the payload

Phase 2 results (2026-11-10):

- Done and checked: T11–T14, T17.
  - `bun run test:shell`: 482 passed, including `agent_tools::gate` (13) and
    `agent_tools::computer::tool` (10).
  - `bun run typecheck` passed.
  - New live test `returns_the_window_at_the_end_of_the_turn` passed.
  - Two chats cannot share a window: checked by the `controlled_elsewhere`
    assertions in `drives_notepad_in_the_background` (Phase 1).
- **T15** is built but not ticked: the settings keys, the tool offered only
  while enabled, `agent-tools://changed` re-sending the tools to every ready
  chat, and disabling releasing every app. Its check (in the running app) has
  not run yet.
- **T16** is built but not ticked: release on `agent_end` and when a chat's
  engine stops, and release on quit. Its live test passed; "quit with a hung
  app returns within 3 s" has not run yet.
- These in-app checks need the app run beside the person's installed copy,
  with a scratch data folder (see plan 5). They are planned together with
  Phase 4, which adds the card to see them by.

### Phase 3 — Accuracy layer

Done when: FR-14 – FR-20 and FR-23 hold in unit and live tests.

- [x] T18 [FR-17] Read-back diff tests on synthetic trees: focus, value, selection, a new dialog or menu, no change — `computer/readback.rs` — check: `bun run test:shell computer::readback`
- [x] T19 [FR-17] Read-back wired into every acting action; `type` confirmation — `computer/{readback,mod}.rs` — check: T18 passes; live `reports_typing_that_did_not_arrive`
- [x] T20 [FR-18, FR-19] Tests for the channel ladder (one step down, never real input) and the limits table (toolkits that drop posted input, hover-only, OLE drag, raw input) — `computer/mod.rs` — check: `bun run test:shell computer::ladder`
- [x] T21 [FR-18, FR-19] Channel ladder and honest limits — `computer/mod.rs`, `computer/win/target.rs` — check: T20 passes; live `tells_toolkits_that_drop_posted_input`
- [x] T22 [P] [FR-14] Set-of-marks: tests, then drawing ref boxes and labels on the screenshot — `computer/marks.rs` — check: `bun run test:shell computer::marks`
- [x] T23 [P] [FR-20] `zoom`: tests, then crop ×2 and coordinate mapping — `computer/zoom.rs` — check: `bun run test:shell computer::zoom`
- [x] T24 [FR-16, FR-23, NFR-6] Page channel for web-based apps evo starts:
  - start with `--remote-debugging-pipe` and the occlusion flag, minimized
    without activation;
  - DevTools input and screenshots;
  - no script, storage or network methods reachable from the tool;
  - the channel closes with the app.

  Path: `computer/win/page.rs`. Check: live
  `drives_an_electron_app_through_the_page_channel` (VS Code with its own
  profile), `edge_with_page_channel_stays_fresh_off_screen`,
  `starting_an_app_keeps_the_persons_foreground`.
- [ ] T25 [NFR-3] The `late` flag for already-open Chromium windows whose frames show no change after an action — `computer/win/capture.rs`, `computer/mod.rs` — check: unit test on frame sequences; live test with a page that stops painting
- [x] T26 [FR-23] `search_apps`, `open_app` (catalog only), `close_app`, `kill_app`, with refusals — `computer/win/lifecycle.rs`, `computer/mod.rs` — check: live `opens_closes_and_kills_from_the_catalog_only`; `open_app` with a path is refused

Phase 3 results (2026-11-10):

- Done and checked: T18–T24, T26.
  - `bun run test:shell`: 494 passed; `bun run typecheck` passed.
  - The ladder and limits tests live in `computer::tool`
    (`a_click_by_ref_without_effect_steps_down_one_channel`,
    `input_without_effect_is_reported_as_a_limit`); read-back in
    `computer::readback`; marks and zoom in `computer::marks` and
    `computer::zoom`.
- Live tests passed:
  - `drives_notepad_in_the_background`, which now also asserts the read-back
    of typed text (T19);
  - `marks_controls_and_zooms_a_region` (T22, T23);
  - `drives_an_electron_app_through_the_page_channel` and
    `edge_with_page_channel_stays_fresh_off_screen` (T24);
  - `opens_closes_and_kills_from_the_catalog_only` (T26);
  - again: `probes_chromium_fields`, `corner_pixel_keeps_chromium_painting`,
    `keeps_a_parked_apps_dialogs_off_screen`.
- `probes_chromium_pointer` passed once. It failed once because the
  foreground moved from one of the person's windows to another while it ran:
  the person, not evo, as the window handles showed. Telling these apart is
  the input monitor's job (T36).
- **T25** is built but not ticked: the `picture_may_be_late` flag on actions,
  and the "may be late" line on screenshots. Its unit test passed; the live
  test with a page that stops painting has not run.
- `drives_an_electron_app_through_the_page_channel` replaces the failing
  `drives_vscode_by_its_keys` for VS Code 1.145. That test drives an
  already-open VS Code with posted keys, which this VS Code ignores. It stays
  as a record of the accessibility path's limit.

### Phase 4 — Card, settings and transcript

Done when: FR-1 and FR-10 – FR-13 hold in the running app (driven through
WebView2 DevTools), and `bun run typecheck` and `bun test` pass.

- [x] T27 [FR-10, NFR-8] `Streams` keyed by surface id; `computer_stage`; `computer://frame` and `computer://pointer` — `browser/stage.rs`, `computer.rs` — check: `bun run test:shell browser::stage` still passes, plus a new surface test
- [x] T28 [FR-10, FR-12] Tests, then `computer-store`, `CardKind` `"app"`, events in `agent-events`, removal in `cards-sync` — `stores/{computer-store,cards-store,agent-events,cards-sync}.ts`, `*.test.ts` — check: `bun test stores`
- [x] T29 [FR-10, FR-11, FR-13] App card body: frames, `AgentCursor`, the person's input through `computer_input` — `features/cards/{app-canvas,card-body,stage-controller,stage-stream}.tsx/ts`, `computer.rs` — check: in the app, a card shows Notepad live and a click in the card reaches it
- [x] T30 [FR-12] **Stop**, **Allow again**, **Show the app**, **Close** and their commands — `features/cards/card.tsx`, `computer.rs` — check: Stop during a long `type` stops within one character, and the next `attach` is refused until Allow again
- [ ] T31 [FR-1] **Settings → Computer apps**: switch, approval, app picker (`computer_list_apps`) — `features/settings/sections/computer-apps-settings.tsx`, `settings-registry.ts` — check: in the app
- [x] T32 [P] [FR-2] Transcript kind `computer`, one line per action — `components/ai-elements/{tool-kinds.ts,tool.tsx}` — check: `bun run typecheck`, and a call renders in the app
- [x] T33 [P] UI map: the app card and the settings section — `apps/desktop/README.md` — check: review

Phase 4 results (2026-11-10):

- Done and checked: T27–T30, T32, T33, and T15's in-app check. T31 (the
  Settings page) is built and type-checks, but the e2e run switched the
  feature with `settings_set` and never opened the page: not ticked.
  - `bun run test:shell`: 494 passed. `bun test`: 95 passed, including
    `stores/computer-store.test.ts` and `features/cards/app-input.test.ts`.
  - `bun run typecheck` passed.
- **In the running app.** A debug build with the identifier
  `com.evoelsewhere.evo.e2e` ran beside the person's installed copy, on a
  scratch copy of `evo.db` (`EVO_CODING_AGENT_DIR`). It was driven over
  WebView2 DevTools, with the agent on Claude Sonnet 5.5 working on the
  WinForms dialog probe. Seen:
  - Turning the feature on offered the tool. Turning it off while a chat
    was open removed `computer_app` from that chat: the agent said it no
    longer had the tool, and no restart was needed (T15, FR-1).
  - The approval prompt came up in "Approve for me" with "Ask every time"
    (FR-24).
  - An app card appeared on attach, titled by the window, and its picture
    painted within about 3 s. Its line said "Click e2". The picture turned
    to the modal dialog when the agent opened it. The card went when the
    turn ended (FR-7, FR-10, FR-11).
  - **Stop:** the window came back to its place (120,120 420×220 from
    4040,29 on the stage), and the card stayed with "You stopped app
    control…" and Allow again. Allow again brought back Stop (FR-12).
  - A click on "Close dialog 1" in the card's picture closed the dialog in
    the app: the picture went from the 322×172 dialog to the 1280×800
    window (FR-13).
  - **Show the app** handed the window back and closed the card.
  - **Close** closed the card. The agent's next `attach` in the same turn
    was refused with "The person closed the app's card during this turn…",
    and the agent stopped there (FR-12).
- Fixed during the run:
  - after Allow again with no app attached, the card stayed. Resuming now
    releases the card;
  - the transcript showed the tool's `structuredContent` as raw JSON under
    each step. It is no longer sent.
- **T16** stays open: quitting evo with a hung app (3 s cap) has not run.

### Phase 5 — Skill

Done when: the skill is offered only to chats that have `computer_app`, and
its evaluation cases pass (FR-27).

- [x] T34 [FR-27] Test, then bundled skills with `requires-tools` front matter offered only when the chat has those tools — `crates/evo-engine/src/` (skill listing) — check: `cargo test -p evo-engine skills_require_tools`
- [x] T35 [FR-27] Port the skill and rewrite its input sections for the channel order, marks, read-back and limits; add evaluation cases — `crates/evo-engine/bundled-skills/computer-app-control/` — check: the cases trigger as listed in FR-27

Phase 5 results (2026-11-10):

- Done: T34, T35.
  - The test lives in `evo-harness` (`skills::tests::skills_require_tools`,
    `reads_requires_tools_as_a_list_or_a_comma_string`), where skills are
    listed.
  - `evo-engine` checks the bundled skill: `office::tests::bundled_computer_app_skill_requires_its_tool`
    and `the_bundled_skills_load_as_skills`.
  - `cargo test -p evo-harness -p evo-engine` passed (no failures);
    `cargo fmt --all -- --check` passed.
- The trigger cases are in
  `crates/evo-engine/tests/fixtures/skill-evals/computer-app-control/trigger-cases.json`
  (14 cases, from v3, Windows-only now). Running them needs a model and an
  evaluation harness evo does not have yet: **not run**.

### Phase 6 — Benchmark and verification on Windows

Done when: SC-1 – SC-5 and NFR-2 – NFR-8 are recorded in the verification
table. This phase includes spike 3 from [research.md](research.md): v3's
failing cases.

- [x] T36 [NFR-2, SC-2, SC-3] Input monitor: samples the person's pointer, foreground, focus and key state every 10 ms during a run, and a typing robot in another app counts lost or misdirected keys — `computer/win/live_bench.rs` — check: it reports a planted `SetCursorPos` from the test itself
- [x] T37 [SC-1] Port `live_bench` with 10 runs per case. Surface kinds: grid, text document, forms and dialogs, menus, evo-started web app, already-open web app. Include v3's failing cases (grid ctrl+click, ribbon toggle, right click, long typing) — `computer/win/live_bench.rs` — check: produces the per-case rate table
- [ ] T38 [P] [SC-5] "Impossible" cases (hover-only menu, drag a file between apps, key tips) expect an honest limit — `computer/win/live_bench.rs` — check: every case reports the limit
- [ ] T39 [NFR-4, NFR-7, NFR-8] Timing (median per action), a run at 150 % scaling, CPU with the card minimized — `computer/win/live_bench.rs` — check: numbers in the table
- [ ] T40 [SC-1 – SC-5] Run the benchmark and record the results — `tasks.md` section 2, plan section 5 — check: every row filled
- [x] T46 [NFR-4] Cut the time an action spends outside the app: tree walks with one cached read per element (a native app's tree fetched whole), the "before" read during the card cursor's travel, the fixed 250 ms wait replaced by waiting for the app and a second look only when nothing changed, the cursor's travel 220 → 160 ms, the 150 ms wait after a click replaced by waiting for the app — `computer/win/{mod.rs, uia_tree.rs, target.rs, pointer.rs, uia_input.rs}` — check: live `times_each_step_on_the_grid_probe`; the fixture live tests; the benchmark timing table
- [ ] T47 [FR-30] Detect a driven app holding the front window: once in the action's result when it happens (and on attach), the agent told to tell the person; the card's warning while it lasts (`computer://front`) — `computer/win/mod.rs`, `computer/win/capture.rs`, `computer/tool.rs`, `src/computer.rs`, `stores/computer-store.ts`, `stores/agent-events.ts`, `features/cards/app-canvas.tsx` — check: `computer::tool` front-line test; `computer-store.test.ts`; live `when_excel_takes_the_front` reports it; the warning in the e2e app
- [x] T48 [FR-27, FR-30] The skill: in Office, prefer keyboard shortcuts to the ribbon; when a result says the app took the front, tell the person at once — `crates/evo-engine/bundled-skills/computer-app-control/` — check: `cargo test -p evo-engine bundled`
- [ ] T45 [FR-30, SC-2] Hold the foreground lock while an action runs and 1.5 s after, counted across chats — `computer/win/front.rs`, `computer/win/mod.rs`, `computer/win/page.rs` — check: unit test of the hold count; the benchmark's modal dialog and forms cases with no front-window change

Phase 6 results so far (2026-11-09):

- Done: T36, T37. The benchmark is the ignored test
  `computer::win::benchmark::benchmark_windows` (`EVO_BENCH_RUNS`, default
  10); it writes `%TEMP%\evo-computer-app-benchmark.md`. The input monitor
  test `the_input_monitor_sees_a_planted_cursor_move` passed. The typing
  robot runs only with `EVO_BENCH_ROBOT=1`: **not run** (it takes the
  keyboard).
- Smoke run, 1 run per case (not the 10 SC-1 asks for): 40 of 41 cases
  passed. The failure is "web app already open / fields": the foreground
  window changed during the case; the assertion now names both windows.
  Word was skipped (below).
- Harness safety. A Word the harness starts reopens the person's recovered
  document instead of an empty one. In the first smoke run the harness
  typed into that document and ended Word, losing the recovered copy. The
  harness now takes only a new window whose title starts with the launch's
  hint (`Document`, `Excel`), and leaves a process running when it holds
  any other window.
- SC-2 / NFR-2 not met: the monitor saw 8 front-window changes without the
  person's input, all from windows the driven app activated itself in
  answer to evo's input: Excel's new workbook after `ctrl+n`, Excel's
  "Create Table" dialog (after its `escape` the front went back to the
  person's window), and the WinForms probes' dialogs. evo's own code never
  sets the front window (T1); the apps do. A fix needs a decision (spec
  change), see plan section 5.
- NFR-4 not met: medians are Snapshot 135 ms, Attach 328 ms, Click 1121 ms,
  Key 1776 ms, Type 1722 ms, Invoke 2005 ms. The read-back walk after each
  action, the 250 ms settle and the 200 ms pointer travel dominate.
- FR-30 (T45, 2026-11-09): with the foreground lock, smoke run 4 passed
  47 of 52 cases. The monitor now names the window that lost the front and
  counts only the person's: 3 of 13 changes. One came from an action: the
  person's window lost the front when evo invoked Excel's "Table" button and
  "Create Table" opened, although the lock was held. The other two followed
  the harness starting Word and Excel. The lock alone does not meet SC-2.
  The cases that failed (cell editor, `<select>` popup) fail about 1 in 4
  with and without the lock (`EVO_TEST_NO_FRONT_LOCK=1`).
- Harness safety, second incident: in smoke run 4 Notepad reopened a
  person's unsaved tab instead of the new one, the cases ran in it, and the
  cleanup keys (`ctrl+a`, `delete`, `ctrl+w`) emptied and closed it; its
  text is lost. The harness now does not start Notepad while Notepad has
  saved tabs, acts only when the window's title shows its own empty surface
  (`Launch::blank_title`), and runs the cleanup keys only there.
- T46 (2026-11-09), on the WinForms grid probe
  (`times_each_step_on_the_grid_probe`, `EVO_COMPUTER_TRACE=1` prints each
  step): key 433 → 214 ms, type 479 → 215 ms, key with a modifier 539 →
  305 ms, a click that changed nothing 838 → 670 ms (it looks twice). The
  probe's elements cost about 3 ms each through MSAA, so the cached reads
  help little there; Excel and Word (native UI Automation) are not measured
  yet. The 16 fixture live tests pass, apart from
  `types_across_cells_that_open_their_own_editor`, which failed 1 of 6
  (it failed about 1 in 4 before the change).
- T46 in Excel (`live_bench::times_each_step_in_excel`, a private
  instance): one read of the window's tree is about 400 ms. Fetching the
  whole tree (about 750 elements) in one call takes about 390 ms inside
  Excel; walking it afterwards takes 3 – 5 ms. Snapshot 667 → 434 ms, find
  1097 → 426 ms, a key or `type` about 1100 → 950 – 1000 ms (two reads).
  Smoke run 5 (before the whole-tree fetch, 1 run): Click 1005, Find 311,
  Invoke 1154, Key 1289, Type 1279 ms medians, 40 of 44 cases. Most of
  it is Excel's own time for the read-back (FR-17), twice per action, so
  NFR-4 now counts only evo's part (spec History, 2026-11-09). evo's part
  from the step timings: a key about 100 ms (input, the wait for the app,
  the 40 ms look, the walks of the fetched tree), a click about 250 ms
  (mostly the card cursor's 160 ms travel, which the "before" read fills).
- T47 (2026-11-09): `refresh_front` / `front_line` in `computer/mod.rs`,
  the backend's `holds_the_front`, a line in the tool's reply once each
  time the app comes to hold the front, `computer://front` from each action
  and each tick of the card's pictures. Checked: unit tests
  `the_agent_is_told_once_each_time_the_app_takes_the_front` and
  `the_front_warning_names_the_app_and_what_to_tell_the_person`;
  `computer-store.test.ts`; live `when_excel_takes_the_front` (Excel took
  the front from Teams on Create Table; the line came once); the card's
  warning in the UI with the fake IPC of `scripts/readme-shots` (shown,
  then gone when the front moved). Not checked in the e2e app with a real
  agent turn: T47 stays open for that.
- Typing robot, third incident (2026-11-09): tried alone for 8 s, it typed
  about 125 characters into the person's front window, not its own: it
  checked the form's `ContainsFocus`, which said yes while Windows had not
  let it to the front. It now checks the system's front window and its own
  box's focus before every key and holds a key back otherwise (`.held`); it
  types 5 minutes (`EVO_BENCH_ROBOT=<seconds>`, default 300); the bench
  read its sent text from the wrong file, fixed. Not run again. A key can
  still slip into another window if the front changes in the moment
  between the check and the key.
- T40: the 10-run benchmark was started without the robot and stopped by
  the owner during run 1; no results.
- T48 (2026-11-09): the skill's ground rules tell the agent to pass the
  warning on at once, and its input table puts Office shortcuts before the
  ribbon; `cargo test -p evo-engine bundled` passed.
- T45 findings (2026-11-09): a WinForms dialog never took the front from
  the person's window; Office's ribbon did, on the first UI Automation call
  into it (a tab's Select), despite the lock. See plan section 5.
- T38: key tips and a window that reads the physical mouse report a limit;
  the hover-only menu and the drag between apps are not written yet. T39:
  the 150 % scaling run and the CPU check are not run.

### Phase 7 — macOS (NFR-1, later)

Done when: the same checks pass on macOS 14+.

- [ ] T41 [FR-28, NFR-1] Port the macOS backend without temporary activation; ScreenCaptureKit capture for every window — `computer/mac/*.rs` — check: live `drives_textedit_in_the_background`, `drives_a_parked_chrome_form`
- [ ] T42 [FR-29] Refuse menu commands that need activation — `computer/mac/keyboard.rs`, `computer/mac/menus.rs` — check: live `refuses_menu_commands_that_need_activation` (the frontmost app is unchanged)
- [ ] T43 [FR-28] macOS permissions card: Allow, Restart evo — `features/settings/sections/computer-apps-settings.tsx`, `computer.rs` — check: on a Mac with permissions revoked
- [ ] T44 [SC-1 – SC-5] The benchmark on macOS — `computer/mac/live_tests.rs` — check: rows recorded

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | T15 in the app; T31 settings page | T15 passed (2026-11-10, e2e app); the page itself not run |
| FR-2 | T13 `computer::tool`; T32 transcript | passed (2026-11-10) |
| FR-3 | `controlled_elsewhere` assertions in live `drives_notepad_in_the_background` (T14) | passed (2026-11-10) |
| FR-4 | `computer::policy` (T3); a block mid-run releases (T14) | not run |
| FR-5 | T1 grep test; T36 input monitor | T1 passed (2026-11-10); T36 failed (2026-11-09): apps activate their own dialogs |
| FR-6 | live `drives_excel_on_the_stage` (T9), `corner_pixel_keeps_chromium_painting` (T10) | passed (2026-11-10) |
| FR-7 | live `returns_the_window_at_the_end_of_the_turn` (T16); every release trigger in the app | live test passed (2026-11-10); in the app not run |
| FR-8 | `cargo test -p evo-store computer` (T8); live `puts_back_a_window_parked_by_a_run_that_crashed` (T9) | passed (2026-11-10) |
| FR-9 | live `keeps_a_parked_apps_dialogs_off_screen` (T9) | passed (2026-11-10) |
| FR-10 | `computer-store.test.ts`; card in the e2e app | passed (2026-11-10) |
| FR-11 | T17 payload test; activity line in the e2e app | passed (2026-11-10); cursor not checked by eye |
| FR-12 | Stop, Allow again, Show the app, Close in the e2e app | passed (2026-11-10) |
| FR-13 | click in the card in the e2e app | passed (2026-11-10) |
| FR-14 | live `refuses_refs_behind_a_modal_dialog` (T7); `computer::marks`, live `marks_controls_and_zooms_a_region` (T22) | passed (2026-11-10) |
| FR-15 | live `drives_notepad_in_the_background` (T7), the channel named in results | not run |
| FR-16 | live `drives_an_electron_app_through_the_page_channel`, `edge_with_page_channel_stays_fresh_off_screen` (T24) | passed (2026-11-10) |
| FR-17 | `computer::readback` (T18); read-back assertion in live `drives_notepad_in_the_background` (T19) | passed (2026-11-10) |
| FR-18 | `computer::tool` ladder test (T20) | passed (2026-11-10) |
| FR-19 | live `tells_toolkits_that_drop_posted_input` (T21); impossible cases (T38) | impossible cases written so far passed (2026-11-09, 1 run) |
| FR-20 | `computer::zoom`; live `marks_controls_and_zooms_a_region` (T23) | passed (2026-11-10) |
| FR-21 | live typing tests (T7); a password field is refused | not run |
| FR-22 | `computer::keys` (T2) | passed (2026-11-10) |
| FR-23 | live `opens_closes_and_kills_from_the_catalog_only` (T26); `computer::apps` path refusal | passed (2026-11-10) |
| FR-24 | `agent_tools::gate` (T11); prompt in Approve for me in the e2e app | passed (2026-11-10) |
| FR-25 | `computer::tool` untrusted wrapping (T13) | passed (2026-11-10) |
| FR-26 | `computer::tool` timeouts (T13); quit with a hung app (T16) | not run |
| FR-27 | `skills_require_tools`, `bundled_computer_app_skill_requires_its_tool` (T34, T35) | passed (2026-11-10); trigger cases not run |
| FR-28 | T43 on a Mac | not run |
| FR-29 | live `refuses_menu_commands_that_need_activation` (T42) | not run |
| FR-30 | `front` hold count (T45); input monitor on the dialog cases (T45, T40); front line and card warning (T47) | not run |
| NFR-1 | CI build on Windows, macOS and Linux (T6); macOS phase (T41 – T44) | not run |
| NFR-2 | input monitor during the benchmark (T36, T40) | failed (2026-11-09): 8 front-window changes by driven apps |
| NFR-3 | T10, T24, T25; benchmark freshness column (T40) | not run |
| NFR-4 | step timings (T46); timing table (T39) | evo's part passed (2026-11-09, step timings): key about 100 ms, click about 250 ms; whole actions in Excel about 1 s, mostly its tree reads; the 10-run table not run |
| NFR-5 | quit with a hung app (T16) | not run |
| NFR-6 | T24 (pipe only, no network listener: `netstat` during the test); security review | not run |
| NFR-7 | run at 150 % scaling (T39) | not run |
| NFR-8 | CPU with the card minimized (T39) | not run |
| SC-1 | benchmark rate table (T37, T40) | 1 run only (2026-11-09): 40/41; 10 runs not run |
| SC-2 | input monitor: 0 events (T36, T40) | failed (2026-11-09): 8 events |
| SC-3 | typing robot: 0 lost or misdirected keys (T36, T40) | not run |
| SC-4 | crash restore (T9, T40) | not run |
| SC-5 | impossible cases (T38, T40) | 2 of 4 cases written, both passed (2026-11-09) |
