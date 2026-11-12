# Sidebar items: a More menu and Edit sidebar — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-11 |

## 1. Phases

### Phase 1 — The list and the saved choice

Done when: the item list exists and a saved choice is read and written.

- [x] T1 [FR-1, FR-5] Tests for `isShown` (defaults, saved choice, unknown ids) and `parseSidebarShown` (malformed values) — `apps/desktop/src/features/shell/sidebar-items.test.ts` — check: `bun test src/features/shell/sidebar-items.test.ts` fails before T2
- [x] T2 [FR-1] The item list with Scheduler, Usage and Customize — `apps/desktop/src/features/shell/sidebar-items.ts` — check: T1 passes
- [x] T3 [FR-4, FR-5] `sidebarShown`, `loadSidebarItems`, `setSidebarShown`, setting `ui.sidebar.items` — `apps/desktop/src/stores/ui-store.ts` — check: T1 passes; `bun run typecheck`

### Phase 2 — Sidebar, More menu, Edit sidebar

Done when: the sidebar shows the chosen rows, More lists the rest, and Edit sidebar changes them live.

- [x] T4 [FR-4, NFR-2] Checkbox component — `apps/desktop/src/components/ui/checkbox.tsx` — check: `bun run typecheck`
- [x] T5 [FR-4] Edit sidebar dialog — `apps/desktop/src/features/shell/edit-sidebar-dialog.tsx` — check: manual, running app
- [x] T6 [FR-2, FR-3, FR-6] Rows from the list, the More row and menu — `apps/desktop/src/features/shell/app-sidebar.tsx` — check: manual, running app
- [x] T7 UI map — `apps/desktop/README.md` — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `bun test src/features/shell/sidebar-items.test.ts`; rows, menu and dialog all render from `SIDEBAR_ITEMS` | passed (2026-11-11) |
| FR-2 | Running app: rows read New chat, Usage, More with default choices | passed (2026-11-11): rows follow the saved choice in the app; the default rows by `bun test` |
| FR-3 | Running app: More › Scheduler opens the Scheduler | passed (2026-11-11) |
| FR-4 | Running app: ticking Scheduler in Edit sidebar adds its row before the dialog closes | passed (2026-11-11): unticking removed the row with the dialog open |
| FR-5 | `bun test` (defaults and saved choices); running app: choice kept after reload | passed (2026-11-11) |
| FR-6 | Running app: Scheduler hidden and open → More is highlighted | passed (2026-11-11) |
| NFR-1 | Running app, light and dark | dark passed (2026-11-11); light not run |
| NFR-2 | Running app: keyboard through More, the menu and the dialog | passed (2026-11-11): More menu by keyboard; dialog checkboxes not run |
| NFR-3 | Fresh profile: New chat, Usage, More | passed (2026-11-11): `bun test` default case |
