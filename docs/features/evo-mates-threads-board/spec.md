# Evo Mates threads, board and linked conversations (Train 2) — Spec

| | |
| --- | --- |
| Status | Implemented |
| Area | desktop UI, desktop shell, engine (channel), `evo-store` |
| Created | 2026-11-29 |
| Updated | 2026-11-29 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Research | [research.md](research.md) |
| Related | [Evo Mates](../evo-mates/spec.md), [messaging basics](../evo-mates-messaging/spec.md), [PO backlog](../evo-mates/po-backlog-2026-11-29.md) (Train 2, B1 – B13) |

## 1. Summary

A channel is one group chat: every line goes into one feed and every mate's
delivery carries the channel's recent lines. Work gets lost in it, and the
owner asked for two things: information passing between conversations, and a
board like an issue tracker for channels that track work. This train adds
work threads (a piece of work, its own feed and context), a board per
channel built on them (cards, columns, mates working the board through
tools), and linked conversations (shares with a link back, links between
places, a consented `share_to` for mates, following threads).

It merges in three parts, each verified and merged on its own: **A threads**
(FR-1 – FR-7), **B board** (FR-8 – FR-15), **C linking** (FR-16 – FR-22).

## 2. Scope

### 2.1 Goals

- Reply in a thread from any channel line; the thread has its own feed,
  composer, typing line, reactions, files and Stop.
- A thread message reaches the thread's mates with the thread as context,
  never the channel's unrelated lines.
- A board per channel, opt-in, that the person and the channel's mates both
  work, with an activity log; assignments become cards.
- Cards and threads linked both ways; the person's cards on Today.
- Share lines into another conversation with a visible card and a link back;
  links between conversations and messages; mates share only with consent.
- Follow threads; a Threads list with unread first; resolve a thread.

### 2.2 Non-goals

- Threads in a mate's direct conversation (it has one mate: replies there
  stay quotes, messaging FR-5).
- Several thread jobs at once in one channel (Later, 6.2 D1).
- Sprints, swimlanes, story points, WIP limits, card templates (Later).
- Moving cards between boards; a board shared by channels.
- A mate reading another mate's direct conversation (evo-mates FR-50 holds).

## 3. User scenarios

### US-1 Keep a piece of work in its own thread (P1)

The person asks in #launch-room "Kai, cut the build". Later, under Kai's
answer, they pick **Reply in thread** and write "add the arm64 build too".
Only Kai (who replied) is woken; its input has the parent line and the
thread, not the channel's other talk. Its answer shows in the thread panel;
the channel line reads "1 reply · Kai · now".

### US-2 Track the work on a board (P1)

The person turns the board on for #launch-room (key `LAUNCH`). Ava, the
lead, assigns two tasks: two cards appear in To do, assigned to Kai and Lin,
each linked to the thread its assignment came from. Kai reports done: its
card moves to Review, the log says "Kai moved LAUNCH-3 to Review", and the
channel shows the same line. The person drags it to Done.

### US-3 See my cards everywhere (P2)

A card assigned to the person shows on Today under **My cards**, overdue
ones first. `LAUNCH-3` written in Analyst's chat is a link that opens the
card.

### US-4 Pass information between conversations (P1)

The person selects three lines in #launch-room and shares them into
Analyst's chat with a note. Analyst's chat shows one "Shared from
#launch-room" card with a link back; Analyst reads it as shared material,
not as the person's request. Both conversations list each other under
**Linked**. Writer, asked to, calls `share_to` #marketing: the person is
asked "Share the launch draft with #marketing?" and on Allow the card shows
in #marketing.

### US-5 Follow threads (P2)

A mate replies in a thread the person started: it shows unread under
**Threads** in the sidebar. The person resolves it: it folds, with
Unresolve.

### Edge cases

- A thread on a line that is itself in a thread: the reply goes to the same
  thread (threads are one level deep).
- A thread's parent no longer in the engine's memory (restart): the
  coordinator rebuilds lines from the channel's entries (FR-7).
- A thread message while the channel works on another message: it waits in
  the channel's queue; one that mentions only a mate at work in the same
  thread steers it (messaging FR-56).
- The board turned off: the tab goes, cards stay; tools answer "This
  channel's board is off".
- A mate removed from a channel: its cards keep their assignee, shown
  greyed; tools refuse it.
- A column removed with cards in it: they move to the first column.
- `share_to` a place the mate is not in and does not know: refused.
- A share of a direct conversation with the person: always asks, whatever
  the setting (B8).

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance |
| --- | --- | --- |
| FR-1 | **Reply in thread** (B10) on any message line of a channel opens a thread panel beside the feed: the parent line on top, the thread's lines, its own composer (mentions, emoji, files), the typing line of the mates at work in it, reactions on its lines, and Stop. A line inside a thread offers no Reply in thread. | Replying in a thread shows the reply in the panel only. |
| FR-2 | The parent line shows a **thread summary**: "3 replies · Writer, Analyst · 2 min ago" with the repliers' faces (up to 3); a click opens the thread. | The count updates as replies land. |
| FR-3 | Thread lines (and hand-offs, notices and route lines of a thread message) stay out of the channel feed and out of channel deliveries. | A channel delivery after a thread carries none of its lines. |
| FR-4 | **Thread routing** (B11): a thread message goes to the mates it mentions; with no mention, to the thread's mates (the parent's author if a mate, mates mentioned in the thread, mates who replied in it); with none of those, by the channel's rule. A quote or the parent's text never routes. | A thread on Kai's line wakes only Kai; "@Lin" in it wakes Lin. |
| FR-5 | **Thread context**: a thread delivery carries the channel frame, the parent line marked as the parent (`instruction="false"`), the thread's lines (at most 30, newest last) and the message; a mate's answer posts in the thread. | A mate's input in a thread has no unrelated channel line. |
| FR-6 | Hand-offs (asks, assigns, tells) made during a thread message stay in the thread; the hand-off limit counts per message, so a thread never uses the channel's count. Stop in the thread stops the job when it is the thread's. | A hand-off made in the thread shows in the thread. |
| FR-7 | After an engine restart, the coordinator rebuilds its lines (with their threads) from the channel's entries, so threads, `react` and quotes keep working. | Restart, reply in an old thread: the parent is in the delivery. |
| FR-8 | **Board, opt-in** (B1): **Turn on board** in the channel's panel asks for a key (proposed from the name, 2 – 6 capitals) and adds a **Board** tab beside **Chat**. Columns start as To do, In progress, Review, Done; they can be renamed, added, reordered or removed. Turning it off hides the tab and keeps the cards. Cards are numbered `KEY-n`, never reused. | Turning it on shows four empty columns. |
| FR-9 | **Cards** (B2): title, Markdown description, assignee (a mate of the channel or the person, with a face), priority (None, Low, Medium, High, Urgent), due date, labels, checklist, links (messages, files, threads), and an activity log of every change with who made it and when. | A mate's move logs "Writer moved LAUNCH-12 to Review". |
| FR-10 | **The person works the board** (B3): drag cards between and within columns; quick add at a column's top; open a card in a side sheet to edit every field; filters by assignee, label and priority; search by key and words; a list view beside the columns. | Dragging to Done saves and shows at once; "assignee: Writer" leaves Writer's cards. |
| FR-11 | **Mates' board tools** (B4): `board_list`, `board_create`, `board_update`, `board_move`, `board_comment`, offered to a seat whose channel's board is on, acting only on that board; no approval (6.2 D7); mates cannot delete cards. | A seat lists, creates, moves and comments; another channel's card is refused. |
| FR-12 | **Assignments become cards** (B4): with the board on, a lead's assign creates a card in the first column assigned to the member, linked to the thread or message it came from; the member's report back moves it to Review (or the last column but one when there is no Review; Done when there are two columns or fewer). | The lead assigns two tasks: two cards appear; reporting moves them. |
| FR-13 | Every board change by a mate or by the coordinator posts a short line in the channel ("Analyst moved LAUNCH-7 to Done"), shown as a board line, never delivered to mates as a message. | The line shows in the feed. |
| FR-14 | **Beyond one channel** (B5): Today shows **My cards** (assigned to the person, overdue first); a mate's profile shows its open cards across channels; `KEY-n` of a board in any message is a link that opens the card. | A card assigned to the person shows on Today; `LAUNCH-12` in a mate's chat opens it. |
| FR-15 | **Thread ↔ card** (B12): **Track on board** on a thread creates a card linked to it; a card's sheet opens its thread; an assignment made in a thread links its card to that thread; the thread's header shows the card's key and column, live. | Track on board creates LAUNCH-13 linked both ways; moving it updates the header. |
| FR-16 | **Share to another conversation** (B6): pick one line, several (select mode) or a stretch (from a line to the end), choose a mate's chat or a channel, add a note. The receiver shows a "Shared from #launch-room" card with the excerpt and a link back; its mate reads it as `<shared from=… instruction="false">`, the note as the person's words. | Sharing three lines into Analyst's chat shows one card; Analyst's next turn can quote it. |
| FR-17 | **Links** (B7): `#channel` and `@mate` in any message are links that open the place; **Copy link** on a line gives `evo://mates/<chat>/<line>`; pasted anywhere, it shows as a link card and opens that line in its place. | Clicking a pasted link opens the original line. |
| FR-18 | **`share_to` for mates** (B8): a mate posts a short summary or a file into a channel it is in (into another mate's own conversation: Later, see History), under its **Share across conversations** setting: Ask every time (default), Allow, Never. A share out of the mate's direct conversation with the person always asks. On Ask the person gets an approval with the target and the text. | Writer asks "Share the launch draft with #marketing?"; on Allow the card shows there. |
| FR-19 | **Linked** (B9): a channel's or a mate chat's panel lists the conversations linked to it (shared into, shared from, cards with links there), newest first, each opening the place. | After FR-16 both conversations list each other. |
| FR-20 | **Following threads** (B13): the person follows threads they started, replied in or were mentioned in (and can follow or unfollow any); **Threads** in the sidebar lists followed threads with unread first and a count; opening one reads it. | A mate's reply in a followed thread marks it unread. |
| FR-21 | **Resolve** a thread: it folds in the channel to its parent with "Resolved", the panel shows Unresolve; a new message reopens it. | Resolving folds it. |
| FR-22 | **Also send to channel** under the thread composer posts the reply in the channel too, marked "replied in a thread". | The reply shows in both. |
| FR-23 | **Threads tab** (the owner's ask, 2026-11-29): a channel's header switches between **Chat**, **Threads** and, with the board on, **Board**. Threads lists the channel's threads, the latest reply first, with the line each started from, replies, who wrote last, the card and whether it is unread; resolved ones fold into their own list. A row opens the thread beside the conversation. | A channel with two threads lists both; the resolved one sits under Resolved. |

### 4.2 Non-functional

| ID | Requirement | Check |
| --- | --- | --- |
| NFR-1 | Every migration (from 0020) keeps old databases upgradable and ships a downgrade script tested on a scratch copy, never on `~/.evo`. | Script run on a copy; integrity ok. |
| NFR-2 | Text from a thread's parent, other mates, shares and links reaches a mate labelled as not the person's instruction. | Tests of the delivery blocks. |
| NFR-3 | Board work by the person starts no model call; a board line wakes no mate. | Usage shows no row. |
| NFR-4 | Layout at 1440, 1000 and 720 px, light and dark; nothing scrolls sideways (`--scan`). | Shots and scan. |
| NFR-5 | A board with 500 cards opens and drags without a visible stall. | Shot with a 500-card fixture; drag works. |
| NFR-6 | A mate never reads a conversation it is not in; shares are explicit visible cards. | Tests of `share_to` refusals. |

## 5. Success criteria

- The person can run a piece of work in a thread and on a card without
  scrolling the channel to find it.
- A mate's input in a thread is smaller than in the channel for the same
  work (only the thread).
- Nothing reaches a mate from another conversation without a visible card.

## 6. Clarifications

### 6.1 Open

None. **Questions for the owner** (decided for now, worth a look on return):

- Should several threads of a channel run at once (D1)?
- Should board tool calls ever need an approval (D7)?

### 6.2 Resolved

Answered 2026-11-29 (owner away, accepted by the product owner), with the
agent's recommended answers:

- D1 Threads at once? → One job at a time per channel; a thread message
  waits in the channel's queue (steering a mate at work in the same thread
  still works). Parallel threads: Later.
- D2 Who a thread wakes → mention first, then the thread's mates, then the
  channel's rule (FR-4).
- D3 Thread depth → one level; replying in a thread line replies in the
  same thread.
- D4 Hand-off limit → per message, as in the channel; the hourly pair limit
  stays per channel.
- D5 Where boards live → the app database (`evo-store`), keyed by the
  channel; not in the channel's entries.
- D6 Card key → 2 – 6 capitals, proposed from the channel's name, editable
  when turning the board on, fixed after; numbers never reused.
- D7 Board tools and approvals → no approval: board changes are visible,
  logged and undoable by the person; mates cannot delete cards.
- D8 Report back → a member's report on an assignment with a card moves it
  to Review (FR-12); the person or the lead moves it to Done.
- D9 Message links → `evo://mates/<chat>/<line>`; they open only inside
  Evo Mates.
- D10 Sharing consent → per-mate setting, Ask every time by default; a
  share from a mate's direct chat with the person always asks.

### 6.3 Assumptions

- A-1 Thread lines are `channel` entries with a `thread` field (the
  parent's id); old entries have none and stay in the channel.
- A-2 Following, unread and resolved state live in the app database
  (part C); part A shows threads without them.
- A-3 Card numbers are per board and never reused, even after a delete by
  the person.

## 7. History

- 2026-11-29 — Draft from the product owner's backlog (Train 2, B1 – B13)
  and research of team-chat and tracker patterns ([research.md](research.md)).
- 2026-11-29 — Gate A (owner away, accepted by the product owner): the ten
  recommended answers (6.2). Gate B (same): plan and tasks; status Accepted.
- 2026-11-29 — Part A (threads, FR-1 – FR-7) built and verified; status In progress.
- 2026-11-29 — Part B (board, FR-8 – FR-15) built and verified (migration 0020).
- 2026-11-29 — Part C (linking, FR-16 – FR-22) built and verified (migration
  0021). FR-18 narrowed (D11, for the product owner to confirm): `share_to`
  posts into channels the mate is in, as a card that wakes nobody; sharing
  into another mate's own conversation is Later, because a card there would
  start that mate's turn (there is no way yet to show it without one). The
  person's Share goes anywhere (FR-16). FR-23 added at the owner's ask
  ("nên thêm option thread nữa"): a Threads tab beside Chat and Board.
  Status Implemented.
