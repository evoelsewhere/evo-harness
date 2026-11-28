# Evo Mates messaging basics (Train 1) — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-29 |

Paths under `apps/desktop/src/` unless they start with `src-tauri/`,
`crates/` or `docs/`.

## Merge 1a — UI and shell

Done when: FR-1, FR-2, FR-7 – FR-9, FR-11 – FR-13 and FR-16 work in the
harness in light and dark at 1440, 1000 and 720 px; typecheck, `bun test` and
the shell's tests pass; merged to main.

- [x] T1 [FR-1] `typingWords` (+test) and the channel's typing line — `features/mates/composer-keys.ts`, `typing-line.tsx`, `channel-view.tsx`
- [x] T2 [FR-2] status rows and "Seen by" (+tests) — `features/mates/thread-rows.ts`, `thread.tsx`, `channel-rows.ts`, `channel-view.tsx`
- [x] T3 [FR-7, FR-8, FR-9, FR-11] `drafts`, `edited`, `hidden` in the window store — `stores/mates-ui-store.ts`
- [x] T4 [FR-7, FR-8, FR-9] Edit, Try again, Unsend, Delete for me — `features/mates/thread.tsx`, `channel-view.tsx`
- [x] T5 [FR-11] drafts in both boxes and the roster — `features/mates/composer.tsx`, `channel-view.tsx`, `roster.tsx`
- [x] T6 [FR-12] emoji data and shortcodes (+test), the picker, Markdown keys and ↑ (+test) — `features/mates/emoji.ts`, `composer-assist.tsx`, `composer-keys.ts`
- [x] T7 [FR-13] slash commands (+test) and their menu — `features/mates/slash-commands.ts`, `composer-assist.tsx`, both boxes
- [x] T8 [FR-16] the icon badge — `src-tauri/src/mates/badge.rs`, `lib.rs`, `features/mates/badge.ts`, `mates/mates-app.tsx`
- [x] T9 harness states and shots; docs (UI map, spec History) — `scripts/readme-shots/`, `apps/desktop/README.md`

## Merge 1b — engine and database

Done when: FR-3 – FR-6, FR-10, FR-14, FR-15, FR-17, FR-18 work; engine,
store, mesh and shell tests pass; the downgrade script is tested on a scratch
copy; merged to main.

- [ ] T10 [FR-3, NFR-1] migration 0019, store functions (+tests), downgrade script tested on a scratch copy — `crates/evo-store/`, `docs/features/evo-mates-messaging/`
- [ ] T11 [FR-3, FR-4] shell commands and the `react` tool (+tests) — `src-tauri/src/mates/`, `src-tauri/src/agent_tools/mates.rs`
- [ ] T12 [FR-4, FR-5] line ids, reply marks and reaction notes in deliveries (+engine tests) — `crates/evo-engine/src/channel/`
- [ ] T13 [FR-6] attachments on mail and seat prompts (+tests) — `crates/evo-mesh/`, `crates/evo-engine/`, shell tools
- [ ] T14 [FR-3, FR-5, FR-6] channel view: reaction chips, Reply, quotes, files on cards — `features/mates/channel-view.tsx`, `channel-rows.ts`
- [ ] T15 [FR-10] Forward — `features/mates/forward-dialog.tsx`, `thread.tsx`, `channel-view.tsx`
- [ ] T16 [FR-14] pins bar — `features/mates/pins-bar.tsx`
- [ ] T17 [FR-15] mute and levels (+shell test) — `src-tauri/src/supervisor/notices.rs`, menus
- [ ] T18 [FR-17] link previews (+shell test), the setting — `src-tauri/src/`, `features/mates/link-card.tsx`, settings
- [ ] T19 [FR-18] mentions of the person — coordinator, `notices.rs`, Today
- [ ] T20 harness, shots, docs; checks

## Verification

| Requirement | Check | Result |
| --- | --- | --- |
| FR-1 | Channel shots with two seats working: "Kai and Mo are typing…" with both faces; `composer-keys.test.ts` (words for one, two, three) | passed (2026-11-29, 1a) |
| FR-2 | `thread.test.ts` (Sent, Seen, Queued, nothing once replied); `channel-rows.test.ts` (Seen by from the route) | passed (2026-11-29, 1a) |
| FR-7 | Edit shot (↑ in an empty box opens the last message in place, Save / Cancel); the engine's `editMessage` as Evo Harness uses it | passed (2026-11-29, 1a); the mate's new answer not run in the real app |
| FR-8 | Try again on the last reply in the hover bar and the menu (code review; the same `editMessage`) | passed by review (2026-11-29, 1a) |
| FR-9 | Unsend on a queued message (`cancelQueued`); Delete for me hides a bubble across restarts (window store) | passed by review (2026-11-29, 1a) |
| FR-11 | Today and roster shots: "Draft: Can you also check the A…" from a saved draft | passed (2026-11-29, 1a) |
| FR-12 | Emoji picker and `:ro` completion shots; `composer-keys.test.ts` (wrapping, shortcodes) | passed (2026-11-29, 1a) |
| FR-13 | Slash menu shot; `composer-keys.test.ts` (filter, parse, check-in schedules) | passed (2026-11-29, 1a) |
| FR-16 | `badge.test.ts`; `mates/badge.rs` test; `cargo check` of the shell | passed (2026-11-29, 1a); the icon itself not seen in the real app |
| NFR-4 | Shots of channel, edit, slash, emoji, shortcode, Today and a conversation at 1440, 1000, 720 px, light and dark; overflow scan: nothing sideways | passed (2026-11-29, 1a) |
| Checks 1a | typecheck; `bun test` 524 pass, 0 fail; `bun run test:shell` 859 pass, 0 fail (temporary agent folder) | passed (2026-11-29) |
| FR-3 – FR-6, FR-10, FR-14, FR-15, FR-17, FR-18 | Merge 1b | not run |
