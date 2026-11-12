# Engines run without a window: the shell hosts them — Spec

| | |
| --- | --- |
| Status | Implemented |
| Area | desktop shell · desktop UI · engine · protocol |
| Created | 2026-11-11 |
| Updated | 2026-11-11 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Session mesh protocol](../session-mesh-protocol/spec.md), [Subagents and nested sessions](../subagents-and-nested-sessions/spec.md), [Memory and the Scheduler](../memory-and-scheduler/spec.md), [Approval modes](../approval-modes/spec.md), [Nested sessions: roles, limits and mail](../session-roles-and-mail/spec.md), ADR [0018](../../architecture/decisions/0018-the-shell-hosts-engines-windows-attach.md) |

## 1. Summary

Today a chat's engine runs only while a window drives it. The shell can
start and stop engine processes, but a window decides when an engine
starts, sets up its tools and model, wakes it for notes and mail, records
its turns, runs the Scheduler and passes settings on. With the tray on,
"background" work is really the main window, hidden. When that window is
gone or reloaded, mail, nested sessions, wakes and scheduled runs stop.
Turns that no window watched are never recorded, so the sidebar, the
session list and memory indexing go wrong.

This feature moves that hosting into the shell. A chat's engine runs while
the chat has work, whether or not a window shows it. Windows attach to
engines to show and steer them, and detach without stopping them. This is
what ADR 0014 left for "a later ADR". It is also what any long-running
multi-session work, such as a team of agents, needs.

## 2. Scope

### 2.1 Goals

- The shell starts, sets up, restarts and stops engines. A window is a
  viewer.
- Work that does not need the person keeps going with every window closed
  or hidden, while the app runs: nested sessions, mail and notes, wakes,
  background subagents and scheduled runs.
- Turns, costs and run records are written whether or not a window watched.
- A question that needs the person (an approval, `ask_user`, an app tool's
  question) reaches them by notification and waits safely until they open
  the chat.
- Settings reach every engine, not only the ones a window holds.

### 2.2 Non-goals

- Running with the app quit, or as a system service. Work needs the evo
  process, as today (the tray keeps it alive).
- Answering approvals without the person, or new approval policies. The
  approval mode decides, as before.
- Remote engines (mesh FR-33).
- Changing what a window shows. The transcript, sidebar and dialogs look the
  same.

## 3. User scenarios

### US-1 Close the window, work goes on (priority P1)

A person starts a chat that runs nested sessions and background agents,
then closes the window (tray on).

Acceptance scenarios:

1. **Given** a running chat with nested sessions, **when** the person closes
   every window, **then** the sessions keep working, their notes reach the
   parent, the parent is woken within its guard rails, and on reopening the
   person sees every turn with its cost.
2. **Given** a separate chat window with a running turn, **when** the person
   closes that window, **then** the turn goes on and the chat shows it
   running in the main window.

### US-2 Scheduled runs with no window (priority P1)

A task is due while evo runs in the tray with no window.

Acceptance scenarios:

1. **Given** a task due in one minute and no window open, **then** its chat
   starts within 20 s of its time, the prompt is sent, the run is recorded,
   and a notification says so.
2. **Given** evo was quit past a task's time, **when** it starts hidden
   (autostart), **then** the missed run runs once.

### US-3 A question while nobody looks (priority P1)

A chat with no window reaches an approval or an `ask_user` question.

Acceptance scenarios:

1. **Given** no window shows the chat, **when** it asks for approval,
   **then** an OS notification says so (clicking it opens the chat), and the
   tray shows that a chat waits.
2. **Given** the waiting chat, **when** the person opens it in any window,
   **then** the open question is shown and answering it continues the run.

### US-4 Settings reach every engine (priority P2)

The person changes the approval mode, a provider key, a plugin or the
sandbox while engines run that no window holds.

Acceptance scenarios:

1. **Given** an engine with no window, **when** the approval mode changes,
   **then** its next tool call follows the new mode; an idle one reloads
   providers and resources like a viewed one.

### Edge cases

- A window reloads (dev reload, crash). Engines go on; the window re-attaches
  and rebuilds its view, including open questions and running agents.
- An engine dies repeatedly. The shell restarts it at most 3 times in
  2 minutes for pending work, then leaves it stopped and notifies.
- Too many idle engines. Engines with no work and no viewer are stopped,
  oldest first, beyond the idle limit. Engines with work are never stopped
  by this.
- The person quits from the tray while work runs. Quit asks once ("3 chats
  are working. Quit anyway?") (FR-11).
- The tray is off and the person closes the last window while work runs.
  The same question as quitting; closing never leaves an invisible process
  with the tray off (FR-11).
- The person stops a chat's engine. Its pending mail waits; the shell does
  not start it again until new mail or a prompt arrives or the person opens
  the chat.
- A draft sits in a chat that is not shown in any window. The wake waits as
  it does today; the draft is known to the shell.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | The shell starts a chat's engine whenever the chat has work or a window asks for it, and gives it the same setup a window gives today: folder (worktree, workspace or home), flags, model, the session tools, app tools and connector tools. An engine started by the shell is indistinguishable from one started for a window. | Given a nested session started with no window open, then it runs with the session, browser, terminal and connector tools it would have had in a window. |
| FR-2 | Windows attach to and detach from engines. Closing a window or reloading it detaches; it never stops an engine that has work. Attaching shows the chat as it is: running turn, running agents, open questions. | Given a chat window with a running turn, when it is closed, then the turn finishes and is recorded. |
| FR-3 | "Work" is a running turn, running subagents, mail or notes waiting for the chat, a prompt waiting to be sent (scheduled run, a started session's brief) or an open question. An engine without work and without viewers is stopped after it settles, beyond an idle limit of 4 engines counted over the whole app, oldest first. | Given five idle engines with no viewers and a limit of 4, then the oldest is stopped; a chat with a running background agent is never stopped. |
| FR-4 | The hub's requests for a host (mesh FR-21 – FR-24) are answered by the shell, with no window: new nested sessions start, mail and notes are delivered, an exited engine with pending mail is restarted at most 3 times in 2 minutes, and at app start every chat with undelivered mail is hosted. | Given mail for a stopped child and no window, when it is sent, then the child's engine starts and gets it. |
| FR-5 | Wakes for notes, background reports and mail (subagents FR-19, mesh FR-28) are decided by the shell with the same guard rails: a 2 s window that gathers notes, no wake while a draft is in the chat's composer or a question is open, at most 5 automated wakes in a row, never an archived chat. Windows tell the shell about drafts. | Given an idle parent with no window, when a child finishes a turn, then the parent is woken once. |
| FR-6 | Each finished turn is recorded by the shell: turn count, cost (subagents included), tokens, preview, model, unread and last activity, whether or not a window watched it. A turn no window showed is unread. | Given a nested session that ran three turns with no window, when the person opens evo, then its row shows three turns, their cost and unread. |
| FR-7 | The Scheduler runs in the shell: due tasks start within 20 s with no window, missed runs run once at launch (also when started hidden), a run whose previous chat still works is skipped, a run's first prompt is kept until sent (it survives a window reload), run records and the next run are written by the shell only. | Given no window, when a task is due, then it starts and its run is recorded; given a window reload right after the start, then the prompt is still sent. |
| FR-8 | Settings reach every running engine, viewed or not: approval mode, providers and keys, skills, plugins, memory, connectors, and a sandbox change (which restarts idle engines). Settings the shell needs (start sessions without asking, connectors switched off per chat) are kept in the app database, not in a window. | Given an engine with no window, when a provider key is added, then its next run can use it without a restart of the app. |
| FR-9 | Open questions (engine approvals, `ask_user`, app tools' questions) are kept by the shell per chat. With no window showing the chat: an OS notification ("Approval needed · {chat}") that opens the chat when clicked, a bell entry, and the tray's menu lists chats that wait. A window that attaches shows every open question of the chat. Nothing answers a question for the person, and an open question never times out. | Given a waiting approval in a chat with no window, when the person opens the chat, then the approval shows and answering it continues the run. |
| FR-10 | Run notifications (reply ready, run failed, engine stopped, engine could not start, scheduled run started) are sent by the shell when no window shows the chat, and recorded in the bell; entries made with no window open are kept until a window shows them. | Given no window, when a run fails, then an OS notification and a bell entry exist. |
| FR-11 | With the tray on, closing the last window keeps evo running; the main window can be opened again from the tray, the dock or a second launch, also after it was destroyed. Quitting (or closing the last window with the tray off) asks once when chats are working, then stops every engine. | Given the main window closed and destroyed, when the person clicks the tray's Open, then a main window opens with the sidebar as it is. Separate chat windows are viewers of the same engines; moving a chat to a window only changes who shows it. |
| FR-12 | The tray shows whether chats are working or waiting for the person. | Given one working and one waiting chat, then the tray says so. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | One owner per decision: a wake, a restart, a turn record or a scheduled run happens once, whatever number of windows are open. | Given three windows open, when a child finishes a turn, then the parent is woken once and the turn recorded once. |
| NFR-2 | The shell makes no agent decisions beyond those listed here (start, set up, wake, record, restart, stop). Prompts, steering, answers and cancelling stay the person's, through a window. | Review against the ADD's ownership rules. |
| NFR-3 | A window reload or a crash of the webview loses nothing: no prompt, mail, wake, run record or open question. | Given a reload during a scheduled run's start, then the run's prompt is sent once. |
| NFR-4 | Older databases upgrade; settings that lived in a window's local storage are moved to the app database on first start. | Given a profile from the previous release, then its settings and tasks are kept. |
| NFR-5 | Engines with no viewer are visible to the person: the sidebar shows them working, and the tray can open them. Nothing runs that the person cannot see and stop. | Given a working chat with no window, then the main window's sidebar shows it working when opened. |

## 5. Success criteria

- SC-1 With every window closed, a nested-session run, a background agent
  and a scheduled task all finish, and their turns and costs are correct
  when the person comes back.
- SC-2 No work is lost or done twice when a window reloads.
- SC-3 A question that needs the person is never silent: they are notified
  within seconds and find it when they open the chat.
- SC-4 Nothing changes for a person who keeps the window open.

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-11 — Q: Idle limit for engines with no viewer? → A: 4, counted over the
  whole app (now in FR-3).
- 2026-11-11 — Q: Ask before quitting (or closing with the tray off) while work
  runs? → A: Yes, once, with the number of working chats (now in FR-11).
- 2026-11-11 — Q: Should an open question time out? → A: No; notification and
  tray make it visible (now in FR-9).
- 2026-11-11 — Q: Do separate chat windows keep their own engines? → A: No; every
  window is a viewer of the shell's engines (now in FR-11).

## 7. History

- 2026-11-11 — Draft. Moves engine hosting from the windows to the shell
  (ADR 0018), as ADR 0014 deferred.
- 2026-11-11 — Gate A: open clarifications answered by the owner (accepted as
  proposed); spec accepted.
- 2026-11-11 — Gate B: plan and tasks accepted by the owner.
- 2026-11-12 — Implemented: phases 1–10 built and run in the app (plan section 5,
  tasks section 2). The tray's own menu and Quit, the settings fan-out to an
  engine no window holds and the local-storage import were checked by unit
  tests only.
