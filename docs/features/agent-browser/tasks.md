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

- [x] T1 [FR-22] Gate tests (classes, modes, local origins, once per site and chat, "Allow for this chat") — `agent_tools/gate.rs` — check: `cargo test agent_tools::gate`
- [x] T2 [FR-22, FR-24] `agent_tools` core: `Tool`, `Class`, `ToolCtx`, `ToolOutput`, registry, dispatch of `external_tool_request` / cancel on `external_tool_request_closed`, `agent_tools_definitions`, ask plumbing — `agent_tools/mod.rs`, `engine.rs`, `lib.rs` — check: `cargo test agent_tools`
- [x] T3 [FR-24] Webview glue: merge the shell's definitions into `configure_external_tools`, ignore requests the shell answers, show `agent://ask` as the chat's approval prompt and answer it — `apps/desktop/src/stores/engine-store.ts` — check: `bun run typecheck`; a prompt from a shell tool shows in the chat
- [x] T4 [FR-4, FR-13, FR-14, FR-16] `browser.rs`: DevTools event ring (`browser_cdp_subscribe`, `browser_cdp_events`), dialogs with deferral (`browser_dialog_pending`, `browser_dialog_answer`), downloads (`browser_download_config`, `browser_downloads`), `browser_status` — check: `cargo test browser`
- [x] T5 [FR-6, FR-8, NFR-1] `browser_page.js`: queued dialog fallback, hover / drag / wheel events, frame-qualified refs, `readPage` formats — check: `node --check`; manual on the test page
- [x] T6 [FR-24] Tab ownership in the shell: `chat` per page, visible tab per chat, last bounds, window per chat, `browser://created` — `browser.rs`; webview `stores/browser-store.ts` adopts — check: `cargo test browser`; typecheck

### Phase 2 — Port the existing tools

Done when: the 13 browser tools and the terminal tools behave as before and the webview tool code is gone.

- [x] T7 [P] [FR-25] Page driver (eval, DevTools, helper script, page info, navigation waits, keys, click-at) — `agent_tools/browser/driver.rs` — check: `cargo test agent_tools::browser::driver`
- [x] T8 [FR-24, FR-25] The 13 browser tools, with classes — `agent_tools/browser/core.rs` — check: the tools work on the test page
- [x] T9 [P] Output log (ring, offsets, markers, command span, waits, echo skip, row moves) ported from `output-log.ts` with its tests — `terminal/log.rs` — check: `cargo test terminal::log`
- [x] T10 [P] Session metadata (`chat`, `owner`, title, integration) fed by the pty reader, `terminal://created` — `terminal.rs`; webview `stores/terminal-store.ts` adopts — check: `cargo test terminal`; typecheck
- [x] T11 [P] The five terminal tools on that state — `agent_tools/terminal.rs` — check: agent-terminal verification rows in the app
- [x] T12 Delete the webview tool code (`features/browser/agent-tools.ts`, `tools/`, `driver/`, `gate*`, `features/terminal/agent-tools.ts`, `features/app-tools/`) — check: `bun run typecheck`, `bun test`

### Phase 3 — Wave 1: input and observation

Done when: FR-1 – FR-10 pass.

- [x] T13 [P] [FR-1] `browser_hover`, `browser_drag`, `browser_click` button and click count — `agent_tools/browser/pointer.rs` — check: hover menu opens, range slider drags
- [x] T14 [P] [FR-2, NFR-3] `browser_wait` — `agent_tools/browser/wait.rs` — check: text after 2 s; stop ends it
- [x] T15 [P] [FR-3] Screenshot options — `agent_tools/browser/screenshot.rs` — check: full-page taller than the viewport
- [x] T16 [P] [FR-4] `browser_dialog` and calls that return while a dialog is open — `agent_tools/browser/dialog.rs` — check: `confirm()` click returns and is answerable
- [x] T17 [P] [FR-5] `browser_upload` — `agent_tools/browser/upload.rs` — check: two files into a `multiple` input; outside path refused
- [x] T18 [P] [FR-6, FR-8] Frames, `browser_frames`, `format` — `agent_tools/browser/read.rs` — check: iframe button clicked; table as Markdown
- [x] T19 [FR-7] `browser_batch` — `agent_tools/browser/batch.rs` — check: four steps, four results, stop at first failure
- [x] T20 [P] [FR-9, FR-10] `browser_tabs create`, `browser_status` — `agent_tools/browser/tabs.rs` — check: create opens a tab; status lists fallbacks

### Phase 4 — Wave 2: environment and diagnostics

Done when: FR-11 – FR-17 pass.

- [x] T21 [P] [FR-11] `browser_viewport` — `agent_tools/browser/viewport.rs` — check: mobile preset matches `(max-width: 480px)`
- [x] T22 [P] [FR-12] `browser_storage`, `browser_storage_read` — `agent_tools/browser/storage.rs` — check: read lists names; clear asks
- [x] T23 [P] [FR-13, FR-15] `browser_network`, `browser_console` — `agent_tools/browser/network.rs` — check: a 500's body is readable
- [x] T24 [P] [FR-14] `browser_mock` — `agent_tools/browser/mock.rs` — check: blocked URL does not load; clear restores
- [x] T25 [P] [FR-16] `browser_downloads`, `browser_download_folder` — `agent_tools/browser/downloads.rs` — check: a download saves into the folder
- [x] T26 [P] [FR-17] `browser_performance`, `browser_pdf` — `agent_tools/browser/performance.rs` — check: report has first contentful paint; a PDF is saved

### Phase 5 — Wave 3: Evo itself

Done when: FR-18 – FR-21 pass.

- [x] T27 [P] [FR-18] `app_panel` (+ the webview applies `agent://panel`) — `agent_tools/app/panel.rs`; `stores/ui-store.ts` — check: Files opens for the agent's chat
- [x] T28 [P] [FR-19] `usage_get` — `agent_tools/app/usage.rs` — check: "this week" matches the Usage page
- [x] T29 [P] [FR-20] `session_search`, `session_read` — `agent_tools/app/sessions.rs` — check: a phrase from an old chat is found
- [x] T30 [FR-21] `preview_start` with `.evo/launch.json` — `agent_tools/app/preview.rs` — check: a configured server starts and shows

### Phase 6 — Wrap-up

- [x] T31 [FR-24] Transcript rows for every new tool — `apps/desktop/src/components/ai-elements/tool.tsx` — check: manual in a finished turn
- [x] T32 [FR-22 – FR-24] Update the desktop README map and the ADD tool paragraph — check: review
- [x] T33 [all] Check each requirement in the running app on a test page, with the window hidden for at least one tool — check: manual (section 2)

The bridge back end is the [browser bridge](../browser-bridge/spec.md) feature and not in these tasks.

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Test page: hover menu, range slider drag, right click | passed (2026-11-08): hover opened the menu item, right-click fired `contextmenu`, a drag across the range slider set it to 88 |
| FR-2 | Test page: text appears after 2 s | passed (2026-11-08): `browser_wait` matched text that appeared after 2 s; timeout reported as an error |
| FR-3 | Test page: full-page, element and region shots | passed (2026-11-08): default, full-page (503×2886 page scaled to 1568 px), element (463×37) and region (200×100) shots |
| FR-4 | Test page: `confirm()` on click | passed (2026-11-08): a `confirm()` click returned with the dialog open; accepted gave `confirm:true`, dismissed `confirm:false`; `alert()` and `prompt()` held the same way |
| FR-5 | Test page: file input, `multiple`, outside path | passed (2026-11-08): two files into a `multiple` input after an approval; a path outside the allowed folders was refused with the folders named |
| FR-6 | Test page: iframe button | passed (2026-11-08): `browser_frames` listed `f1`; `browser_find` with `frame f1` and a click on `f1:ref_4` changed the frame's text; `browser_read_page frame f1` read it |
| FR-7 | One `browser_batch` of four steps | passed (2026-11-08): one `browser_batch` of four steps returned four results |
| FR-8 | Test page: table as Markdown | passed (2026-11-08): `format markdown` gave the table as Markdown |
| FR-9 | `browser_tabs create` | passed (2026-11-08): `browser_tabs create` opened `browser-2`, the list marked it active |
| FR-10 | `browser_status` | passed (2026-11-08): `browser_status` listed the DevTools-backed tools |
| FR-11 | Mobile preset and dark scheme | passed (2026-11-08): the mobile preset with `reload true` made the page see 390×844 at 3x and match `(max-width: 480px)` (a page reads its viewport meta only at load, so the tool says so without `reload`); reset restored 518×704 |
| FR-12 | Cookies and storage read, set, clear | passed (2026-11-08): `browser_storage_read` listed the cookie and the localStorage key masked; `clear` of cookies asked and removed the cookie |
| FR-13 | A failing request's headers and body | passed (2026-11-08): a 500 response's headers (`x-test`) and body were readable with its `requestId` |
| FR-14 | Block and answer a request | passed (2026-11-08): a blocked `/api/fail` failed with `ERR_BLOCKED_BY_CLIENT`; an answered one returned 200 (115 B); after `clear` the request returned the real 500 |
| FR-15 | Console level, since, clear | passed (2026-11-08): `level error` showed two `console.error` lines, `clear` set the cursor, the next read said there were no new errors |
| FR-16 | A download | passed (2026-11-08): with no folder the download was blocked; with a folder it saved `evo-test (1).txt` (18 B); a page off this machine or an unapproved site stays blocked (`browser::tests::a_site_downloads_only_after_the_person_approved_it`; no non-local site could be reached from the test machine) |
| FR-17 | Performance report and PDF | passed (2026-11-08): the report had TTFB, FCP, LCP, CLS; `browser_pdf` saved a 55 KB PDF of the test page |
| FR-18 | `app_panel` for a chat showing and not showing | partly (2026-11-08): `app_panel files` opened Files for the chat that was showing; that an event for a chat that is not showing changes nothing is covered by `chat-panels.test.ts`, not by a second live chat |
| FR-19 | `usage_get` against the Usage page | partly (2026-11-08): `usage_get` returned the per-model table for 7 days and calls the same `evo_store::usage::summary` as the Usage page; the numbers were not compared side by side |
| FR-20 | `session_search` / `session_read` | passed (2026-11-08): `session_search` found chats by a word with snippets; `session_read` returned a chat's turns |
| FR-21 | `preview_start` | passed (2026-11-08): `preview_start dev` from `.evo/launch.json` asked once (shell, folder, command), matched the server's ready line after 0.4 s and opened `http://localhost:8799/`, which read back its page |
| FR-22 | Each approval mode × class (unit tests + app) | passed (2026-11-08): `cargo test agent_tools::gate`; in the app Ask every time asked for viewport, mock, upload, PDF and download folder, Approve edits asked only for the cookie change, Approve for me asked nothing |
| FR-23 | Review of result wording | passed (2026-11-08): reviewed in the runs: page reads, network, console, storage, downloads and past chats end with an untrusted-data note; results open with what happened and the page |
| FR-24 | Tool answered with the window hidden; transcript rows; another chat's tab id | passed (2026-11-08): all tools above ran from the shell's registry in real chats; with the main window hidden by the OS a run of terminal, browser, storage, `usage_get`, `notify` and `session_read` still completed; the transcript shows their rows ("Opened …", "Browser: waited for … failed") |
| FR-25 | Windows (DevTools) and the script path | partly (2026-11-08): the Windows DevTools path was exercised throughout; the script path needs a non-Windows machine |
| NFR-1 | A tool needing DevTools on the script path | not run: needs a platform without DevTools (not available here) |
| NFR-2 | A long page read cut at 20,000 | passed (2026-11-08): a 72,204-character page read came back cut at 30,000 with "truncated at 30000 characters; raise maxChars…" (the page read's own default, see spec History) |
| NFR-3 | Stop a turn during `browser_wait` | passed (2026-11-08): stopping the turn during a 55 s `browser_wait` ended it 0.3 s later with "operation aborted" |
