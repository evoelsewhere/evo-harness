# Quick entry — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-23 |
| ADD sections | [Desktop shell](../../architecture/README.md) |
| ADRs | [0018 The shell hosts engines; windows attach to them as viewers](../../architecture/decisions/0018-the-shell-hosts-engines-windows-attach.md) |

The first part of each section is the box as built (FR-1 – FR-10). The
parts marked **Answers in the box** add FR-11 – FR-22 and NFR-3, and change
FR-4 – FR-7 and NFR-1 (spec History, 2026-11-23).

## 1. Context and constraints

- The tray, the main window's lifecycle and the "open this when the main
  window is built again" pattern live in
  `apps/desktop/src-tauri/src/system.rs` (`build_tray`, `show_main`,
  `open_in_main`, `app_open_pending`; shell-hosted-engines FR-11, FR-12).
  The tray's left click calls `show_main` today.
- App settings are keys in the app database read with `saved_flag`
  (`system.tray`, `system.keepAwake`) and changed through `system_set`;
  Settings › General (`src/features/settings/sections/general-settings.tsx`)
  shows them.
- A new chat is `engine-store.newChat()` (the workspace the sidebar shows,
  an untouched draft reused) and its first message `sendPrompt(text)`. Only
  `engine-store` talks to the engine, so the main window, not the box,
  starts the chat.
- Global shortcuts need `tauri-plugin-global-shortcut` (Tauri 2's official
  plugin). On Wayland global shortcuts may not register (NFR-2).
- On Windows a window built on the event loop's thread (a tray click)
  deadlocks: windows are built at setup or off that thread.
- The colour scheme is `localStorage["evo.theme"]` (`theme-provider.tsx`);
  windows of the app share it (one origin).

**Answers in the box** — what it builds on:

- Engines are hosted by the shell and windows are viewers
  ([ADR 0018](../../architecture/decisions/0018-the-shell-hosts-engines-windows-attach.md)).
  `engine_attach` / `engine_detach` record which chat each window shows
  (`supervisor.rs`, `viewers`, keyed by window label); every engine line is
  emitted to all windows as `engine://message` (`engine.rs`). The shell keeps
  open questions per chat and serves them to a window that attaches. Two
  windows can show one chat; an answer given in one settles it for both.
- Chat windows ("Open in › New window", `src/lib/windows.ts`,
  `window.rs::chat_window_open`) already run the whole app page as a viewer
  of one chat: `chatWindowId` makes `engine-store`, `split-store`, `app.tsx`
  and a few menus skip what only the main window does (background chats,
  updates, split view, the sidebar, `app://open-chat`). About fifteen guards
  read `chatWindowId`.
- `session-store` shares every saved chat row with the other windows
  (`CHAT_UPDATED`, `CHAT_REMOVED`), so a chat made in another window shows in
  the main window's sidebar at once.
- Replies are drawn by `components/markdown.tsx` and
  `components/reply-blocks/*` (charts, cards, tables, steps, diagrams,
  `widget-frame.tsx` for views), turns by `features/chat/chat-turn.tsx` with
  `turn-view.ts` (step lines, `runningStep`, `groupTitle`), approvals and
  questions by `features/chat/approval-panel.tsx` and `questions-panel.tsx`.
  All of it is ordinary React in the page; nothing needs a native child
  webview.
- Shell notices (`supervisor/notices.rs`) announce run ends and questions of
  chats no focused window shows, and `open_chat` sends a click to a non-main
  window that views the chat, else to the main window. `route` picks the
  window for bell entries: `main`, else the alphabetically first window,
  which could be `quick-entry`.
- The box today: `quick-entry.html` + `src/quick-entry/` (a field only,
  `core:default` permissions), 680 logical px wide, height clamped to
  84 – 320 px by `quick_entry::window_height`.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | The shell adds a window, a shortcut and a hand-off; the main window starts the chat as **New chat** does. **Answers in the box:** the box is one more viewer window (ADR 0018); it starts, steers and answers a chat through the same `engine-store` paths as any window. The shell only decides when the box keeps its chat (FR-18), where a notice click goes, and the window's pin and size. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | Only `apps/desktop` changes. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | No protocol change; `app://open-chat` (a shell → window event) gains an optional `target`. |
| Models come from models.dev; no hardcoded model facts | n/a | No model facts. |
| Real engine in the app; settings and credentials reload live | ok | Settings apply at once (register / unregister the shortcut, build / close the window, the new-chat time read on each show). The box talks to the real engine through the shell. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | The box sends the person's own text through `sendPrompt`. **Answers in the box:** approvals and questions use the engine's own choices and the shell's question store; the box only hides some behind **Open in Evo** (FR-14) and never answers anything itself. Reply content (views in `widget-frame`) keeps its sandbox. The box's capability grows to what chat windows have that it uses (3.7). |
| Desktop UI structure and `DESIGN.md` | ok | The box conversation is a feature (`features/quick-entry/`) that uses `features/chat` only through a new public `index.ts`, and talks to the engine only through `engine-store`. Same tokens, shadow-as-border card, no new colours. |

No ADR: the box is a viewer window as ADR 0018 already allows; no new
process, plane, store or trust boundary.

## 3. Design

### 3.1 Overview

```text
shortcut / tray click ──▶ quick_entry::toggle ──▶ position on monitor under mouse, show, focus
                                                     │
quick-entry.html (React, tiny) ── Enter ──▶ quick_entry_submit(text)
                                                     │ hide box
                                                     ├─ main window exists → emit_to(main, "app://quick-entry", {text}) + show_main
                                                     └─ none → pending = text, show_main (built again)
main window (engine-store) ── listen / quick_entry_pending ──▶ newChat() → sendPrompt(text)
```

The flow above is now **Ctrl+Enter**. **Answers in the box** — Enter:

```text
 quick-entry.html  ── field (instant, as today) ──────────────┐
   first keystroke: import("@/features/quick-entry")          │  NFR-1: loads while typing
   ┌──────────────────────────────────────────────────────────▼───────────┐
   │ QuickConversation  (engine-store + session-store in "box" mode)        │
   │   Enter: newChat({ box: true }) → sendPrompt(text)  ── engine_attach ──┼──▶ shell supervisor
   │   turns: chat-turn (compact) · markdown + reply-blocks · step lines    │      viewers["quick-entry"] = chat
   │   questions: answerable(q) → inline panel | "Evo needs you"           │◀── engine://message (all windows)
   │   header: New · Open in Evo · Pin · Close                              │
   └─────────────┬───────────────────────────────┬──────────────────────────┘
                 │ quick_entry_chat(chat, used)   │ quick_entry_open({chat, target})
                 ▼                                ▼
       shell QuickEntryState              hide box; system::open_in_main(chat, target)
       (chat, last_used, pinned,               └─▶ main: app://open-chat → selectSession(chat), go to target
        position) → keep_chat() on show
```

The box stays one small page that shows at once. Typing its first character
starts loading the conversation module, which brings `engine-store`, the
chat renderer and the reply blocks into the box, in a **box mode** much like
a chat window's. From then on the box is a viewer of one chat at a time: it
makes the chat, attaches to its engine, streams it, answers what it may, and
hands it to the main window on request. The shell keeps the few facts that
must outlive the box's page and decide things while it is hidden: which chat
it holds, when it was last used, the pin and the position.

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| desktop shell | `apps/desktop/src-tauri/src/quick_entry.rs` | The window (build, toggle, place, hide on blur), the global shortcut, settings, the hand-off to the main window, pure decisions with unit tests. | FR-1–FR-7, FR-9 |
| desktop shell | `apps/desktop/src-tauri/src/system.rs` | Tray left click asks `quick_entry` whether to toggle the box; settings carry `quickEntry`, `quickEntryShortcut`. | FR-3, FR-9 |
| desktop shell | `apps/desktop/src-tauri/src/lib.rs`, `Cargo.toml`, `capabilities/quick-entry.json` | Plugin, state, commands, window events, the window's permissions. | FR-1, FR-2 |
| desktop UI | `apps/desktop/quick-entry.html`, `src/quick-entry/` | The box: auto-growing field, Enter / Shift+Enter / Esc, theme. | FR-5, FR-7, FR-8, FR-10, NFR-1 |
| desktop UI | `apps/desktop/vite.config.ts` | Second page in the build. | NFR-1 |
| desktop UI | `src/stores/engine-store.ts` | `app://quick-entry` and `quick_entry_pending` start a new chat with the text. | FR-5, FR-6 |
| desktop UI | `src/features/settings/sections/general-settings.tsx` | Quick entry switch and shortcut recorder. | FR-9 |

**Answers in the box:**

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| desktop shell | `src-tauri/src/quick_entry.rs` | `QuickEntryState` gains `chat`, `last_used`, `pinned`, `position`. New pure functions with unit tests: `keep_chat`, `window_height` (monitor-relative limit), `pinned_placement`. Commands `quick_entry_chat`, `quick_entry_open`, `quick_entry_pin`; `quick_entry://shown` carries the chat to show; blur skips a pinned box; `Moved` records a pinned position; `show_chat` for notice clicks. | FR-4, FR-6, FR-7, FR-11, FR-15, FR-17, FR-18, FR-19 |
| desktop shell | `src-tauri/src/system.rs` | `open_in_main` takes an optional target and passes it in `app://open-chat`; `SystemSettings` gains `quickEntryNewChatAfter`; `system_set` takes it. | FR-9, FR-15, FR-18 |
| desktop shell | `src-tauri/src/supervisor/notices.rs` | `route` never picks `quick-entry` for the bell; `open_chat` sends a click for the box's chat to the box (`quick-entry://notice`), which decides between itself and the main window. | FR-19 |
| desktop shell | `src-tauri/capabilities/quick-entry.json` | Adds what the conversation uses: `opener:default` (links), `core:window:allow-start-dragging` (pinned header), `dialog:allow-ask`/`allow-message` (a page's own dialogs, as chat windows). | FR-12, FR-17 |
| desktop UI | `src/lib/windows.ts` | `quickEntryWindow` (true in the box) and `viewerWindow` (a chat window or the box): the "not the main window" test the guards use. | FR-11, FR-16 |
| desktop UI | `src/stores/engine-store.ts`, `split-store.ts`, `ui-store.ts` | Guards that mean "not the main window" read `viewerWindow`; the box's own chat follows `quick-entry://shown` instead of `chatWindowId`. `newChat` gains `{ box: true }` (no draft row in the sidebar until the first message, workspace re-read from the database). In the box, side panel requests (`ui-store.openPanel`, and opening a file or page in the panel) become `quick_entry_open` with that target. | FR-5, FR-12, FR-15, FR-16 |
| desktop UI | `src/stores/session-store.ts` | `reloadShownWorkspace()` reads `sidebar.workspace` again (the main window may have changed it since the box loaded). | FR-5 |
| desktop UI | `src/features/chat/index.ts` (new), `chat-turn.tsx`, `approval-panel.tsx`, `questions-panel.tsx` | A public entry: `UserMessage`, `AssistantMessage` with a `compact` option (steps as single lines, no expansion, no turn changes or checkpoints), `ApprovalPanel` and `QuestionsPanel` with a `compact` option (one-line summary, no diff). | FR-12, FR-13, FR-14 |
| desktop UI | `src/features/quick-entry/` (new): `quick-conversation.tsx`, `box-header.tsx`, `box-questions.tsx`, `answerable.ts` (+ test), `box-bootstrap.ts`, `index.ts` | The conversation: header, turns, follow-up field with Stop, scrolling that follows the latest text, "Evo needs you", keyboard map, size reporting; `answerable(question)` → `inBox` / `inBoxWithReview` / `elsewhere`; a slim bootstrap (sessions, appearance, connectors, customize — what a chat window loads, minus updates and split view). | FR-11 – FR-14, FR-20 – FR-22, NFR-3 |
| desktop UI | `src/quick-entry/quick-entry.tsx`, `main.tsx` | The field stays; the first keystroke (or a shown chat) imports `@/features/quick-entry`; Enter / Ctrl+Enter / Ctrl+N / Ctrl+O routed per FR-5; the page renders the conversation under the field once a chat exists. | FR-5, FR-11, NFR-1 |
| desktop UI | `src/features/settings/sections/general-settings.tsx`, `settings-registry.ts` | **Start a new chat after** select (5, 15, 30, 60 min, Never). | FR-9, FR-18 |
| docs | `apps/desktop/README.md`, ADD process view | The box as a viewer window in the UI map and in the list of windows that attach to engines. | — |

### 3.3 Interfaces

Tauri commands:

```rust
quick_entry_submit(text: String) -> ()      // the box: send, hide, hand to the main window
quick_entry_hide() -> ()                     // the box: Esc
quick_entry_resize(height: f64) -> ()        // the box: field grew (logical px)
quick_entry_pending() -> Option<String>      // main window, once loaded: text sent while it was gone
system_set_shortcut(shortcut: String) -> SystemSettings   // Settings: record a shortcut
```

`system_set` takes the new key `quickEntry`. `SystemSettings` gains
`quickEntry: bool`, `quickEntryShortcut: String`,
`quickEntryShortcutOk: bool` (false when the shortcut could not be
registered).

Events: `quick-entry://shown` to the box (focus the field, re-read the
theme); `app://quick-entry` `{ text }` to the main window.

Shortcut strings use the global-shortcut plugin's syntax
(`Ctrl+Alt+Space`, `Alt+Space`, `CommandOrControl+Shift+K`).

**Answers in the box:**

```rust
// The box holds `chat` (None: empty). `used`: a message sent or a reply
// finished now. The shell keeps it for `keep_chat` (FR-18).
quick_entry_chat(chat: Option<String>, used: bool) -> ()
// Hide the box and show `chat` in the main window, at `target` (FR-15, FR-12, FR-14).
quick_entry_open(chat: String, target: Option<OpenTarget>) -> ()
// Pin or unpin; unpinning forgets the position (FR-17).
quick_entry_pin(pinned: bool) -> ()
// Show the box (after a notice click the box chose to answer itself).
quick_entry_show() -> ()
```

```ts
type OpenTarget =
  | { kind: "question"; id: string }          // Review in Evo, Evo needs you
  | { kind: "turn"; id: string }               // a block that opens the main window
  | { kind: "panel"; panel: PanelId; item?: string };  // side panel: a file path or a page address
```

Events:

- `quick-entry://shown` `{ chat: string | null, pinned: boolean }` — the
  chat the box shows on this opening (null: empty, FR-18), and the pin.
- `quick-entry://notice` `{ chat: string }` — a notification of the box's
  chat was clicked; the box calls `quick_entry_show` or, when its open
  question is `elsewhere`, `quick_entry_open` (FR-19).
- `app://open-chat` `{ chat, target? }` — `target` is new and optional; a
  main window without it behaves as before.

`SystemSettings` gains `quickEntryNewChatAfter: number | null` (minutes;
null is Never); `system_set` takes the key `quickEntryNewChatAfter`.

### 3.4 Data and storage

App settings (database, `evo_store::settings`): `system.quickEntry`
(bool, default true), `system.quickEntryShortcut` (string, default per
platform). In memory: the pending text for a main window being built, taken
once.

**Answers in the box:**

- New app setting `system.quickEntryNewChatAfter` (number of minutes or
  null for Never; default 15). No migration: settings are key/value rows.
- In memory in the shell (`QuickEntryState`), lost on quit (A-6): `chat`,
  `last_used` (ms), `pinned`, `position` (physical px). The box's page holds
  the same chat in its stores; the shell's copy decides on each show.
- The chat itself is an ordinary chat row and session, saved by the engine
  and `session-store` as any chat (FR-16). Nothing new on disk.

### 3.5 Behaviour

- **Start.** With quick entry on: build the hidden window, register the
  shortcut (a failure is logged and reported in settings, not fatal).
- **Toggle.** Shown and focused → hide. Otherwise place it (centre of the
  monitor under the mouse horizontally, 22 % from its top) and show, focus,
  emit `quick-entry://shown`. Nothing while quitting.
- **Blur.** `WindowEvent::Focused(false)` on `quick-entry` hides it, except
  within 300 ms of showing (the tray click's own focus change).
- **Submit.** Trimmed text; blank does nothing. Hide, then hand off as in
  3.1. The box clears its field after the call returns.
- **Main window.** On `app://quick-entry` (main window only): close Settings,
  Customize, Scheduler, Usage and plugin pages, `newChat()`, `sendPrompt`.
  On load, `quick_entry_pending` wins over the last chat.
- **Settings.** Off: unregister the shortcut, close the window, tray click
  shows the main window. On: build and register. New shortcut: register it
  first; on success unregister the old one and save; on failure keep the old.

**Answers in the box:**

- **Load (NFR-1).** The field page renders as today. Its first `input`
  event, or a `shown` with a chat, starts `import("@/features/quick-entry")`
  and its bootstrap (sessions, appearance, connectors, customize). Enter
  waits for that promise, which has usually settled by then; the engine
  start that follows is the same as in the main window, so the reply's first
  text arrives at the same time (NFR-1's check).
- **Send (FR-5).** Enter in the empty box: `reloadShownWorkspace()`,
  `newChat({ box: true })`, `sendPrompt(text)`, `engine_attach` as the
  `quick-entry` window, `quick_entry_chat(chat, used: true)`. Enter on a
  chat: `sendPrompt` (queued while running, as the main window does).
  Ctrl+Enter on the empty box: `quick_entry_submit` (unchanged). Ctrl+Enter
  on a chat: `sendPrompt`, then `quick_entry_open(chat)`.
- **Conversation (FR-11 – FR-13).** The page lays out header, turns and
  field in one card; its `ResizeObserver` reports the card's height, and the
  shell clamps it to between the one-line height and 60 % of the monitor's
  work area (`window_height(height, monitor_height)`), then the turns scroll.
  The turns follow the latest text while the person is within 48 px of the
  bottom. Turns use the compact chat renderer; reply blocks are the app's
  own, laid out at the box's width.
- **Questions (FR-14).** `answerable(question)`:
  - a tool approval → `inBox`, except an edit → `inBoxWithReview`;
    the app's own requests (computer or app control, the person's browser,
    plugin consent: `shell:` request ids) with choices → `inBox`;
  - `ask_user` with exactly one question → `inBox`; more → `elsewhere`;
  - anything else (a prompt without choices) → `elsewhere`.
  `inBox` draws the compact approval / question panel; `elsewhere` draws
  "Evo needs you" with **Open in Evo** (`target: question`). Answers go
  through `engine-store` as in the main window; the shell settles the
  question for every viewer.
- **Open in Evo (FR-15, FR-12).** `quick_entry_open(chat, target)` hides the
  box (a pinned one too) and calls `open_in_main(chat, target)`; the main
  window selects the chat and scrolls to the question or turn, or opens the
  side panel tab. The box keeps the chat and stays attached.
- **Hide (FR-7, NFR-3).** Esc, Close, the shortcut, or blur when not pinned
  hide the window. The page wraps the conversation in React `<Activity
  mode="hidden">` while hidden, so streamed text updates the stores but is
  not drawn until the box shows again.
- **Show (FR-18, FR-22).** On every show the shell computes
  `keep_chat(chat_exists_and_not_archived, busy_or_waiting, last_used, now,
  after)` — busy or waiting comes from the supervisor's work for the chat —
  and emits `shown { chat | null, pinned }`. Null: the box detaches, empties
  its turns, keeps the field's text and focuses it. A chat: the box shows it
  (attaching again if it had let go) and focuses the follow-up field.
  **New chat** / Ctrl+N: detach, empty, `quick_entry_chat(None, false)`.
- **Pin (FR-17, FR-4).** `quick_entry_pin(true)` sets `pinned`; blur no
  longer hides; the header is a drag region; `WindowEvent::Moved` stores the
  position while pinned. A show uses the stored position if a monitor still
  contains it, else the usual placement. Unpin forgets the position.
- **Notices (FR-19).** The box is a viewer of its chat, so the shell already
  announces its run ends and questions when the box is not focused (hidden
  counts as not focused). A click: `open_chat` sees the `quick-entry`
  viewer and emits `quick-entry://notice`; the box checks the chat's open
  question with `answerable` and calls `quick_entry_show` or
  `quick_entry_open(chat, target)`. Bell entries never route to the box.
- **No main window (FR-6).** Nothing in the box needs it. `open_in_main`
  already rebuilds it and keeps the chat pending; the target is kept with it.

### 3.6 Errors and edge cases

- Shortcut taken by another app: the plugin's register fails; Settings shows
  "Another app uses this shortcut" and keeps the previous one.
- No monitor under the mouse (it moved off screen): the primary monitor.
- The main window is being built: the pending text waits for its page.

**Answers in the box:**

- The conversation module fails to load: the box falls back to the
  Ctrl+Enter path for that message (the main window answers) and logs the
  error.
- Engine or provider error (FR-20): the compact turn shows the same
  `TurnError` / `explain-error` text the main window shows, plus **Open in
  Evo**.
- The box's chat is archived or deleted elsewhere: `CHAT_REMOVED` or an
  archived row empties the box at once; `keep_chat` also checks the row.
- The engine restarts or the shell stops it while idle: the box attaches
  again when it shows the chat, as the main window does on selecting it.
- Monitor removed while pinned: the stored position is outside every
  monitor; the usual placement is used.
- Main window and box both show the chat: both receive `engine://message`;
  `notices::wanted` stays false while either is focused.
- The person types in the box while the main window answers a question of
  the same chat: the shell's question settles in both; the box's panel goes.

### 3.7 Security and trust

The text is the person's own, typed into an Evo window, and goes through the
same path as the message box. The `quick-entry` window has `core:default`
only (events, its own window); app commands are open to every window, as for
chat windows, and plugin views stay refused.

**Answers in the box:** the box now drives chats, so it holds the same power
as a chat window: it sends prompts and answers approvals. It gains only the
plugin permissions the conversation uses (`opener:default` for links,
`core:window:allow-start-dragging`, `dialog:allow-ask` and
`dialog:allow-message`); not `opener:allow-open-path`, notifications or the
folder picker. Approvals are never answered without the person's click, and
an edit's approval always shows the file and offers the full diff (SC-6).
Views inside replies run in `widget-frame`'s sandbox as in the main window.

### 3.8 Configuration

| Key | Type | Default | Applies | UI |
| --- | --- | --- | --- | --- |
| `system.quickEntry` | bool | `true` | at once | Settings › General › Quick entry |
| `system.quickEntryShortcut` | string | `Alt+Shift+Space` | at once | Settings › General › Quick entry shortcut |
| `system.quickEntryNewChatAfter` | number (minutes) or null | `15` | next show | Settings › General › Start a new chat after |

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Render the box inside `index.html` with a mode flag | Loads the whole app (stores, engine client, code editors) into an always-present hidden window: slow and heavy (NFR-1). |
| Build the window on demand | Its first show waits for the page to load; tray clicks would also build on the event loop thread (deadlock on Windows). |
| Start the chat from the box's own page | Only `engine-store` talks to the engine; a second engine client would duplicate chat creation and drafts. *(Superseded for Enter: the box now loads `engine-store` itself, see below.)* |
| Default `Alt+Space` on Windows | Windows' system menu shortcut and a common launcher key. |
| Default `Ctrl+Alt+Space` | Taken by another desktop assistant on the person's machine (registration failed in the first run). |
| Answers: a small engine client and renderer written for the box | Duplicates chat creation, questions, queuing and every reply block; breaks "only `engine-store` talks to `engine/client.ts`"; FR-12 (charts and cards) would mean copying `reply-blocks`. |
| Answers: load the whole app page in the box from the start | Breaks NFR-1 (100 ms open) for the common case of a box that is opened and closed. Loading on the first keystroke gives the same reuse without the cost. |
| Answers: the box is a chat window (`chat-<id>`) shown small | Chat windows own their chat: the main window focuses them instead of showing the chat, against FR-15 / FR-16; their label and page are tied to one chat, while the box changes chat (FR-18). |
| Answers: the main window answers and the box mirrors it | Requires the main window (FR-6 says it may be gone) and a mirroring channel; the shell already broadcasts engine lines to every window. |
| Answers: classify questions in the shell | The box receives live questions from engine lines, not only from the shell's store, so it needs the classification itself; one TypeScript function decides both the panel and the notice click. |

## 5. As-built notes

- The shell also keeps the hidden box out of the window count: closing the
  last real window with the tray off destroys the box too, so the app still
  exits as before; the box is never asked "Quit anyway?" and its close button
  (Alt+F4) only hides it (`system.rs`, `on_close_requested`, `request_quit`).
- Default shortcut `Alt+Shift+Space` (spec 6.3), not the per-platform
  defaults first planned.

**Answers in the box** (2026-11-23):

- **Start a new chat after** is set with its own command,
  `system_set_new_chat_after(minutes | null)`, not `system_set` (which takes
  a switch's on/off).
- `app://open-chat` is now sent to the main window only (`emit_to`) and
  carries `OpenRequest { chat, target? }`; `app_open_pending` returns the
  same shape (it returned the chat id). The main window keeps the last target
  in `engine-store.openTarget`; the thread scrolls to it, and a Files target
  opens the file.
- The shell also emits `quick-entry://hidden` when it hides the box, so the
  page can stop drawing (`<Activity mode="hidden">`, NFR-3).
- No `newChat({ box: true })`: drafts already live only in the window that
  makes them, so the box uses `newChat()` as is and the chat reaches the
  sidebar with its first message. `engine-store.letGo()` lets the box drop
  its chat (detach; an untouched draft is discarded).
- The box reads `ui-store`'s saved state but never writes it (a read-only
  storage), so it cannot overwrite what the main window saved since.
  Side panel requests go through `forwardPanelsTo` / `forwardPanel` in
  `ui-store.ts` (also `files-store.open`).
- An approval answered in the box takes the follow-up field's place, as it
  takes the composer's in the main window: Enter and Esc answer it there, so
  Esc does not hide the box while one shows.
- `QuestionsPanel` is used as it is (no compact form): one question already
  fits the box.
- `box-header.tsx`, `box-questions.tsx` and `box-bootstrap.ts` became parts of
  `quick-conversation.tsx` and `box.ts`.
- `window_set_theme` leaves the box alone: the app's `ThemeProvider`, which
  the conversation uses, asked it to put Mica on the calling window, and
  Mica filled the box's transparent margin (seen in the first trial).
- The header drags the box whether it is pinned or not (spec FR-17, changed
  after the first trial); only a pinned box keeps its position.
- The notice click (`notices::open_chat` → `quick_entry::notice_clicked`)
  and the pending target are glue without a unit test of their own; `route`
  has one.
