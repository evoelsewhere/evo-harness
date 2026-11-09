# Computer app control in the background — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop shell, desktop UI, engine (skill, approvals) |
| Created | 2026-11-09 |
| Updated | 2026-11-10 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Floating cards](../floating-cards/spec.md), [Agent browser](../agent-browser/spec.md), [Approval modes](../approval-modes/spec.md), [Agent tools by layer](../agent-tools-layers/spec.md); background: [research.md](research.md) |

## 1. Summary

With this feature the agent can work inside one desktop app that is open on
the person's computer, such as a spreadsheet, a document or a chat app. The
person keeps their own mouse, keyboard and screen while it does. When the
agent takes control, the app's window moves out of the person's view. It is
not minimized and not made invisible. The person watches it live in a
floating card with a virtual cursor, the same way they watch the agent's
browser pages today. When control ends, the window comes back exactly where
it was.

evo v3 shipped this feature, but its clicks and typing often missed. There
were two main reasons:

- Web-based apps stopped repainting while off screen, so the agent acted on
  an old picture.
- The agent clicked by guessed coordinates where it could have acted on a
  known element.

This version keeps the v3 concept and the same rule of never touching the
person's input. Accuracy has to come from three things:

- acting on identified elements first;
- a picture that is never stale;
- checking every step.

## 2. Scope

### 2.1 Goals

- The agent attaches to one app window per chat, observes it and acts in it
  (click, type, press keys, set values, scroll, drag) while the window is
  off the person's screen.
- evo never uses the person's pointer or keyboard for the agent. The pointer
  does not move, the keyboard focus does not change, and the front window
  does not change.
- The window is never hidden by tricks: not minimized, not made transparent,
  not cloaked. It only moves out of view, and it comes back exactly as it was
  when control ends, also after a crash of evo.
- The controlled app shows as a live card over the chat with a virtual
  cursor, using the floating-card model of the agent's browser. The card has
  **Stop**, **Allow again**, **Show the app** and **Close**.
- Accuracy that holds up from run to run:
  - act on elements by reference before coordinates;
  - give the model a picture that matches the app's current state;
  - read back the result of every step;
  - say plainly when a step cannot be done in the background.
- Windows first, macOS next, with the same tool and the same card.
- The person decides which apps may be controlled and whether each call asks
  first.

### 2.2 Non-goals

- A virtual monitor or display driver.
- Borrowing the person's real pointer or keyboard, even for a moment.
- Hiding a window by transparency, cloaking, minimizing or moving it to
  another virtual desktop or Space.
- Controlling the whole desktop, or several apps at once from one chat. One
  window per chat, plus that window's own dialogs and menus.
- A separate operating-system session or account for the agent, such as
  Windows' agent workspace. This is left for later; the design keeps room to
  add it as a second backend without changing the tool.
- Apps running as administrator, games and other raw-input apps,
  remote-desktop windows and protected system UI.
- Using the clipboard. The agent types values instead.
- Recipes for particular apps. Every app is learned by looking at it.
- The person's own browser. The [Browser bridge](../browser-bridge/spec.md)
  covers that, and web pages go through the [Agent browser](../agent-browser/spec.md).

## 3. User scenarios

### US-1 The agent works in my app while I keep working (priority P1)

The person asks the agent to fill a table in a spreadsheet that is already
open. The spreadsheet leaves the screen and a card shows it. The person keeps
writing an email in another app. The agent finishes, and the spreadsheet comes
back where it was with the table filled.

Acceptance scenarios:

1. **Given** an open spreadsheet and computer app control enabled, **when**
   the agent attaches to it, **then** its window leaves the person's screen
   without being minimized, and a card shows it live over the chat.
2. **Given** the person typing in another app during the whole task, **then**
   none of their keystrokes is lost or lands in the spreadsheet, the pointer
   never moves on its own, and the other app stays in front.
3. **Given** the agent's turn ends, **then** the spreadsheet is back at its
   previous position, size and state (normal, maximized or minimized). It is
   not brought to the front.

### US-2 Watch, stop and take over (priority P1)

While the agent works, the person watches the card: the virtual cursor moves
to each target just before the input lands, and an activity line says what
the agent is doing.

Acceptance scenarios:

1. **Given** an attached app, **when** the agent clicks an element, **then**
   the card's cursor moves there first and the activity line names the
   action.
2. **Given** the agent typing a long text, **when** the person presses
   **Stop**, **then** typing stops within one character. Held keys and mouse
   buttons are released, and the agent cannot act on the app again until the
   person presses **Allow again**.
3. **Given** a card, **when** the person presses **Show the app**, **then**
   the window comes back to the person's screen and control ends for good in
   this turn.
4. **Given** a card, **when** the person presses **Close**, **then** the app
   is released, and the agent cannot attach any app again in this turn.

### US-3 Dialogs and menus stay with the app (priority P1)

The agent uses a command that opens a dialog, a context menu or a side pane
in the controlled app.

Acceptance scenarios:

1. **Given** an attached app, **when** a dialog of that app opens, **then**
   the dialog does not appear on the person's screen. The card shows it, and
   the agent's next look and action go to it.
2. **Given** a dialog still open when control ends, **then** it comes back
   centred over its window, so the person never faces an app blocked by a
   dialog they cannot see.

### US-4 Accurate work in a web-based app (priority P1)

The agent works in an app built on web technology (a chat app, a code
editor), either one evo starts or one the person already has open.

Acceptance scenarios:

1. **Given** a web-based app started by the agent through evo, **when** the
   agent clicks, types or scrolls, **then** the input reaches the page and
   the next picture shows the result.
2. **Given** a web-based app the person had already open, **when** the agent
   acts, **then** the input reaches the page, and the next picture shows the
   result. The window keeps one pixel on screen so that it keeps painting
   (FR-16). If it still lags, its pictures are labelled as possibly late.
3. **Given** a web-based app the person had already open, **when** the agent
   wants the more reliable page-level channel, **then** it asks the person,
   naming the app, before restarting it. Without a yes, nothing restarts.

### US-5 The app comes back after a crash (priority P1)

evo is killed, or crashes, while an app is off screen.

Acceptance scenarios:

1. **Given** an app off screen and evo killed, **when** evo starts again,
   **then** the window is back where it was, provided it is still open, still
   the same process and still off screen.

### US-6 Choose what may be controlled (priority P1)

The person opens **Settings → Computer apps**.

Acceptance scenarios:

1. **Given** the feature off (the default), **then** the agent has no
   computer app tool.
2. **Given** a blocked app, **then** it is missing from the agent's window
   list, and attaching, starting, closing or ending it is refused.
3. **Given** "Ask every time", **when** the agent sends any acting call,
   **then** the approval panel lists each action as it will run (for example
   `click e12`, `type "Dear team…" (240 chars) into e5`, `key ctrl+s`).

### US-7 The agent says what it cannot do (priority P2)

The task needs something that background input cannot deliver, such as a
menu that only opens on hover, dragging a file into another app, or a game.

Acceptance scenarios:

1. **Given** such a step, **then** the tool result says it cannot be done in
   the background and why. The agent asks the person to do that step, for
   example by pressing **Show the app**, and does not report it as done.

### US-8 The agent starts the app it needs (priority P2)

The person asks for work in an app that is not open.

Acceptance scenarios:

1. **Given** an installed app, **when** the agent starts it, **then** the app
   starts without coming to the front, and its first window is attached off
   screen.
2. **Given** a name that matches no installed app, **then** nothing starts.
   The agent can never start a path or a command line.

### Edge cases

- **The person brings the window back** from the taskbar or Dock while the
  agent controls it. The next action moves it out of view again; only
  **Show the app** hands it back for good.
- **The app hangs.** Only that chat waits. evo stays responsive, and quitting
  evo waits at most 3 s for it.
- **The window closes or the app quits** while attached. The session is
  released, the card closes, and the agent is told.
- **The same window and two chats.** The second chat cannot attach it. The
  window list marks it as controlled from another chat.
- **A modal dialog blocks the main window.** Looking and acting go to the
  dialog.
- **A scaled display** (125–200 %), including apps that do not handle DPI.
  Positions stay correct.
- **A model without image input.** The tool still works through the element
  tree, and a screenshot request says that pictures are not available.
- **A secure or password field.** Typing is refused, and the agent asks the
  person.
- **The app was minimized or maximized before attach.** It runs at the stage
  size while controlled, and returns minimized or maximized.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | **Settings → Computer apps** holds: on/off (default off); **Ask every time** (default) or **Allow without asking**; allowed apps; blocked apps (block wins). The app picker lists running and installed apps with their icons, and a typed executable name can be added. | Toggle off: the tool is gone from the next call. Block an app: it disappears from the window list. |
| FR-2 | The agent gets one tool, `computer_app`, which takes an ordered batch of actions: `list_windows`, `attach`, `detach`, `status`, `screenshot`, `snapshot`, `find`, `zoom`, `click`, `hover`, `scroll`, `drag`, `type`, `key`, `invoke`, `set_value`, `wait`, `search_apps`, `open_app`, `close_app`, `kill_app`. When an action fails, the rest of the batch is skipped and reported as skipped, except `detach`. | A batch `click` (fails), `type`, `detach`: the result shows the click error, `type` skipped, `detach` done. |
| FR-3 | One attached window per chat, and one chat per window. Attaching another window releases the first. | Attach A then B in one chat: A is back on screen. Attaching A from a second chat is refused with the reason. |
| FR-4 | These are never listed or attachable: evo's own windows, the OS shell and security processes, processes with higher privileges than evo, and apps the settings forbid. On macOS also system UI, System Settings, password managers, terminals and script or automation tools. While an allow or block list is set, the app is checked again on every acting call. | Block an attached app mid-run: the next action is refused and the app is released. |
| FR-5 | **No real input.** For the agent, evo never moves the person's pointer, changes keyboard focus or the front window, or sends key or mouse events through the system's shared input stream. | The benchmark's input monitor sees no pointer, focus, front-window or key-state change caused by evo (NFR-2). |
| FR-6 | **Off-screen stage.** On attach the window is moved fully out of the person's view. It is not minimized, transparent or cloaked, and keeps its taskbar or Dock entry. A resizable window gets a fixed stage size of 1280×800 (1024×640 logical on displays scaled above 125 %) so the agent always sees the same layout; a fixed-size dialog keeps its own size. Two exceptions keep one pixel (one point on macOS) of the window visible in a screen corner: a web-based app without a page-level channel (FR-16), which is also kept on top so it keeps painting; and every window on macOS, where a window cannot leave the displays. | Attach a maximized editor: it is off screen at the stage size; release: it is maximized again where it was. |
| FR-7 | Control ends, and the window returns to its exact previous placement without being activated, on any of: `detach`, **Stop**, **Show the app**, **Close**, the end of the agent's turn, the chat being closed, or evo quitting. | Each trigger in turn: the window is back with its previous position, size and state. |
| FR-8 | Every off-screen window is recorded until it returns. On the next start evo puts back each recorded window that is still open, still the same process and still off screen. | Kill evo with an app off screen, start evo: the window is back. |
| FR-9 | Dialogs, menus, drop-down lists, panes and palettes that the controlled app opens never stay on the person's screen. They are moved next to the window as they appear. They are part of what the agent sees and acts on, and open dialogs are centred back over the window when control ends. | A command that opens a dialog: the dialog never shows on the person's screen; the next snapshot lists its buttons. |
| FR-10 | **Live card.** An attached app shows as a floating card of its chat, with the same drag, resize, chip and open-in-panel behaviour as browser cards. It shows the window and its open popups as they are now: about 6–10 pictures a second while the agent acts, fewer when idle, none while no card or panel shows it. | Attach: the card appears within 0.5 s; minimise it to a chip: capture stops. |
| FR-11 | **Virtual cursor and activity.** Before each pointer action lands, the card's cursor moves to the target (FR-17 of floating cards), and the activity line names the action. | Click by ref: the cursor reaches the element before the click lands. |
| FR-12 | **Card controls.** **Stop** revokes control for the chat and interrupts the running action mid-way; held buttons and modifiers are released. **Allow again** lifts it. **Show the app** hands the window back for good. **Close** releases the app and refuses `attach` until the turn ends. Stop is enforced by evo and the agent cannot lift it. | Stop during a long `type`: input stops within one character; the next `attach` is refused until Allow again. |
| FR-13 | The person's clicks, scrolls and keys inside the card reach the app through the same background channels, so the person can help without the window coming back. | Click a button in the card: the app reacts and the window stays off screen. |
| FR-14 | **Observe.** `snapshot` and `find` return the element tree. Each line has a role, name, value, states (`[selected]`, `[checked]`, `[disabled]`), a ref and a box in screenshot pixels. Refs are never reused, and a ref into a closed dialog or a removed element is refused with the reason. `screenshot` returns the window at a size the model receives without further scaling (the stage is 1:1). With `marks: true`, visible elements are numbered on the picture with their refs. | A snapshot after a dialog closes: using a ref from inside it is refused with "dialog closed". |
| FR-15 | **Elements first.** `click`, `type`, `set_value` and `invoke` with a ref act on that element through the app's own structured interface where it offers one. Coordinates are used only when there is no element. Each result says which channel delivered the action. | `click e12` on a toolbar button: the result says it was invoked through the app's interface. |
| FR-16 | **Web-based apps.** An app built on web technology that evo starts with `open_app` is started with a page-level control channel. That channel is local to evo and closed when the app quits. The agent's input and pictures for it go through that channel, so they work and stay current while the window is off screen. A web-based app the person had already open is driven through its accessibility tree. Its window keeps one pixel on top in a screen corner (FR-6), so it keeps painting and updating. The agent may restart such an app to get the page-level channel only after the person says yes to a question that names the app; it never restarts one on its own. | Start a web-based chat app through evo: type and send a draft while off screen; each picture shows the new text. With an already-open one, the agent's restart question appears, and a "no" leaves the app running. |
| FR-17 | **Read-back.** Every acting result reports what changed, as read back from the app: focus, value, selection, a new dialog or menu. `type` reports whether the field shows the text as typed. When no change is seen, the result says so, and input is never retyped blindly. | Type into a field that rejects input: the result says `not confirmed` and the text is not typed again. |
| FR-18 | **Fallback ladder.** When an action by element shows no effect, the tool may try the next background channel once and says so. It never falls back to real input. | A button that ignores the structured press is pressed through the next channel; the result names both attempts. |
| FR-19 | **Honest limits.** When a step needs what background input cannot deliver (hover-only menus, drag and drop between apps, key tips, raw-input apps, a canvas where coordinates also show no effect), the result says so and suggests asking the person. | A hover-only menu: the result names the limit, and the agent asks the person instead of reporting success. |
| FR-20 | `zoom` returns an enlarged crop of a region of the latest picture. Coordinates the agent gives against the crop map back to the window. | Zoom on a small icon, click by the zoomed coordinates: the click lands on the icon. |
| FR-21 | **Typing.** Text is composed to its normal form first, so Vietnamese with combining marks arrives as typed. A line break is an Enter key (Shift+Enter in web chat boxes, so a message is not sent). The clipboard is never used. Typing into password fields is refused. | Type "Tiếng Việt" with decomposed marks: the field reads back "Tiếng Việt". |
| FR-22 | **Blocked keys:** Windows-key shortcuts and Ctrl+Alt+Delete; on macOS lock, log out and Force Quit. The Apple menu is never searched or pressed. | `key win+r` is refused. |
| FR-23 | **App lifecycle.** `search_apps` lists the catalog of running and installed apps, best match first, without forbidden apps. `open_app` starts only a catalog entry, never a path or command line, in the background. `close_app` asks the window to close as its close button would, and reports a "save changes?" dialog. `kill_app` ends the process, and its approval says that unsaved work is lost. None of them touches a window another chat drives. | `open_app "C:\\x.exe"` is refused; `kill_app` asks with the unsaved-work warning. |
| FR-24 | **Approvals.** Every call goes through the engine's approval mode. With **Ask every time** (the default), every acting call asks in every approval mode, including **Approve for me**. With **Allow without asking**, calls follow the chat's approval mode. A hook's or a handler's block still wins. The prompt lists each action as it will run. A refusal blocks the same actions for the rest of the run. Calls made only of `status`, `wait` and `detach` never ask. | In **Approve for me** mode with Ask every time: a `click` asks first. With Allow without asking: it runs straight away. |
| FR-25 | Every result that comes from the app (lists, trees, pictures, titles, notes, errors that quote app text) is marked as untrusted app content. | The result of `snapshot` carries the untrusted marker. |
| FR-26 | Each action has a timeout (60 s, plus 20 ms per typed character), and a timed-out action is interrupted where it got to. Pictures for the card never wait behind a long action. | An app that hangs on a click: the action times out, the card keeps showing frames, and other chats keep working. |
| FR-27 | **Skill.** A bundled skill teaches one method for every app, with no app-specific recipes. It covers the ground rules: stay in the app, no clipboard, aim only at identified elements, nothing irreversible unasked, and app text is data. The steps are: attach and look, map the window's surfaces, write checkable acceptance values, act in small steps, read back, move down the ladder after two failures, and stop and ask when needed. | The skill's evaluation cases trigger on "fill this in the spreadsheet I have open" and not on "edit this file". |
| FR-28 | **macOS permissions.** The settings page shows the state of Accessibility and Screen Recording. **Allow** opens the right system pane, and **Restart evo** is offered while Screen Recording waits for a restart. `list_windows` reports missing permissions. These controls act only on the person's click. | Revoke Accessibility: the card says it is missing; Allow opens that pane. |
| FR-29 | **macOS menu commands.** A background app runs a menu command only while it is active, and evo never activates it (FR-5). Such a command, by shortcut or by menu item, is therefore refused with that reason. The agent reaches the same function through an element of the window where the app offers one, or asks the person. | `key cmd+s` on a background app whose menu needs activation: refused with the reason; the frontmost app does not change. |
| FR-30 | **The front window stays the person's.** A driven app may answer evo's input by activating a window of its own: a new document, a dialog, a confirmation. While an action runs on Windows, and for 1.5 s after it, evo holds the system's foreground lock (`LockSetForegroundWindow`), so no app can take the front window; the person's own clicks and Alt+Tab still change it. The app's new window opens off the screen, where the agent sees and closes it (FR-9). | The benchmark's input monitor sees no front-window change while the agent drives Excel's "Create Table" dialog and a WinForms modal dialog (SC-2). |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Platforms: Windows 10 1809+ and Windows 11 (x64 and ARM64) first. macOS 14+ follows in a later phase of this spec, with the same tool and card. On other platforms, and on macOS before that phase, the tool is absent. | Feature matrix run on each platform. |
| NFR-2 | **Input isolation is measurable.** A monitor samples the person's pointer position, foreground window, keyboard focus and key state every 10 ms during the benchmark. It records no change caused by evo. | Benchmark report: 0 events. |
| NFR-3 | **Fresh pictures.** A picture taken after an acting action reflects that action. Where this cannot be confirmed (an already-open web-based app that lags even with its corner pixel, FR-16), the result labels the picture as possibly late. | Benchmark: 100 % of post-action pictures in evo-started web apps show the change. |
| NFR-4 | Latency: evo's own part of an action by ref on a native app takes at most 300 ms (median). The app's part is not counted: handling the input, and handing out its UI Automation tree for the read-back (FR-17), which an action reads before and after. The whole action's time is reported per app; in Excel one read of the tree takes about 400 ms, so an action with its read-back takes about 1 s. The first card picture appears within 500 ms of attach. | Step timings per action (`EVO_COMPUTER_TRACE`); benchmark timing table. |
| NFR-5 | A hung app never freezes evo or other chats. Quitting evo waits at most 3 s for windows to return; any left go back on the next start (FR-8). | Live test with a deliberately hung app. |
| NFR-6 | Security: the page-level channel of FR-16 is not reachable from other machines or other users and is closed with the app. Protected apps (FR-4) are refused at every entry point. No clipboard access. Stop is enforced natively. | Security review checklist; port scan while a web app is controlled. |
| NFR-7 | Positions are correct on displays scaled 100–200 % and for apps unaware of DPI. | Benchmark run at 150 % scaling. |
| NFR-8 | Capture costs nothing while no card or panel shows the app. | CPU sample with the card minimized to a chip. |

## 5. Success criteria

- SC-1 On the benchmark of surface kinds, each case runs 10 times and succeeds
  at least 95 % of the time on average, with no single case below 80 %. The
  surface kinds are: a grid (spreadsheet), a text document, forms and dialogs,
  menus, an evo-started web-based app and an already-open web-based app. In
  v3, failing cases moved between runs; this criterion measures that.
- SC-2 Across the whole benchmark, there are zero changes to the person's
  pointer, keyboard focus or front window caused by evo.
- SC-3 A person typing in another app for a 5-minute agent task loses no
  keystroke, and none of their keystrokes lands in the controlled app.
- SC-4 After evo is killed mid-run, every off-screen window is back on the
  next start.
- SC-5 For any step it cannot do in the background, the agent says so and
  never reports it as done (checked on the benchmark's "impossible" cases).

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-09 — Q: Use a virtual monitor (display driver) as the stage? → A:
  No (section 2.2).
- 2026-11-09 — Q: May the agent briefly borrow the real pointer and keyboard
  when background input fails? → A: No, never (FR-5, FR-18).
- 2026-11-09 — Q: Hide the window by transparency, cloaking or minimizing?
  → A: No. The window only moves out of view (FR-6).
- 2026-11-09 — Q: Does the app come back when control is done? → A: Yes,
  exactly where it was (FR-7).
- 2026-11-09 — Q: How does the person watch? → A: A floating card with a
  virtual cursor, like the agent's browser (FR-10, FR-11).
- 2026-11-09 — Q: Web apps the person already has open stop repainting when
  fully off screen. May one pixel of such a window stay on screen, on top in
  a corner? → A: Yes, only for web-based apps without a page-level channel.
  A spike must confirm it keeps them painting; if one still lags, its pictures
  are labelled "may be late" (FR-6, FR-16, NFR-3).
- 2026-11-09 — Q: May the agent restart an already-open web-based app to get
  the page-level channel? → A: Only after the person says yes to a question
  naming the app; never on its own (FR-16, US-4).
- 2026-11-09 — Q: On macOS, may the agent activate a background app for a
  moment to run a menu command, as v3 did? → A: No. Such commands are refused
  with the reason (FR-29).
- 2026-11-09 — Q: Is macOS in the first release? → A: Windows first; macOS
  14+ in a later phase of this spec (NFR-1).
- 2026-11-09 — Q: Does **Ask every time** apply in every approval mode? → A:
  Yes, including **Approve for me**. **Allow without asking** follows the
  chat's mode (FR-24).
- 2026-11-09 — Q: Stage size? → A: A fixed 1280×800 (1024×640 logical above
  125 % scaling); fixed-size dialogs keep their size (FR-6).
- 2026-11-09 — Q: Use the OS's own agent session (Windows' agent workspace)
  as a backend? → A: Not now. The design keeps room to add it later (section
  2.2).

## 7. History

- 2026-11-09 — Draft. Ports the v3 concept (tag `v3.1.1`) to evo, with the
  background-only constraints decided above.
- 2026-11-09 — The owner took the proposed answers to all seven open
  questions; FR-29 added; FR-6, FR-16, FR-24, NFR-1 and NFR-3 updated.
- 2026-11-09 — Gate A: the owner accepted the spec. Status stays `Draft`
  until the plan is accepted (gate B).
- 2026-11-09 — Gate B: the owner accepted the plan and ADR 0015. Status
  `Accepted`; tasks written.
- 2026-11-10 — Implementation started (tasks phase 1). Status `In progress`.
- 2026-11-09 — The first benchmark runs showed driven apps taking the front
  window for their own dialogs (SC-2). The owner chose to hold the
  foreground lock while an action runs: FR-30 added.
- 2026-11-09 — Measured in Excel, one read of its UI Automation tree takes
  about 400 ms inside Excel, and the read-back reads it twice per action.
  The owner chose to count only evo's own part in NFR-4 and report the
  whole action's time per app.
