# Evo Mates UI v2 — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-29 |

Paths are under `apps/desktop/src/` unless they start with `docs/` or
`scripts/`. Pure logic first, with its `bun test` test in the same task.

## Phase 1 — Foundations

Done when: presence, window preferences, layout rules, plain previews and the
keyboard hook exist with tests, and the typecheck passes.

- [x] T1 [FR-5, FR-4] `presenceOf` / `usePresence` / channel presence — `features/mates/presence.ts`, `presence.test.ts` — bun test
- [x] T2 [FR-5, NFR-3, NFR-6] `PresenceFace` with corner mark and label — `features/mates/presence-face.tsx` — typecheck
- [x] T3 [FR-1, FR-3, FR-30] persisted `useMatesUiStore` — `stores/mates-ui-store.ts` — typecheck
- [x] T4 [FR-1, FR-8, FR-31] `sidebarForm`, `panelDocks`, conversation order, next unread — `features/mates/sidebar-layout.ts`, `sidebar-layout.test.ts` — bun test
- [x] T5 [FR-4] `plainPreview` — `features/mates/plain-preview.ts`, `plain-preview.test.ts` — bun test
- [x] T6 [FR-8] `useMatesKeys` and the shortcuts sheet — `features/mates/use-mates-keys.ts`, `shortcuts-sheet.tsx` — typecheck

## Phase 2 — Sidebar, rail, switcher

Done when: the sidebar folds to a rail by width and Ctrl+B, sections fold,
rows show presence, bold unread, counts and ⋯, and Ctrl+K opens the switcher.

- [x] T7 [FR-1 – FR-3] sidebar full and rail forms, foldable sections, home rows with badges — `features/mates/mates-shell.tsx`
- [x] T8 [FR-2, FR-4 – FR-6] rows and tiles with presence, counts, plain previews, ⋯, Pause and Mute — `features/mates/roster.tsx`
- [x] T9 [FR-7, NFR-5] switcher items and ranking (+test), the dialog — `features/mates/switcher-items.ts`, `switcher-items.test.ts`, `quick-switcher.tsx`
- [x] T10 [FR-1, FR-7, FR-8, FR-31] window layout: sidebar form, switcher, sheet, keys, panel dock or float with scrim — `mates/mates-app.tsx`

## Phase 3 — Conversations and channels

Done when: a mate's conversation and a channel show day separators, the New
line, jump to latest, find, quote jumps, steps chips, hand-off cards and the
new headers.

- [x] T11 [FR-11, FR-12, FR-14, FR-17, FR-18] `day`, `stopped` rows, `stepItems`, long-text rule (+tests) — `features/mates/thread-rows.ts`, `thread.test.ts`
- [x] T12 [FR-15] `findMatches` and highlighting (+test), the find bar — `features/mates/find.ts`, `find.test.ts`, `find-bar.tsx`
- [x] T13 [FR-17] steps chip — `features/mates/steps-chip.tsx`
- [x] T14 [FR-10 – FR-18] thread: sticky days, New line, pill, hover time and bar, fold, quote jump, chip, stopped, no dots while a card waits — `features/mates/thread.tsx`
- [x] T15 [FR-10, FR-27] card frame, paused bar — `features/mates/conversation.tsx`
- [x] T16 [FR-9] mate header — `features/mates/mate-header.tsx`
- [x] T17 [FR-11, FR-20] channel day rows and route faces (+tests) — `features/mates/channel-rows.ts`, `channel-rows.test.ts`
- [x] T18 [FR-19] channel header with members, rule chip, Stop, Find, ⋯ — `features/mates/channel-header.tsx`
- [x] T19 [FR-15, FR-20, FR-21] channel view: hand-off cards, `@` list with role and presence, find marks — `features/mates/channel-view.tsx`

## Phase 4 — Profile, home, gallery, files, empty states

Done when: the profile has its hero and Overview, Today its greeting,
counters, team strip and kinds, the gallery its strip, Files its day groups
and views, and every empty list its sentence and action.

- [x] T20 [FR-29] `EmptyState` — `features/mates/empty-state.tsx`
- [x] T21 [FR-22 – FR-24, FR-31] profile hero, quick actions, Overview, save on blur — `features/mates/mate-panel.tsx`, `mate-overview.tsx`
- [x] T22 [FR-25, FR-26] Today counters, team strip, Needs you kinds (+tests) — `features/mates/today.tsx`, `today-sections.ts`, `today-sections.test.ts`, `lib/mates.ts`
- [x] T23 [FR-28] gallery steps strip and cards — `features/mates/gallery.tsx`
- [x] T24 [FR-30] Files by day, grid and list (+test) — `features/mates/files-page.tsx`, `files-page.ts`, `files-page.test.ts`
- [x] T28 [FR-30] thumbnails in the Files grid: pictures, text pages, CSV tables (+test) — `features/mates/file-preview.ts`, `file-preview.test.ts`, `files-page.tsx`, `scripts/readme-shots/mock-mates-v2.js`

## Phase 5 — Docs and verification

Done when: the docs describe what was built and every check below has a
result.

- [x] T25 [NFR-2] DESIGN.md Evo Mates layer, `apps/desktop/README.md` UI map, evo-mates spec History and ux.md pointer — docs
- [x] T26 [NFR-1, verification] sample states and the v2 capture with overflow scan — `scripts/readme-shots/mock-tauri.js`, `scripts/readme-shots/mates_v2_shots.py`
- [x] T27 checks: typecheck, `bun test`, build; screenshots light and dark at 1440, 1000, 720 px — record below

## Verification

| Requirement | Check | Result |
| --- | --- | --- |
| FR-1 – FR-3 | Shots at 720 px (rail: Today, Files, faces with marks, channels, footer icons) and 1440 / 1000 px (full sidebar, foldable sections with counts); `sidebar-layout.test.ts` | passed (2026-11-29) |
| FR-4 – FR-6 | Roster shots with working, needs you, paused, unread and a code-block preview read as text; `presence.test.ts`, `plain-preview.test.ts` | passed (2026-11-29) |
| FR-7, FR-8, NFR-5 | Switcher shot ("a": mates, channels, actions); shortcuts sheet shot; Ctrl+I, Ctrl+F, Ctrl+K and Ctrl+N driven by the harness; `switcher-items.test.ts` (100 rankings of 70 items under 5 ms each) | passed (2026-11-29) |
| FR-9, FR-10 | Mate header shots (state word, Find, model pill, profile, ⋯); a waiting mate shows no typing dots; the paused bar is the composer's (as-built) | passed (2026-11-29) |
| FR-11 – FR-18 | Three-day conversation shots: sticky day labels, New line, Jump to latest, fold with Show more, find "links" 1 of 8 with marks, steps chip; `thread.test.ts` (day rows, Stopped, step items, long text), `find.test.ts` | passed (2026-11-29) |
| FR-19 – FR-21 | Channel shots: header with instructions line, faces with seat state, rule chip, Stop, ⋯; members list with Lead, state and Message; hand-off cards with both faces; `channel-rows.test.ts` | passed (2026-11-29) |
| FR-22 – FR-24 | Profile shots at 1440 (docked) and 1000 px (floating with scrim): hero on the mate's colour, quick actions, Overview (model, approvals, today, following up, channels, latest files) | passed (2026-11-29) |
| FR-25, FR-26 | Today shots: greeting, four counters, team strip, Needs you "Approval" from `engine_questions`, oldest first; `today-sections.test.ts` | passed (2026-11-29) |
| FR-27 | Waiting mate shot: the approval under "Lin needs you · Your approval" | passed (2026-11-29) |
| FR-28 – FR-30 | Gallery shot with the steps strip; empty states in the sidebar and Overview; Files grid with thumbnails and day groups; `files-page.test.ts`, `file-preview.test.ts` | passed (2026-11-29) |
| FR-31 | Profile docked at 1440, floating at 1000; rail at 720 | passed (2026-11-29) |
| NFR-1 | Overflow scan of every view at 1440, 1000 and 720 px, and Today and a conversation at 1180, 900, 760 and 640 px, light and dark: no box scrolls sideways | passed (2026-11-29) |
| NFR-2, NFR-3 | Light and dark shots of every view; presence pulse and lifts under `motion-safe` only (code review); reduced motion not emulated | passed (2026-11-29), reduced motion by review |
| NFR-4 | Diff review: no engine, protocol or database change | passed (2026-11-29) |
| NFR-6, NFR-7 | Rows, tiles and icon buttons named (`aria-label`), presence marks labelled; new strings reviewed for developer words | passed (2026-11-29) |
| Checks | `bun run typecheck`; `bun test` 513 pass, 0 fail (455 before this feature); `vite build`; `cargo test -p evo-engine -p evo-store -p evo-mesh` (no Rust changed): 1021 passed, 2 failed — the two known failures (`external_transport…`, `bundled_evo_creator…`) | passed (2026-11-29) |
| Real app | Not run: the window was checked in the fake-IPC harness only (scripts/readme-shots), as the brief allowed | not run |
