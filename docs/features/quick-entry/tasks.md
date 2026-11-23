# Quick entry — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-23 |

## 1. Phases

### Phase 1 — The box

Done when: the shortcut and the tray click open the box, Enter starts a new
chat in the main window, Esc and blur keep the text, and Settings turns it
off and changes the shortcut.

- [x] T1 [FR-2, FR-3, FR-4, FR-6] Unit tests for the pure decisions: tray click target, placement, pending text, default shortcut — `apps/desktop/src-tauri/src/quick_entry.rs` — check: `bun run test:shell quick_entry`
- [x] T2 [FR-1, FR-2, FR-4, FR-5, FR-6, FR-7, FR-9] Shell module: window, toggle, blur, global shortcut, settings, hand-off and commands; plugin, capability and wiring — `apps/desktop/src-tauri/src/quick_entry.rs`, `src/lib.rs`, `src/system.rs`, `Cargo.toml`, `capabilities/quick-entry.json` — check: `cargo check`, `bun run test:shell`
- [x] T3 [FR-3] Tray left click toggles the box while quick entry is on — `apps/desktop/src-tauri/src/system.rs` — check: T1's test; manual tray click
- [x] T4 [FR-5, FR-7, FR-8, FR-10, NFR-1] The box page — `apps/desktop/quick-entry.html`, `apps/desktop/src/quick-entry/`, `apps/desktop/vite.config.ts` — check: `bun run build`; manual check
- [x] T5 [FR-5, FR-6] Main window starts the chat from `app://quick-entry` and `quick_entry_pending` — `apps/desktop/src/stores/engine-store.ts` — check: manual check in the running app
- [x] T6 [FR-9] Settings › General: Quick entry switch and shortcut recorder — `apps/desktop/src/features/settings/sections/general-settings.tsx` — check: manual check in Settings
- [x] T7 UI map and features index — `apps/desktop/README.md`, `docs/features/README.md` — check: read

### Phase 2 — Shell: the box's chat, pin and hand-off

Done when: the shell keeps the box's chat, last use, pin and position,
decides on each show whether the box keeps its chat, hands a chat to the
main window with a target, and sends notice clicks for the box's chat to the
box (plan 3.2, 3.3, 3.5).

- [x] T8 [FR-4, FR-11, FR-17, FR-18] Failing unit tests for the pure decisions: `keep_chat` (inside and past the time, Never, running or waiting, row missing or archived), `window_height` with a monitor height (one line minimum, 60 % maximum), `pinned_placement` (position on a monitor, position on none) — `apps/desktop/src-tauri/src/quick_entry.rs` — check: `bun run test:shell quick_entry` fails for the right reason
- [x] T9 [FR-7, FR-11, FR-17, FR-18, FR-22] `QuickEntryState` gains `chat`, `last_used`, `pinned`, `position`; commands `quick_entry_chat`, `quick_entry_pin`, `quick_entry_show`; `quick-entry://shown` carries `{ chat, pinned }` from `keep_chat`; blur skips a pinned box; `Moved` stores a pinned position; resize clamps to the monitor — `quick_entry.rs`, `src/lib.rs` — check: T8 passes; `cargo check`
- [x] T10 [FR-6, FR-15] `quick_entry_open(chat, target)`; `system::open_in_main` takes an optional target, keeps it pending with the chat when the main window is rebuilt, and sends it in `app://open-chat`; unit test that a pending target is taken once with its chat — `quick_entry.rs`, `system.rs` — check: `bun run test:shell`
- [x] T11 [P] [FR-19] `notices::route` never picks `quick-entry`; `open_chat` emits `quick-entry://notice` when the box views the chat; unit tests on both decisions — `apps/desktop/src-tauri/src/supervisor/notices.rs` — check: `bun run test:shell notices`
- [x] T12 [FR-9, FR-18] `system.quickEntryNewChatAfter` (default 15, null = Never) in `SystemSettings` and `system_set`; unit test on the default and on null — `system.rs` — check: `bun run test:shell system`
- [x] T13 [P] [FR-12, FR-17] Box capability: `opener:default`, `core:window:allow-start-dragging`, `dialog:allow-ask`, `dialog:allow-message` — `apps/desktop/src-tauri/capabilities/quick-entry.json` — check: `cargo check` (capability schema)

### Phase 3 — Stores: a viewer mode for the box

Done when: the app's stores run in the box as a viewer of one changing chat,
a chat window still behaves as before, and the main window opens a chat at a
target.

- [x] T14 [FR-11, FR-16] `quickEntryWindow` and `viewerWindow` in `lib/windows.ts`; go through every `chatWindowId` guard and switch the ones that mean "not the main window" to `viewerWindow` (background chats, updates, split view, `app://open-chat`, `app://quick-entry`, sidebar, menus); keep the ones that mean "this window's one chat" — `src/lib/windows.ts`, `src/stores/engine-store.ts`, `src/stores/split-store.ts`, `src/app/app.tsx`, `src/app/use-app-bootstrap.ts`, `src/features/shell/chat-menu.tsx`, `src/features/shell/workspace-header.tsx`, `src/components/ai-elements/session-proposal.tsx` — check: `bun run typecheck`; a chat window opened with "Open in › New window" behaves as before (manual)
- [x] T15 [FR-5, FR-16] `newChat({ box: true })` (no draft row before the first message) and `session-store.reloadShownWorkspace()`; a test that the shown workspace is read again — `src/stores/engine-store.ts`, `src/stores/session-store.ts` (+ test) — check: `bun test session-store`; `bun run typecheck`
- [x] T16 [FR-12, FR-14, FR-15] The main window applies the `target` of `app://open-chat` (scroll to a question or turn, open a side panel tab with its item); in the box, `ui-store.openPanel` and opening a file or page in the panel call `quick_entry_open` with a panel target — `src/stores/engine-store.ts`, `src/stores/ui-store.ts`, `src/features/chat/chat-thread.tsx` — check: `bun run typecheck`; manual in Phase 6

### Phase 4 — Chat pieces the box can use

Done when: `features/chat` has a public entry with compact turns and compact
approval and question panels, and `answerable` is tested.

- [x] T17 [P] [FR-12, FR-13] `features/chat/index.ts` exporting `UserMessage`, `AssistantMessage`, `ApprovalPanel`, `QuestionsPanel`; `AssistantMessage` `compact`: every step one line (label, running mark, outcome, failure), no expansion, no turn changes or checkpoints; reply blocks unchanged — `src/features/chat/index.ts`, `chat-turn.tsx`, `turn-view.ts` (+ test for the one-line labels) — check: `bun test turn-view`; `bun run typecheck`
- [x] T18 [P] [FR-14] `ApprovalPanel` and `QuestionsPanel` `compact`: a one-line summary (file and change counts, command, address), the same choices, no diff — `src/features/chat/approval-panel.tsx`, `questions-panel.tsx` — check: `bun run typecheck`
- [x] T19 [P] [FR-14] Failing tests, then `answerable(question)` → `inBox` / `inBoxWithReview` / `elsewhere` (tool approval, edit, computer or app control, Webbridge, plugin consent, one and several `ask_user` questions, sign-in) — `src/features/quick-entry/answerable.ts`, `answerable.test.ts` — check: `bun test answerable`

### Phase 5 — The conversation in the box

Done when: Enter in the box answers in place with charts and cards,
follow-ups, step lines, short approvals and **Open in Evo**; the box hides,
pins, starts fresh after the set time and opens from a notification.

- [x] T20 [FR-11, FR-12, FR-13, FR-21, FR-22, NFR-3] `features/quick-entry/`: `quick-conversation.tsx` (header, compact turns, follow-up field with Stop, scrolling that follows the latest text within 48 px of the bottom, `<Activity mode="hidden">` while hidden, keyboard map), `box-header.tsx` (workspace, title, New chat, Open in Evo, Pin, Close, drag region when pinned), `box-bootstrap.ts` (sessions, appearance, connectors, customize), `index.ts` — check: `bun run typecheck`; `bun run build`
- [x] T21 [FR-14] `box-questions.tsx`: `answerable` picks the compact panel, the panel with **Review in Evo**, or "Evo needs you" with **Open in Evo** — `src/features/quick-entry/box-questions.tsx` — check: `bun run typecheck`
- [x] T22 [FR-5, FR-11, FR-18, NFR-1] The box page: import `@/features/quick-entry` on the first input or a shown chat; Enter / Ctrl+Enter / Ctrl+N / Ctrl+O per FR-5; `quick-entry://shown { chat, pinned }` shows the chat or empties the box keeping the text of the field; `quick_entry_chat` on send, reply end and New chat; fall back to `quick_entry_submit` when the import fails — `src/quick-entry/quick-entry.tsx`, `main.tsx` — check: `bun run build` (the first chunk of the box still holds no engine code); manual
- [x] T23 [FR-19] `quick-entry://notice`: `quick_entry_show`, or `quick_entry_open` with the open question when it is `elsewhere` — `src/features/quick-entry/quick-conversation.tsx` — check: manual
- [x] T24 [FR-20] Errors in the compact turn: the `TurnError` text and next step of the main window, plus **Open in Evo** — `src/features/chat/chat-turn.tsx`, `src/features/quick-entry/quick-conversation.tsx` — check: manual with no provider set up on scratch data
- [x] T25 [FR-9, FR-18] Settings › General: **Start a new chat after** (5, 15, 30, 60 minutes, Never) and its search keywords — `src/features/settings/sections/general-settings.tsx`, `src/features/settings/settings-registry.ts` — check: manual in Settings

### Phase 7 — Resizing (FR-23)

Done when: the box with a chat resizes by its edges and keeps the size.

- [x] T28 [FR-23] Unit tests for `is_users_resize` and `fixed_size`; the window resizable while it shows a chat, `user_size` and `expected` in `QuickEntryState`, `Resized` handling, `quick-entry://sized`, the size kept by `quick_entry_resize` and `place` — `apps/desktop/src-tauri/src/quick_entry.rs`, `src/lib.rs` — check: `bun run test:shell quick_entry`
- [x] T29 [FR-23] The page fills a resized window: card, conversation and turns stretch — `src/quick-entry/quick-entry.tsx`, `src/features/quick-entry/quick-conversation.tsx` — check: `bun run typecheck`; manual

### Phase 8 — Pictures and files (FR-24)

Done when: pasting or dropping a picture or file on the box attaches it and
it is sent with the message.

- [x] T30 [FR-24] Export the composer's attachment helpers from `features/chat`; `startChat` / `followUp` build the prompt with attachments (and steer while a turn runs) — `src/features/chat/index.ts`, `src/features/quick-entry/box.ts` — check: `bun run typecheck`
- [x] T31 [FR-24] The box page: paste and drop handling, chips with thumbnails and remove, sent with Enter / Ctrl+Enter — `src/quick-entry/quick-entry.tsx` — check: `bun run typecheck`; manual

### Phase 6 — Docs and verification

Done when: the UI map and the ADD describe the box as a viewer window, and
every new and changed requirement has a recorded check.

- [x] T26 UI map (the conversation of the box), ADD process view (the box among the windows that attach to engines), features index — `apps/desktop/README.md`, `docs/architecture/README.md`, `docs/features/README.md` — check: read
- [ ] T27 Run the verification table below on a scratch agent folder (the debug build migrates the real database) and record the results — `docs/features/quick-entry/tasks.md` — check: every row has a result

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Debug app on scratch data: a `quick-entry.html` page loads beside the main page at start | passed (2026-11-23) |
| FR-2 | Manual check: shortcut from another app toggles the box | not run — `Ctrl+Alt+Space` was taken by another app on the test machine; default moved to `Alt+Shift+Space`, not yet pressed |
| FR-3 | Unit test `a_tray_click_opens_the_box_only_while_quick_entry_is_on`; manual tray click | unit passed (2026-11-23); manual: the person's check passed (2026-11-23) |
| FR-4 | Unit test `the_box_opens_centred_in_the_upper_part_of_the_monitor`; the box sat at (620, 264) on a 1920×1200 monitor | passed (2026-11-23); two monitors not run |
| FR-5 | Manual check: Enter starts a new chat with the text | passed (2026-11-23, the person's check: the main window's engine started a run with the sent text) |
| FR-6 | Unit test `text_sent_without_a_main_window_is_taken_once`; manual check with the main window gone | unit passed (2026-11-23); manual not run |
| FR-7 | Manual check: Esc and blur keep the text | blur hid the box when another app took focus (2026-11-23); Esc not run by the agent |
| FR-8 | Unit test `the_box_grows_with_its_text_up_to_a_limit`; manual long text | unit passed (2026-11-23); manual not run |
| FR-9 | Settings reported `quickEntryShortcutOk: false` for the taken `Ctrl+Alt+Space`; recording a new shortcut | taken-shortcut report passed (2026-11-23); recording not run |
| FR-10 | Manual check: light and dark | not run |
| NFR-1 | `bun run build`: the quick entry page loads its own 2.4 kB entry plus shared React/style chunks, no engine code | passed (2026-11-23) |
| NFR-2 | Windows checked; macOS and Linux | Windows passed (2026-11-23); macOS, Linux not run |
| FR-4 (pinned) | Unit test `pinned_placement`; manual: pin, drag, hide, show | unit passed (2026-11-23); manual not run |
| FR-5 (Enter answers in the box) | Manual: Enter shows the reply in the box and the main window stays; Ctrl+Enter opens the main window; Enter and Ctrl+Enter on a chat | Enter: passed (2026-11-23, debug build on scratch data: the reply streamed in the box, the main window stayed); Ctrl+Enter not run |
| FR-6 (box without main window) | Manual: close the main window (tray on), ask in the box, then **Open in Evo** rebuilds it on the chat | not run |
| FR-7 (chat kept) | Manual: click away during a run; the run goes on; reopening shows it | not run |
| FR-11 | Unit test `window_height` with a monitor; manual: a long reply stops at 60 % and scrolls; scrolled up, it stays | unit passed; manual passed (2026-11-23, debug build on scratch data: header, turns, follow-up, Stop; the box grew and scrolled) |
| FR-12 | Manual: a reply with a list, code, a chart and a card is drawn in the box; the button of the card works; a file opens the main window on it | chart passed (2026-11-23, debug build on scratch data: a bar chart block drawn in the box); card button and file not run |
| FR-13 | `bun test turn-view` one-line labels; manual: one line per step, no output | unit passed; manual passed (2026-11-23, debug build on scratch data: "Read the block guide", "Ran echo …" as single lines) |
| FR-14 | `bun test answerable`; manual: an edit approval answered in the box settles in the main window; a three-part question shows **Open in Evo** only | unit passed; manual passed (2026-11-23, debug build on scratch data: an edit approval with Review in Evo, Enter in the box allowed it and it went from both windows; an app-control request showed "Evo needs you" with Open in Evo); 2026-11-23 change (the app's own requests answered in the box): unit passed, the box page ran the new rule; a live app-control prompt not seen again (already allowed on the scratch data) |
| FR-15 | Unit test on the pending target; manual: **Open in Evo** on a running chat shows it running in the main window | passed (2026-11-23, debug build on scratch data: Ctrl+O hid the box and the main window showed the chat with its chart) |
| FR-16 | Manual: a chat sent from the box shows in the sidebar with its title; a message sent in the main window shows in the box | box → sidebar passed (2026-11-23, debug build on scratch data); main → box not run |
| FR-17 | Manual: pin, click another app, the box stays; drag, hide, show at the same place | not run |
| FR-18 | Unit test `keep_chat`; manual: 5 minutes, wait 6, open: empty, the earlier chat in the sidebar | unit passed; reopening within the time kept the chat (2026-11-23, debug build on scratch data); the reset after the time not run |
| FR-19 | Unit tests on `route` and `open_chat`; manual: a long task, click away, click the notification | `route` unit passed (2026-11-23); manual not run |
| FR-20 | Manual on scratch data with no provider: the error and its next step in the box | passed (2026-11-23, debug build on scratch data: a provider 400 showed in the turn with Open in Evo) |
| FR-21 | Manual: keyboard only; a screen reader announces the finished reply | not run |
| FR-22 | Manual: open the box on a chat, type: the text goes in the follow-up field | not run |
| NFR-1 (answers) | `bun run build`: the first chunk of the box holds no engine code; manual: open time, first reply text at about the same moment as the main window | build passed (2026-11-23): the first chunk of the box has no engine code (engine-store loads only with the conversation chunk); manual not run |
| NFR-3 | Manual: one engine for the chat with both windows showing it; no drawing while the box is hidden (React Profiler) | not run |
| FR-10 (box margin) | Screen capture of the box | passed (2026-11-23, debug build on scratch data): no margin, the window is the card with the system's shadow and rounded corners |
| FR-23 | Unit tests `is_users_resize`, `box_size`; manual: resize the box with a chat, hide and show | unit passed (2026-11-23); manual passed (2026-11-23, debug build on scratch data: an outside resize was kept and the card filled the window; the person resized the box by its edge and it filled the new size) |
| FR-24 | Manual: paste a picture, drop a file, send; the reply and the chat show both | passed (2026-11-23, debug build on scratch data: a pasted picture showed as a chip, was sent and read ("RED"); a dropped text file was attached as a path and read ("4711"); a dropped PDF was attached and sent like any file. The agent's `document_preview` refuses files outside the workspace, as in the main window, so it read the PDF with a command); after the drop fix, a CDP drag of a PDF onto the box (`Input.dispatchDragEvent`) attached it; the person dragged a file from Explorer onto the box and it attached (2026-11-23) |
