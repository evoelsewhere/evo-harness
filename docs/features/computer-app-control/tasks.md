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

- [ ] T11 [FR-24] Gate tests for `Class::Control`:
  - with `permission = ask`, it asks in `ask`, `auto-edits` and `auto`;
  - with `allow`, it follows the mode;
  - a refusal blocks the same action for the rest of the run;
  - `status`, `wait` and `detach` never ask.

  Path: `agent_tools/gate.rs`. Check: `bun run test:shell agent_tools::gate`.
- [ ] T12 [FR-24] `Class::Control` in the gate — `agent_tools/{gate,mod}.rs` — check: T11 passes
- [ ] T13 [FR-2, FR-25, FR-26] Tests for the batch runner (skip after failure, `detach` still runs), per-action approval summaries (`click e12`, `type "…" (240 chars) into e5`), untrusted wrapping, timeouts — `computer/mod.rs` — check: `bun run test:shell computer::tool`
- [ ] T14 [FR-2, FR-3, FR-25, FR-26] The `computer_app` tool: schema (plan 3.3), batch runner, one window per chat and one chat per window, results — `computer/mod.rs`, `agent_tools/mod.rs` — check: T13 passes; live `two_chats_cannot_share_a_window`
- [ ] T15 [FR-1] Settings keys, tool registered only while enabled, `configure_external_tools` re-sent on change, disabling releases every app — `agent_tools/mod.rs`, `computer.rs`, `stores/engine-store.ts` — check: in the app, the tool appears and disappears without a restart
- [ ] T16 [FR-7] Release on `agent_end`, chat close and quit (3 s cap, own threads) — `engine.rs`, `computer.rs`, `lib.rs` — check: live `returns_the_window_at_the_end_of_the_turn`; quit with a hung app returns within 3 s
- [ ] T17 [P] [FR-11] `computer_` activity kind and pointer events emitted before each pointer action — `agent_tools/activity.rs`, `computer/mod.rs` — check: unit test for the payload

### Phase 3 — Accuracy layer

Done when: FR-14 – FR-20 and FR-23 hold in unit and live tests.

- [ ] T18 [FR-17] Read-back diff tests on synthetic trees: focus, value, selection, a new dialog or menu, no change — `computer/readback.rs` — check: `bun run test:shell computer::readback`
- [ ] T19 [FR-17] Read-back wired into every acting action; `type` confirmation — `computer/{readback,mod}.rs` — check: T18 passes; live `reports_typing_that_did_not_arrive`
- [ ] T20 [FR-18, FR-19] Tests for the channel ladder (one step down, never real input) and the limits table (toolkits that drop posted input, hover-only, OLE drag, raw input) — `computer/mod.rs` — check: `bun run test:shell computer::ladder`
- [ ] T21 [FR-18, FR-19] Channel ladder and honest limits — `computer/mod.rs`, `computer/win/target.rs` — check: T20 passes; live `tells_toolkits_that_drop_posted_input`
- [ ] T22 [P] [FR-14] Set-of-marks: tests, then drawing ref boxes and labels on the screenshot — `computer/marks.rs` — check: `bun run test:shell computer::marks`
- [ ] T23 [P] [FR-20] `zoom`: tests, then crop ×2 and coordinate mapping — `computer/zoom.rs` — check: `bun run test:shell computer::zoom`
- [ ] T24 [FR-16, FR-23, NFR-6] Page channel for web-based apps evo starts:
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
- [ ] T26 [FR-23] `search_apps`, `open_app` (catalog only), `close_app`, `kill_app`, with refusals — `computer/win/lifecycle.rs`, `computer/mod.rs` — check: live `opens_closes_and_kills_from_the_catalog_only`; `open_app` with a path is refused

### Phase 4 — Card, settings and transcript

Done when: FR-1 and FR-10 – FR-13 hold in the running app (driven through
WebView2 DevTools), and `bun run typecheck` and `bun test` pass.

- [ ] T27 [FR-10, NFR-8] `Streams` keyed by surface id; `computer_stage`; `computer://frame` and `computer://pointer` — `browser/stage.rs`, `computer.rs` — check: `bun run test:shell browser::stage` still passes, plus a new surface test
- [ ] T28 [FR-10, FR-12] Tests, then `computer-store`, `CardKind` `"app"`, events in `agent-events`, removal in `cards-sync` — `stores/{computer-store,cards-store,agent-events,cards-sync}.ts`, `*.test.ts` — check: `bun test stores`
- [ ] T29 [FR-10, FR-11, FR-13] App card body: frames, `AgentCursor`, the person's input through `computer_input` — `features/cards/{app-canvas,card-body,stage-controller,stage-stream}.tsx/ts`, `computer.rs` — check: in the app, a card shows Notepad live and a click in the card reaches it
- [ ] T30 [FR-12] **Stop**, **Allow again**, **Show the app**, **Close** and their commands — `features/cards/card.tsx`, `computer.rs` — check: Stop during a long `type` stops within one character, and the next `attach` is refused until Allow again
- [ ] T31 [FR-1] **Settings → Computer apps**: switch, approval, app picker (`computer_list_apps`) — `features/settings/sections/computer-apps-settings.tsx`, `settings-registry.ts` — check: in the app
- [ ] T32 [P] [FR-2] Transcript kind `computer`, one line per action — `components/ai-elements/{tool-kinds.ts,tool.tsx}` — check: `bun run typecheck`, and a call renders in the app
- [ ] T33 [P] UI map: the app card and the settings section — `apps/desktop/README.md` — check: review

### Phase 5 — Skill

Done when: the skill is offered only to chats that have `computer_app`, and
its evaluation cases pass (FR-27).

- [ ] T34 [FR-27] Test, then bundled skills with `requires-tools` front matter offered only when the chat has those tools — `crates/evo-engine/src/` (skill listing) — check: `cargo test -p evo-engine skills_require_tools`
- [ ] T35 [FR-27] Port the skill and rewrite its input sections for the channel order, marks, read-back and limits; add evaluation cases — `crates/evo-engine/bundled-skills/computer-app-control/` — check: the cases trigger as listed in FR-27

### Phase 6 — Benchmark and verification on Windows

Done when: SC-1 – SC-5 and NFR-2 – NFR-8 are recorded in the verification
table. This phase includes spike 3 from [research.md](research.md): v3's
failing cases.

- [ ] T36 [NFR-2, SC-2, SC-3] Input monitor: samples the person's pointer, foreground, focus and key state every 10 ms during a run, and a typing robot in another app counts lost or misdirected keys — `computer/win/live_bench.rs` — check: it reports a planted `SetCursorPos` from the test itself
- [ ] T37 [SC-1] Port `live_bench` with 10 runs per case. Surface kinds: grid, text document, forms and dialogs, menus, evo-started web app, already-open web app. Include v3's failing cases (grid ctrl+click, ribbon toggle, right click, long typing) — `computer/win/live_bench.rs` — check: produces the per-case rate table
- [ ] T38 [P] [SC-5] "Impossible" cases (hover-only menu, drag a file between apps, key tips) expect an honest limit — `computer/win/live_bench.rs` — check: every case reports the limit
- [ ] T39 [NFR-4, NFR-7, NFR-8] Timing (median per action), a run at 150 % scaling, CPU with the card minimized — `computer/win/live_bench.rs` — check: numbers in the table
- [ ] T40 [SC-1 – SC-5] Run the benchmark and record the results — `tasks.md` section 2, plan section 5 — check: every row filled

### Phase 7 — macOS (NFR-1, later)

Done when: the same checks pass on macOS 14+.

- [ ] T41 [FR-28, NFR-1] Port the macOS backend without temporary activation; ScreenCaptureKit capture for every window — `computer/mac/*.rs` — check: live `drives_textedit_in_the_background`, `drives_a_parked_chrome_form`
- [ ] T42 [FR-29] Refuse menu commands that need activation — `computer/mac/keyboard.rs`, `computer/mac/menus.rs` — check: live `refuses_menu_commands_that_need_activation` (the frontmost app is unchanged)
- [ ] T43 [FR-28] macOS permissions card: Allow, Restart evo — `features/settings/sections/computer-apps-settings.tsx`, `computer.rs` — check: on a Mac with permissions revoked
- [ ] T44 [SC-1 – SC-5] The benchmark on macOS — `computer/mac/live_tests.rs` — check: rows recorded

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | T15 in the app; T31 settings page | not run |
| FR-2 | T13 `computer::tool`; T32 transcript | not run |
| FR-3 | live `two_chats_cannot_share_a_window` (T14) | not run |
| FR-4 | `computer::policy` (T3); a block mid-run releases (T14) | not run |
| FR-5 | T1 grep test; T36 input monitor | T1 passed (2026-11-10); T36 not run |
| FR-6 | live `drives_excel_on_the_stage` (T9), `corner_pixel_keeps_chromium_painting` (T10) | passed (2026-11-10) |
| FR-7 | live `returns_the_window_at_the_end_of_the_turn` (T16); every release trigger in the app | not run |
| FR-8 | `cargo test -p evo-store computer` (T8); live `puts_back_a_window_parked_by_a_run_that_crashed` (T9) | passed (2026-11-10) |
| FR-9 | live `keeps_a_parked_apps_dialogs_off_screen` (T9) | passed (2026-11-10) |
| FR-10 | T27 surface test; T29 in the app | not run |
| FR-11 | T17 payload test; cursor seen in the app (T29) | not run |
| FR-12 | T30 in the app | not run |
| FR-13 | T29 click in the card | not run |
| FR-14 | live `refuses_refs_behind_a_modal_dialog` (T7); `computer::marks` (T22) | not run |
| FR-15 | live `drives_notepad_in_the_background` (T7), the channel named in results | not run |
| FR-16 | live `drives_an_electron_app_through_the_page_channel`, `edge_with_page_channel_stays_fresh_off_screen` (T24) | not run |
| FR-17 | `computer::readback` (T18); live `reports_typing_that_did_not_arrive` (T19) | not run |
| FR-18 | `computer::ladder` (T20) | not run |
| FR-19 | live `tells_toolkits_that_drop_posted_input` (T21); impossible cases (T38) | not run |
| FR-20 | `computer::zoom` (T23) | not run |
| FR-21 | live typing tests (T7); a password field is refused | not run |
| FR-22 | `computer::keys` (T2) | passed (2026-11-10) |
| FR-23 | live `opens_closes_and_kills_from_the_catalog_only` (T26) | not run |
| FR-24 | `agent_tools::gate` (T11) | not run |
| FR-25 | `computer::tool` untrusted wrapping (T13) | not run |
| FR-26 | `computer::tool` timeouts (T13); quit with a hung app (T16) | not run |
| FR-27 | `skills_require_tools` (T34); evaluation cases (T35) | not run |
| FR-28 | T43 on a Mac | not run |
| FR-29 | live `refuses_menu_commands_that_need_activation` (T42) | not run |
| NFR-1 | CI build on Windows, macOS and Linux (T6); macOS phase (T41 – T44) | not run |
| NFR-2 | input monitor during the benchmark (T36, T40) | not run |
| NFR-3 | T10, T24, T25; benchmark freshness column (T40) | not run |
| NFR-4 | timing table (T39) | not run |
| NFR-5 | quit with a hung app (T16) | not run |
| NFR-6 | T24 (pipe only, no network listener: `netstat` during the test); security review | not run |
| NFR-7 | run at 150 % scaling (T39) | not run |
| NFR-8 | CPU with the card minimized (T39) | not run |
| SC-1 | benchmark rate table (T37, T40) | not run |
| SC-2 | input monitor: 0 events (T36, T40) | not run |
| SC-3 | typing robot: 0 lost or misdirected keys (T36, T40) | not run |
| SC-4 | crash restore (T9, T40) | not run |
| SC-5 | impossible cases (T38, T40) | not run |
