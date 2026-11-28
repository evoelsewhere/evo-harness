# Evo Mates messaging basics (Train 1) — Spec

| | |
| --- | --- |
| Status | Draft |
| Area | desktop UI, desktop shell, engine (channel), `evo-mesh`, `evo-store` |
| Created | 2026-11-29 |
| Updated | 2026-11-29 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Research | [research.md](research.md) |
| Related | [Evo Mates](../evo-mates/spec.md), [Evo Mates UI v2](../evo-mates-ui-v2/spec.md), [PO backlog](../evo-mates/po-backlog-2026-11-29.md) (Train 1, Q1 – Q4) |

## 1. Summary

Evo Mates looks like a messaging app but lacks what people use in every
messaging app on day one: seeing who is typing, message status, editing,
trying again, unsending, forwarding, drafts, pins, muting, a badge on the
icon, link cards, an emoji picker, slash commands and mentions of themselves.
In channels, mates cannot react to, reply to, or attach files to each
other's messages. This train adds those basics to mates' conversations and
channels, from the product owner's backlog (Train 1 with Q1 – Q4).

## 2. Scope

### 2.1 Goals

- A channel feels alive: who is typing, who saw a message, reactions and
  replies from the person and from mates, files passed between mates.
- The person controls their own messages: edit, try again, unsend, delete
  for me, forward, drafts, pins.
- The person controls attention: mute for a time, notification levels per
  channel, an unread count on the app icon, mentions of them collected.
- The message box does what people expect: emoji, shortcodes, Markdown
  keys, ↑ to edit, slash commands.

### 2.2 Non-goals

- Threads in a side panel (6.1 Q1): replies stay quotes in the feed.
- Browsing every earlier version of a reply ("1 of 2") (6.1 Q3).
- Mates editing or unsending their own messages.
- Train 2 (browser and app control in Evo Mates), Train 3 (tasks, brief,
  saved items, search filters) and Train 4 of the backlog.

## 3. User scenarios

### US-1 See a channel at work (P1)

1. **Given** Writer and Analyst working in a channel, **then** the feed's foot
   reads "Writer and Analyst are typing…" with their faces and moving dots;
   hovering a face shows its step ("Writer · Reading 3 files…"); each goes
   when its line lands.
2. **Given** a message the lead and one member took in, **then** under it
   "Seen by" shows their two faces.

### US-2 Mates acknowledge, reply and pass files (P1)

1. **Given** the lead assigns work to Kai, **when** Kai reacts 👍 to the
   assignment line, **then** the line shows a 👍 chip with Kai's face and no
   new turn starts for anyone.
2. **Given** Lin answers one of Kai's lines, **then** Lin's line shows a
   quote of Kai's line that jumps to it.
3. **Given** Mo hands Lin a screenshot, **then** the hand-off card shows the
   picture, the person can open it, and Lin's model receives it as a picture.

### US-3 Own my messages (P1)

1. **Given** a message the mate has not answered, **when** the person edits
   it, **then** the bubble shows "Edited" and the mate answers the new text.
2. **Given** a reply, **when** the person presses Try again, **then** a new
   reply replaces it.
3. **Given** a message still queued, **when** the person unsends it, **then**
   it leaves the queue and the mate never sees it.
4. **Given** a reply, **when** the person forwards it to another mate with a
   note, **then** that mate's conversation shows "Forwarded from Writer"
   above it.
5. **Given** text typed in a box, **when** the person switches away, quits
   and comes back, **then** the text is there and the row says "Draft: …".
6. **Given** three pinned messages, **then** a bar under the header steps
   through them and jumps to each.

### US-4 Control attention (P1)

1. **Given** a channel muted for 8 hours, **then** it sends no notification
   and its row shows a muted bell; after 8 hours it notifies again.
2. **Given** a channel at "Mentions and Needs you", **then** only lines that
   mention the person or need them notify.
3. **Given** two unread conversations, **then** the app icon and the tray
   show 2.
4. **Given** a mate writes "@you can you check this?" in a channel, **then**
   the line is highlighted, notifies, and is listed under Today › Mentions.

### US-5 A message box like the others (P2)

1. `:thu` offers 👍 and Enter inserts it; the emoji button opens a picker.
2. Ctrl+B, Ctrl+I and Ctrl+E wrap the selection in bold, italics and code.
3. ↑ in an empty box opens the last message for editing.
4. `/` lists commands; `/checkin every weekday 9:00 inbox` adds a check-in.
5. A link in a reply shows a card with title, site and picture when link
   previews are on.

### Edge cases

- A mate reacting to its own line, or reacting twice: refused, or the second
  replaces the first.
- A reaction from a mate never wakes a mate and never counts toward the
  hand-off limit.
- An edit while the mate is working: offered only after the turn ends.
- A forwarded file that no longer exists: forwarded as text with the name.
- A link preview that fails or times out (3 s): the link stays text.
- A muted mate that needs the person (an approval): the row still shows
  Needs you; the notification follows the level (Needs you always notifies
  unless the level is Nothing).

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | **Typing line** in a channel (Q1): at the feed's foot, the faces of the working mates with moving dots and "A is typing…", "A and B are typing…" or "A and N others are typing…"; hovering a face shows "Name · step"; a mate leaves the line when its line lands; no dots while a card waits; reduced motion holds them still. | Two seats working: one line with both names; one lands: one name left. |
| FR-2 | **Message status** (M1) under the person's last message in a mate's conversation: Sent, Queued (waiting for room or for the running turn), Seen (the mate's turn started on it), Replied; in a channel, "Seen by" with the faces of the mates whose turn took it in. | A message while the mate works reads Queued, then Seen, then Replied. |
| FR-3 | **Reactions in channels** (Q2): the person and every mate may react to any line with the six reactions of evo-mates FR-60; a line shows chips with counts and, on hover, who reacted; one reaction per reactor per line, a second pick replaces it, the same pick removes it; reactions are kept across restarts. | A 👍 from the person and from Kai shows "👍 2" with both faces after a restart. |
| FR-4 | **Mates react** (Q2) with a tool that names a line of the channel or a message of its conversation and a reaction; it never starts a turn for anyone, does not count toward the hand-off limit, at most one per mate per line, never on its own lines; the reacted-to mate is told at its next turn as information ("Kai reacted 👍 to your line: …"). | A seat's `react` call adds the chip, no seat wakes, the limit counter is unchanged; the author's next delivery carries the note. |
| FR-5 | **Replies in channels** (Q3): the person may reply to any line (hover Reply, Esc removes the quote); a mate's line that answers a specific line shows its quote; a quote jumps to its line and highlights it. Mates name the line they answer by its short id, which every delivery now shows. | A reply to Kai's line shows the quote; clicking it jumps to Kai's line. |
| FR-6 | **Files between mates** (Q4): a mate may attach files from its folder or the channel's to a hand-off or its line; the files show under the card or line as previews and cards; the receiving mate gets them as attachments: pictures as images for a model that reads images, other files by path. | Mo hands Lin a PNG: the card shows it; Lin's turn input holds the image. |
| FR-7 | **Edit** (M2) the person's last message in a mate's conversation once the mate is idle: the bubble shows "Edited" and the mate answers the new text; the old answer is replaced. ↑ in an empty box starts it (FR-12). | Edit "plan monday" to "plan tuesday": "Edited", and the answer is about Tuesday. |
| FR-8 | **Try again** (M3) on the last reply in a mate's conversation: the reply is answered again, optionally with another model picked from the menu; the failed-reply Retry stays. | Try again gives a new reply in place of the old. |
| FR-9 | **Unsend** (M4) a message that is still queued in a mate's conversation or a channel; **Delete for me** hides any message from the person's view, kept hidden across restarts. | Unsend while Queued: the mate's next turn never sees it; Delete for me hides a bubble after a restart. |
| FR-10 | **Forward** (M5) a message with its files to a mate or channel picked from a list, with an optional note; the receiver shows "Forwarded from <name>" above it and the mate reads it as quoted, not as the person's instruction unless the note asks. | Forward Writer's reply to Analyst with "check this": Analyst's conversation shows the label and answers the note. |
| FR-11 | **Drafts** (M6): text and attachments in a box are kept per conversation across switches and restarts; the roster row reads "Draft: …" while one waits. | Type, restart: the text is back and the row says Draft. |
| FR-12 | **Composer** (M11): an emoji picker button; `:shortcode` completion with arrow keys and Enter; Ctrl+B, Ctrl+I, Ctrl+E wrap the selection; ↑ in an empty box edits the last message (FR-7); in mates' conversations and channels. | Each key and the picker work in both boxes. |
| FR-13 | **Slash commands** (M12): `/` lists `/remind`, `/checkin`, `/pause`, `/resume`, `/stop`, `/memory`, `/handoff @mate` (channels), `/clear-quote`, filtered as typed, each with a hint; Enter runs it. | `/checkin every weekday 9:00 inbox` adds a check-in in Following up. |
| FR-14 | **Pinned messages** (M7): pin any message in a mate's conversation or channel; a bar under the header shows the latest pin, steps through them and jumps to each; Unpin from the bar or the message. | Pin three: the bar shows "1 of 3" and steps. |
| FR-15 | **Mute and levels** (M8): mute a mate or channel for 1 hour, 8 hours, until tomorrow 8:00 or until turned back on; a channel's level is All, Mentions and Needs you, or Nothing; a muted or quiet place shows a bell-off mark in its row. | A channel muted 1 hour sends nothing, then notifies again. |
| FR-16 | **Unread count on the icon** (M9): the taskbar or dock icon and the tray show the number of unread or waiting mates and channels, not counting muted ones. | Two unread: the icon shows 2. |
| FR-17 | **Link previews** (M10): a web address in a mate's reply or channel line shows a card (title, site, picture) that the shell fetches; Settings › Evo Mates has the switch (6.1 Q4); a failure leaves the link as text. | A link with previews on shows a card; off, plain text. |
| FR-18 | **Mentions of the person** (M13): a mate's line that writes `@you` is highlighted, notifies at the All and Mentions levels, and is listed under Today › Mentions until opened. | A mate's "@you can you check?" shows in Today and notifies. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Every store change has a migration and a written downgrade path; an older Evo refuses a newer database with a message, as today. | Migration test; the downgrade script run on a copy. |
| NFR-2 | A mate's reaction, a typing update and a status change start no model call. | Usage shows no new row for them. |
| NFR-3 | Link previews fetch only `http(s)` pages, no more than 512 KB, with a 3 s timeout, never send cookies, and are cached for a day. | Shell test of the limits. |
| NFR-4 | UI follows DESIGN.md and Evo Mates UI v2; screenshots at 1440, 1000 and 720 px in light and dark; no sideways scroll. | Shots and the scan. |
| NFR-5 | Text from another mate, a forwarded message or a link preview is data, never the person's instruction. | Engine test of the labels. |

## 5. Success criteria

- SC-1 Every item of the backlog's Train 1 and Q1 – Q4 is in Evo Mates or
  marked left with the owner's decision.
- SC-2 In a channel job of three mates, the person can see at any moment who
  is typing, who saw their message and who acknowledged an assignment.

## 6. Clarifications

### 6.1 Open

1. [NEEDS CLARIFICATION: real threads in channels?] — proposed answer: no;
   replies are quotes in the feed (FR-5); threads need routing per thread in
   the engine.
2. [NEEDS CLARIFICATION: how mates react?] — proposed answer: a `react` tool
   for mates (direct and seats) that never wakes anyone, does not count
   toward the hand-off limit, one per mate per line, told to the author as
   information at its next turn (FR-4); reactions in a new table (migration
   0019) with a downgrade script.
3. [NEEDS CLARIFICATION: keep earlier versions of a reply browsable ("1 of
   2")?] — proposed answer: no; Try again and Edit replace the reply; the
   saved history keeps the old branch.
4. [NEEDS CLARIFICATION: link previews on or off by default?] — proposed
   answer: off; turning them on contacts the linked sites.
5. [NEEDS CLARIFICATION: ship in one merge, or two?] — proposed answer: two
   merges: **1a** UI and shell (FR-1, FR-2, FR-7 – FR-9, FR-11 – FR-13,
   FR-16, FR-18) first, then **1b** engine and database (FR-3 – FR-6, FR-10,
   FR-14, FR-15, FR-17).

### 6.2 Resolved

None yet.

### 6.3 Assumptions

- A-1 Pins, mutes, levels, drafts and "Delete for me" are window or app
  settings, not new tables: only reactions need a migration (NFR-1).
- A-2 "Seen" for a channel member is its seat's turn starting on the
  message; the shell already knows which seats a message woke.
- A-3 Evo has no setting for the person's name yet: `@you` is the mention;
  mates are told to use it when they need the person.

## 7. History

- 2026-11-29 — Draft, from the product owner's backlog (Train 1 with Q1 –
  Q4), a check of each item against the code and research of messaging
  apps ([research.md](research.md)); five questions open (6.1).
