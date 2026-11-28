# Evo Mates messaging basics — Research

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Backlog | [po-backlog-2026-11-29.md](../evo-mates/po-backlog-2026-11-29.md) (Train 1, Q1 – Q4) |
| Updated | 2026-11-29 |

Patterns only, no product names (repository rule). Sources: the agent's
knowledge of the leading messaging and team-chat apps, and 2026 write-ups on
read receipts, group typing indicators and agents that react in team chats.

## 1. What the code already has (checked 2026-11-29)

| Item | In Evo today | Left |
| --- | --- | --- |
| Q1 typing in channels | A "Kai is working… Stop" row per working seat | One grouped line with faces, names and steps |
| Q2 reactions | The person reacts in a mate's direct chat (evo-mates FR-60, table `mate_reactions`) | Channels; mates reacting; several reactors |
| Q3 replies | The person quotes in a direct chat (FR-61); a channel reply is tied to the message it answers (`replyTo`, FR-56) but shows no quote | Reply in channels; mates replying to a line |
| Q4 attachments | Channel lines show files a mate names by path (FR-83); mail carries text only | Files on hand-offs, pictures to vision models |
| M1 status | "Delivered", "Next", "Seen while working" | Sent → Queued → Seen → Replied; "Seen by" faces in channels |
| M2 edit | The engine edits a sent message by branching (`editMessage`, used by Evo Harness) | Edit in Evo Mates |
| M3 retry | A failed reply has Retry (`TurnError`) | Try again on the last reply |
| M4 unsend | A channel's waiting message has Remove | Unsend a queued direct message; Delete for me |
| M5 forward | — | Forward to a mate or channel |
| M6 drafts | — | Drafts per conversation |
| M7 pins | — | Pinned messages bar |
| M8 mute | A mate's Notifications switch | Mute for a time; channel levels |
| M9 badge | — | Unread count on the taskbar / dock icon and tray |
| M10 link previews | — | Cards for links |
| M11 composer | Enter / Shift+Enter, attach | Emoji picker, `:shortcode:`, Markdown keys, ↑ to edit |
| M12 slash commands | Evo Harness has a `/` menu | A `/` menu in Evo Mates |
| M13 mentions of the person | — | `@you` highlights, notifies, lists in Today |

## 2. Patterns

| Pattern | How common | Taken |
| --- | --- | --- |
| **Typing line** at the feed's foot naming who types ("A is typing…", "A and B are typing…", "A and 2 others…"), faces beside it; the line goes when the message lands | very common | yes |
| **Status under the last own message**: Sent, Delivered, Read; in groups "Seen by" with small faces | very common | yes, as Sent → Queued → Seen → Replied |
| **Reactions as chips under the bubble** with a count; hover lists who reacted; several people per message | very common in team chat | yes, in channels |
| **Agents react to acknowledge** a request instead of posting a line, and may swap the reaction when done; reactions are rate-limited and never trigger a reply | common (2026 agent guidance) | yes: a `react` tool, info only, never wakes |
| **Reply with a quote** that jumps to the original; threads in a side panel in team chat | very common | quotes yes; threads no (Gate A) |
| **Attachments on any message**, previewed for everyone | very common | yes, for mates' hand-offs and lines |
| **Edit** with an "Edited" mark; **regenerate** for AI replies | very common | yes |
| **Unsend** before it is read; **Delete for me** | common | yes |
| **Forward** with "Forwarded from" | very common | yes |
| **Drafts** kept per conversation, "Draft:" in the list | very common | yes |
| **Pinned messages** bar under the header | common | yes |
| **Mute for a time**, notification levels per group | very common | yes |
| **Unread count on the app icon** | very common | yes |
| **Link previews** fetched by the app, switchable | very common | yes, off by default (Gate A) |
| **Composer**: emoji picker, `:shortcode:`, Markdown keys, ↑ edits the last message | very common | yes |
| **Slash commands** menu | common in team chat | yes |
| **Mentions of me** highlighted and collected | very common | yes |

## 3. Lessons for mates

- A mate's acknowledgement should cost nothing: a reaction, not a reply, and
  never a new turn for the mate it reacts to.
- "Seen" for a mate means its turn started on the message; "Replied" means its
  answer landed. These are the states the person actually waits on.
- Agents reacting too often is noise: one reaction per mate per message, and
  none on its own lines.
