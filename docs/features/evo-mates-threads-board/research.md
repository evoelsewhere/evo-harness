# Evo Mates threads, board and linked conversations — Research

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Backlog | [po-backlog-2026-11-29.md](../evo-mates/po-backlog-2026-11-29.md) (Train 2, B1 – B13) |
| Updated | 2026-11-29 |

Patterns only, no product names (repository rule). Sources: the agent's
knowledge of the leading team-chat apps, issue trackers and Kanban tools, and
of how agent teams keep shared work visible.

## 1. What the code already has (checked 2026-11-29)

| Item | In Evo today | Left |
| --- | --- | --- |
| Channel feed | One feed per channel: `channel` custom entries of the channel's root chat (`message`, `route`, `handoff`, `notice`, `job`, `retro`); the coordinator keeps every line in memory and gives each seat the lines since its last turn, at most 30 (`crates/evo-engine/src/channel/coordinator.rs`) | Lines that belong to a thread; context per thread |
| Replies | A reply is a quote that jumps to the original (messaging FR-5); a mate answers a line with `↩ id` | A real thread with its own feed |
| Lines after a restart | The coordinator's `lines` start empty when the engine starts again; the window still shows every entry | Rebuild lines from the entries, so a thread's parent and helpers are known |
| Assignments | A lead's `[assign]` mail is a hand-off card; a member's answer back is a report (`Owed`) | A card per assignment, moved when the report comes |
| Tasks | — (evo-mates T1 was planned, moved to Train 2) | Board, cards, columns, activity |
| Forward | Forward a line into another mate's chat or channel, marked `<evo-mates-forward>` (messaging FR-10) | Several lines, a note, a card with a link back, both sides linked |
| Mates' reach | A mate reads only its own chat and the channels it is in (evo-mates FR-50) | A consented `share_to` |

## 2. Patterns

| Pattern | How common | Taken |
| --- | --- | --- |
| **Reply in thread** from a message's hover bar; the thread opens in a side panel beside the feed, the parent on top | very common | yes |
| **Summary under the parent**: "3 replies", the last repliers' faces, "Last reply 2 min ago"; click opens the panel | very common | yes |
| Thread replies stay out of the main feed unless the author ticks **Also send to channel** | very common | yes |
| Following: authors, repliers and people mentioned follow a thread; a **Threads** view lists followed threads, unread first | common | yes |
| Resolve / mark as done on a thread (folds it, shows "Resolved") | common in work tools | yes |
| A bot in a thread answers in the thread and sees the thread, not the whole channel | common for chat agents | yes (B11) |
| **Board per project**: columns To do / In progress / Review / Done, renamable, reorderable; cards with a key `KEY-12` | very common | yes |
| Card fields: title, Markdown description, assignee with face, priority (Low → Urgent), due date, labels, checklist, links, activity log | very common | yes |
| Drag within and between columns; quick add at a column's top; side sheet to edit; filters (assignee, label, priority), search, list view | very common | yes |
| `KEY-12` in any message becomes a link to the card | very common | yes |
| "My issues" across projects; overdue highlighted | very common | yes, on Today |
| A card has its discussion thread; a thread becomes a card ("Create issue from message") | common | yes |
| Agents move cards and comment through tools; each change shows in the activity log and as a short channel line | emerging | yes |
| Swimlanes, sprints, story points, burndown | common in large trackers | no — Later; one team's board stays simple |
| WIP limits per column | some | no — Later |
| **Share message to another conversation** with a note; the receiver sees a card with the excerpt and a link back | very common | yes |
| `#channel` and `@person` links; **Copy link** on a message; a pasted message link unfurls | very common | yes |
| Cross-posting by bots needs the workspace's consent; per-bot "Ask / Allow / Never" | emerging for agents | yes |
| A "Linked" or "References" list on an item | common in trackers | yes |

## 3. Choices this research led to

- **One thread job at a time per channel.** The coordinator holds one open
  message and its hand-off count; running thread jobs side by side would
  need a coordinator per thread. A thread message sent while the channel
  works waits in the channel's queue, like any message; a message for a
  mate at work in the same thread still steers it. Parallel threads go to
  Later.
- **Who a thread wakes**: a mention in the new message decides; with none,
  the thread's mates (its starter, mates mentioned in it, mates who replied);
  with none of those, the channel's rule.
- **What a thread delivery carries**: the parent line, labelled as the
  parent and never routed, and the thread's lines; never the channel's.
- **Boards live in the app database**, not in the channel's entries: the
  person edits them while no engine runs, and Today reads every board.
- **Board tools need no approval**: they only change the board, every change
  is logged with who made it and posted in the channel, and mates cannot
  delete a card. Sharing outside the conversation does need consent (B8).
