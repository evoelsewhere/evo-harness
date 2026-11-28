# Evo Mates threads, board and linked conversations (Train 2) — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-29 |

Paths under `apps/desktop/src/` unless they start with `src-tauri/`,
`crates/` or `docs/`.

## Part A — threads (merge on its own)

Done when: a thread replies, routes and carries only its context; tests and
shots pass.

- [x] T1 [FR-4, FR-5] Tests first: `thread_mark`, thread routing (mention, the thread's mates, the channel's rule), a thread delivery with its parent and no channel lines — `crates/evo-engine/src/channel/coordinator.rs`, `route.rs` — `cargo test -p evo-engine channel`
- [x] T2 [FR-3 – FR-6] Coordinator: lines with threads, `thread_mark`, routing, deliveries, entries tagged with the thread, steer in a thread, late reports — `coordinator.rs`, `route.rs` — T1 passes
- [x] T3 [FR-7] `restore` from the channel's entries before the first turn, steer, mail or `channel_line` — `coordinator.rs`, `crates/evo-engine/src/modes.rs` — test of the rebuild from entries
- [x] T4 [FR-1 – FR-3] Tests first: `channelRows` without thread lines, `threadSummaries`, `openJob`, `threadRows` — `features/mates/channel-rows.ts`, `channel-rows.test.ts` — `bun test`
- [x] T5 [FR-1, FR-2] Thread panel, Reply in thread, summary under the parent, thread composer (draft per thread, queue, steer, Stop), typing per thread — `features/mates/channel-view.tsx` (ThreadPanel), `channel-lines.tsx`, `line-tools.tsx`, `lib/mates.ts` — shots
- [x] T6 [NFR-4] Sample thread in the fake IPC; shots at 1440, 1000, 720 px, light and dark, `--scan` — `scripts/readme-shots/mock-mates-v2.js`, `docs/features/evo-mates-threads-board/assets/` — scan empty

## Part B — board (merge on its own)

Done when: board on, cards worked by the person and mates, assignments make
cards, Today shows my cards.

- [ ] T7 [FR-8, FR-9, NFR-1] Migration 0020 and `boards.rs` with tests; downgrade script tested on a scratch copy — `crates/evo-store/` , `docs/features/evo-mates-threads-board/downgrade-0020.py`
- [ ] T8 [FR-8 – FR-10, FR-14] Shell commands and events — `src-tauri/src/mates/board.rs`, `lib.rs`
- [ ] T9 [FR-11, FR-13] Seat tools `board_*`, `channel_post_line` RPC, `board` entries — `src-tauri/src/agent_tools/mates.rs`, `crates/evo-engine/src/modes.rs`, `coordinator.rs`
- [ ] T10 [FR-12, FR-15] Assignments create cards (linked to their thread), reports move them — `coordinator.rs` — tests
- [ ] T11 [FR-8 – FR-10] Board view, card sheet, list view, filters, search, drag — `features/mates/board-*.tsx`, `lib/mates.ts`, `stores/` — tests of the pure parts, shots
- [ ] T12 [FR-14, FR-15] My cards on Today, profile cards, `KEY-n` links, thread header chip, Track on board — shots
- [ ] T13 [NFR-4, NFR-5] Board fixture (500 cards), shots, scan

## Part C — linking (merge on its own)

Done when: shares, links, `share_to` with consent, Linked, Threads list,
resolve and Also send to channel work.

- [ ] T14 [FR-16, FR-20, NFR-1] Migration 0021 (`mate_shares`, `thread_follows`) with tests; downgrade script
- [ ] T15 [FR-16, FR-19] Share lines (select mode, dialog, card, markup read as quoted), Linked panel
- [ ] T16 [FR-17] Links: `#channel`, `@mate`, Copy link, `evo://mates/…` cards
- [ ] T17 [FR-18, NFR-6] `share_to` tool with the per-mate setting and approval
- [ ] T18 [FR-20 – FR-22] Follow, Threads in the sidebar, unread, resolve, Also send to channel
- [ ] T19 [NFR-4] Shots and scan

## Verification

| Requirement | Check | Result |
| --- | --- | --- |
| FR-1 | `channel-rows.test.ts` (thread rows: parent and its own lines); shots of the thread panel at 1440 (docked), 1000 and 720 (over the feed), light and dark | passed (2026-11-29, A); Stop and steering inside a thread not run in the real app |
| FR-2 | `channel-rows.test.ts` (`threadSummaries`: count, repliers, last); shot of "3 replies · Mo, Kai +1 · 4 min ago" | passed (2026-11-29, A) |
| FR-3 | `channel-rows.test.ts` (thread lines out of the feed); coordinator: `recent_for` takes lines with no thread only | passed (2026-11-29, A) |
| FR-4 | Coordinator tests `a_thread_wakes_its_mates_unless_a_mention_decides`, `a_thread_mark_names_the_parent_and_leaves_the_message` (a quoted parent never routes) | passed (2026-11-29, A) |
| FR-5 | Coordinator test `a_thread_delivery_carries_its_parent_and_lines_only` | passed (2026-11-29, A) |
| FR-6 | Code review: entries posted during a thread message carry its thread (`post`), late reports use the seat's thread, the count is per message; `openJob` test | passed by test and review (2026-11-29, A); a model's hand-off in a thread not run in the real app |
| FR-7 | Coordinator test `lines_come_back_from_the_entries`; `restore` runs before turn, steer, mail and `channel_line` | passed by test and review (2026-11-29, A); an engine restart not run in the real app |
| NFR-2 | The parent goes as `<parent … instruction="false">` (delivery test) | passed (2026-11-29, A) |
| NFR-4 (A) | Shots of channel and thread at 1440, 1000, 720 px, light and dark; `--scan`: nothing sideways | passed (2026-11-29, A) |
| Checks A | typecheck; `bun test` 540 pass, 0 fail; `cargo test -p evo-engine`: 1033 passed with the 2 known failures; `cargo fmt --check`, `git diff --check` | passed (2026-11-29) |
