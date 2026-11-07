# Side panel tabs, browser pages and terminals per chat — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop UI |
| Created | 2026-11-07 |
| Updated | 2026-11-07 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Agent terminal](../agent-terminal/spec.md), [Approval modes](../approval-modes/spec.md) |

## 1. Summary

The side panel's tabs, the browser's pages and the terminals were one set for
the whole window: a browser opened in chat A was still there in chat B. Each
chat (session) now has its own: showing another chat shows that chat's tabs,
pages and shells, and a chat that never opened any shows none. What went out of
sight keeps running.

## 2. Scope

### 2.1 Goals

- The side panel's open tabs and the visible one are kept per chat.
- A browser page or terminal belongs to the chat it was opened in (by the
  person or by the agent) and shows only while that chat does.
- The agent of a chat sees and acts on the pages and terminals of its own chat
  only.
- Pages and shells survive a chat switch and a window reload.

### 2.2 Non-goals

- Files, Side chat and Memory panel contents: they stay shared, only their tabs
  follow the chat.
- Moving a page or terminal to another chat.
- Keeping shells across an app restart (they end with the app, as before).

## 3. User scenarios

### US-1 Another chat does not show my browser (priority P1)

A person opens a browser page in chat A and switches to chat B.

Acceptance scenarios:

1. **Given** chat A with an open browser page and terminal, **when** the person
   shows chat B that never opened any, **then** the side panel is closed and
   has no tabs, no page and no terminal.
2. **Given** that, **when** the person shows chat A again, **then** its tabs,
   its visible tab, the page and the shell are as they were.
3. **Given** chat B opens the terminal, **then** it starts its own shell and
   chat A's stays out of sight.

### US-2 The agent works in its own chat (priority P1)

Acceptance scenarios:

1. **Given** chat A has a terminal tab, **when** chat B's agent calls
   `terminal_list` or `browser_tabs`, **then** it sees none of A's.
2. **Given** an agent of a chat that is not showing, **when** it opens the
   browser or a terminal, **then** the tab opens in its own chat's tabs and
   the visible chat is not changed.

### Edge cases

- A chat that is deleted, archived or an untouched draft that is discarded ends
  its shells and pages.
- After a window reload each page and shell goes back to its chat; one whose
  chat is unknown goes to the first chat shown.
- Closing the Terminal or Browser tab ends only the showing chat's shells or
  pages.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | Whether the side panel is open, its open tabs and the visible tab are kept per chat; a chat that never opened any shows none, with the panel closed. | Show a new chat after chat A opened Terminal and Browser: the panel is closed and has no tabs. |
| FR-2 | A browser page belongs to the chat that opened it; only the showing chat's pages are listed and shown, the others keep running hidden. | Chat A opens a page, show chat B: no tab, the page is hidden; show A: the page is back. |
| FR-3 | A terminal belongs to the chat it was opened in (an agent's tab: its chat); only the showing chat's terminals are listed. | Same check with a shell. |
| FR-4 | Showing a chat again restores its tabs, visible tab, pages and shells. | Switch A → B → A: unchanged. |
| FR-5 | The terminal and browser tools act on the calling chat's tabs; a tab id of another chat is "no tab". An agent in a chat that is not showing opens its tool in its own chat's tabs only. | In chat B, `terminal_list` shows none of chat A's tabs. |
| FR-6 | A chat that is gone (deleted, archived, untouched draft discarded) ends its shells and pages and drops its saved tabs. | Discard a draft that had a terminal: the shell ends. |
| FR-7 | After a window reload pages and shells return to their chats (kept in session storage); those of unknown chat go to the first chat shown. | Reload the window: each chat still has its own. |
| FR-8 | Closing the Terminal or Browser tool tab ends only the showing chat's shells or pages; a chat switch closes nothing. | Close Browser in chat A: chat B's pages stay. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | A chat switch restarts no shell or page and loses no terminal output. | Switch away and back during a running command: its output is there. |

## 5. Success criteria

- SC-1 A chat never shows, lists or lets its agent touch another chat's page or
  terminal.
- SC-2 Switching chats back and forth loses nothing.

## 6. Clarifications

### 6.1 Open

n/a — none open.

### 6.2 Resolved

- 2026-11-07 — Q: Does a new chat inherit the side panel's tabs? → A: No, it
  shows none until it opens one (the person's request).
- 2026-11-07 — Q: Does the agent of a chat that is not showing take over the
  side panel? → A: No; its tab opens in its own chat and its page stays hidden
  until the chat shows (FR-5).

## 7. History

- 2026-11-07 — A new chat starts with the side panel closed (the person asked why it
  was always open): the panel's open state is per chat too.
- 2026-11-07 — Draft and implementation started from the person's report that
  a browser and a terminal opened in one session showed in every other.
