# Floating cards for the agent's browser pages and terminals — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-08 |

## 1. Phases

Shell paths are under `apps/desktop/src-tauri/src/`, UI paths under `apps/desktop/src/`.

### Phase 1 — Spike: how a page is drawn

Done when: the decision (stage mode or fallback) and its numbers are in the plan's as-built notes.

- [x] T1 [FR-9] S1: frames from a hidden (or off-screen `visible`) webview with `Page.startScreencast` on the local test page and a video page; record frames per second and latency — scratch script, not committed — check: ≥ 8 fps, or the fallback is chosen
- [x] T2 [FR-7, FR-9] S2: with a 1280×800 emulation, a click, wheel and typed text sent as `Input.dispatch*` from picture coordinates land where `browser_click` would — check: slider drag, text field, scroll on the test page
- [x] T3 [FR-9] S3: CPU with three staged pages at the chosen rate — check: noted in the plan

### Phase 2 — Shell

Done when: FR-9, FR-10 and the events of FR-1, FR-2, FR-15 hold; `bun run test:shell` passes.

- [x] T4 [FR-9, FR-10] Tests for the stage (constants, rate to frame divider, stream start / change / stop, frame events) — `browser/stage.rs` — check: `bun run test:shell browser::stage`
- [x] T5 [FR-9] `browser_stage` and `browser://frame` — `browser/stage.rs`, `browser.rs`, `lib.rs` — check: frames reach the webview on the test page
- [x] T6 [FR-9] A reset or a size on a staged page — no change needed: the stage is a real 1280×800 webview — `browser/stage.rs` — check: a viewport override then a reset gives 1280×800 on a staged page
- [x] T7 [FR-1, FR-15] `browser://driven` once per page on its first tool use or creation — `browser/tabs.rs`, `browser.rs` — check: unit test; event seen in the app
- [x] T8 [FR-2] `agent://activity` around browser and terminal tool calls — `agent_tools/mod.rs` — check: unit test for the resolved tab; event seen in the app

### Phase 3 — Cards store and layer

Done when: FR-1 – FR-6, FR-11 – FR-14 and FR-16 hold with placeholder bodies.

- [x] T9 [FR-3, FR-4, FR-5, FR-13, FR-16] Tests for the cards store (per chat, three visible, chips, keep positions, remove with the tab, reload) — `stores/cards-store.test.ts` — check: `bun test`
- [x] T10 [FR-3, FR-4, FR-5, FR-13, FR-16] The cards store — `stores/cards-store.ts` — check: T9 passes
- [x] T11 [FR-14] `agentCards` setting and its switch — `stores/ui-store.ts`, `features/settings/sections/general-settings.tsx` — check: off: no card by itself, chips remain
- [x] T12 [FR-1, FR-2, FR-15] Apply `browser://driven`, `agent://activity` and agent-owned `terminal://created` — `stores/agent-events.ts` — check: a tool-opened page makes a card, a person-opened page does not
- [x] T13 [FR-2, FR-5, FR-11, FR-12] Card layer, card (bar, drag, resize, minimize, hide), no focus stealing — `features/cards/card-layer.tsx`, `card.tsx` — check: drag in the app; a menu over a card shows the menu
- [x] T14 [FR-4, FR-6] Chip row above the composer and open-in-panel — `features/cards/chip-row.tsx`, `features/chat/chat-view.tsx`, `stores/ui-store.ts` — check: minimize → chip → card; open in panel removes the card

### Phase 4 — Bodies

Done when: FR-7 – FR-10 hold.

- [x] T15 [FR-7, FR-8, FR-9] Browser body: canvas, frames, scale to fit, input forwarding, activity ring, stale-frame state — `features/cards/browser-body.tsx` — check: click, type, scroll a page in a card; the page does not reload moving to the panel
- [x] T16 [FR-7, FR-8] Terminal body: move the xterm element in, fit, view-only, "Open in panel" — `features/cards/terminal-body.tsx` — check: output scrolls; scrollback survives the move
- [x] T17 [FR-6, FR-9] A staged page in the panel shows as the same canvas, letterboxed; a tab shown in the panel has no card — `features/browser/browser-panel.tsx`, `stores/browser-store.ts` — check: expand a card into the panel
- [x] T18 [FR-10] Browser tools with no card and no panel showing — check: the agent-browser batch of checks with the panel closed (screenshot, click, dialog, download)

### Phase 5 — Docs and verification

- [x] T19 [all] Update the desktop UI map and the ADD process-view line — `apps/desktop/README.md`, `docs/architecture/README.md` — check: review
- [x] T20 [all] Check each requirement in the running app, in two chats — check: manual (section 2)

### Phase 6 — The virtual cursor

Done when: FR-17 holds.

- [x] T21 [FR-17] Tests and code for the pointer event of a tool's mouse call — `browser/stage.rs`, `browser.rs`, `agent_tools/browser/driver.rs` — check: `bun run test:shell browser::stage`
- [x] T22 [FR-17] The page-to-picture mapping with tests — `features/cards/input.ts`, `input.test.ts` — check: `bun test`
- [x] T23 [FR-17] The cursor over the picture: arrow, glide, ring, fade — `features/cards/agent-cursor.tsx`, `stage-stream.ts`, `browser-canvas.tsx`, `index.css` — check: an agent click shows it
- [x] T24 [FR-17] Check in the running app — check: manual (section 2)

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Panel closed, the agent opens a page and a shell: a card each | passed (2026-11-08): panel closed, the agent opened a page and a shell: a card each |
| FR-2 | Card bar: kind, title, activity line, buttons | passed (2026-11-08): the bar showed the kind, title and "Clicking ref_1" with a ring while the tool worked; Minimize and Open in side panel (hide is minimize, see the plan) |
| FR-3 | Two chats: A's cards hidden in B, back in place in A | passed (2026-11-08): chat B showed none of chat A's cards; back in A they were where they were |
| FR-4 | Minimize → chip → card | passed (2026-11-08): Minimize made a chip above the composer, a click on it brought the card back |
| FR-5 | Drag, resize, window shrink | passed (2026-11-08): drag, corner resize at 16:10, and a smaller window (710×432 layer) kept the card inside |
| FR-6 | Open in panel: panel on that tab, card gone | passed (2026-11-08): Open in side panel opened the panel on that tab and removed the card |
| FR-7 | Click, type, scroll in a browser card; terminal card view-only | passed (2026-11-08): a click, typing and a wheel turn on the picture reached the page (title set, field filled, scrolled 430 px); the shell card takes no input |
| FR-8 | Move card → chip → panel: page state and scrollback kept | passed (2026-11-08): a page marker and a typed value survived card → chip → card → panel; the shell's output moved with its xterm |
| FR-9 | `browser_status` 1280×800 with the panel closed; small card scaled; `browser_viewport` size wins | passed (2026-11-08): the page saw 1280×800 with the panel closed while the card was 358×223; the picture scaled to fit |
| FR-10 | Agent-browser checks with the panel closed | passed (2026-11-08): with the panel closed and the setting off, navigate, click, a confirm dialog, a screenshot (1265×800) and a download worked |
| FR-11 | A card appears while typing in the composer | passed (2026-11-08): the composer kept its focus when a card appeared (Approve for me); a pending approval replaces the composer, which is not the card |
| FR-12 | A menu over a card | passed (2026-11-08): a dropdown menu opened over a card was the top element where they overlapped |
| FR-13 | A fourth tab becomes a chip | passed (2026-11-08): with a chip already, three more tabs gave three cards and one chip |
| FR-14 | Setting off: no card by itself | passed (2026-11-08): setting off: a tab became a chip, no card by itself |
| FR-15 | A page the person opened has no card | passed (2026-11-08): a page opened by the person got no card |
| FR-16 | Close the tab: card and chip go | passed (2026-11-08): closing the tab removed its chip (and a card the same way) |
| FR-17 | An agent click on the page: arrow and ring at the button, fading after a few seconds | passed (2026-11-08): the arrow and the ring showed over the button with "Clicking ref_1" on the bar; the element sat at the mapped point and was hidden after four seconds |
| NFR-1 | Drag a card over a live page | passed (2026-11-08): about 10 frames a second kept arriving during a drag and the card followed the pointer exactly; the feel itself was not judged by a person |
| NFR-2 | One webview per tab | passed (2026-11-08): the shell's webview count equalled the store's tab count |
| NFR-3 | Windows; others not available here | passed (2026-11-08): checked on Windows; other platforms are not available here (the browser card says a live view needs Windows) |
| NFR-4 | An approval shows in the chat, not in a card | passed (2026-11-08): an approval showed in the chat and in no card |
