# Workspace rail and "Needs you" — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-23 |

## 1. Phases

### Phase 1 — Rules

Done when: the sidebar model's tests pass.

- [x] T1 [FR-2, FR-7, FR-9, FR-10] Tests for `initials`, `timeGroup`, `foldRuns`, `needsYou`, `rollUp` — `apps/desktop/src/features/shell/sidebar-model.test.ts` — check: `bun test` fails (module missing)
- [x] T2 [FR-2, FR-7, FR-9, FR-10] Implement the model — `apps/desktop/src/features/shell/sidebar-model.ts` — check: T1 passes

### Phase 2 — State

Done when: the shown workspace is kept, saved and used by New chat.

- [x] T3 [FR-4, FR-5] `shownWorkspace` / `showWorkspace` and the load fallback; drop `collapsed` — `apps/desktop/src/stores/session-store.ts` — check: typecheck
- [x] T4 [FR-6] `newChat()` defaults to the shown workspace — `apps/desktop/src/stores/engine-store.ts` — check: typecheck; manual
- [x] T5 [FR-2, FR-7] `useChatStates` — `apps/desktop/src/features/shell/use-chat-states.ts` — check: typecheck

### Phase 3 — UI

Done when: the sidebar shows the rail, Needs you and the grouped list.

- [x] T6 [FR-1, FR-2, FR-3] Workspace rail — `apps/desktop/src/features/shell/workspace-rail.tsx` — check: manual
- [x] T7 [FR-3, FR-7–FR-11] `NeedsYou`, `WorkspaceChats`, run folds, Older fold — `apps/desktop/src/features/shell/sidebar-chats.tsx` — check: manual
- [x] T8 [FR-1, FR-4] Layout and follow-the-open-chat — `apps/desktop/src/features/shell/app-sidebar.tsx` — check: manual
- [x] T9 UI map — `apps/desktop/README.md` — check: read

### Phase 4 — Names and icons

Done when: the rail shows names and each workspace's icon, and Change icon works.

- [x] T10 [FR-13] Tests for the logo candidates and image types — `apps/desktop/src/lib/project-logo.test.ts` — check: `bun test` fails
- [x] T11 [FR-13] Candidate list and types — `apps/desktop/src/lib/project-logo.ts` — check: T10 passes
- [x] T12 [FR-13, FR-14] Icon store: chosen icons, logo lookup — `apps/desktop/src/stores/workspace-icon-store.ts` — check: typecheck
- [x] T13 [FR-12 – FR-15] `WorkspaceIcon`, icon set, Change icon dialog; rail names; Needs you icons — `apps/desktop/src/features/shell/workspace-icon.tsx`, `workspace-rail.tsx`, `sidebar-chats.tsx`, `stores/ui-store.ts` — check: manual in the dev app

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Manual: rail in the app (light and dark) | passed (2026-11-23) — sample data, light and dark |
| FR-2 | `sidebar-model.test.ts` `rollUp`; manual dot on a waiting chat's workspace | passed (2026-11-23) |
| FR-3 | Manual: only the shown workspace's chats listed | passed (2026-11-23) — sample data |
| FR-4 | Manual: open a chat of another workspace from Needs you | passed (2026-11-23) — opening a teams-check chat from Needs you selects its icon |
| FR-5 | Manual: restart keeps the shown workspace | not run — load reads `sidebar.workspace` (sample data); a real restart not tried |
| FR-6 | Manual: New chat after picking a rail icon | not run |
| FR-7 | `sidebar-model.test.ts` `needsYou`; manual | passed (2026-11-23) |
| FR-8 | Manual: pinned chat inside its workspace | passed (2026-11-23) — sample data |
| FR-9 | `sidebar-model.test.ts` `timeGroup`; manual Older fold | passed (2026-11-23) |
| FR-10 | `sidebar-model.test.ts` `foldRuns`; manual | passed (2026-11-23) |
| FR-11 | Manual: nested sessions under their parent; Archived last | not run — nested sessions not in the sample data; Archived last passed |
| FR-12 | Manual: names under rail icons in the dev app | passed (2026-11-23) — sample data |
| FR-13 | `project-logo.test.ts`; manual: evo-harness shows its app icon | passed (2026-11-23) — tests, and a sample logo found and shown; the real folders not checked |
| FR-14 | Manual: Change icon…, restart keeps it | passed (2026-11-23) — dialog and saved icon and emoji read at load (sample data); a real restart not tried |
| FR-15 | Manual: Needs you rows show the icon | passed (2026-11-23) — sample data |
| FR-16 | Manual: switch Workspace names off in Settings → Appearance | passed (2026-11-23) — sample data: off 56 px and no names; on again from the switch, 72 px with names |
| NFR-1 | `git diff --stat` touches `apps/desktop/src` and docs only | passed (2026-11-23) |
| NFR-2 | `bun test src/features/shell/sidebar-model.test.ts` | passed (2026-11-23) |
| NFR-3 | Manual: light and dark; keyboard focus on rail icons | not run — light and dark checked; keyboard focus not tried |
