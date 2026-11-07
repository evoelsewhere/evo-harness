# Side panel tabs, browser pages and terminals per chat — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-07 |

## 1. Phases

### Phase 1 — Per-chat tabs, pages and shells

Done when: FR-1 – FR-8 pass.

- [x] T1 [FR-7] Session-storage map from page label / pty id to chat — `apps/desktop/src/lib/tab-chats.ts` — check: `bun run typecheck`
- [x] T2 [FR-1, FR-4, FR-5] Per-chat open tabs and visible tab in the UI store — `apps/desktop/src/stores/ui-store.ts` — check: show a new chat, the panel shows the launcher
- [x] T3 [FR-2, FR-4, FR-7, FR-8] Browser tabs carry their chat; the store mirrors the showing chat's — `apps/desktop/src/stores/browser-store.ts` — check: A → B → A keeps A's page
- [x] T4 [FR-3, FR-4, FR-7, FR-8] Terminal tabs carry their chat; the panel shows the showing chat's — `apps/desktop/src/stores/terminal-store.ts`, `features/terminal/terminal-panel.tsx` — check: same with a shell
- [x] T5 [FR-5] Terminal and browser tools act on the calling chat's tabs — `features/terminal/agent-tools.ts`, `features/browser/agent-tools.ts`, `stores/engine-store.ts` — check: `terminal_list` in chat B lists none of chat A's
- [x] T6 [FR-1 – FR-4, FR-6] Follow the showing chat; release a chat that is gone — `apps/desktop/src/stores/chat-scope.ts`, `app/use-app-bootstrap.ts` — check: switch chats in the running app
- [x] T7 [FR-1, FR-5, FR-6, FR-7] Unit tests for the ownership map and the chat swap — `apps/desktop/src/stores/chat-panels.test.ts` — check: `bun test`
- [x] T8 [FR-1 – FR-5] Update the desktop UI map — `apps/desktop/README.md` — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Running app: show a new chat after another opened Terminal and Browser | passed (2026-11-07) — `openTabs` empty, panel closed; unit tests |
| FR-2 | Running app: chat A's page, show chat B | passed (2026-11-07) — B lists none, A's page back on return |
| FR-3 | Running app: same with a shell | passed (2026-11-07) |
| FR-4 | Running app: A → B → A | passed (2026-11-07) |
| FR-5 | Real chats: B's agent counts the tabs it sees | passed (2026-11-07) — `terminal_list` and `browser_tabs` empty in B while A had both |
| FR-6 | Discard a draft that had a terminal | not run |
| FR-7 | Reload the window | passed (2026-11-07) — tabs stayed with their chat |
| FR-8 | Close Browser in one chat while another has pages | not run |
| NFR-1 | Switch away and back during a running command | not run |
