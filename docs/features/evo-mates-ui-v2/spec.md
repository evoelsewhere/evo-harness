# Evo Mates UI v2: a richer, clearer teammate app — Spec

| | |
| --- | --- |
| Status | Implemented |
| Area | desktop UI (Evo Mates window) |
| Created | 2026-11-28 |
| Updated | 2026-11-29 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Research | [research.md](research.md) |
| Related | [Evo Mates](../evo-mates/spec.md) (FR-6, FR-29 – FR-38, FR-58 – FR-62, FR-73, FR-84; [ux.md](../evo-mates/ux.md)), [QA 2026-11-28](../evo-mates/qa-2026-11-28.md) |

## 1. Summary

Evo Mates works, but the owner finds it "too easy, simple": every view is a
flat list or a plain chat with the same weight everywhere, a mate's state
is a word, the window cannot fold for a narrow screen (QA H4), and the
profile is a settings form. Mature team-chat and AI-teammate apps
([research.md](research.md)) show what is missing: clear hierarchy,
presence on faces, a quick switcher and keyboard paths, day and unread
markers in conversations, expandable work steps, hand-off cards, a profile
with an overview, and a home that shows the team at work. This feature
redesigns the Evo Mates window around those patterns, keeps every v1
behaviour, and closes the open UI findings of the QA log.

## 2. Scope

### 2.1 Goals

- A clear visual hierarchy in every view: what needs the person first, then
  what is new, then who is working, then the rest.
- Presence everywhere a face shows: working, needs you, couldn't reply,
  queued, paused.
- A sidebar that folds to a rail of faces in a narrow window or on demand
  (QA H4), with collapsible sections and unread counts.
- Fast navigation: a quick switcher with actions, next/previous and next
  unread by keyboard, a shortcuts sheet.
- Richer conversations: day separators, a "New" line, jump to latest, time
  on hover, folded long messages, find in conversation, reply quotes that
  jump, expandable work steps, a stopped line.
- Richer channels: a header with members and their states, the reply rule
  and Stop; hand-off cards with faces; a mention list with roles and states.
- A mate profile with a hero, quick actions and an overview of what the
  mate is, does and costs; fields that save as the person leaves them.
- A home (Today) with a greeting, counters, the team at a glance and a
  clear "Needs you" inbox.
- Designed empty states, onboarding steps above the gallery, Files by day
  in a grid or a list.
- Light and dark, wide to narrow, with no sideways scroll.

### 2.2 Non-goals

- Threads in a side panel for channels: a mate answers a channel, not a
  thread; routing replies per thread is engine work for a later feature
  (evo-mates A-24). Replies to a message stay as quotes (evo-mates FR-61).
- New mate behaviour: holding messages while paused (QA D6), editing and
  clearing memories (QA B7), opening a memory entry from its chip (QA B4)
  and model conduct findings (A3b, C6) stay with the Evo Mates feature.
- Evo Harness's views. Shared components used by both apps change only
  where Evo Mates needs it and Harness keeps its look.
- New data in the engine, the protocol or the database: everything here is
  shown from what Evo already has. Only window preferences (sidebar folded,
  sections folded, Files grid or list) are kept, as other UI preferences
  are.

## 3. User scenarios

### US-1 See the team at a glance (priority P1)

A person opening Evo Mates wants to see at once what waits for them, what
is new and who is working on what, so that they act on the right thing
first.

Acceptance scenarios:

1. **Given** one mate waiting on an approval, two unread conversations and
   one mate working, **when** the person opens Today, **then** the counters
   read 1 needs you, 2 unread, 1 working, the waiting mate is first in
   Needs you with what it waits for, and the working mate shows its face
   moving with "Working…" in the team strip.
2. **Given** the same state, **then** the sidebar shows the waiting mate's
   face with the needs-you mark, the unread ones in bold with a badge, and
   the working one with the working mark.

### US-2 Move around by keyboard (priority P1)

A person who talks to several mates wants to jump between them without the
mouse.

Acceptance scenarios:

1. **Given** the window, **when** the person presses Ctrl+K and types "ka",
   **then** a switcher lists Kai and the actions and messages that match;
   Enter opens Kai.
2. **Given** a conversation, **when** they press Alt+↓, **then** the next
   conversation in the sidebar opens; Alt+Shift+↓ opens the next unread or
   waiting one.
3. **Given** the window, **when** they press Ctrl+/, **then** a sheet lists
   every shortcut.

### US-3 Use Evo Mates in a narrow window (priority P1)

A person who keeps Evo Mates beside other work wants it to stay usable
when narrow (QA H4).

Acceptance scenarios:

1. **Given** a window 720 px wide, **then** the sidebar shows as a rail of
   faces with their marks and badges, each with its name on hover, and the
   conversation keeps at least 440 px.
2. **Given** a wide window, **when** the person presses Ctrl+B, **then**
   the sidebar folds to the rail and stays folded after a restart; Ctrl+B
   unfolds it.
3. **Given** any width from 640 px up, light or dark, **then** no view
   scrolls sideways.

### US-4 Read a long conversation (priority P1)

A person coming back to a mate after a day wants to find where they left
off and what happened.

Acceptance scenarios:

1. **Given** replies that came while the person was away, **when** they
   open the conversation, **then** a "New" line sits above the first unread
   message and the view starts there.
2. **Given** the person scrolled up and a reply lands, **then** a "Jump to
   latest · 1 new" pill shows; clicking it goes to the bottom.
3. **Given** messages over three days, **then** each day starts with a
   separator ("Today", "Yesterday", "Wednesday 26 November") that sticks to
   the top while its messages scroll.
4. **Given** Ctrl+F and a word, **then** the matches are marked, the count
   shows, and Enter / Shift+Enter step through them.
5. **Given** a reply quote, **when** it is clicked, **then** the view
   scrolls to the quoted message and highlights it briefly.

### US-5 See how a mate worked (priority P2)

A person wants to see what a mate did for a reply without developer
words.

Acceptance scenarios:

1. **Given** a reply that read three files and searched the web, **then**
   a chip "Read 3 files · Searched the web" shows under it; clicking it
   lists each step in plain words with its file or query.
2. **Given** a mate at work, **then** the chip shows the step running now
   and grows as steps finish.
3. **Given** the person pressed Stop, **then** a "Stopped" line shows
   where the reply ended.

### US-6 Follow a channel's work (priority P1)

A person running a channel wants to see who is in it, who is working, and
how work moved between mates.

Acceptance scenarios:

1. **Given** a channel with four mates, two working, **then** the header
   shows the face stack with the two working marks, the reply rule and
   **Stop**; clicking the faces lists every member with role and state.
2. **Given** a lead that assigned work to two mates, **then** each
   hand-off shows as a card: the lead's face → the member's face,
   "assigned", a one-line summary that opens to the full text.
3. **Given** the person types `@`, **then** each member shows with face,
   name, role and state.

### US-7 Know a mate (priority P2)

A person wants to understand a mate before changing it.

Acceptance scenarios:

1. **Given** a mate, **when** the person opens its profile, **then** they
   see a hero (large face on the mate's colour, name, role, state), quick
   actions (Message, Pause, Mute, Pin, more) and an Overview: model,
   approvals, sending, apps, today's replies and cost, what it follows up
   next, its channels and its latest files.
2. **Given** the person changes the name and leaves the field, **then** it
   is saved and a "Saved" mark shows; no Save button is needed.

### Edge cases

- No mates: the gallery greets the person (evo-mates A-5) with the steps
  strip; the rail shows only Today, Files and New.
- Fifty mates and twenty channels: the sidebar, rail and switcher stay
  smooth; the rail scrolls.
- A mate with no replies yet: no "New" line, no day separator, the
  greeting and starters (evo-mates FR-74).
- A reply quote whose message is no longer loaded or was compacted: the
  click does nothing visible but a quiet "Not in this part of the
  conversation" notice.
- Find with no match: "No matches", Enter does nothing.
- A channel member removed while working: its mark goes when its seat
  stops.
- Reduced motion: presence marks hold still; the face stays still.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | The sidebar must have two forms: **full** (280 px) and **rail** (68 px, faces only). It shows as a rail when the window is narrower than 760 px, or when the person folds it (Ctrl+B or the fold button); the person's choice is kept across restarts and applies only when the window is wide enough. | At 720 px the rail shows; at 1280 px Ctrl+B folds and the choice survives a reload; at 720 px Ctrl+B does not unfold past the rule. |
| FR-2 | The rail must show Today (with a badge of what needs the person), Files, New, every pinned mate, mate and channel as a 40 px tile with its presence mark and unread badge, and the footer actions as icons; every tile names itself on hover and has the same menu as its full row. | Hover a tile shows its name; right-click shows its menu; badges match the full sidebar. |
| FR-3 | The full sidebar's **Pinned**, **Mates** and **Channels** sections must fold and unfold from their headers, remember it, and show how many of their conversations are unread when folded; Today and Files stay on top. | Fold Mates with an unread mate: the header shows 1; reload: still folded. |
| FR-4 | A roster row must show: unread in bold with an unread badge (Evo keeps an unread mark per conversation, not a count); a second line that says, in this order, "Needs you" (accent), the step running now or "Working…", "Queued", "Paused", "Couldn't reply", else the last message as plain text with Markdown marks removed (QA G2); a channel row says "Working…" with the working mate's name (QA C3), else "Name: last line". | Given each state, the row's line matches; a reply that is a code block previews as text without backticks. |
| FR-5 | Every face in the roster, rail, header, profile, mention list, Today and a channel's member list must carry the mate's **presence mark** at its corner: working (accent, a soft pulse unless reduced motion), needs you (amber), couldn't reply (red), queued or paused (hollow grey); idle has none. | Given a mate in each state, the mark matches in every place; with reduced motion, nothing pulses. |
| FR-6 | Each row must show a **⋯** button on hover or focus that opens its menu; a mate's menu adds **Pause / Resume** (QA D7) and **Mute / Unmute notifications**. | Hover a row: ⋯ shows; Pause in the menu pauses the mate. |
| FR-7 | Ctrl+K must open a **quick switcher** over the window: with no text, the recent conversations and the main actions; with text, matching mates (name, role), channels, actions (New mate, New channel, Import a team, Today, Files, Settings, Customize, Pause all, Toggle theme, Fold sidebar) and, from two characters, matching messages (evo-mates FR-76); arrow keys move, Enter opens, Esc closes. The sidebar's search box still filters the roster. | Type "ka": Kai is first; Enter opens Kai; type "new ch": New channel opens its dialog. |
| FR-8 | Keyboard: Alt+↑/↓ previous / next conversation in sidebar order; Alt+Shift+↑/↓ previous / next unread or waiting one; Ctrl+B fold sidebar; Ctrl+I open or close the profile or channel members; Ctrl+F find in the conversation; Ctrl+/ shortcuts sheet; Ctrl+K, Ctrl+N, Ctrl+1 – 9 and Esc as before (⌘ on macOS). Shortcuts do not fire while a dialog is open, except Esc. | Manual check of each; the sheet lists all. |
| FR-9 | A mate's header must show its live face with presence, its name, and its state word or role; on the right **Find**, the model pill, the profile toggle and a ⋯ menu with the row's actions. A paused mate's header shows **Resume**. | Visual check; Resume in the header resumes. |
| FR-10 | The conversation must show the composer area's state plainly: while a question or approval waits, no typing dots show (QA B5); a paused mate shows a "Paused · Resume" bar above the box. | Given a waiting question card, no dots; given a paused mate, the bar shows. |
| FR-11 | Messages must be grouped by day with a separator ("Today", "Yesterday", weekday and date) that sticks to the top of the conversation while its day scrolls; the 15-minute time lines (evo-mates FR-58) stay inside a day. | Messages over three days show three separators; scrolling keeps the current one on top. |
| FR-12 | Opening a conversation with unread replies must place a **New** line above the first unread reply and start there; the view follows new messages only when the person is at the bottom, and otherwise shows a **Jump to latest** pill with the count of new messages. | Given two replies while away: the line and the start position; scrolled up when a reply lands: the pill with 1. |
| FR-13 | A message must show its time on hover (inside a run too), and its hover bar must hold the six reactions, Reply, Copy and, for the mate's replies, the reply's details (model, time, cost). | Hover any bubble: time and bar; details show model and cost. |
| FR-14 | A message longer than 16 lines or 1,200 characters must fold to that height with **Show more** / **Show less** (QA F1). | A 6,000-character message folds; Show more unfolds. |
| FR-15 | Ctrl+F (or Find in the header) must open a find bar for the shown conversation or channel: matches are marked in the messages, the bar shows "n of m", Enter and Shift+Enter step, Esc closes and clears the marks. It searches the messages loaded in the view. | A word written twice: "1 of 2", Enter goes to the second. |
| FR-16 | Clicking a reply's quote must scroll to the quoted message and highlight it for about 1.5 s; if it is not in the view, a quiet notice says so. | Reply to an earlier message; click the quote: the original flashes. |
| FR-17 | A mate's reply must show its work as a **steps chip** ("Read 3 files · Searched the web") that opens in place to a list of each step in plain words with its target (file name, page, query) and whether it failed or was denied; while the mate works, the chip shows the running step. Raw tool names never show (evo-mates FR-31; QA B3). | A turn with reads and a search: the chip and its list; a denied edit shows "Not allowed: edit notes.md". |
| FR-18 | A reply the person stopped must end with a "Stopped" line (QA D2). | Stop a running reply: the line shows. |
| FR-19 | A channel's header must show its name, the first line of its instructions, a face stack with presence marks that opens the **members** list (face, name, role, state, Message), the reply rule as a chip that opens a menu to change it, **Stop** while any member works (QA C7), **Find**, and a ⋯ menu with Run retro, Export, Settings and Delete. | Two members working: two marks and Stop; change the rule from the chip: the channel's rule changes. |
| FR-20 | A hand-off must show as a card: the sender's face → the receiver's face, the kind as a verb ("assigned", "asked", "told"), a one-line summary, and the full text when opened (QA C1); the route line shows the picked mates' faces. | A lead assigning two mates: two cards with both faces; opening one shows its text. |
| FR-21 | The `@` list must show each member's face with presence, name and role (QA C2), and `@everyone` first. | Type `@`: every member with role. |
| FR-22 | The profile must open with a **hero**: the face (72 px, live) on a band tinted with the mate's colour, name, role and state, and quick actions **Message**, **Pause / Resume**, **Mute / Unmute**, **Pin / Unpin** and ⋯ (Duplicate, Export, Delete). | Visual check; each action works. |
| FR-23 | The profile's first tab, **Overview**, must show: model, approvals and sending in plain words; apps and skills switched on (counts); today's replies and cost; the next three Following up items; the channels the mate is in (each opens); its latest four files (each opens). The other tabs follow: **Profile** (face, colour, name, role, instructions), **Settings**, **Apps**, **Memory**, **Files**. | A mate with a check-in, two channels and files: each shows and opens. |
| FR-24 | Profile fields (name, role, instructions) must save when the person leaves the field or presses Enter in a one-line field, show "Saved" for a moment, and an empty name is refused with a note (QA A6). | Change the role and Tab away: "Saved", the roster shows the new role. |
| FR-25 | Today must open with a greeting for the time of day and the date, four counters — **Needs you**, **Unread**, **Working now**, **Spent today** — each of which scrolls to its section, and **Your team**: every mate's face with presence and its now line (the step, the state or "Free"), each opening its conversation. | Given the US-1 state, the counters and strip match. |
| FR-26 | Today's **Needs you** must list each waiting mate or channel oldest first with its face, its place and what it waits for ("Approval", "Question", "Sign-in", "Secret", else "Waiting for you"), and open it. | A waiting approval reads "Approval" with the mate's face; clicking opens its card. |
| FR-27 | An approval, question or secret card in a mate's conversation must sit under a header with the mate's face, "{name} needs you" and the kind. | Visual check on each kind. |
| FR-28 | The gallery must open with a three-step strip (Pick a role · It introduces itself · Shape it as you go), **Describe a mate** as the first card, then roles and teams in the current tabs; cards lift on hover and show the face on its plate. | Visual check; Describe a mate works as before. |
| FR-29 | Every empty list must show one sentence and one action: no mates (New mate), no channels (New channel), no search match (Clear search), empty Memory, empty Following up (Add a check-in), no files, a channel with no messages (its members and "@mention a mate"). | Visual check of each. |
| FR-30 | Files must group by day (Today, Yesterday, dates) and offer **Grid** and **List**; the choice is kept. In the grid every file shows a **thumbnail**: a picture fills its tile, a text file (Markdown, text, code) shows its first lines as a small page with its headings, a CSV or TSV its first rows as a small table, anything else its kind; each with its kind as a tag. A thumbnail is read only once its tile is on screen. | Toggle to List, reload: List; a picture, a Markdown note and a CSV each show their thumbnail. |
| FR-31 | Widths: from 1100 px the profile docks beside the conversation; under it, it floats over the conversation with a scrim that closes it on click; under 760 px the sidebar is a rail (FR-1). | Checks at 1440, 1000 and 720 px. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | No view scrolls sideways at 1440, 1180, 900, 760 and 640 px wide, light or dark. | The QA overflow scan (`scrollWidth > clientWidth`) on each view at each width. |
| NFR-2 | Light and dark both follow DESIGN.md's tokens; presence colours and text on tints keep WCAG AA contrast; the Mates colour layer (DESIGN.md "Evo Mates layer", with the profile hero band of 6.2) is the only place mate hues show. | Screenshots in both themes; contrast check of the marks and badges. |
| NFR-3 | Reduced motion stops every pulse, lift and face loop. | Emulated reduced motion: nothing moves. |
| NFR-4 | No change to the engine, the protocol or the database; window preferences live with the app's other UI preferences. | Diff review. |
| NFR-5 | With 50 mates and 20 channels the switcher opens and filters within 100 ms and the sidebar scrolls smoothly. | Measured in the fake-IPC harness. |
| NFR-6 | Every action is reachable by keyboard with a visible focus ring; icon buttons have names; the presence mark has a text equivalent. | Keyboard walk; accessibility tree check. |
| NFR-7 | UI words follow evo-mates NFR-7: "Evo", "mate", no developer words. | Text search of new strings. |

## 5. Success criteria

- SC-1 In a design review against [research.md](research.md), every
  "Taken" pattern is present in the window.
- SC-2 A person can open any mate, the next unread one and any action
  without the mouse.
- SC-3 At 720 px the window is fully usable with the conversation at least
  440 px wide.
- SC-4 The QA findings H4, B3, B5, C1, C2, C3, C7, D2, D7, F1, G2 and A6
  are closed.

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-28 — Q: May the profile hero's band use the mate's colour, extending
  DESIGN.md's Evo Mates layer? → A: yes, a soft tint at the plate's strength;
  nothing else gets the mate's hue (FR-22, NFR-2).
- 2026-11-28 — Q: Ctrl+K becomes a quick switcher instead of focusing the
  sidebar search? → A: yes; the search box stays for filtering the roster
  (FR-7).
- 2026-11-28 — Q: Fold the sidebar to a rail of faces under 760 px and on
  Ctrl+B? → A: yes (FR-1, FR-2).
- 2026-11-28 — Q: Leave side-panel threads in channels out of this round? →
  A: yes; replies stay quotes that jump to the original (2.2, FR-16).
- 2026-11-29 — Owner, looking at the Files grid: "có thumbnail sẽ đẹp hơn"
  (thumbnails would look better). → Every file in the grid shows a thumbnail
  of what it holds (FR-30).

### 6.3 Assumptions

- A-1 "Working now" counts mates whose conversation or any channel seat is
  working — because that is what the person sees moving.
- A-2 The "New" line uses the conversation's unread mark when it is opened:
  the replies after the person's last message are new — because Evo keeps
  an unread flag per conversation, not a read position.
- A-3 Find searches the messages loaded in the view, not the whole history
  — the roster search (evo-mates FR-76) already covers the whole history.
- A-4 The steps chip reads the turn's tool steps already in the transcript;
  steps of a channel's seats are not shown in the channel — because a seat's
  steps live in its own chat.
- A-5 Day separators use the computer's local time and language for dates.

## 7. History

- 2026-11-28 — Draft, from the owner's request for a richer Evo Mates UI;
  research of team-chat and AI-teammate apps ([research.md](research.md));
  four questions open (6.1).
- 2026-11-28 — Gate A: the owner accepted the spec with the four proposed
  answers (6.2).
- 2026-11-28 — Gate B: the owner accepted the plan; tasks written; status
  In progress.
- 2026-11-29 — Owner asked for thumbnails in Files: FR-30 changed (6.2).
- 2026-11-29 — Implemented and verified in the fake-IPC harness (tasks.md);
  status Implemented.
