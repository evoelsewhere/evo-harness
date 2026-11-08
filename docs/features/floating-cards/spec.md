# Floating cards for the agent's browser pages and terminals — Spec

| | |
| --- | --- |
| Status | Accepted |
| Area | desktop UI, desktop shell |
| Created | 2026-11-08 |
| Updated | 2026-11-08 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Side panel tabs, browser pages and terminals per chat](../session-panels/spec.md), [Agent browser](../agent-browser/spec.md), [Agent terminal](../agent-terminal/spec.md) |

## 1. Summary

When an agent works in a browser page or a terminal, the person has to open the
side panel to see it. With this feature each page or shell the agent is using
appears as a small floating card over the chat, one card per tab, so the
person watches the work while the panel stays closed. The cards belong to the
chat (session) that opened them. Pages the agent drives always lay out at a
desktop size, wherever they are shown, so a closed panel or a small card never
turns the web into its phone layout. ChatGPT's desktop app offers a similar
floating view of what its agent does; this is Evo's version, inside the main
window.

## 2. Scope

### 2.1 Goals

- An agent-driven browser page or terminal tab shows as a card in the chat's
  window without opening the side panel.
- One card per tab; cards can be dragged, resized, minimized to a chip, expanded
  into the panel, or hidden, and nothing about the tab changes when they are.
- Cards follow the chat: showing another chat shows that chat's cards only.
- A browser page the agent drives lays out at a desktop viewport whatever its
  card or panel size, hidden or shown.
- The agent's tools work the same with the panel closed as with it open.

### 2.2 Non-goals

- An always-on-top window outside the main window (a later step if wanted).
- The person's own Chrome or Edge ([Browser bridge](../browser-bridge/spec.md)).
- Cards for Files, Memory or Side chat.
- Pages and shells the person opened themselves: they stay in the panel.

## 3. User scenarios

### US-1 Watch the agent browse without the panel (priority P1)

The person asks the agent to research something and keeps the panel closed. The
agent opens a page; a card appears over the chat showing that page live, with
what the agent is doing ("Clicking ref_4"). A second page the agent opens
becomes a second card.

Acceptance scenarios:

1. **Given** a chat with the panel closed, **when** the agent's `browser_navigate`
   opens a page, **then** a card with that page appears over the chat and the
   panel stays closed.
2. **Given** a card, **when** the agent clicks in the page, **then** the card's
   activity line says what it is doing and the page updates live.
3. **Given** two pages opened by the agent, **then** there are two cards.

### US-2 Watch a command run (priority P1)

The agent runs a build or a dev server in a terminal. A card shows the shell's
live output while the panel is closed.

Acceptance scenarios:

1. **Given** the panel closed, **when** the agent's `terminal_run` starts a shell
   tab, **then** a card with its output appears.
2. **Given** a terminal card, **when** more output arrives, **then** it scrolls.

### US-3 Keep chats apart (priority P1)

Chat A's agent has two cards. The person switches to chat B: A's cards are gone
from view and B's (none, or its own) show. Back in A, the cards are where they
were, still live.

Acceptance scenarios:

1. **Given** cards in chat A, **when** chat B is shown, **then** none of A's cards
   is visible; **when** A is shown again, **then** its cards are back at their
   positions and sizes.
2. **Given** a new chat, **then** it has no cards.

### US-4 Move cards out of the way (priority P2)

The person drags a card to a corner, shrinks it, minimizes it to a chip above the
composer, brings it back from the chip, or expands it into the side panel.

Acceptance scenarios:

1. **Given** a card, **when** dragged or resized, **then** it stays inside the chat
   area and keeps its place after a chat switch.
2. **Given** a minimized card, **then** a chip with its icon and title is in the
   chat's chip row; clicking it brings the card back.
3. **Given** a card, **when** "open in panel" is used, **then** the side panel opens
   on that tab and the card goes away (no duplicate).

### US-5 Take over a page (priority P2)

The person clicks, types or scrolls in a browser card to help the agent (a
login, a captcha). A terminal card is for watching; typing happens in the panel.

Acceptance scenarios:

1. **Given** a browser card, **when** the person types into a page field, **then**
   the page receives it.
2. **Given** a terminal card, **then** it is view-only and offers "Open in panel".

### US-6 Pages look like a desktop (priority P1)

The agent opens a site with the panel closed (or in a small card). The site
lays out as on a normal desktop screen, so the agent's screenshots and clicks
mean the same as in a full browser.

Acceptance scenarios:

1. **Given** the panel closed, **when** the agent reads the page's size
   (`browser_status` or `browser_viewport`), **then** it is a desktop size
   (1280×800), not the card's or panel's.
2. **Given** a small card, **then** the page is shown scaled to fit, not reflowed.
3. **Given** `browser_viewport` set a size, **then** that size wins.

### Edge cases

- A menu or dialog open over the chat: cards hide behind it as pages in the
  panel do and come back after.
- The window gets smaller: cards move inside it.
- Many tabs: past three visible cards the rest are chips only.
- A tab closes (agent or person): its card and chip go.
- The panel's Browser or Terminal tool is open on a tab: that tab has no card.
- Chat stopped or finished: cards stay until closed.
- The main window is hidden or minimized: the tools keep working; cards simply
  are not visible.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | A browser page or terminal tab the agent opens or uses shows as a card over its chat's window, without opening the side panel. | US-1.1, US-2.1 |
| FR-2 | One card per tab. Its bar shows the kind, the page title and host or the shell name, and the agent's current action; its buttons are minimize, open in panel and hide. | US-1.2 |
| FR-3 | Cards belong to a chat: only the showing chat's cards are visible; others keep running; positions, sizes and minimized state are kept per chat and tab. | US-3 |
| FR-4 | Minimized or hidden cards are chips in a row above the composer, in the chat's order; a chip shows the tab's icon and title and brings its card back. | US-4.2 |
| FR-5 | A card can be dragged and resized inside the chat area; it opens at a default place that does not cover the composer, stacking from the top right; a window resize keeps it inside. | US-4.1 |
| FR-6 | "Open in panel" opens the side panel on that tab and removes its card; a tab shown in the panel has no card. | US-4.3 |
| FR-7 | A browser card is interactive: clicks, typing and scrolling reach the page. A terminal card is view-only with "Open in panel". | US-5 |
| FR-8 | Moving a tab between card, chip and panel never reloads the page or restarts the shell. | Page state and scrollback survive the moves. |
| FR-9 | A page the agent drives lays out at a desktop viewport (default 1280×800) whether it is in a card, in the panel or hidden; cards and the panel show it scaled to fit. A size set with `browser_viewport` wins. | US-6 |
| FR-10 | Browser tools work with no card and no panel showing (reads, clicks, screenshots, dialogs, downloads). | The agent-browser checks pass with the panel closed. |
| FR-11 | A card appearing does not take focus from the composer or the person's typing. | Typing in the composer is not interrupted. |
| FR-12 | A card never sits above a menu or dialog. | A menu over a card shows the menu. |
| FR-13 | At most three cards are visible per chat; further tabs are chips. | A fourth tab becomes a chip. |
| FR-14 | A setting "Show cards while the agent works" (default on) turns automatic cards off; the chips remain and bring a card up on click. | Setting off: no card appears by itself. |
| FR-15 | Cards appear only for tabs the agent opened or acted on; pages and shells the person opened stay in the panel. | A page opened by the person has no card. |
| FR-16 | A card and its chip go when the tab closes. | Closing the tab removes both. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Dragging and resizing a card feels immediate (no page flicker or reload) on a page with video or animation. | Manual drag on a live page. |
| NFR-2 | The card layer adds no second copy of a page: it is the same page shown in another place. | One webview per tab. |
| NFR-3 | Platform: the browser card works wherever the panel's browser does; terminal cards work on every platform. | Windows checked; others not available here. |
| NFR-4 | Cards never carry the agent's approval questions: approvals stay in the chat. | An approval shows in the chat, not in a card. |

## 5. Success criteria

- SC-1 The person can ask for a research task, leave the panel closed, and see
  every page the agent visits without any extra click.
- SC-2 Switching between chats never shows another chat's page or shell.
- SC-3 A site the agent visits with the panel closed renders its desktop layout.

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-08 — Q: floating card in the main window or an OS window? → A: a card in the main window, per session (person's choice).
- 2026-11-08 — Q: when a turn ends, do cards stay? → A: yes, until closed (accepted with the spec).
- 2026-11-08 — Q: how many cards at once? → A: three, the rest as chips (FR-13, accepted with the spec).
- 2026-11-08 — Q: is hiding a card closing the tab? → A: no, it removes the card and keeps the chip (FR-2, accepted with the spec).
- 2026-11-08 — Q: do cards appear by themselves? → A: yes, with a setting to turn it off (FR-14).
- 2026-11-08 — Q: is a browser card interactive? → A: yes; a terminal card is view-only (FR-7).
- 2026-11-08 — Q: does the desktop viewport apply in the panel too? → A: yes for pages the agent drives, scaled to fit; pages the person opens keep their real size (FR-9).
- 2026-11-08 — Q: browser only? → A: browser and terminal, and the browser keeps a desktop size when shown small or hidden (person's choice).

## 7. History

- 2026-11-08 — Accepted by the person; plan and tasks written.
- 2026-11-08 — Clarified: auto cards with a setting, interactive browser card, desktop viewport in the panel too.
- 2026-11-08 — Draft: research on ChatGPT's desktop PiP (no primary source), then the person chose cards in the main window per session, for browser and terminal, with a desktop viewport.
