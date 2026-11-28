# Evo Mates threads, board and linked conversations (Train 2) — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Status | In progress |
| Updated | 2026-11-29 |

## 1. Context and constraints

- The channel coordinator (`crates/evo-engine/src/channel/coordinator.rs`)
  runs one message of the person at a time: it posts `channel` custom
  entries on the channel's root chat, keeps every line in memory, delivers
  to seats over the shell (`channel_deliver`) and reads their reports and
  hand-offs from mail. The window reads the entries (`channel-store`).
- Seats' tools that act in the window or the app database live in the shell
  (`src-tauri/src/agent_tools/mates.rs`, e.g. `react`); the shell asks the
  channel's engine through `AppHost::request(chat, method, params)`.
- The app database is `evo-store` (next migration 0020); the engine opens it
  with `evo_store::open()` off the async thread.
- Rules: engine owns agent logic; bun only; never `tauri dev`; migrations
  never edited once applied; "Evo" in UI and docs.

## 2. Architecture check

| Rule | How this plan keeps it |
| --- | --- |
| Engine owns agent logic | Thread routing and context, assignments → cards: engine. The shell only runs seat tools and window commands. |
| Protocol changes together | Thread markup is text inside the prompt (like messaging's quote markup); new RPC methods (`channel_post_line`) are listed in `rpc/protocol.rs` and called by the shell. |
| Layers point down | `evo-store` gains tables and functions only; no engine types. |
| Persistent store | Same app database; new tables by migration; no new store (no ADR). |
| Trust | Parent lines, shares and other mates' text reach mates labelled `instruction="false"`; `share_to` asks by default; tools act only on the seat's own channel. |
| UI structure | New views in `features/mates/`; data through `lib/mates.ts` APIs and stores. |

## 3. Design

### 3.1 Overview — part A, threads (FR-1 – FR-7)

**Markup.** The thread composer sends `<evo-mates-thread root="LINE"/>`
followed by the message. The coordinator reads it with `thread_mark`,
resolves the root (a line that is itself in a thread gives that thread,
D3) and strips it before posting.

**Entries.** `message`, `route`, `handoff`, `notice`, `job` entries of a
thread message carry `"thread": ROOT`. Old entries have none (A-1).

**Coordinator state.** `Line` gains `thread: Option<String>`; `State` gains
`thread` (the open message's thread), `seat_thread` (each seat's last
delivery's thread, for late reports) and `restored`.

**Routing (FR-4).** `routable(text)` first (quotes never route); mentions
decide as in the channel; else `thread_mates(root)` (parent's author if a
mate, mates who wrote in the thread, mates mentioned in the thread's lines)
with `Reason::Thread`; else the channel's rule (`route::route`, router under
`auto`). `asking` (FR-72) applies only to channel messages.

**Context (FR-5).** `delivery` and `mate_delivery` take an optional
`ThreadFrame { root, parent: (author, text) }`: inside `<channel>` they add
`<thread root=… />`, `<parent author=… instruction="false">…</parent>` and
the thread's lines as `<recent>`; the closing words say the reply posts in
the thread. Channel deliveries take `recent_for` lines with no thread only.

**Hand-offs (FR-6).** Everything posted while the open message runs gets
`state.thread`; late reports use `seat_thread`. The count stays per message.
Steering (`steer`) reads the markup: a steer must be in the open message's
thread (or both in the channel).

**Restore (FR-7).** `restore(harness)` runs once before the first `turn`,
`steer`, `on_mail` or `channel_line`: it reads the root chat's `channel`
entries and rebuilds `lines` (id, author, name, text, thread), then sets
every known seat's `seen` to the end so nothing is re-delivered.

**Window.** `channel-rows.ts`: `channelRows` drops entries with a `thread`;
`threadSummaries(entries)` gives per root {count, repliers, last};
`openJob(entries)` gives the thread (or channel) of the message still
running. `thread-panel.tsx`: header ("Thread" · #channel · close), parent
bubble, rows of the thread, typing line, Composer with `thread` set (draft
key `chat#root`, placeholder "Reply…"). `channel-view.tsx`: the panel as a
right column (380 px) at ≥ 1000 px, over the feed below; the hover bar gets
**Reply in thread**; the parent shows the summary button.

### 3.2 Overview — part B, board (FR-8 – FR-15)

**Data (migration 0020).** `boards(channel_id PK, key, enabled, next_number,
created_ms)`, `board_columns(id PK, channel_id, name, position)`,
`board_cards(id PK, channel_id, number, column_id, position REAL, title,
description, assignee, priority, due_ms, labels JSON, checklist JSON, links
JSON, thread_chat, thread_root, source JSON, created_by, created_ms,
updated_ms)`, `board_events(id PK, card_id, actor, kind, text, at_ms)`;
indexes on `(channel_id, column_id, position)` and `(assignee)`. Functions in
`evo-store/src/boards.rs`; every change writes an event in the same
transaction.

**Shell.** Commands `board_get`, `board_set` (on/off, key), `board_columns`
(rename, add, move, remove), `card_create`, `card_update`, `card_move`,
`card_delete`, `card_events`, `my_cards`, `mate_cards`; each emits
`mates://board {channel}`. Seat tools `board_list`, `board_create`,
`board_update`, `board_move`, `board_comment` in `agent_tools/mates.rs`
(offered only when the seat's channel's board is on); after a change they
ask the channel's engine `channel_post_line` to post a `board` entry.

**Engine.** On a lead's assign with the board on, the coordinator creates a
card (first column, assignee, `source = {from, to, message}`, thread link)
and posts a `board` line; on the member's report back it moves that card to
Review (FR-12). `board` entries are lines for the window only, never in
`lines` for deliveries (FR-13).

**Window.** A **Chat · Board** switch in the channel header when the board
is on; `board-view.tsx` (columns, pointer drag with a drop line,
quick add, filters, search, list view), `card-sheet.tsx` (fields, checklist,
links, activity), `KEY-n` links in Markdown (`card-links.ts`), Today's **My
cards**, the profile's open cards, the thread header's card chip and **Track
on board**.

### 3.3 Overview — part C, linking (FR-16 – FR-22)

**Data (migration 0021).** `mate_shares(id PK, from_chat, from_lines JSON,
excerpt, to_chat, note, by, at_ms)`; `thread_follows(chat_id, root,
following, resolved, last_read_ms, PK(chat_id, root))`.

**Share (FR-16).** Select mode in a channel or chat (click lines, or "From
here"), the existing forward dialog's target picker with a note; the shell
command `share_lines` writes the share row and sends the target a message
`<evo-mates-share from="#launch-room" link="evo://…" lines="3">…</evo-mates-share>`
plus the note. The coordinator's `split_quoted` and the direct chat's
markup treat it as quoted (`instruction="false"`); the window draws the
card.

**Links (FR-17).** `message-links.ts` parses `evo://mates/<chat>/<line>`,
`#channel`, `@mate`; Markdown links route through it; Copy link on the
hover bar.

**`share_to` (FR-18).** Seat and mate tool in the shell; setting
`mates.share.<mateId>` = `ask|allow|never`; Ask (and any share from the
mate's direct chat with the person) goes through the existing approval
card; on yes it calls the same `share_lines` path.

**Linked (FR-19)** lists `mate_shares` rows and card links for a chat.
**Threads (FR-20 – FR-22)**: follow rows written when the person starts,
replies or is mentioned; sidebar **Threads** view; resolve folds; the
thread markup gains `also="channel"`, which posts the reply in both.

### 3.4 Interfaces

| Interface | Change |
| --- | --- |
| Prompt text to a channel | `<evo-mates-thread root="…"/>` (A), `also="channel"` (C), `<evo-mates-share …>` (C) |
| `channel` entries | `thread` field (A); `kind: "board"` (B); `kind: "share"` card in the receiving chat (C) |
| RPC (engine) | `channel_post_line` (B) |
| Shell commands | board and card commands (B); `share_lines`, `thread_follow`, `thread_read`, `thread_resolve`, `linked_of` (C) |
| Seat tools | `board_*` (B), `share_to` (C) |

### 3.5 Behaviour

- One job at a time per channel (D1): the window's channel queue holds
  thread messages too; their queued bubbles show in the thread panel.
- Stop in the thread panel shows only while the open job is that thread's.

### 3.6 Errors and edge cases

- Unknown root id in the markup → the message goes to the channel, with a
  notice "That thread is gone: sent to the channel".
- Tools on a board that is off, a card of another channel, an assignee not
  in the channel → refused with the reason.

### 3.7 Security and trust

- Parent and thread lines from mates are labelled; the parent never routes.
- Board tools only touch the seat's own channel's board.
- `share_to` asks by default; a share out of a direct chat always asks; no
  share reaches a place the mate is not in or does not know.

### 3.8 Configuration

- Board per channel (`boards.enabled`, key).
- `mates.share.<mateId>`: `ask` (default), `allow`, `never` (C).

## 4. Alternatives considered

- **Threads as separate chats** (a sub-coordinator per thread): parallel
  threads for free, but a second engine per thread and seats shared between
  them; rejected for now (D1).
- **Board in the channel's entries**: no migration, but Today and the board
  editor would need the engine running; rejected (D5).

## 5. As-built notes

- Part A: the line components moved from `channel-view.tsx` to
  `channel-lines.tsx` so the thread panel draws lines the same way; the
  thread panel stays in `channel-view.tsx` (it uses the channel's box). The
  panel docks from 1200 px, not 1000: with the sidebar open, a 1000 px
  window leaves too little for both. A thread message whose parent is
  unknown goes to the channel with a `thread_gone` notice.
- Part B: the shell's tools post a board line through `channel_post_line`;
  the coordinator posts its own (assignments, reports) directly. A board
  line posted while a thread message runs carries that thread, so it shows
  in the thread. `CardPatch` takes `null` to clear `assignee`, `dueMs` and
  `threadRoot` (a field left out stays). `KEY-n` links use the
  `https://local.card/` prefix (like file links) and Markdown's
  `onCardLink`. The card sheet is one per window (`CardSheet` in
  `mates-app.tsx`); the board's state is `stores/board-store.ts`. The
  downgrade script also refuses when `user_version` and
  `schema_migrations` disagree, and backs the file up before applying.
