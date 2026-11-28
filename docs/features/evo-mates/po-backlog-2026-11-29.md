# Evo Mates — product owner backlog (2026-11-29)

The owner asked for a product owner's pass: Evo Mates measured against what
people expect from the messaging apps they use every day, and from a team
of AI teammates. This is that pass. It lists what is still missing after
[spec.md](spec.md) (FR-1 – FR-85) and the UI v2 redesign
([../evo-mates-ui-v2/spec.md](../evo-mates-ui-v2/spec.md), FR-1 – FR-31),
groups it into release trains, and gives each item a priority.

The gaps come from the two specs, not from a sweep of the running app.
Before a train's spec, check each item against the code; drop what already
exists.

Patterns only, no product names (repository rule).

## How to read it

- **P0**: people notice it missing within the first day. Ship in the next train.
- **P1**: makes a team of mates work well together, or keeps a heavy user.
- **P2**: polish or reach; later.
- **Needs**: UI (front end only), Shell (Tauri), Engine (engine or mesh), DB (a migration).
  Every DB item needs a downgrade path. The owner had to roll back
  `evo.db` by hand on 2026-11-28 to run an older release.

## Already queued with the redesign session (2026-11-28/29)

| # | Item | Needs |
|---|------|-------|
| Q1 | Typing line in channels: faces, dots, "Writer and Analyst are typing…", step on hover | UI |
| Q2 | Mates react to each other's and the person's messages in channels; the person reacts in channels; reactor faces on the badge; a reaction never wakes a mate | Engine, DB |
| Q3 | Mates and the person reply to a specific message in channels, with a quote that jumps to it; decide on real threads at Gate A | Engine |
| Q4 | Mates attach pictures and files to hand-offs and channel lines; the receiver gets them as attachments, pictures for vision models | Engine |
| Q5 | Browser, app control and Webbridge in Evo Mates: a browser pane for mate chats, rich step rows, per-mate switches, approvals | UI, Shell |

## Train 1 — Messaging basics (P0)

What every messaging app has and Evo Mates lacks.

| # | Item | Acceptance in one line | Needs |
|---|------|------------------------|-------|
| M1 | **Message status** on the person's messages: Sent → Queued → Seen → Replied; in a channel, "Seen by" with the faces of the mates that took it in | A queued message shows Queued, then Seen when the mate's turn starts | UI, Shell |
| M2 | **Edit and resend** the person's last message while the mate has not answered, or after it answered (the old reply folds into "Earlier answer") | Edit, then the bubble shows "Edited" and the mate answers the new text | UI, Engine |
| M3 | **Retry** a failed reply and **Try again** on any last reply, optionally with another model | "Couldn't reply · Retry" works; Try again shows the new reply and keeps the old one under "1 of 2" | UI, Engine |
| M4 | **Unsend** a message the mate has not started on; **Delete for me** hides any message | Unsend before Seen removes it from the queue | UI, Shell |
| M5 | **Forward** a message, with its files, to another mate or channel, with an optional note | The receiver sees "Forwarded from Writer" above it | UI, Shell |
| M6 | **Drafts** kept per conversation; the roster row says "Draft: …" | Type, switch away and back, and the text is still there after a restart | UI |
| M7 | **Pinned messages** per conversation and channel, shown as a bar under the header that cycles through them | Pin three, and the bar steps through them and jumps to each | UI, DB |
| M8 | **Mute** a mate or channel (1 h, 8 h, until tomorrow, always) and a **notification level** per channel: All, Mentions and Needs you, Nothing | A muted channel never notifies; its row shows a bell-off icon | UI, Shell, DB |
| M9 | **Unread badge** on the app's taskbar or dock icon and the tray | Two unread conversations show "2" on the icon | Shell |
| M10 | **Link previews** for web addresses in bubbles: title, site and image, fetched by the shell, off in Settings | A link shows a card; with previews off it stays text | UI, Shell |
| M11 | **Composer**: an emoji picker and `:shortcode:` completion, Shift+Enter for a new line, Markdown shortcuts (Ctrl+B, Ctrl+I, Ctrl+E for code), ↑ to edit the last message | Each shortcut works; ↑ in an empty box opens edit | UI |
| M12 | **Slash commands** in the composer: `/remind`, `/checkin`, `/pause`, `/stop`, `/memory`, `/handoff @mate`, `/clear-quote`, listed as you type `/` | `/checkin every weekday 9:00 inbox` adds a check-in | UI |
| M13 | **Mentions of the person**: a mate writing `@you` (or the person's name) in a channel highlights the line, notifies, and lists it under Today › Mentions | A mention shows in Today and notifies even at the Mentions level | UI, Engine |

## Train 2 — Mates at the computer (Q5)

Already queued. It stays second so the basics land first.

## Train 3 — A team that works together (P1)

| # | Item | Acceptance in one line | Needs |
|---|------|------------------------|-------|
| T1 | **Tasks in a channel**: every *assigns* becomes a task with its owner's face, status (Open, Working, Done, Blocked) and due time, in a **Tasks** tab and a header counter; a mate's *tells … done* closes it; the person can reassign or close | The lead assigns two tasks, and the tab shows both with live status, then Done | UI, Engine, DB |
| T2 | **Channel brief**: one shared Markdown note per channel (goal, decisions, links) that mates read every turn and may update with a journal line; the person edits it in a side tab | A decision written by the lead shows in the brief and the next turn uses it | UI, Engine |
| T3 | **Shared in this conversation**: Photos, Files and Links tabs in the profile and the channel panel | A picture sent last week is in Photos | UI |
| T4 | **Membership lines** in channels: joined, left, lead changed, reply rule changed, paused | Adding a member shows "Analyst joined" | UI, Engine |
| T5 | **Add a mate to this conversation**: from a direct chat, creates a channel with both mates and the last messages as context | The new channel opens with a "Started from your chat with Writer" line | UI, Shell |
| T6 | **Saved items**: bookmark any message; a Saved page lists them across mates | Saved survives a restart and jumps to the message | UI, DB |
| T7 | **Search filters**: `from:`, `in:`, `has:file`, `has:link`, `before:`, `after:`, and **Jump to date** | `from:Writer has:file` lists only Writer's messages with files | UI, Shell |
| T8 | **Cost and model on hover** for each reply, and the day's cost in the header tooltip | Hovering a reply shows "$0.004 · model" | UI |

## Train 4 — Comfort and reach (P1/P2)

| # | Item | Priority | Needs |
|---|------|----------|-------|
| C1 | **Send later / remind me**: schedule a message to a mate or channel | P1 | UI, Shell |
| C2 | **Pop out** a conversation into its own window | P1 | UI, Shell |
| C3 | **Select several messages** to copy, forward, save or delete | P1 | UI |
| C4 | **Export** a conversation or channel to Markdown or PDF | P1 | UI, Shell |
| C5 | **Reply from the notification** and grouped notifications per conversation | P1 | Shell |
| C6 | **Sounds** for sent and received messages, with a switch | P2 | UI |
| C7 | **The person's status**: "Away until 14:00" or "Focus"; mates hold non-urgent first messages and say so | P2 | UI, Engine |
| C8 | **Appearance**: density (comfortable, compact), text size, bubble or flat lines | P2 | UI |
| C9 | **Folders** for mates (Work, Personal) as sidebar tabs | P2 | UI, DB |
| C10 | **Polls** a mate or the person posts in a channel; mates may vote with a reason | P2 | UI, Engine |
| C11 | **Voice messages**: record, transcribed through a connected provider; **Read aloud** for replies | P2 (phase 4) | UI, Shell |
| C12 | **Translate** a message into the person's language | P2 | UI, Engine |

## Order and gates

1. Finish the UI v2 redesign (in build).
2. Train 1, with Q1 – Q4 folded in, as one spec: it is all about the
   conversation and the channel feed. Gate A decides real threads (Q3) and
   how mates react (Q2).
3. Train 2 (Q5).
4. Train 3.
5. Train 4, P1 items first.

Each train is its own spec, plan and tasks with Gates A and B, verified with
screenshots in light and dark at 1440, 1000 and 720 px, and merged to main.

## History

- 2026-11-29 — Written at the owner's request ("act as the product owner who
  has used many messaging apps"); the order above is the product owner's
  call and the owner may change it.
