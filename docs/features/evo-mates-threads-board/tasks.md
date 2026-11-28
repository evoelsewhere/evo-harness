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

- [x] T7 [FR-8, FR-9, NFR-1] Migration 0020 and `boards.rs` with tests; downgrade script tested on a scratch copy — `crates/evo-store/` , `docs/features/evo-mates-threads-board/downgrade-0020.py`
- [x] T8 [FR-8 – FR-10, FR-14] Shell commands and events — `src-tauri/src/mates/board.rs`, `src-tauri/src/lib.rs`
- [x] T9 [FR-11, FR-13] Seat tools `board_*`, `channel_post_line` RPC, `board` entries — `src-tauri/src/agent_tools/mates.rs`, `crates/evo-engine/src/modes.rs`, `coordinator.rs`
- [x] T10 [FR-12, FR-15] Assignments create cards (linked to their thread), reports move them — `coordinator.rs` — tests
- [x] T11 [FR-8 – FR-10] Board view, card sheet, list view, filters, search, drag — `features/mates/board-view.tsx`, `board-logic.ts`, `board-key-dialog.tsx`, `card-sheet.tsx`, `lib/mates.ts`, `stores/board-store.ts` — tests of the pure parts, shots
- [x] T12 [FR-14, FR-15] My cards on Today, profile cards, `KEY-n` links, thread header chip, Track on board — shots
- [x] T13 [NFR-4, NFR-5] Board fixture (500 cards), shots, scan

## Part C — linking (merge on its own)

Done when: shares, links, `share_to` with consent, Linked, Threads list,
resolve and Also send to channel work.

- [x] T14 [FR-16, FR-20, NFR-1] Migration 0021 (`mate_shares`, `thread_follows`) with tests; downgrade script
- [x] T15 [FR-16, FR-19] Share lines (select mode, dialog, card, markup read as quoted), Linked panel
- [x] T16 [FR-17] Links: `#channel`, `@mate`, Copy link, `evo://mates/…` cards
- [x] T17 [FR-18, NFR-6] `share_to` tool with the per-mate setting and approval
- [x] T18 [FR-20 – FR-22] Follow, Threads in the sidebar, unread, resolve, Also send to channel
- [x] T20 [FR-23] Threads tab in the channel header — `features/mates/channel-threads.tsx`, `channel-rows.ts` (`channelThreads`), `channel-header.tsx`, `mates-app.tsx` — shots
- [x] T19 [NFR-4] Shots and scan

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
| FR-8 | `boards.rs` tests (on with four columns, a bad key refused, off keeps the cards and the key); shots of the Chat · Board switch and the board at 1440, 1000, 720 px | passed (2026-11-29, B) |
| FR-9 | `boards.rs` tests (fields, priorities checked, every change logged with who: "moved LAUNCH-2 to Review", "assigned it to Lin, …"); card sheet shots | passed (2026-11-29, B) |
| FR-10 | `board-logic.test.ts` (filters, search, order, drop index, dates); Playwright on the fake IPC: dragging LAUNCH-4 onto Done calls `card_move` (Done, index 1), "Assignee: Kai" leaves LAUNCH-5 and LAUNCH-1; list view shots | passed (2026-11-29, B) |
| FR-11 | Shell tests `assignees_are_members_or_the_person`, `due_dates_read_as_local_time`; tools offered only to a seat whose channel's board is on (`board_of_seat`), cards of another board refused (`card_of`) | passed by test and review (2026-11-29, B); a model's tool call not run in the real app |
| FR-12 | Coordinator test `an_assignment_makes_a_card_linked_to_its_thread`; `boards.rs` test of `assignment_card` and `review_column` | passed by test and review (2026-11-29, B); a lead's real assignment not run in the real app |
| FR-13 | `board` entries drawn as lines with the key as a link (channel shot: "Kai moved LAUNCH-1 to Review"); they never enter the coordinator's lines | passed (2026-11-29, B) |
| FR-14 | `boards.rs` test `my_cards_are_open_cards_on_boards_that_are_on`; `board-logic.test.ts` (`linkCards`, `splitCardKeys`); Today shot with My cards | passed (2026-11-29, B); the profile's Open cards not shot |
| FR-15 | Thread shot with the LAUNCH-5 · In progress chip; Track on board and the card's Open its thread by review | passed by test and review (2026-11-29, B) |
| NFR-1 (0020) | `downgrade-0020.py` on a scratch copy of a schema-18 database taken to 20 with a board and a card: back to 19, 31 tables as at 19, integrity ok, backup written; refuses when `user_version` and `schema_migrations` disagree | passed (2026-11-29) |
| NFR-3 | The person's board commands only write the database and emit `mates://board`; no engine call | passed by review (2026-11-29) |
| NFR-4 (B) | Shots of board, list, card, thread, Today and channel at 1440, 1000, 720 px, light and dark; `--scan`: nothing sideways (the columns scroll on purpose, marked `data-scrolls-x`) | passed (2026-11-29, B) |
| NFR-5 | 500-card fixture: the board draws 820 ms after the Board tab's click in the Vite dev build (headless, unminified, includes the fetch); shot | passed (2026-11-29); not measured in a release build |
| Checks B | typecheck; `bun test` 545 pass; `bun run test:shell` 871 pass; `cargo test -p evo-engine` 1034 passed with the 2 known failures; store 96, mesh 53; clippy clean on the changed files; fmt, `git diff --check` | passed (2026-11-29) |
| FR-16 | `links-logic.test.ts` (share markup round-trip, `instruction="false"`); coordinator test `shared_lines_are_quoted_and_mentions_of_the_person_found`; Playwright on the fake IPC: select Kai's and Lin's lines, Share into Ava with a note → `share_record` (both line ids, the note) and a prompt to `chat-ava` starting `<evo-mates-share from="#Launch room" … lines="2" instruction="false">` | passed (2026-11-29, C); the mate's next turn quoting it not run in the real app |
| FR-17 | `links-logic.test.ts` (message links, `#channel` / `@mate` links, longest name first, not in code); Copy link put `evo://mates/chan-launch/k1` on the clipboard; link cards by review | passed (2026-11-29, C) |
| FR-18 | Shell test `sharing_asks_by_default_and_always_from_the_direct_chat`; refusals (not a member, never, the same channel) by review; the card through `channel_post_share` | passed by test and review (2026-11-29, C); a model's call not run; mate conversations as targets: Later (D11) |
| FR-19 | Linked dialog shot (shared from #Research notes, shared into Ava); `links.rs` test `a_share_links_both_conversations`; mate Overview › Linked | passed (2026-11-29, C) |
| FR-20 | `links.rs` test `a_followed_thread_is_unread_until_read_and_a_reply_reopens_it`; Threads page shot with unread first and the sidebar count; Unfollow → `thread_set(following: false)`; opening the thread → `thread_read` | passed (2026-11-29, C) |
| FR-21 | Same `links.rs` test (resolve, a reply reopens); Resolve → `thread_set(resolved: true)`; shots of "Resolved" on the summary and the panel's Unresolve | passed (2026-11-29, C) |
| FR-22 | Also send to #Launch room → prompt `<evo-mates-thread root="k1" also="channel"/>`; the coordinator posts `alsoChannel`; shot of "Replied in a thread · View" in the feed | passed (2026-11-29, C) |
| FR-23 | Threads tab shots at 1440, 1000, 720 px, light and dark (open thread with its LAUNCH-5 chip, the resolved one folded) | passed (2026-11-29, C) |
| NFR-1 (0021) | `downgrade-0021.py` on a scratch copy taken 18 → 21 with a share and a follow: back to 20, 35 tables as at 20, integrity ok, backup written | passed (2026-11-29) |
| NFR-2, NFR-6 | Shares reach mates as `<evo-mates-share … instruction="false">` (quoted, never routed); `share_to` only into the mate's own channels, asking by default and always from its direct chat | passed by test and review (2026-11-29) |
| NFR-4 (C) | Shots of the threads tab, Threads page, thread, share, Linked, channel and board at 1440, 1000, 720 px, light and dark; `--scan`: nothing sideways | passed (2026-11-29, C) |
| Checks C | typecheck; `bun test` 549 pass; `bun run test:shell` 872 pass; `cargo test -p evo-engine` 1035 passed with the 2 known failures; store 98, mesh 53; clippy clean on changed files; fmt, `git diff --check` | passed (2026-11-29) |
| Review A #1 | Test `a_hand_off_woken_seat_reports_to_the_jobs_place` (`place_of_seat`, `join_job` on a hand-off) | passed (2026-11-29) |
| Review A #2 | Test `channel_lines_are_owed_after_thread_work` (`seen` per seat and place; thread lines only those not yet given) | passed (2026-11-29) |
| Review A #3 | Test `a_late_report_stays_in_its_own_place`; `on_mail` sends only the job's seats into the job; retro, share and tool board lines carry `thread: null` | passed by test and review (2026-11-29) |
| Review A #4 | Test `asking_is_kept_per_place` | passed (2026-11-29) |
| Review A #5 | Test `a_restore_merges_lines_and_rebuilds_what_each_seat_saw`; the restore holds an async lock across the read and sets done only on success | passed by test and review (2026-11-29) |
| Review A #6 – #8 | Thread panel keyed by its root; only one box takes dropped files (the thread's while open); the channel's box shows Stop and runs `/stop` only when the job is in the channel | passed by review and typecheck (2026-11-29); not run in the real app |
| Review A #9 | Test `untrusted_text_cannot_fake_the_persons_words` (parent, recent lines, quotes, a mate's text, reaction notes; other markup kept) | passed (2026-11-29) |
| Review A #10 | FR-5 says 20; `openJob` counts only the latest route (`channel-rows.test.ts`) | passed (2026-11-29) |
| Review B #1, #2 | Engine test `a_report_pays_its_own_assignments_card` (`Owed` keeps a queue per pair with the assignment's card id; a question's answer moves none); store `report_moves_card` moves only forward (test in `a_column_goes_with_its_cards_moved…`); identical re-sent assigns make no card (per message) | passed (2026-11-29) |
| Review B #3 | `board_get_card` (numbered checklist, labels, description, made by, activity); `board_update` takes `add_labels`, `remove_labels`, `add_items`, `check`, `uncheck`, `remove_items`; store test `checklist_and_labels_change_from_what_the_card_holds` | passed (2026-11-29); a model's calls not run |
| Review B #4 | Shell test `a_due_date_that_does_not_read_is_an_error_not_a_clear` (RFC 3339 with `Z` or an offset, seconds, plain dates) | passed (2026-11-29) |
| Review B #5 | Store tests (moves left, the first column's cards right, positions renumbered, a logged move each, by channel); the window asks first with the count ("Its 2 cards move to To do…", Playwright on the fake IPC); one board line through the channel's engine | passed (2026-11-29); the board line not run in the real app |
| Review B #6 | The title keeps the person's typing (keyed by the card, follows changes only when not focused); labels and checklist through `card_labels` / `card_checklist`; a failed move reads the board again | passed by review and typecheck (2026-11-29) |
| Review B #7 | Keys set only when they change; stable `showCardLink` / `openLinkPath`; `cards_assigned` reads channels once and each board once; `watchBoards` / `watchThreads` stop with the window | passed by review (2026-11-29) |
| Review B #8 | `markdown-prose.ts` (fences ``` and ~~~, an open fence, indented code, inline code of any backticks, links, images, autolinks, bare URLs); `board-logic.test.ts` cases for each; `linkPlaces` uses it too | passed (2026-11-29) |
| Review B #9 | Shell test `board_text_is_data_and_cannot_fake_a_block`; `board_list` and `board_get_card` answer inside `<board-data instruction="false">` and say who made each card | passed (2026-11-29) |
| Checks (reviews) | typecheck; `bun test` 550 pass; `bun run test:shell` 874 pass; `cargo test -p evo-engine` 1042 passed with the 2 known failures; store 100; fmt; clippy clean on changed files | passed (2026-11-29) |
