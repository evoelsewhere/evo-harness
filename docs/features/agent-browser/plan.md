# Agent browser and app control: closing the tool gaps — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-08 |
| ADD sections | [4. Process view](../../architecture/README.md#4-process-view), [8. Interfaces](../../architecture/README.md#8-interfaces), [9. Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts) |
| ADRs | [0013 App-side agent tools run in the Rust shell](../../architecture/decisions/0013-app-side-agent-tools-run-in-the-rust-shell.md); [0012 Native messaging bridge](../../architecture/decisions/0012-native-messaging-bridge-to-the-persons-browser.md) (the bridge back end only, later) |

## 1. Context and constraints

- **Today.** The 13 `browser_*` tools and the `terminal_*` tools are TypeScript in
  the webview (`features/browser/`, `features/terminal/agent-tools.ts`),
  registered by `engine-store.ts` as external tools. The shell only offers Tauri
  commands under them (`browser_*`, `terminal_*`, `browser_cdp` on Windows).
- **Decision (ADR 0013).** The tools move into the Rust shell. The shell answers
  an engine's `external_tool_request` itself, as the mesh hub already does for
  the session tools (`src-tauri/src/mesh.rs`, the reader loop in `engine.rs`).
- **Gaps** (see the spec): no hover / drag / right click, no wait, screenshots
  are the viewport only, a page dialog blocks the call, no upload, frames, batch,
  emulation, storage, network headers and bodies, mocking, downloads,
  performance or PDF; the app tools cannot open a panel, read usage or search
  old chats.
- **Windows own pages and shells today.** A child webview and a pty belong to the
  window that created them (`BrowserState.tabs: label → window`,
  `Session.window`) and stream to it. A tool that creates one must pick a
  window (the one showing the chat, else `main`), and the webview must learn of
  it (events below).
- **Rules.** The webview owns presentation only; only `engine-store.ts` talks to
  the engine; approval modes (ADR 0010; app-side tools gated by the app); pages
  and shells are per chat ([session panels](../session-panels/spec.md)).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | The loop stays in the engine; the tools are native features answered by the shell (ADR 0013). |
| Layers point down; library crates stay free of engine/desktop concerns | ok | No crate change in this feature (the bridge adds one, in its own plan). |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | Tool definitions travel in `configure_external_tools`; no new engine command. The shell and webview events below are additive. |
| Models come from models.dev; no hardcoded model facts | n/a | |
| Real engine in the app; settings and credentials reload live | ok | The approval mode is read from the app database on every call. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Classes gate through the approval mode; upload, download and PDF paths follow the sandbox path rules. |
| Desktop UI structure and `DESIGN.md` | ok | The webview shows what the shell reports and asks what it raises; no tool logic stays there. |

## 3. Design

### 3.1 Overview

```text
engine ── external_tool_request ──▶ engine.rs reader ──▶ agent_tools::on_line
                                                              │ registry lookup, ToolCtx { app, chat, cancel }
                                                              │ gate (class, origin, approval mode) ── ask ──▶ agent://ask ─▶ webview prompt
                                                              ▼                                    ◀── agent_ask_answer ──┘
                                     browser::*   terminal::*   app::*     (page driver · output log · database)
                                                              │
engine ◀── external_tool_response ◀── EngineState::write ◀────┘     events to the webview: browser://created, terminal://created, agent://panel
```

### 3.2 Components

All paths under `apps/desktop/src-tauri/src/` unless noted.

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| shell | `agent_tools/mod.rs` | `Tool`, `Class`, `ToolCtx`, `ToolOutput`; the registry; `on_line` (dispatch of `external_tool_request`, cancel on `external_tool_request_closed`, answer through `EngineState::write`); `agent_tools_definitions`; ask plumbing (`agent://ask`, `agent_ask_answer`); UI events. | FR-22, FR-24 |
| shell | `agent_tools/gate.rs` | Approval by class, local origins, once per site and chat, "Allow for this chat". | FR-22 |
| shell | `agent_tools/browser/` (`driver.rs`, `core.rs`, `pointer.rs`, `wait.rs`, `screenshot.rs`, `dialog.rs`, `upload.rs`, `read.rs`, `batch.rs`, `tabs.rs`, `viewport.rs`, `storage.rs`, `network.rs`, `mock.rs`, `downloads.rs`, `performance.rs`) | The page driver (eval, DevTools, helper script, page info, navigation waits, script fallbacks) and one module per tool group. | FR-1 – FR-17, FR-25 |
| shell | `browser.rs`, `browser_page.js` | Tab ownership (`chat`, visible tab per chat, last bounds), DevTools event ring, dialogs, downloads, `browser_status`; the page helper script. | FR-4, FR-6, FR-8, FR-13, FR-14, FR-16 |
| shell | `terminal.rs`, `terminal/log.rs` (new) | Per-session metadata (`chat`, `owner`, title, integration) and the plain-text output log with OSC 133 markers, fed by the pty reader; waits. | agent-terminal FR-4 – FR-16 |
| shell | `agent_tools/terminal.rs` | `terminal_list`, `terminal_run`, `terminal_send`, `terminal_read`, `terminal_kill` on that state. | agent-terminal |
| shell | `agent_tools/app/` (`panel.rs`, `usage.rs`, `sessions.rs`, `preview.rs`) | `app_panel` (an event the webview applies), `usage_get` and `session_search` / `session_read` (app database and transcripts), `preview_start` (launch config, terminal and browser tools in process). | FR-18 – FR-21 |
| desktop UI | `apps/desktop/src/stores/engine-store.ts` | Merges `agent_tools_definitions` into `configure_external_tools`; ignores requests for tools the shell answers; shows `agent://ask` as the chat's approval prompt and answers it. | FR-24 |
| desktop UI | `apps/desktop/src/stores/browser-store.ts`, `terminal-store.ts`, `ui-store.ts` | Adopt pages and shells the shell creates (`browser://created`, `terminal://created`), report the visible tab and bounds to the shell, apply `agent://panel`. | FR-18, FR-24 |
| desktop UI | `apps/desktop/src/features/browser/`, `features/terminal/agent-tools.ts`, `features/app-tools/` | Deleted once the Rust tools replace them (only UI stays). | ADR 0013 |
| desktop UI | `apps/desktop/src/components/ai-elements/tool.tsx` | Transcript rows for the new tools. | FR-24 |

### 3.3 Interfaces

New tools: `browser_hover`, `browser_drag`, `browser_wait`, `browser_dialog`,
`browser_upload`, `browser_batch`, `browser_status`, `browser_frames`,
`browser_viewport`, `browser_storage` (change), `browser_storage_read` (observe),
`browser_mock`, `browser_downloads`, `browser_download_folder`,
`browser_performance`, `browser_pdf`, `app_panel`, `usage_get`, `session_search`,
`session_read`, `preview_start`. Changed: `browser_click` (`button`,
`clickCount`), `browser_screenshot` (`fullPage`, `ref`, `region`, `scale`),
`browser_read_page` (`format`, `frame`), `browser_find` (`frame`),
`browser_network` (`filter`, `requestId`), `browser_console` (`level`, `since`,
`clear`), `browser_tabs` (`create`).

Shell ↔ webview (additive, camelCase):

- Commands: `agent_tools_definitions() -> [{name, description, inputSchema}]`;
  `agent_ask_answer(requestId, index)`; `agent_window_chat(window, chat)` (which
  chat a window shows); `browser_select_tab(chat, tab)` (the visible tab of a
  chat); `browser_status(tab)`; DevTools events, dialogs and downloads as in the
  shell tasks.
- Events: `agent://ask {chat, requestId, title, description, options}`,
  `agent://ask-closed {requestId}`, `agent://panel {chat, panel, action}`,
  `browser://created {tab, url, chat, window}`, `terminal://created {id, chat,
  owner, shell, shellId, cwd, integration}`, `browser://cdp`, `browser://dialog`,
  `browser://download`.

### 3.4 Data and storage

- Per-tab DevTools event ring (1,000 events), dialog slot, download list, request
  bodies on demand: in memory.
- Per-shell output log (1 MiB ring, running offsets, markers): in memory.
- "Allowed once per site and chat" grants and the chat → window map: in memory.
- `.evo/launch.json` in the project for `preview_start`: read, never written by
  the agent.

### 3.5 Behaviour

- A call: look up the tool → resolve the chat's tab (tools that need none skip
  it) → gate → run with the chat's `ToolCtx` → answer. A stopped turn
  (`external_tool_request_closed`) cancels the call and closes its questions.
- A tool that opens a page or a shell for a chat creates it in the window showing
  that chat (else `main`), emits `browser://created` / `terminal://created` and
  the webview adopts it into that chat's tabs.
- `app_panel` and "show the browser" emit `agent://panel`; the webview opens the
  tool in that chat's tabs (as `openPanel(panel, chat)` does) and the shell waits
  for the panel's bounds before it acts on pixels (350 ms, as before).
- Dialog, wait, screenshot and the other tool behaviours are as specified; the
  page helper script (`window.__evo`) stays the same JavaScript.

### 3.6 Errors and edge cases

| Case | Result |
| --- | --- |
| No DevTools on this platform | `browser_status` lists it; the tool uses its script path or returns "not available on this platform: use …" |
| Tab gone mid-call | "The tab closed." |
| Window hidden or closed to the tray | Tools keep working: pages and shells live in the shell; the window is only needed to show them |
| No webview answers an ask (no window) | The call waits until the turn is stopped (as an approval does today) |
| Cross-origin frame without DevTools | "cannot reach this frame without DevTools access" |
| Upload path outside the sandbox | Refused with the allowed folders |
| Mock left on after the turn | Cleared when the chat's turn ends or the tab leaves the origin |

### 3.7 Security and trust

- Observe / act / change classes gate through the approval mode; acts on
  non-local origins ask once per site and chat in "Ask every time"; changes ask in
  "Ask every time" and "Approve edits".
- Storage values, response bodies and console text are untrusted data, marked in
  results; cookie values are masked unless asked for.
- Upload, download and PDF paths follow the sandbox's allowed folders.
- No tool changes settings, credentials or the approval mode.

### 3.8 Configuration

| Key | Type | Default | Where | Applies |
| --- | --- | --- | --- | --- |
| `.evo/launch.json` | file | none | project | `preview_start` |
| Download folder | path | the chat's working folder `downloads/` | set by `browser_download_folder` | next download |

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Keep the tools in the webview | Window-dependent and untestable without it (ADR 0013). |
| Only DevTools, drop the script path | macOS and Linux webviews have no DevTools access; the script path keeps most of wave 1 there. |
| A native input layer for macOS and Linux now | Large and platform-specific; later work behind the same driver. |
| One `browser_do` tool with an action argument | Harder for the model to choose from and to gate by class. |

## 5. Phases (to become tasks)

1. **Foundation (shell):** `agent_tools` core (registry, dispatch, ask, gate), the webview glue; tab ownership and the DevTools events, dialogs and downloads in `browser.rs`.
2. **Port:** the 13 browser tools and the terminal tools to Rust with the output log, same behaviour; the webview tool code deleted.
3. **Wave 1:** hover, drag, click variants, wait, screenshot options, dialogs, upload, frames, batch, text and Markdown reading, tab create.
4. **Wave 2:** viewport and emulation, storage, network headers and bodies, mocking, console options, downloads, performance, PDF.
5. **Wave 3:** `app_panel`, `usage_get`, `session_search` / `session_read`, `preview_start`.
6. **Bridge back end:** the transport from the [bridge plan](../browser-bridge/plan.md), later.

## 6. As-built notes

Recorded 2026-11-08.

- **Page dialogs (FR-4).** The app's dialog plugin replaces `window.alert` and
  `window.confirm` in every webview and frame with promise-returning versions, and
  its script runs before ours, so the real functions cannot be captured in the
  page's own world (the hidden-iframe idea in the first draft does not work: the
  helper frame is overridden too). The shell instead registers an isolated world
  (`evo-dialogs`, `browser/hooks.rs`) holding the real functions; the page script's
  replacements call into it through a synchronous DOM event, so the page blocks in a
  real dialog that `ScriptDialogOpening` holds for `browser_dialog`. New-document
  scripts need the Page domain enabled in WebView2, and a tab's first document is
  created before the script is registered, so the world is also created in the
  current document after every navigation. `prompt` was never overridden and needs
  none of this. Without DevTools (other platforms) the page script's fallback log
  answers dialogs instead.
- **Mobile viewport (FR-11).** A page reads its viewport meta tag only when it loads,
  so `browser_viewport` takes `reload` and says so in its result when a mobile
  preset is applied without it.
- **Downloads (FR-16).** The "download from an untrusted origin asks" rule is not
  implemented: with no folder set every download is cancelled and listed as
  blocked, and setting the folder (a command-class change that asks) is the
  person's approval for pages to save there. The folder is one app-wide setting.
- **Classes.** `Observe` / `Act` / `Change` became `Read` / `Edit` / `Command`
  ([agent-tools-layers](../agent-tools-layers/spec.md), FR-7); behaviour is the same.
- **Not done:** `preview_start` against a real `.evo/launch.json` was not run in the
  app; the window-hidden case, element and region screenshots, `browser_mock`
  blocking, the script path (non-Windows) and NFR-1 – NFR-3 are unverified (see the
  verification table).
- **Environment.** In the test environment `https://example.com` failed with
  `ERR_QUIC_PROTOCOL_ERROR` inside WebView2 (not the tools); local pages were used.
