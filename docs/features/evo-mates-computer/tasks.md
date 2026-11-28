# Evo Mates at the computer (Train 2) — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-29 |

- [x] T1 [FR-3] `mates.computer` rule (+test), definitions filter and call refusal — `src-tauri/src/mates/computer.rs`, `src-tauri/src/agent_tools/mod.rs`
- [x] T2 [FR-3] profile switches — `features/mates/mate-panel.tsx`, `stores/mates-prefs-store.ts`
- [x] T3 [FR-2] step words for browser, Webbridge and app control (+test) — `features/mates/thread-rows.ts`, `thread.test.ts`
- [x] T4 [FR-1] chat scope and floating cards in the Mates window — `mates/mates-app.tsx`
- [x] T5 docs, shots, checks

## Verification

| Requirement | Check | Result |
| --- | --- | --- |
| FR-1 | Code review: the Mates window starts the chat scope and mounts `CardLayer` over a mate's conversation (a channel's seats are Later); the cards' own tests | passed by review (2026-11-29); a live page not run in the real app |
| FR-2 | `thread.test.ts` (browser, Webbridge and app-control words) | passed (2026-11-29) |
| FR-3 | `mates/computer.rs` tests (missing means on; off removes the tools and refuses a call; Chrome apart from Evo's pages); profile shot with Your Chrome off | passed (2026-11-29) |
| Checks | typecheck; `bun test`; `bun run test:shell` 868 pass | passed (2026-11-29) |
| NFR-3 | Profile shots at 1440, 1000, 720 px, light and dark; overflow scan | passed (2026-11-29) |
