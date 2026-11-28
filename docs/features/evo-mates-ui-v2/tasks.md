# Evo Mates UI v2 — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-28 |

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
- [ ] T21 [FR-22 – FR-24, FR-31] profile hero, quick actions, Overview, save on blur — `features/mates/mate-panel.tsx`, `mate-overview.tsx`
- [ ] T22 [FR-25, FR-26] Today counters, team strip, Needs you kinds (+tests) — `features/mates/today.tsx`, `today-sections.ts`, `today-sections.test.ts`, `lib/mates.ts`
- [ ] T23 [FR-28] gallery steps strip and cards — `features/mates/gallery.tsx`
- [ ] T24 [FR-30] Files by day, grid and list (+test) — `features/mates/files-page.tsx`, `files-page.ts`, `files-page.test.ts`

## Phase 5 — Docs and verification

Done when: the docs describe what was built and every check below has a
result.

- [ ] T25 [NFR-2] DESIGN.md Evo Mates layer, `apps/desktop/README.md` UI map, evo-mates spec History and ux.md pointer — docs
- [ ] T26 [NFR-1, verification] sample states and the v2 capture with overflow scan — `scripts/readme-shots/mock-tauri.js`, `scripts/readme-shots/mates_v2_shots.py`
- [ ] T27 checks: typecheck, `bun test`, build; screenshots light and dark at 1440, 1000, 720 px — record below

## Verification

| Requirement | Check | Result |
| --- | --- | --- |
| FR-1 – FR-3 | Shots at 720 px (rail) and 1440 px; Ctrl+B in the harness | not run |
| FR-4 – FR-6 | Shots of the roster with every state; `presence.test.ts`, `plain-preview.test.ts` | not run |
| FR-7, FR-8, NFR-5 | Switcher shot; `switcher-items.test.ts`; keys in the harness | not run |
| FR-9, FR-10 | Mate header shots, paused mate | not run |
| FR-11 – FR-18 | Long conversation shots; `thread.test.ts`, `find.test.ts` | not run |
| FR-19 – FR-21 | Channel shots, members popover, `@` list; `channel-rows.test.ts` | not run |
| FR-22 – FR-24 | Profile shots (Overview) | not run |
| FR-25, FR-26 | Today shots; `today-sections.test.ts` | not run |
| FR-27 | Approval card shot | not run |
| FR-28 – FR-30 | Gallery, empty states, Files shots; `files-page.test.ts` | not run |
| FR-31 | Shots at 1440, 1000, 720 px | not run |
| NFR-1 | Overflow scan at 1440, 1180, 900, 760, 640 px, light and dark | not run |
| NFR-2, NFR-3 | Light and dark shots; reduced motion emulated | not run |
| NFR-4 | Diff review: no engine, protocol or DB change | not run |
| NFR-6, NFR-7 | Accessibility tree and string review | not run |
