# Evo Mates UI v2 — Research: how mature teammate apps design their views

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Earlier research | [evo-mates/research-2026-11.md](../evo-mates/research-2026-11.md) (features), [evo-mates/ux.md](../evo-mates/ux.md) (first layout) |
| QA it answers | [evo-mates/qa-2026-11-28.md](../evo-mates/qa-2026-11-28.md) (H4, B3, B5, C1 – C3, C7, D2, D7, F1, G2, A6) |
| Updated | 2026-11-28 |

The owner found Evo Mates "too easy, simple". The earlier research chose
*features*; this page looks at *how* mature team-chat apps and AI-teammate
apps present them: hierarchy, density, states and navigation. Sources: the
agent's knowledge of the leading team-chat, messaging and AI-teammate apps,
and a web search of 2026 write-ups on agent presence, agent inboxes and chat
UX. Patterns only; no product names, artwork or wording are taken (repository
rule). Each pattern says how common it is: **very common**, **common**,
**some**.

## 1. Where Evo Mates is thin today

Measured against those apps, the v1 window is a correct but flat chat:

- **Hierarchy.** Every sidebar row, header and page uses the same 13 px text
  on the same grey; nothing tells the eye what matters first (who needs me,
  who is working, what is new).
- **Presence.** A mate's state is a word in its row; the face does not show
  it at a glance, and a channel row shows a member count while its mates
  work (QA C3).
- **Navigation.** One search box; no switcher, no next-unread, no way to
  fold the sidebar (QA H4: it never folds in a narrow window).
- **Messages.** No date separators, no "new since you left" line, no jump
  to latest, no time on hover; long messages never fold (F1); step lines are
  flat text that never open (B3); hand-offs repeat the whole bubble (C1).
- **Profile.** A settings form in five tabs; no overview of what the mate
  is, does and costs.
- **Home.** Today is four plain lists; nothing shows the team as a team.
- **Headers.** A channel bar is four identical icon buttons; Stop lives only
  in the message box (C7).

## 2. Patterns

### 2.1 Sidebar and roster

| Pattern | How common | Taken |
| --- | --- | --- |
| Sidebar folds to a **rail of avatars/icons** when the window narrows or on a toggle; tooltips name each item; badges stay visible | very common | yes — FR-1, FR-2 |
| Sections (Home, Pinned, Direct, Channels) **collapse** and remember it; a section header shows an unread count when folded | very common | yes — FR-3 |
| **Unread is bold** (name and line) plus a count badge; mentions/needs-you get the accent | very common | yes — FR-4 |
| **Presence on the avatar**: a small status dot/ring at its corner (online, busy, away); for agents: working, waiting on a person, error | very common (people), common (agents) | yes — FR-5 |
| The row's second line shows **what the agent is doing now** ("Reading 3 files…") instead of the last message while it works | common for agents | yes — FR-4 |
| Hover reveals a row's quick actions (⋯ menu); right-click has the same | very common | yes — FR-6 |
| Pinned items as a compact strip of faces | some | kept from v1 |

### 2.2 Navigation and keyboard

| Pattern | How common | Taken |
| --- | --- | --- |
| A **quick switcher / command palette** (Ctrl+K): conversations, people, actions, recent first, fuzzy match, arrow keys | very common | yes — FR-7 |
| **Next / previous conversation** and **next unread** by keyboard (Alt+↑/↓, Alt+Shift+↑/↓) | very common | yes — FR-8 |
| A **shortcuts sheet** (Ctrl+/) | common | yes — FR-8 |
| Toggle sidebar and detail panel by keyboard | common | yes — FR-8 |

### 2.3 Conversation view

| Pattern | How common | Taken |
| --- | --- | --- |
| **Day separators** ("Today", "Yesterday", "Monday 24 November") that stick to the top while scrolling | very common | yes — FR-11 |
| A **"New" divider** at the first unread message; the view opens there when the person was away | very common | yes — FR-12 |
| Stick to bottom only when already near it; a **jump-to-latest pill** with the count of new messages below | very common | yes — FR-12 |
| **Time on hover** for messages inside a run | very common | yes — FR-13 |
| A compact **hover toolbar** (quick reactions, reply, copy, more) on the message's top corner | very common | yes, refined — FR-13 |
| **Long messages fold** with "Show more" | common | yes — FR-14 |
| **Find in conversation** (Ctrl+F) with match count and up/down | common | yes — FR-15 |
| Clicking a **reply quote scrolls to the original** and highlights it | very common | yes — FR-16 |
| Side-panel **threads** in channels | very common in team chat | no — needs engine routing per thread; left (spec 2.2) |
| Agent **tool steps as expandable chips** in plain words; a live "now doing" line | common for agents | yes — FR-17 |
| A visible **stopped / interrupted** line | common | yes — FR-18 |

### 2.4 Channels and hand-offs

| Pattern | How common | Taken |
| --- | --- | --- |
| Header: name, topic/instructions line, **member face stack** that opens a member list with each one's state | very common | yes — FR-19 |
| One **overflow menu** for rare actions (export, settings, delete); only the frequent ones as buttons | very common | yes — FR-19 |
| **Stop** in the header while work runs | common for agents | yes — FR-19 |
| **Hand-off cards**: from-face → to-face, a verb ("assigned", "asked", "told"), a one-line summary that expands | common in agent tools | yes — FR-20 |
| Mention list with **avatar, name, role and state** | very common | yes — FR-21 |

### 2.5 Profile

| Pattern | How common | Taken |
| --- | --- | --- |
| A profile **hero**: large avatar on a colour band, name, role, status, then **quick actions** (message, mute, pause, more) | very common | yes — FR-22 |
| An **overview** before the settings: key facts (model, permissions, tools), activity (recent work, cost), where it is a member | common for agents | yes — FR-23 |
| Settings grouped under headed cards; edits save on leaving a field or show **unsaved** | very common | yes — FR-24 |

### 2.6 Home, activity and approvals

| Pattern | How common | Taken |
| --- | --- | --- |
| A home with a **greeting**, **counters** (waiting on you, unread, working now, spend) and the **team at a glance** (each agent's face and what it is doing) | common for agent workspaces | yes — FR-25 |
| An **inbox of what waits on the person**, each with enough context to decide and the agent's face, oldest first | very common for agents | yes — FR-26 |
| Approval/question cards framed with **who is asking** and why, in the conversation | common | yes — FR-27 |
| Activity feed of everything agents did | common | partly: Today's "Your team today" and "New" cover it; a full log is left |

### 2.7 Onboarding, empty states, files

| Pattern | How common | Taken |
| --- | --- | --- |
| First run: a short **"how it works" strip** (pick, it introduces itself, shape it) above the gallery | common | yes — FR-28 |
| Every empty list has **one sentence and one action** | very common | yes — FR-29 |
| Files as **grid or list**, grouped by day | common | yes — FR-30 |

### 2.8 Responsive and theme

| Pattern | How common | Taken |
| --- | --- | --- |
| Three widths: wide (sidebar + conversation + panel), medium (panel floats), narrow (sidebar rail) | very common | yes — FR-1, FR-31 |
| Light and dark built from the same tokens; status colours readable in both | very common | yes — NFR-2 |
| Reduced motion turns every loop off | very common | kept (NFR-3) |

## 3. Agent-specific lessons from the 2026 write-ups

- Agents need **presence like people**, but with richer states: working,
  waiting on a person, blocked, done. Show it where the eye already is (the
  face), not only in words.
- An approval needs **context**: who asks, what for, in which place; a bare
  pair of buttons breeds approval fatigue.
- The home should answer three questions first: **what waits on me, what
  is new, who is doing what**.
- Work should be visible **while** it happens (a "now doing" line), not
  only when it is done.
