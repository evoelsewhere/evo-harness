# Quick entry — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-23 |
| ADD sections | [Desktop shell](../../architecture/README.md) |
| ADRs | none |

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

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | The shell adds a window, a shortcut and a hand-off; the main window starts the chat as **New chat** does. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | Only `apps/desktop` changes. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | No protocol change. |
| Models come from models.dev; no hardcoded model facts | n/a | No model facts. |
| Real engine in the app; settings and credentials reload live | ok | Settings apply at once (register / unregister the shortcut, build / close the window). |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | The box sends the person's own text through the same `sendPrompt` path; approvals are unchanged. The `quick-entry` window gets `core:default` only. |
| Desktop UI structure and `DESIGN.md` | ok | A separate small page; Geist, shadow-as-border card, no new colours. |

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

### 3.4 Data and storage

App settings (database, `evo_store::settings`): `system.quickEntry`
(bool, default true), `system.quickEntryShortcut` (string, default per
platform). In memory: the pending text for a main window being built, taken
once.

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

### 3.6 Errors and edge cases

- Shortcut taken by another app: the plugin's register fails; Settings shows
  "Another app uses this shortcut" and keeps the previous one.
- No monitor under the mouse (it moved off screen): the primary monitor.
- The main window is being built: the pending text waits for its page.

### 3.7 Security and trust

The text is the person's own, typed into an Evo window, and goes through the
same path as the message box. The `quick-entry` window has `core:default`
only (events, its own window); app commands are open to every window, as for
chat windows, and plugin views stay refused.

### 3.8 Configuration

| Key | Type | Default | Applies | UI |
| --- | --- | --- | --- | --- |
| `system.quickEntry` | bool | `true` | at once | Settings › General › Quick entry |
| `system.quickEntryShortcut` | string | `Alt+Shift+Space` | at once | Settings › General › Quick entry shortcut |

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Render the box inside `index.html` with a mode flag | Loads the whole app (stores, engine client, code editors) into an always-present hidden window: slow and heavy (NFR-1). |
| Build the window on demand | Its first show waits for the page to load; tray clicks would also build on the event loop thread (deadlock on Windows). |
| Start the chat from the box's own page | Only `engine-store` talks to the engine; a second engine client would duplicate chat creation and drafts. |
| Default `Alt+Space` on Windows | Windows' system menu shortcut and a common launcher key. |
| Default `Ctrl+Alt+Space` | Taken by another desktop assistant on the person's machine (registration failed in the first run). |

## 5. As-built notes

- The shell also keeps the hidden box out of the window count: closing the
  last real window with the tray off destroys the box too, so the app still
  exits as before; the box is never asked "Quit anyway?" and its close button
  (Alt+F4) only hides it (`system.rs`, `on_close_requested`, `request_quit`).
- Default shortcut `Alt+Shift+Space` (spec 6.3), not the per-platform
  defaults first planned.
