# Evo Mates at the computer (Train 2) — Spec

| | |
| --- | --- |
| Status | Implemented |
| Area | desktop UI (Evo Mates window), desktop shell (agent tools) |
| Created | 2026-11-29 |
| Updated | 2026-11-29 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Evo Mates](../evo-mates/spec.md) (2.2: mates use the browser, app control and Webbridge "as chats do today"), [Floating cards](../floating-cards/spec.md), [Agent browser](../agent-browser/spec.md), [App control](../computer-app-control/spec.md), [PO backlog](../evo-mates/po-backlog-2026-11-29.md) (Train 2, Q5) |

## 1. Summary

Mates already get the browser, app control and Webbridge tools in their
turns, but the Evo Mates window cannot show that work: no live page, no
take-over, steps read "Used the browser", and the person cannot decide per
mate whether it may browse or control apps. Agent apps show a working
agent's screen live beside the chat, let the person take over, describe
each step in words and let the person switch an agent's tools. This train
brings that to Evo Mates.

## 2. Scope

### 2.1 Goals

- See a mate's page or controlled app live, over its conversation, and take
  over, as Evo Harness's floating cards do.
- Read browser, app-control and Webbridge steps in plain words.
- Switch, per mate, whether it may use the browser, control apps on the
  computer, and use the person's own Chrome (Webbridge); the shell enforces
  it.

### 2.2 Non-goals

- A mate's own computer or virtual machine (Evo Mates 2.2, phase 3).
- Live cards for channel seats (a seat's chat is not the one the window
  shows): Later.
- New approval rules: the browser and app-control gates stay as they are
  (ADR 0013, ADR 0015); their cards show in the Mates frame of UI v2 FR-27.

## 3. User scenarios

### US-1 Watch and take over (P1)

1. **Given** Writer opens a page in its conversation, **then** a card with
   the live page floats over the conversation; the person can click into it
   to take over, and close or shrink it.
2. **Given** a mate controls an app, **then** the app's card shows it, with
   its control state, as in Evo Harness.

### US-2 Read what it did (P1)

1. **Given** a reply after browsing, **then** its steps chip opens to
   "Opened example.com", "Clicked “Sign in”", "Typed in “Search”", "Took a
   screenshot", never a tool's name.

### US-3 Decide what a mate may do (P1)

1. **Given** Writer's profile › Apps › At the computer, **when** the person
   switches the browser off, **then** Writer's next turn has no browser
   tools, and a call that still names one is refused with a plain reason.
2. **Given** "Your Chrome" off, **then** Writer can browse in Evo's own
   pages but not in the person's Chrome tabs.

### Edge cases

- App control is offered only while Evo's Computer setting is on; the
  mate's switch cannot turn it on.
- Switches apply to the mate's direct conversation and its seats.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | The Evo Mates window must follow its mates' computer work: the browser pages and controlled apps of the shown mate's conversation float as live cards over it (Evo Harness's floating cards: live picture, take over, shrink, close), and the shell's panel and card events reach this window. | Given a mate's page, the card shows over its conversation. |
| FR-2 | Browser, Webbridge and app-control steps read in plain words with their target: opened (host), clicked / typed in / chose (the element's words), scrolled, read the page, took a screenshot, ran a script, waited, controlled (the app). | `stepItems` test of each kind. |
| FR-3 | Each mate has three switches in its profile (Apps › At the computer): **Browser**, **App control** (shown while Evo's Computer setting is on) and **Your Chrome** (Webbridge); on by default. Off removes the tools from the mate's turns and refuses a call that names one, in its conversation and its seats. | Shell test of the rule; profile shot. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | No new approval path: the gates of ADR 0013 and 0015 stay in force for mates. | Review. |
| NFR-2 | The switches live in the app setting `mates.computer`, no migration. | Review. |
| NFR-3 | Shots at 1440, 1000 and 720 px, light and dark, no sideways scroll. | Shots and the scan. |

## 5. Success criteria

- SC-1 A person watching a mate book something on a site sees the page live
  and can take over without leaving Evo Mates.

## 6. Clarifications

### 6.1 Open

None. **Questions for the owner:** live cards for channel seats (Later).

### 6.2 Resolved

- 2026-11-29 (owner away, accepted by the product owner) — Q: a side panel
  or the floating cards? → A: the floating cards Evo Harness already has,
  so both apps show a working agent the same way (FR-1).
- 2026-11-29 (owner away, accepted by the product owner) — Q: default of the
  per-mate switches? → A: on, as chats have them today (Evo Mates 2.2);
  app control still needs Evo's Computer setting (FR-3).

### 6.3 Assumptions

- A-1 "Your Chrome" off refuses a browser call on a Webbridge tab; the
  mate's own pages stay allowed.

## 7. History

- 2026-11-29 — Draft from the backlog's Train 2 (Q5) and a read of the
  browser, app-control and Webbridge code; Gates A and B accepted by the
  product owner (owner away); implemented and verified in the fake-IPC
  harness and with shell tests.
