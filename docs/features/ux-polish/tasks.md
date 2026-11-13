# UX polish: clearer errors, states and text — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-13 |

## 1. Phases

### Phase 1 — Errors, overlap and states

Done when: a failed turn explains itself, nothing covers a pane, and loading
and stopped steps read as such.

- [x] T1 [FR-1] Tests for `explainError` — `apps/desktop/src/features/chat/explain-error.test.ts` — check: `bun test` fails before T2
- [x] T2 [FR-1] `explainError` and the failed-turn block (sentence, Open Providers / Retry / Compact, Details) — `apps/desktop/src/features/chat/{explain-error.ts,turn-error.tsx,chat-turn.tsx}` — check: T1 passes; running app
- [x] T3 [FR-2] The summary card is not pinned while split — `apps/desktop/src/features/chat/summary-card.tsx` — check: running app
- [x] T4 [FR-3] "Loading models…" while the engine starts or the chat loads — `apps/desktop/src/components/ai-elements/model-selector.tsx`, `apps/desktop/src/features/chat/chat-composer.tsx` — check: running app
- [x] T5 [FR-4] A step ended by Stop reads "· stopped", in neutral colours — `apps/desktop/src/components/ai-elements/tool.tsx` — check: running app

### Phase 2 — Text people read

Done when: runs are told apart, previews are plain, and secondary text is readable.

- [x] T6 [FR-5] Tests and `displayTitle`; `useDisplayTitle` in the sidebar and pane headers — `apps/desktop/src/lib/{chat-title.ts,chat-title.test.ts}`, `apps/desktop/src/hooks/use-display-title.ts`, `apps/desktop/src/features/shell/sidebar-chats.tsx`, `apps/desktop/src/features/split/pane-header.tsx` — check: `bun test`; running app
- [x] T7 [FR-9, FR-10] Tests and `plainText`; plain previews and full titles on hover in the sidebar, pane headers and Scheduler past runs — `apps/desktop/src/lib/{plain-text.ts,plain-text.test.ts}`, `apps/desktop/src/features/scheduler/scheduler-page.tsx` — check: `bun test`; running app
- [x] T8 [FR-6] Darker `--muted-foreground` in light and dark — `apps/desktop/src/index.css` — check: measured contrast

### Phase 3 — Finding things and motion

Done when: shortcuts are listed, suggestions fit the place, and overlays and
the composer behave as specified.

- [x] T9 [FR-7] The shortcut registry and dialog; Ctrl/⌘+/; the palette entry — `apps/desktop/src/features/shell/{shortcuts.ts,shortcuts-dialog.tsx}`, `apps/desktop/src/stores/ui-store.ts`, `apps/desktop/src/app/{app.tsx,use-app-bootstrap.ts}`, `apps/desktop/src/features/sessions/command-palette.tsx` — check: running app
- [x] T10 [FR-8] Tests and `suggestionsFor`; the empty chat reads its pane's folder and repository — `apps/desktop/src/features/chat/{suggestions.ts,suggestions.test.ts,chat-empty.tsx}` — check: `bun test`; running app
- [x] T11 [FR-11] `--ease-spring` on overlay content — `apps/desktop/src/index.css` — check: running app
- [x] T12 [FR-12] Focus back to the composer after Send; the running step on the working line — `apps/desktop/src/features/chat/{chat-composer.tsx,chat-turn.tsx}` — check: running app
- [x] T13 UI map — `apps/desktop/README.md` — check: review

## 2. Verification

In-app checks ran on 2026-11-13 in the person's dev app (real data,
WebView2 CDP). They were read-only: screens were opened and chats
selected, and no prompt was sent.

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `bun test src/features/chat/explain-error.test.ts` (4); running app: a Morning briefing run's 401 reads "Anthropic rejected the API key." with Open Providers and Details | passed (2026-11-13) |
| FR-2 | Running app: two panes open, no summary card on screen | passed (2026-11-13) |
| FR-3 | Running app: a chat being loaded shows "Loading models…", then the model | passed (2026-11-13) |
| FR-4 | Running app: the stopped app step reads "Using an app · screenshot · stopped", neutral; no "App step failed" remains | passed (2026-11-13) |
| FR-5 | `bun test src/lib/chat-title.test.ts` (5); running app: the sidebar shows Morning briefing · Nov 13, Nov 12 … Nov 8 | passed (2026-11-13) |
| FR-6 | Running app: muted text 5.5:1 on the background, 5.13:1 on muted, 4.82:1 on accent (light); dark by calculation: 5.11–6.17:1 | passed (2026-11-13) |
| FR-7 | Running app: Ctrl+/ opens the dialog with General, Composer, Side panel, Split view and Chat menu | passed (2026-11-13) |
| FR-8 | `bun test src/features/chat/suggestions.test.ts` (5); running app: This computer → Plan my day…; evo-harness → "Review the 28 changed files" first | passed (2026-11-13) |
| FR-9 | `bun test src/lib/plain-text.test.ts` (4); running app: the Scheduler past runs have no markdown marks | passed (2026-11-13) |
| FR-10 | Running app: sidebar rows, pane headers and past runs have their full text as a title | passed (2026-11-13) |
| FR-11 | Running app: an open dialog's `--tw-ease` is the spring `linear()` | passed (2026-11-13) |
| FR-12 | `bun run typecheck` | not run in the app — it needs a prompt sent, and the checks were read-only |
| NFR-1 | Review against DESIGN.md: achromatic tokens, one accent, no new weights | passed (2026-11-13) |
| NFR-2 | `apps/desktop/package.json` unchanged | passed (2026-11-13) |
