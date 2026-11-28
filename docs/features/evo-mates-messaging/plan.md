# Evo Mates messaging basics (Train 1) — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-29 |
| ADD sections | [Desktop UI, shell, engine, store](../../architecture/README.md) |
| ADRs | [0028](../../architecture/decisions/0028-two-front-ends-in-one-app-process.md) (unchanged) |

## 1. Context and constraints

- Mates' views: `apps/desktop/src/features/mates/` (thread, composer, channel
  view and rows, roster, Today), window preferences in
  `stores/mates-ui-store.ts` (Evo Mates UI v2).
- The engine already edits a sent message by branching (`editMessage` in
  `engine-store.ts`, engine `navigate`) and removes a queued one
  (`cancelQueued`, engine `cancel_queued`): FR-7 – FR-9 are UI.
- Channels: the coordinator (`crates/evo-engine/src/channel/coordinator.rs`)
  writes `channel` entries (message, route, handoff, notice, job, retro);
  `delivery` / `mate_delivery` frame what a seat reads. A message entry
  already has `replyTo` (FR-56 of Evo Mates).
- Mates' own tools live in the shell (`src-tauri/src/agent_tools/mates.rs`);
  reactions of the person in `mate_reactions` (migration 0016).
- Notices: `src-tauri/src/supervisor/notices.rs` decides a mate's notice
  (`mate_wanted`).
- The window's taskbar or dock icon: Tauri 2 `set_badge_count` (macOS,
  Linux) and `set_overlay_icon` (Windows).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | Line ids, reply marks and attachments in deliveries are the coordinator's; the shell keeps reactions, the badge and link previews (native features). |
| Layers point down | ok | `evo-mesh` gains an optional `attachments` field on mail; no new dependency. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | ok | Channel entries gain optional fields (`replyTo` shown, `files`, `mentions`); older entries read as before. |
| Models come from models.dev | ok | Pictures go to a seat only when its model reads images (models.dev `input`). |
| Real engine in the app | ok | |
| Untrusted input stays untrusted | ok | Forwarded text, other mates' lines and link previews are labelled data (NFR-5); link previews fetch with no cookies, size and time limits (NFR-3). |
| Desktop UI structure and DESIGN.md | ok | Reaction chips, status words and the typing line follow the Evo Mates layer. |
| A schema change adds a migration and keeps old databases upgradable | ok | 0019 `channel_reactions`; downgrade script under `docs/features/evo-mates-messaging/` (NFR-1). |

## 3. Design

### 3.1 Overview — merge 1a (UI and shell)

| Requirement | Where | How |
| --- | --- | --- |
| FR-1 typing line | `channel-view.tsx`, `typing-words.ts` (+test) | One line at the feed's foot from the working seats (`useSeats`), faces stacked, words from `typingWords(names)`, step on hover from the seat's runtime when loaded; hidden while a seat waits. Replaces the per-seat rows; Stop stays in the header. |
| FR-2 status | `thread-rows.ts` (+test), `channel-rows.ts` (+test) | A `status` row under the person's last bubble: Queued (the chat is in `queued` or the message waits in the engine's queue), Seen (its turn runs), nothing once a reply lands (the reply is the "Replied"); replaces "Delivered" with "Sent". In channels, a person's line gets `seenBy` from its `route.picked`. |
| FR-7 edit | `thread.tsx`, `mates-ui-store.ts` | Edit on the person's last bubble when idle: an inline field; Save calls `editMessage(turn.id, composePrompt(text, reply, []))`; the new prompt's text is kept in `edited[chat]` for the "Edited" mark. |
| FR-8 try again | `thread.tsx` | Try again on the last reply: `editMessage(turn.id, turn.prompt)`. |
| FR-9 unsend, delete for me | `thread.tsx`, `channel-view.tsx`, `mates-ui-store.ts` | Unsend on a queued bubble: `cancelQueued(entryId)`; the channel's waiting line says Unsend. Delete for me: `hidden[chat]` keys, filtered out of the rows. |
| FR-11 drafts | `mates-ui-store.ts` (`drafts`), `composer.tsx`, `channel-view.tsx`, `roster.tsx` | The box's text is saved per chat on change (debounced) and read on mount; the roster line says "Draft: …" for a chat not shown. |
| FR-12 composer | `emoji.ts` (+test), `emoji-picker.tsx`, `composer-keys.ts` (+test), both boxes | A picker of common emoji by group with search; `:code` completion; Ctrl+B/I/E wrap the selection; ↑ in an empty box starts the edit of FR-7 (a window event the thread listens to). |
| FR-13 slash commands | `slash-commands.ts` (+test), both boxes | `/` opens a list filtered as typed; `/checkin <schedule> <text>` parses "every day / weekdays / weekly on Mon,Fri at 9:00" into the existing add-check-in call; `/pause`, `/resume`, `/stop`, `/memory`, `/remind <text>` (sent as a request to the mate), `/handoff @mate` (inserts a mention), `/clear-quote`. |
| FR-16 icon badge | `badge.ts`, `mates-app.tsx`; shell `app_set_badge` (`src-tauri/src/system.rs`) | The window counts unread or waiting places not muted; macOS and Linux get `set_badge_count`; Windows gets an overlay icon drawn on a canvas (a red circle with the number) as PNG bytes. |

### 3.2 Overview — merge 1b (engine and database)

| Requirement | Where | How |
| --- | --- | --- |
| FR-3, FR-4 reactions | migration `0019_channel_reactions.sql`, `crates/evo-store/src/mates.rs`; shell `mates/reactions.rs` commands; tool `react` in `agent_tools/mates.rs`; coordinator `delivery` notes | A row per (chat, message, reactor); the tool is offered to mates and seats; it validates the line id against the channel's entries, refuses the mate's own line, upserts, emits `mates://reactions`; it never mails or wakes anyone. The author's next delivery carries "<name> reacted 👍 to your line: …" and marks it told. |
| FR-5 replies | coordinator, `channel-rows.ts`, `channel-view.tsx` | Every recent line in a delivery carries its short id (`[m12] Kai: …`); a seat's reply that starts with `↩ m12` (or names `reply_to` in `send_message`) is stored with `replyTo`; the person's channel quote uses the direct chat's markup. The view shows the quote and jumps to it. |
| FR-6 files | `evo-mesh` `MailBody.attachments`, `agent_tools` `send_message` `files`, coordinator | Paths in the mate's folder or the channel's; the receiving seat's prompt carries them as image blocks when its model reads images, else as `@path` lines; the hand-off entry records them; the card shows them. |
| FR-10 forward | `forward-dialog.tsx`, `mates-store.ts` | Pick a mate or channel; the target is opened and gets `<evo-mates-forward from="…">…</evo-mates-forward>` plus the note; the thread shows "Forwarded from". |
| FR-14 pins | app setting `mates.pins` (no migration, A-1), `pins-bar.tsx` | A list of message keys and excerpts per chat. |
| FR-15 mute, levels | app setting `mates.mute`, `notices.rs` (`mute_wanted`, +test), menus | Until-time and level per chat; checked before a notice. |
| FR-17 link previews | shell `link_preview` command (+test), `link-card.tsx`, Settings › Evo Mates switch (off) | `http(s)` only, no cookies, 512 KB, 3 s, `og:`/`<title>`; cached a day in memory. |
| FR-18 mentions | coordinator flags `mentionsPerson` on a line with `@you`; `notices.rs`; Today › Mentions | Notified at All and Mentions levels. |

### 3.3 Interfaces

```ts
// 1a: shell
invoke("app_set_badge", { count: number, png: number[] | null })
// 1b: shell
invoke<ChannelReaction[]>("channel_reactions", { chat })
invoke("channel_react", { chat, message, reaction: ReactionKind | null })   // the person
invoke<LinkPreview | null>("link_preview", { url })
// tool for mates and seats
react { line: "m12", reaction: "like" | "heart" | "dislike" | "laugh" | "emphasis" | "question" | null }
```

### 3.4 Data and storage

- 1a: `evo.mates-ui` (window local storage) gains `drafts`, `edited`,
  `hidden`. No database change.
- 1b: migration `0019_channel_reactions.sql`:
  `channel_reactions(chat_id, message, reactor, reaction, at_ms, told,
  PRIMARY KEY (chat_id, message, reactor))`. Downgrade: drop the table and
  set `user_version` back to 18 (script and notes in this folder), tested on
  a scratch copy. App settings `mates.pins`, `mates.mute`,
  `mates.linkPreviews`.

### 3.5 Behaviour

- Status words (FR-2) come only from what the window sees; a status change
  starts no call (NFR-2).
- Edit and Try again run only while the mate is idle; the old reply stays in
  the saved history, off the shown branch.

### 3.6 Errors and edge cases

As the spec's edge cases; a failed `editMessage` shows the error toast and
keeps the bubble.

### 3.7 Security and trust

Forwarded messages, other mates' lines, reactions and link previews reach a
model as labelled data (NFR-5). The badge PNG is drawn by the window from a
number. Link previews: see NFR-3.

### 3.8 Configuration

| Key | Type | Default | Where |
| --- | --- | --- | --- |
| `drafts`, `edited`, `hidden` | maps per chat | {} | `evo.mates-ui` |
| `mates.pins` | map per chat | {} | app settings (1b) |
| `mates.mute` | map per chat | {} | app settings (1b) |
| `mates.linkPreviews` | bool | false | app settings, Settings › Evo Mates (1b) |

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Pins and mutes as tables | App settings carry them without a migration (NFR-1, the owner's rollback). |
| Mates react by posting a line | Costs a turn and wakes others; a tool does not. |
| Threads | Decided out (6.2 Q1). |

## 5. As-built notes

- 1a: the queued status words read "Queued · read at the next step" /
  "Queued · after this turn" with **Unsend**; "Delivered" became the FR-2
  word (Sent, Seen, Queued) and nothing shows once the reply lands.
- 1a: the icon badge is `mates_set_badge` in `src-tauri/src/mates/badge.rs`;
  Windows gets a 32×32 RGBA overlay the window draws; the tray keeps its
  own counts.
- 1a: drafts keep text only, not attachments.
- 1a: `/remind` asks the mate in a message (its agenda tool keeps it), as
  the mate would if asked in words.
