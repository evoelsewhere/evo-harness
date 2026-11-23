# Quick entry — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop shell, desktop UI |
| Created | 2026-11-23 |
| Updated | 2026-11-23 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Shell-hosted engines](../shell-hosted-engines/spec.md) (FR-11, FR-12: the tray and the app's lifecycle; windows attach to engines as viewers), [Evo Webbridge](../browser-bridge/spec.md) (FR-10, FR-14, FR-25: a chat outside the main window, answered in one place and settled in both), [Approval modes](../approval-modes/spec.md), [Agent tools](../agent-tools/spec.md) (`ask_user`), [Notifications](../notifications/spec.md), [Workspace rail](../workspace-rail/spec.md) (Needs you) |

## 1. Summary

Quick entry is a small floating message box that opens over whatever the
person is doing, from the tray icon or a keyboard shortcut that works in any
app. The person types a request and presses Enter: Evo starts a new chat with
it and answers in the box, so a quick question never pulls the person out of
the app they are in. The box grows into a small conversation: the reply
streams in, each tool step shows as one line, a short approval or question is
answered in place, and a follow-up is typed at the bottom. One key moves the
chat to the main window, where it is a chat like any other. The box goes away
with Esc or a click elsewhere, keeping its chat and what was typed.

Until 2026-11-23 the box only handed its text to the main window (FR-5 as
first written). Answering in the box is the change specified by FR-11 –
FR-22.

## 2. Scope

### 2.1 Goals

- Start a chat from anywhere in one step, without finding the Evo window.
- Read the answer and ask a follow-up without leaving the app in use.
- Move a chat to the main window in one step, with nothing lost, when it
  needs more room or more attention.
- Open from the tray icon (left click) and from a global shortcut.
- Appear at once: the box is ready before it is asked for.
- Never lose a half-typed request or a running chat when the box is
  dismissed.

### 2.2 Non-goals

- Choosing the agent, workspace, model or approval mode in the box: the new
  chat starts as the main window's **New chat** would.
- Attachments, screenshots or dictation in the box.
- Slash commands and mentions in the box: the text is sent as typed.
- A list or search of past chats in the box: earlier chats are in the main
  window's sidebar; the box shows one chat at a time.
- Showing everything the main window shows: tool details, diffs, the side
  panel and checkpoints stay in the main window (FR-12, FR-13, FR-14).
- Several boxes at once, or a box per chat.

## 3. User scenarios

### US-1 Ask from anywhere (priority P1)

A person working in another app wants to hand Evo a task without switching
windows first, so that asking costs no more than a keystroke.

Acceptance scenarios:

1. **Given** Evo runs (window open, hidden or in the tray), **when** the
   person presses the quick entry shortcut, **then** the box appears in the
   upper middle of the screen under the mouse, with the caret in it.
2. **Given** the box is open with text, **when** the person presses Enter,
   **then** a new chat starts in the workspace the sidebar shows with the
   text as its first message, and the box shows the reply as it arrives; the
   main window does not come forward.
3. **Given** the box is open with text, **when** the person presses
   Ctrl+Enter (⌘+Enter on macOS), **then** the box hides, the main window
   comes forward on a new chat and the text is sent as its first message.
4. **Given** the box is open, **when** the person presses the shortcut again,
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

### US-5 Read the answer and follow up in the box (priority P1)

A person who asked a quick question wants the answer next to their work and
a way to ask on, so that a short exchange never needs the main window.

Acceptance scenarios:

1. **Given** a message was sent from the box, **then** the box grows to show
   the message, each tool step as one line while it runs and when done, and
   the reply as it streams; a field for a follow-up stays at the bottom.
2. **Given** the reply is longer than the box can hold, **then** the box
   stops growing at its limit and the conversation scrolls, following the
   latest text unless the person scrolled up.
3. **Given** the reply has finished, **when** the person types a follow-up
   and presses Enter, **then** it is sent in the same chat and its reply
   shows in the box.
4. **Given** the chat is running, **when** the person presses **Stop**,
   **then** the turn stops as it would in the main window.

### US-6 Answer a short approval or question in the box (priority P1)

A person whose chat stops to ask for an approval or an answer wants to give
it in place when it is short, and to be sent to the main window when it is
not, so that a quick chat is not blocked and nothing important is approved
blind.

Acceptance scenarios:

1. **Given** the chat asks to approve one action (an edit, a command, a web
   request), **then** the box shows a one-line summary of it with the same
   choices the main window offers; **when** the person picks one, **then**
   the chat goes on.
2. **Given** an approval of an edit, **then** next to its choices the box
   offers **Review in Evo**, which opens the chat in the main window on that
   approval.
3. **Given** the chat asks one question (`ask_user` with options or a short
   answer), **then** the person answers it in the box.
4. **Given** the chat asks for something the box does not answer (FR-14),
   **then** the box says Evo needs the person and offers **Open in Evo**.
5. **Given** an approval or question shown in both the box and the main
   window, **when** it is answered in one, **then** it goes from both.

### US-7 Continue in the main window (priority P1)

A person whose quick question turned into real work wants to move it to the
main window, so that they get the full view without starting again.

Acceptance scenarios:

1. **Given** a chat in the box (running or not), **when** the person presses
   **Open in Evo** or Ctrl+O (⌘+O on macOS), **then** the box hides and the
   main window comes forward on that chat, with every message, step and
   open question, and a running turn keeps running.
2. **Given** the chat was opened in the main window, **when** the person
   opens the box again within the new-chat time (FR-18), **then** the box
   shows the same chat, including what was added in the main window.

### US-8 Step away and come back (priority P1)

A person who clicks back into their work while Evo is still answering wants
the answer to keep coming and to find it again, so that the box never costs
them a reply.

Acceptance scenarios:

1. **Given** a chat running in the box, **when** the person clicks outside
   it, **then** the box hides and the chat keeps running.
2. **Given** that hidden chat finishes or needs the person, **then** Evo
   tells the person with a notification; **when** they click it, **then**
   the box opens on that chat (or the main window, when the box cannot
   answer what is needed — FR-14).
3. **Given** the box is pinned, **when** the person clicks outside it,
   **then** it stays shown, and it can be dragged to another place by its
   header.

### US-9 Start fresh after a while (priority P2)

A person who opens the box for a new question some time after the last one
wants an empty box, and one who comes back soon wants the same chat, so that
unrelated questions do not pile into one chat.

Acceptance scenarios:

1. **Given** the box's chat was last used more than 15 minutes ago (the
   default), **when** the box opens, **then** it is empty, ready for a new
   chat; the earlier chat stays in the sidebar.
2. **Given** it was last used less than that ago, **when** the box opens,
   **then** it shows that chat.
3. **Given** a chat in the box, **when** the person presses **New chat** or
   Ctrl+N (⌘+N on macOS), **then** the box is empty, ready for a new chat.

### Edge cases

- The main window was destroyed (closed with the tray on after a rebuild):
  a chat in the box runs without it; Ctrl+Enter or **Open in Evo** builds it
  again and shows the chat once its page has loaded.
- Enter on empty or blank text does nothing; Shift+Enter adds a line.
- Typing with an input method: Enter picks the word and sends nothing.
- Several monitors: the box opens on the monitor under the mouse; a pinned
  box whose monitor is gone opens on the monitor under the mouse.
- The app is quitting: the box does not open.
- The saved shortcut cannot be registered at start (another app took it):
  Evo starts without it, logs a warning and Settings shows the shortcut as
  unavailable; the tray click still opens the box.
- A follow-up sent while the chat runs is queued, as in the main window.
- The box's chat is archived or deleted in the main window: the box opens
  empty the next time.
- The new-chat time passes while the chat runs or waits for the person: the
  box keeps that chat until it is done and answered.
- The box was reset to a new chat while text was typed but not sent: the
  text stays in the field.
- No model is set up, or the provider fails: the box shows the same readable
  error and next step the main window shows, with **Open in Evo**.
- The chat is open in the main window at the same time: both show the same
  messages as they arrive.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | A quick entry window exists while quick entry is on: frameless, transparent, always on top, not in the taskbar, hidden until asked for, built at start so it opens without loading. | Given quick entry is on, when the app starts, then a hidden `quick-entry` window exists. |
| FR-2 | A global shortcut toggles the box from any app: it opens it when hidden, hides it when shown. Default `Alt+Shift+Space` (Option+Shift+Space on macOS). | Given another app is focused, when the shortcut is pressed, then the box shows and has focus; pressed again, it hides. |
| FR-3 | A left click on the tray icon toggles the box while quick entry is on; with it off the click shows the main window. The tray menu's **Open Evo** always shows the main window. | Unit test on the click decision; manual check on the tray. |
| FR-4 | The box opens centred horizontally on the monitor under the mouse, in its upper part, with the caret in the text field. A pinned box opens where it was left (FR-17). | Unit test on the position; manual check on two monitors. |
| FR-5 | Enter sends the trimmed text when it is not blank: a new chat starts as **New chat** would (the workspace the sidebar shows, the agent, model and approval mode a new chat gets) with the text as its first message, and the box shows it (FR-11). Ctrl+Enter (⌘+Enter on macOS) sends it the earlier way: the box hides and clears, the main window comes forward on the new chat (an untouched draft reused). In a chat already shown in the box, Enter sends a follow-up in that chat and Ctrl+Enter sends it and opens the chat in the main window. Shift+Enter adds a line. *(Changed 2026-11-23: Enter used to open the main window.)* | Given the empty box with "hello", when Enter is pressed, then the box shows "hello" and its reply and the main window stays where it was; with Ctrl+Enter, the main window shows a new chat whose first message is "hello". |
| FR-6 | When there is no main window, a chat still starts and runs from the box; whatever opens the main window (Ctrl+Enter, **Open in Evo**, **Review in Evo**) builds it again and shows the chat once its page has loaded. | Unit test on the pending chat; manual check. |
| FR-7 | Esc, or the box losing focus while it is not pinned, hides it and keeps its text and its chat for the next time it opens (subject to FR-18). Hiding never stops the chat. | Manual check, also with a turn running. |
| FR-8 | The field grows with the text up to a limit, then scrolls. | Manual check with a long text. |
| FR-9 | Settings › General has a **Quick entry** switch (default on), the shortcut, which the person can record anew, and **Start a new chat after** (FR-18). A shortcut another app holds is refused with a message and the old one stays. Changes apply at once, without restarting. | Manual check in Settings. |
| FR-10 | The box follows the app's light or dark theme. | Manual check in both themes. |
| FR-11 | Once the box shows a chat, it is a conversation: a header with the workspace, the chat's title, **New chat**, **Open in Evo**, **Pin** and **Close**; the messages; and a field at the bottom for a follow-up, with **Stop** while a turn runs. The box grows with its content up to 60 % of the height of its monitor, then the conversation scrolls, following the latest text unless the person scrolled up. | Given a sent message with a long reply, then the box stops growing at its limit and scrolls with the reply; scrolled up, it stays put. |
| FR-12 | The box streams the reply as it arrives and draws it as the main window does: text, lists, links, tables, code with a copy button, images, and the rich blocks of a reply (stats, charts, cards, tables, steps, callouts, buttons, diagrams, views), sized to the box's width. A block's action works from the box as in the main window. What opens in the main window's side panel (a file, a document preview, a page) opens the main window on it. *(Changed 2026-11-23: rich blocks were first one line.)* | Given a reply with a list, code, a chart and a card, then all four are drawn in the box and the card's button works; clicking a file opens the main window on it. |
| FR-13 | Each tool step shows as one line with what it did (for example "Read 3 files", "Ran `bun test`"), a running mark while it runs and its outcome when done; a failed step says so. Details, output and diffs are not shown in the box. | Given a turn that reads files and runs a command, then the box shows one line per step and no output. |
| FR-14 | The box answers, in place, every approval and permission request with a one-line summary and the same choices the main window offers: one action (an edit, a command, a web request, a tool call) and the app's own requests (control of the computer or of an app, the person's browser, a plugin's consent). It also answers an `ask_user` with one question (options or a short answer). An approval of an edit also offers **Review in Evo**. What remains (several questions at once, a prompt without choices) shows as "Evo needs you" with **Open in Evo**. An answer in the box or the main window settles it in both. *(Changed 2026-11-23: the app's own requests first went to the main window.)* | Answer an edit approval and a request to act in an app in the box: the chat goes on and the main window's request goes; a question with three parts shows **Open in Evo** only. |
| FR-15 | **Open in Evo** (button and Ctrl+O / ⌘+O) and **Review in Evo** hide the box and bring the main window forward on the box's chat (on the approval or block for **Review in Evo** and FR-12), without stopping or restarting it. The box keeps the chat as its own (FR-18). | Given a running chat in the box, when **Open in Evo** is pressed, then the main window shows it running with every message so far. |
| FR-16 | The box's chat is an ordinary chat: it shows in the main window's sidebar at once, named after its first message as the app names chats, appears in **Needs you** when it waits or is unread, and a message sent to it in the main window shows in the box. | Send from the box: the sidebar shows the chat with its title; send a message in the main window: the box shows it. |
| FR-17 | **Pin** keeps the box shown when it loses focus. The box can be dragged by its header (and the empty box by its logo); a pinned box opens where it was left until it is unpinned; Esc, **Close** and the shortcut still hide it. The pin lasts until the person unpins it or the app quits. | Pin, click another app: the box stays; drag it, hide and show: it is where it was left. |
| FR-18 | When the box opens and its chat was last used (a message sent or a reply finished) longer ago than **Start a new chat after** — 5, 15 (default), 30 or 60 minutes, or Never — it opens empty for a new chat. A chat that runs or waits for the person is kept regardless. **New chat** and Ctrl+N (⌘+N on macOS) empty the box at any time. Text typed but not sent stays in the field. | Set 5 minutes, ask, wait 6 minutes, open the box: it is empty and the earlier chat is in the sidebar. |
| FR-19 | When the box's chat finishes a turn or needs the person while the box is hidden and the main window does not show that chat in front, Evo notifies as the app does for a chat (following the notification settings). Clicking the notification opens the box on that chat, or the main window when what is needed is not answered in the box (FR-14). | Ask a long task, click away; when it ends, a notification shows; clicking it opens the box on the chat. |
| FR-20 | When the chat fails (no model set up, provider error, engine stopped), the box shows the same readable error and next step the main window shows, with **Open in Evo**. | With no provider set up, send from the box: the error and its next step show in the box. |
| FR-21 | The box can be used with the keyboard alone: Enter, Shift+Enter, Ctrl+Enter, Ctrl+O, Ctrl+N and Esc as above (⌘ for Ctrl on macOS), Tab through the header, the steps' links and the approval choices, and the app's Stop shortcut stops a running turn. The reply is announced to screen readers as it completes, not word by word. | Manual check with the keyboard only and with a screen reader. |
| FR-22 | Opening the box always shows the field focused: on a chat, the follow-up field with its caret at the end. | Open the box on a chat: typing goes into the follow-up field. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | The box shows within 100 ms of the shortcut or click: its page is loaded while hidden and does not load the main app. What it needs to show a conversation may load after the first send, as long as the first text of the reply shows no later than it would in the main window. | Manual check of the open time; the first text of a reply shows in the box and the main window at about the same moment. |
| NFR-2 | Works on Windows, macOS and Linux (X11). Wayland may refuse global shortcuts; the tray click still works there. | Manual check per platform. |
| NFR-3 | A chat in the box costs no more than the same chat in the main window: it is the same running chat, not a second one, and a hidden box draws nothing. | Manual check: one engine for the chat; no drawing while the box is hidden. |

## 5. Success criteria

- SC-1 A person starts a chat from another app with the shortcut, a request
  and Enter, without touching the Evo window.
- SC-2 Dismissing the box never loses typed text or a running chat.
- SC-3 Turning quick entry off leaves the tray and the app as they were
  before this feature.
- SC-4 A person asks a short question from another app, reads the answer and
  asks one follow-up, and the main window never comes forward.
- SC-5 A chat started in the box continues in the main window in one step,
  with every message, step and open question there.
- SC-6 An approval for an edit is never accepted from the box without the
  person seeing which file it changes and being offered the full review.

## 6. Clarifications

### 6.1 Open

n/a — no open questions.

### 6.2 Resolved

- 2026-11-23 — How does the box open? Both a left click on the tray icon and
  a global shortcut; the tray menu keeps **Open Evo** for the main window.
  (The person's choice.)
- 2026-11-23 — What does Enter do? Start a new chat with the text and bring
  the main window forward on it; the answer is read there, not in the box.
  (The person's choice.) *Superseded by the next answers the same day.*
- 2026-11-23 — Can it be turned off and the shortcut changed? Yes, in
  Settings › General. (The person's choice.)
- 2026-11-23 — Should the box answer, instead of always sending to the main
  window? → Yes: Enter answers in the box; Ctrl+Enter keeps the earlier
  behaviour (open in the main window). No setting to swap them. (FR-5, FR-11)
- 2026-11-23 — How are approvals and questions handled in the box? → Simple
  ones (one action, one question) are answered in the box; an edit also
  offers the full review; anything else goes to the main window. (FR-14)
- 2026-11-23 — Should the app's own permission requests (acting in an app,
  the browser, a plugin's consent) be allowed from the box too? → Yes,
  after the first trial: every request with choices is answered in the box.
  (FR-14)
- 2026-11-23 — What happens on a click outside a box with a chat? → It hides
  as before and the chat keeps running, with a notification when it needs
  the person; a **Pin** keeps the box shown and lets it be dragged. (FR-7,
  FR-17, FR-19)
- 2026-11-23 — Does the box draw charts and cards? → Yes, every block of a
  reply, as the main window does; only side panel content opens the main
  window. (FR-12)
- 2026-11-23 — When does the box start a new chat? → After 15 minutes
  unused by default, settable from 5 minutes to Never; **New chat** at any
  time. (FR-18)

### 6.3 Assumptions

- A-1 The default shortcut is `Alt+Shift+Space` on every platform, not
  `Alt+Space` as first proposed: Windows uses Alt+Space for every window's
  system menu, and launchers and other assistants take it. `Ctrl+Alt+Space`
  was tried first and found taken by another desktop assistant on the
  person's machine (2026-11-23); Settings reports a taken shortcut and lets
  the person record another.
- A-2 The new chat starts where **New chat** would (the workspace the sidebar
  shows, the agent and model a new chat gets); the box offers no choice.
- A-3 Quick entry is on by default, also with the tray icon off (the shortcut
  works without it).
- A-5 After **Open in Evo**, the box keeps the chat (FR-15) rather than
  emptying — so the shortcut finds the same chat again; the new-chat time
  still applies.
- A-6 The pin and the pinned position are not kept across a restart of the
  app — because a pin is for the task at hand.
- A-7 The box's height limit is 60 % of its monitor's height — tall enough
  for a useful answer, short enough to leave the work beside it visible.

## 7. History

- 2026-11-23 — Specified and accepted with the person in one round; work
  started.
- 2026-11-23 — Default shortcut moved to `Alt+Shift+Space` after
  `Ctrl+Alt+Space` turned out to be taken (6.3).
- 2026-11-23 — The person asked for answers in the box with a way to open
  the main window. Answers in the box, follow-ups, steps, approvals and
  questions, pin, notifications and a new-chat time specified (FR-11 –
  FR-22, NFR-3, US-5 – US-9); FR-4 – FR-7, FR-9 and NFR-1 changed; the
  non-goal "answers inside the box" removed.
- 2026-11-23 — FR-11 – FR-22 accepted by the person (gate A).
- 2026-11-23 — The person asked for charts and cards in the box: FR-12 now
  draws every reply block; assumption A-4 removed.
- 2026-11-23 — Plan for FR-11 – FR-22 accepted by the person (gate B);
  tasks T8 – T27.
- 2026-11-23 — In the first trial the person could not move the box: the
  header now drags it whether pinned or not (FR-17).
- 2026-11-23 — The person asked to allow the app's own permission requests
  in the box as well (FR-14 changed, 6.2).
