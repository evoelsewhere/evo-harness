# Agent browser and app control: closing the tool gaps — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-08 |

## 1. Phases

All paths under `apps/desktop/src-tauri/src/` unless noted. Tests are `cargo test`
in `apps/desktop/src-tauri`. Tasks marked `[P]` have disjoint files.

### Phase 1 — Foundation in the shell

Done when: a tool registered in `agent_tools` is answered by the shell for an
engine's `external_tool_request`, behind the gate, with the window hidden, and the
webview shows its questions.

- [ ] T1 [FR-22] Gate tests (classes, modes, local origins, once per site and chat, "Allow for this chat") — `agent_tools/gate.rs` — check: `cargo test agent_tools::gate`
- [ ] T2 [FR-22, FR-24] `agent_tools` core: `Tool`, `Class`, `ToolCtx`, `ToolOutput`, registry, dispatch of `external_tool_request` / cancel on `external_tool_request_closed`, `agent_tools_definitions`, ask plumbing — `agent_tools/mod.rs`, `engine.rs`, `lib.rs` — check: `cargo test agent_tools`
- [ ] T3 [FR-24] Webview glue: merge the shell's definitions into `configure_external_tools`, ignore requests the shell answers, show `agent://ask` as the chat's approval prompt and answer it — `apps/desktop/src/stores/engine-store.ts` — check: `bun run typecheck`; a prompt from a shell tool shows in the chat
- [ ] T4 [FR-4, FR-13, FR-14, FR-16] `browser.rs`: DevTools event ring (`browser_cdp_subscribe`, `browser_cdp_events`), dialogs with deferral (`browser_dialog_pending`, `browser_dialog_answer`), downloads (`browser_download_config`, `browser_downloads`), `browser_status` — check: `cargo test browser`
- [ ] T5 [FR-6, FR-8, NFR-1] `browser_page.js`: queued dialog fallback, hover / drag / wheel events, frame-qualified refs, `readPage` formats — check: `node --check`; manual on the test page
- [ ] T6 [FR-24] Tab ownership in the shell: `chat` per page, visible tab per chat, last bounds, window per chat, `browser://created` — `browser.rs`; webview `stores/browser-store.ts` adopts — check: `cargo test browser`; typecheck

### Phase 2 — Port the existing tools

Done when: the 13 browser tools and the terminal tools behave as before and the webview tool code is gone.

- [ ] T7 [P] [FR-25] Page driver (eval, DevTools, helper script, page info, navigation waits, keys, click-at) — `agent_tools/browser/driver.rs` — check: `cargo test agent_tools::browser::driver`
- [ ] T8 [FR-24, FR-25] The 13 browser tools, with classes — `agent_tools/browser/core.rs` — check: the tools work on the test page
- [ ] T9 [P] Output log (ring, offsets, markers, command span, waits, echo skip, row moves) ported from `output-log.ts` with its tests — `terminal/log.rs` — check: `cargo test terminal::log`
- [ ] T10 [P] Session metadata (`chat`, `owner`, title, integration) fed by the pty reader, `terminal://created` — `terminal.rs`; webview `stores/terminal-store.ts` adopts — check: `cargo test terminal`; typecheck
- [ ] T11 [P] The five terminal tools on that state — `agent_tools/terminal.rs` — check: agent-terminal verification rows in the app
- [ ] T12 Delete the webview tool code (`features/browser/agent-tools.ts`, `tools/`, `driver/`, `gate*`, `features/terminal/agent-tools.ts`, `features/app-tools/`) — check: `bun run typecheck`, `bun test`

### Phase 3 — Wave 1: input and observation

Done when: FR-1 – FR-10 pass.

- [ ] T13 [P] [FR-1] `browser_hover`, `browser_drag`, `browser_click` button and click count — `agent_tools/browser/pointer.rs` — check: hover menu opens, range slider drags
- [ ] T14 [P] [FR-2, NFR-3] `browser_wait` — `agent_tools/browser/wait.rs` — check: text after 2 s; stop ends it
- [ ] T15 [P] [FR-3] Screenshot options — `agent_tools/browser/screenshot.rs` — check: full-page taller than the viewport
- [ ] T16 [P] [FR-4] `browser_dialog` and calls that return while a dialog is open — `agent_tools/browser/dialog.rs` — check: `confirm()` click returns and is answerable
- [ ] T17 [P] [FR-5] `browser_upload` — `agent_tools/browser/upload.rs` — check: two files into a `multiple` input; outside path refused
- [ ] T18 [P] [FR-6, FR-8] Frames, `browser_frames`, `format` — `agent_tools/browser/read.rs` — check: iframe button clicked; table as Markdown
- [ ] T19 [FR-7] `browser_batch` — `agent_tools/browser/batch.rs` — check: four steps, four results, stop at first failure
- [ ] T20 [P] [FR-9, FR-10] `browser_tabs create`, `browser_status` — `agent_tools/browser/tabs.rs` — check: create opens a tab; status lists fallbacks

### Phase 4 — Wave 2: environment and diagnostics

Done when: FR-11 – FR-17 pass.

- [ ] T21 [P] [FR-11] `browser_viewport` — `agent_tools/browser/viewport.rs` — check: mobile preset matches `(max-width: 480px)`
- [ ] T22 [P] [FR-12] `browser_storage`, `browser_storage_read` — `agent_tools/browser/storage.rs` — check: read lists names; clear asks
- [ ] T23 [P] [FR-13, FR-15] `browser_network`, `browser_console` — `agent_tools/browser/network.rs` — check: a 500's body is readable
- [ ] T24 [P] [FR-14] `browser_mock` — `agent_tools/browser/mock.rs` — check: blocked URL does not load; clear restores
- [ ] T25 [P] [FR-16] `browser_downloads`, `browser_download_folder` — `agent_tools/browser/downloads.rs` — check: a download saves into the folder
- [ ] T26 [P] [FR-17] `browser_performance`, `browser_pdf` — `agent_tools/browser/performance.rs` — check: report has first contentful paint; a PDF is saved

### Phase 5 — Wave 3: Evo itself

Done when: FR-18 – FR-21 pass.

- [ ] T27 [P] [FR-18] `app_panel` (+ the webview applies `agent://panel`) — `agent_tools/app/panel.rs`; `stores/ui-store.ts` — check: Files opens for the agent's chat
- [ ] T28 [P] [FR-19] `usage_get` — `agent_tools/app/usage.rs` — check: "this week" matches the Usage page
- [ ] T29 [P] [FR-20] `session_search`, `session_read` — `agent_tools/app/sessions.rs` — check: a phrase from an old chat is found
- [ ] T30 [FR-21] `preview_start` with `.evo/launch.json` — `agent_tools/app/preview.rs` — check: a configured server starts and shows

### Phase 6 — Wrap-up

- [ ] T31 [FR-24] Transcript rows for every new tool — `apps/desktop/src/components/ai-elements/tool.tsx` — check: manual in a finished turn
- [ ] T32 [FR-22 – FR-24] Update the desktop README map and the ADD tool paragraph — check: review
- [ ] T33 [all] Check each requirement in the running app on a test page, with the window hidden for at least one tool — check: manual (section 2)

The bridge back end is the [browser bridge](../browser-bridge/spec.md) feature and not in these tasks.

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Test page: hover menu, range slider drag, right click | not run |
| FR-2 | Test page: text appears after 2 s | not run |
| FR-3 | Test page: full-page, element and region shots | not run |
| FR-4 | Test page: `confirm()` on click | not run |
| FR-5 | Test page: file input, `multiple`, outside path | not run |
| FR-6 | Test page: iframe button | not run |
| FR-7 | One `browser_batch` of four steps | not run |
| FR-8 | Test page: table as Markdown | not run |
| FR-9 | `browser_tabs create` | not run |
| FR-10 | `browser_status` | not run |
| FR-11 | Mobile preset and dark scheme | not run |
| FR-12 | Cookies and storage read, set, clear | not run |
| FR-13 | A failing request's headers and body | not run |
| FR-14 | Block and answer a request | not run |
| FR-15 | Console level, since, clear | not run |
| FR-16 | A download | not run |
| FR-17 | Performance report and PDF | not run |
| FR-18 | `app_panel` for a chat showing and not showing | not run |
| FR-19 | `usage_get` against the Usage page | not run |
| FR-20 | `session_search` / `session_read` | not run |
| FR-21 | `preview_start` | not run |
| FR-22 | Each approval mode × class (unit tests + app) | not run |
| FR-23 | Review of result wording | not run |
| FR-24 | Tool answered with the window hidden; transcript rows; another chat's tab id | not run |
| FR-25 | Windows (DevTools) and the script path | not run |
| NFR-1 | A tool needing DevTools on the script path | not run |
| NFR-2 | A long page read cut at 20,000 | not run |
| NFR-3 | Stop a turn during `browser_wait` | not run |
