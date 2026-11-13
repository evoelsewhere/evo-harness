# Split view: several chats side by side in one window — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-13 |

## 1. Phases

### Phase 1 — The split state and pane-scoped reads

Done when: the split store's rules and the pane view pass their tests, and
the engine store can focus a held chat synchronously and show a chat
without making it active.

- [x] T1 [FR-1, FR-4, FR-6, FR-7, FR-11 – FR-13] Tests for the split rules (open on a side, replace, swap, close, follow `activeId`, drop a shown chat, drop a missing chat) and for the ratio limits — `apps/desktop/src/stores/split-rules.test.ts` — check: `bun test src/stores/split-rules.test.ts` fails before T2
- [x] T2 [FR-1, FR-4, FR-6, FR-7, FR-10 – FR-12] The split rules (pure) and the split store, persisted per main window — `apps/desktop/src/stores/split-rules.ts`, `apps/desktop/src/stores/split-store.ts` — check: T1 passes
- [x] T3 [FR-3] Tests for `paneView` (the store itself for the active chat, a view for another, one view per snapshot and chat) — `apps/desktop/src/stores/pane-view.test.ts` — check: `bun test src/stores/pane-view.test.ts`
- [x] T4 [FR-3, FR-4, FR-12, NFR-2] `PaneChatContext`, `usePaneEngine`, `paneView`, `focusPane`, `showChat`; closing, forgetting or moving out the focused pane's chat focuses the other pane — `apps/desktop/src/stores/engine-store.ts`, `apps/desktop/src/stores/pane-view.ts` — check: T3 passes; `bun run typecheck`

### Phase 2 — Two panes on screen

Done when: two chats show side by side, each live, and a click in either
acts on that chat.

- [x] T5 [FR-3] The chat view reads its pane's chat: thread, composer, banner, approvals, questions, todo bar, queued messages, user messages, cards, agent runs, session cards. The composer takes outside drafts and file drops only while focused — `apps/desktop/src/features/chat/*.tsx`, `apps/desktop/src/features/cards/{card-layer,chip-row}.tsx`, `apps/desktop/src/components/ai-elements/{agent-run,session-proposal}.tsx` — check: `bun run typecheck`; one pane looks unchanged (NFR-3)
- [x] T6 [FR-1, FR-4 – FR-6, FR-13, NFR-3 – NFR-5] Split area (stable pane slots, one tree for one or two panes), pane header (title, working dot, open in window, close, focus ring), divider (veils while dragged, keyboard), narrow switcher; the chat card renders it — `apps/desktop/src/features/split/{split-area,pane-header}.tsx`, `apps/desktop/src/app/app.tsx` — check: running app

### Phase 3 — Drag, menu and keys

Done when: a chat can be dragged in, and the same works from the menu and
the keyboard.

- [x] T7 [FR-2, FR-7, FR-13] Tests for the drop-zone geometry (halves, the pane under the pointer, swap, narrow) and the preview box — `apps/desktop/src/features/split/drag.test.ts` — check: `bun test src/features/split/drag.test.ts`
- [x] T8 [FR-2, FR-7, FR-11] Pointer-driven drag (payloads, threshold, Esc, swallowed click), the gliding drop preview and the drag ghost; draggable sidebar rows; pane headers drag to swap — `apps/desktop/src/features/split/{drop-geometry,drag}.ts`, `drop-overlay.tsx`, `apps/desktop/src/features/shell/sidebar-chats.tsx` — check: T7 passes; running app
- [x] T9 [FR-8] **Open in › Split view** in the chat menu; Ctrl/⌘+1, Ctrl/⌘+2 and Ctrl/⌘+\ — `apps/desktop/src/features/shell/chat-menu.tsx`, `apps/desktop/src/app/use-app-bootstrap.ts` — check: running app

### Phase 4 — Motion and restore

Done when: pane changes are animated (and still with reduce motion), and the
layout comes back after a restart.

- [x] T10 [FR-9, NFR-1, NFR-6] The spring curve; pane enter, leave and FLIP (slide, clip reveal); the reduced-motion path — `apps/desktop/src/features/split/motion.ts`, `split-area.tsx` — check: running app, with reduce motion off and on
- [x] T11 [FR-10] Follow the active chat and the chat list; restore the saved pair once the last chat shows — `apps/desktop/src/stores/split-store.ts`, `apps/desktop/src/stores/engine-store.ts`, `apps/desktop/src/app/use-app-bootstrap.ts` — check: running app, after a restart
- [x] T12 UI map — `apps/desktop/README.md` — check: review

### Phase 5 — Nested sessions beside their parent

Done when: a started session's card opens the session beside its parent,
by button or by drag.

- [x] T13 [FR-14] `openBeside` (shared with Open in › Split view); the session card's **Open beside** and card drag — `apps/desktop/src/stores/split-store.ts`, `apps/desktop/src/components/ai-elements/session-proposal.tsx`, `apps/desktop/src/features/shell/chat-menu.tsx` — check: `bun run typecheck`; running app

## 2. Verification

In-app checks ran on 2026-11-13 in a debug build (`com.evoelsewhere.evo.e2e`) on a copy of the user's data, driven over WebView2 CDP with real mouse and key events.

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `bun test src/stores/split-rules.test.ts` (17); running app: a chat dragged to the right half opens a second pane | passed (2026-11-13) |
| FR-2 | `bun test src/features/split/drag.test.ts` (8); running app: the preview covers the half or pane under the pointer, and the ghost follows the pointer | passed (2026-11-13) |
| FR-3 | `bun test src/stores/pane-view.test.ts` (4); running app: a prompt typed in each pane went to its own chat, and both ran at once ("LEFT" and "RIGHT") | passed (2026-11-13); answering an approval from the other pane was not run (the same pointer-down focus path as Send) |
| FR-4 | Running app: a click in the other pane moves the focus ring, the sidebar highlight and `activeId` | passed (2026-11-13) |
| FR-5 | Running app: a divider drag (584/1006 px), arrow keys (0.50 → 0.46), End (the right pane at its 360 px minimum) | passed (2026-11-13) |
| FR-6 | Running app: the close button and Ctrl+\ close a pane; the other pane gets the focus | passed (2026-11-13); closing a pane while its chat works was not run (closing never stops or detaches the engine) |
| FR-7 | `bun test`; running app: a header dropped on the other half swaps the panes and mirrors the ratio | passed (2026-11-13) |
| FR-8 | Running app: Open in › Split view from a sidebar row's context menu; Ctrl+1, Ctrl+2, Ctrl+\ | passed (2026-11-13) |
| FR-9 | Running app: close plays a leave animation and then a FLIP; open plays an enter animation and a FLIP; with reduce motion emulated, a swap plays no animation (2 without it) | passed (2026-11-13) |
| FR-10 | Running app: after a restart the window showed the same two chats, the same focus and the same ratio | passed (2026-11-13) |
| FR-11 | Running app: a chat open in its own window, put in a pane: its window closed and its 3 turns showed in the pane | passed (2026-11-13) |
| FR-12 | Running app: archiving the focused pane's chat and the other pane's chat each closed that pane, and the remaining chat took the focus | passed (2026-11-13) |
| FR-14 | Running app: Open beside on a started session's card; the card dragged to the right half | not run — the dev app the person tests runs without a debugging port |
| FR-13 | Running app at 1000 px wide (678 px area): one pane and the switcher; the switcher shows the other chat | passed (2026-11-13) |
| NFR-1 | Running app (debug build, CDP attached): divider drag 57 fps (p95 18 ms); swap animation 59 fps (p95 18 ms). Laying both chats out on every frame gave 36 fps, which led to the FR-5 change | passed (2026-11-13) |
| NFR-2 | Running app: `showChat` attaches the pane's chat; both engines stayed ready and attached through the run | passed (2026-11-13) |
| NFR-3 | Running app: one pane renders the same tree and layout as before (no header, no divider) | passed (2026-11-13) |
| NFR-4 | Running app: panes are labelled regions; the divider is a focusable separator moved by keys; Ctrl+1/2, Ctrl+\ and the menu item work without a mouse | passed (2026-11-13) |
| NFR-5 | Running app: light and dark | passed (2026-11-13) |
| NFR-6 | `apps/desktop/package.json` unchanged | passed (2026-11-13) |
