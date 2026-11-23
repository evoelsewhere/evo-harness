# Quick entry — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop shell, desktop UI |
| Created | 2026-11-23 |
| Updated | 2026-11-23 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Shell-hosted engines](../shell-hosted-engines/spec.md) (FR-11, FR-12: the tray and the app's lifecycle) |

## 1. Summary

Quick entry is a small floating message box that opens over whatever the
person is doing, from the tray icon or a keyboard shortcut that works in any
app. The person types a request and presses Enter: Evo starts a new chat with
it and brings the main window forward on that chat. The box goes away with
Esc or a click elsewhere, keeping what was typed.

## 2. Scope

### 2.1 Goals

- Start a chat from anywhere in one step, without finding the Evo window.
- Open from the tray icon (left click) and from a global shortcut.
- Appear at once: the box is ready before it is asked for.
- Never lose a half-typed request when the box is dismissed.

### 2.2 Non-goals

- Answers inside the box. The chat runs in the main window, as any chat.
- Choosing the agent, workspace, model or approval mode in the box: the new
  chat starts as the main window's **New chat** would.
- Attachments, screenshots or dictation in the box.
- Slash commands and mentions in the box: the text is sent as typed.

## 3. User scenarios

### US-1 Ask from anywhere (priority P1)

A person working in another app wants to hand Evo a task without switching
windows first, so that asking costs no more than a keystroke.

Acceptance scenarios:

1. **Given** Evo runs (window open, hidden or in the tray), **when** the
   person presses the quick entry shortcut, **then** the box appears in the
   upper middle of the screen under the mouse, with the caret in it.
2. **Given** the box is open with text, **when** the person presses Enter,
   **then** the box hides, the main window comes forward on a new chat in the
   workspace the sidebar shows, and the text is sent as its first message.
3. **Given** the box is open, **when** the person presses the shortcut again,
   **then** the box hides.

### US-2 Ask from the tray (priority P1)

A person who keeps Evo in the tray wants the tray icon to open the box, so
that the icon is a way to ask, not only a way to find the window.

Acceptance scenarios:

1. **Given** quick entry and the tray icon are on, **when** the person left
   clicks the icon, **then** the box opens.
2. **Given** quick entry is off, **when** the person left clicks the icon,
   **then** the main window comes forward, as before.
3. **Given** quick entry is on, **when** the person picks **Open Evo** in the
   tray's menu, **then** the main window comes forward.

### US-3 Dismiss without losing text (priority P2)

A person who changed their mind wants the box gone without losing what they
typed, so that they can come back to it.

Acceptance scenarios:

1. **Given** the box has text, **when** the person presses Esc or clicks
   outside it, **then** the box hides; **when** it opens again, **then** the
   text is still there.

### US-4 Turn it off or change the shortcut (priority P2)

A person whose shortcut clashes with another app, or who does not want the
box, wants to change or switch it off.

Acceptance scenarios:

1. **Given** Settings › General, **when** the person turns Quick entry off,
   **then** the shortcut does nothing in any app and a tray click opens the
   main window.
2. **Given** Settings › General, **when** the person records a new shortcut,
   **then** the new one opens the box and the old one no longer does.
3. **Given** another app holds the shortcut, **when** the person records it,
   **then** Evo says it could not use it and keeps the shortcut it had.

### Edge cases

- The main window was destroyed (closed with the tray on after a rebuild):
  it is built again and starts the new chat once its page has loaded.
- Enter on empty or blank text does nothing; Shift+Enter adds a line.
- Several monitors: the box opens on the monitor under the mouse.
- The app is quitting: the box does not open.
- The saved shortcut cannot be registered at start (another app took it):
  Evo starts without it, logs a warning and Settings shows the shortcut as
  unavailable; the tray click still opens the box.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | A quick entry window exists while quick entry is on: frameless, transparent, always on top, not in the taskbar, hidden until asked for, built at start so it opens without loading. | Given quick entry is on, when the app starts, then a hidden `quick-entry` window exists. |
| FR-2 | A global shortcut toggles the box from any app: it opens it when hidden, hides it when shown. Default `Alt+Shift+Space` (Option+Shift+Space on macOS). | Given another app is focused, when the shortcut is pressed, then the box shows and has focus; pressed again, it hides. |
| FR-3 | A left click on the tray icon toggles the box while quick entry is on; with it off the click shows the main window. The tray menu's **Open Evo** always shows the main window. | Unit test on the click decision; manual check on the tray. |
| FR-4 | The box opens centred horizontally on the monitor under the mouse, in its upper part, with the caret in the text field. | Unit test on the position; manual check on two monitors. |
| FR-5 | Enter sends the trimmed text when it is not blank: the box hides and clears, the main window comes forward on a new chat (as **New chat**: the workspace the sidebar shows, an untouched draft reused) and the text is its first message. Shift+Enter adds a line. | Given the box with "hello", when Enter is pressed, then the main window shows a new chat whose first message is "hello". |
| FR-6 | When there is no main window, sending builds it again and the new chat starts once its page has loaded. | Unit test on the pending text; manual check. |
| FR-7 | Esc, or the box losing focus, hides it and keeps the text for the next time it opens. | Manual check. |
| FR-8 | The field grows with the text up to a limit, then scrolls. | Manual check with a long text. |
| FR-9 | Settings › General has a **Quick entry** switch (default on) and the shortcut, which the person can record anew. A shortcut another app holds is refused with a message and the old one stays. Changes apply at once, without restarting. | Manual check in Settings. |
| FR-10 | The box follows the app's light or dark theme. | Manual check in both themes. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | The box shows within 100 ms of the shortcut or click: its page is loaded while hidden and does not load the main app. | Manual check; the quick entry page bundle holds no chat or engine code. |
| NFR-2 | Works on Windows, macOS and Linux (X11). Wayland may refuse global shortcuts; the tray click still works there. | Manual check per platform. |

## 5. Success criteria

- SC-1 A person starts a chat from another app with the shortcut, a request
  and Enter, without touching the Evo window.
- SC-2 Dismissing the box never loses typed text.
- SC-3 Turning quick entry off leaves the tray and the app as they were
  before this feature.

## 6. Clarifications

### 6.1 Open

n/a — no open questions.

### 6.2 Resolved

- 2026-11-23 — How does the box open? Both a left click on the tray icon and
  a global shortcut; the tray menu keeps **Open Evo** for the main window.
  (The person's choice.)
- 2026-11-23 — What does Enter do? Start a new chat with the text and bring
  the main window forward on it; the answer is read there, not in the box.
  (The person's choice.)
- 2026-11-23 — Can it be turned off and the shortcut changed? Yes, in
  Settings › General. (The person's choice.)

### 6.3 Assumptions

- The default shortcut is `Alt+Shift+Space` on every platform, not
  `Alt+Space` as first proposed: Windows uses Alt+Space for every window's
  system menu, and launchers and other assistants take it. `Ctrl+Alt+Space`
  was tried first and found taken by another desktop assistant on the
  person's machine (2026-11-23); Settings reports a taken shortcut and lets
  the person record another.
- The new chat starts where **New chat** would (the workspace the sidebar
  shows, the agent and model a new chat gets); the box offers no choice.
- Quick entry is on by default, also with the tray icon off (the shortcut
  works without it).

## 7. History

- 2026-11-23 — Specified and accepted with the person in one round; work
  started.
- 2026-11-23 — Default shortcut moved to `Alt+Shift+Space` after
  `Ctrl+Alt+Space` turned out to be taken (6.3).
