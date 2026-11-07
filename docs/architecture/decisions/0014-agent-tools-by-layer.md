# 0014. Where each agent tool lives: engine or shell, never the webview

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-08 |
| Feature | [Agent tools by layer](../../features/agent-tools-layers/spec.md) |
| ADD sections | [4. Process view](../README.md#4-process-view), [5. Building blocks](../README.md#5-building-blocks), [8. Interfaces](../README.md#8-interfaces) |
| Related | [0013 App-side agent tools run in the Rust shell](0013-app-side-agent-tools-run-in-the-rust-shell.md), [0010 The engine enforces the approval mode](0010-engine-enforces-approval-mode.md), [0003](0003-engine-runs-as-a-child-process-per-chat.md) |

## Context

Tools reach the agent from four places and each decides alone where its code
lives, how it is registered, and where it is gated and rendered. A tool's name now
appears in up to seven lists (the engine's registration in `session.rs`, the
built-in name list, the approval classification, the read-only agent list, two
reserved-name checks, the desktop's transcript map). ADR 0013 settled the
browser, terminal and app-control tools; the rest were never decided together.

Inventory (2026-11-08; `engine` = `crates/evo-engine` or `crates/evo-tools`,
`shell` = `apps/desktop/src-tauri`, `webview` = `apps/desktop/src`):

| Tool(s) | Lives in today | Needs |
| --- | --- | --- |
| `read`, `write`, `edit`, `bash`, `powershell`, `grep`, `find`, `ls`, `image` | engine (`evo-tools`) | the workspace, processes, the sandbox |
| `docs`, `document_preview`, `lsp`, `web_search`, `web_fetch`, `memory`, `skill` | engine | files, network, language servers, the model's resources |
| `agent`, `message_agent`, `send_message`, `ask`, `reply`, `list_nodes`, `cancel_node` | engine | the agent loop and the mesh wire |
| `ask_user`, `update_todos` | engine | the protocol's dialog channel (the webview only renders) |
| `visualize_read_me`, `show_widget` | engine (output only) | nothing native; the webview renders |
| plugin tools | engine (`evo-extensions`) | the plugin ABI |
| `list_sessions`, `message_session`, `start_session` | shell (mesh hub) | the app database, chats, worktrees |
| `browser_*`, `terminal_*`, `app_panel`, `usage_get`, `session_search`, `session_read`, `preview_start` | shell (ADR 0013, in progress; today webview for the first two groups) | webviews, ptys, the database, windows |
| `create_scheduled_task`, `list_/update_/delete_scheduled_task` | **webview** (`features/scheduler`, timers in `scheduler-store.ts`) | the database, timers, starting chats |
| `notify`, `send_file` | **webview** (`features/chat/user-tools.ts`) | notifications, files, the chat transcript |
| `show_file` | **webview** (`features/browser`) | the Files panel |
| MCP connector tools (`mcp__*`, search / call) | **webview** routes to the shell's pool (`stores/engine-store.ts` → `mcp_call`) | the shell's MCP pool, permissions |

## Decision

One rule decides the layer:

- **Engine** — a tool whose work is on the workspace, a process, the network or
  the agent's own session, and that can run with no app window: the file, shell,
  search, language, web, memory, skill, sub-agent and mesh-messaging tools, and
  `ask_user` / `update_todos` (they talk through the protocol's dialog channel).
  The engine registers, classifies and gates them (ADR 0010).
- **Shell** — a tool that needs a native app resource: a webview, a pty, the app
  database, the scheduler, notifications, the MCP pool, windows and panels. The
  shell registers them in `agent_tools`, answers the engine's
  `external_tool_request` itself and gates them (ADR 0013).
- **Webview** — no tool logic. It shows state and asks the person what a tool
  raised.

So these move to the shell, under `agent_tools/` (a later spec, in this order):
(1) `notify`, `send_file`, `show_file`; (2) the MCP connector tools (the call is
already `mcp_call`; only the routing is in the webview); (3) the scheduler tools (create, list, update, delete a task: database work).
The scheduler's timers and the runner that starts a task's chat stay in the
webview for now: starting a chat's engine is still the window's job, and moving
that is its own decision (a later ADR). Nothing moves out of the engine.

Two things follow, specified in the feature: one metadata table per layer (name,
class, read-only-safe, replay-safe) to replace the repeated name lists, and one
class vocabulary. The engine and the shell both classify a call as **read**,
**edit** or **command** for the approval mode; the shell's browser tools add the
origin rule on top (acting on a non-local site) but map to those classes.

## Consequences

- Every tool has exactly one home and one gate; the webview has none.
- Scheduled runs and background chats work with the window hidden once the
  scheduler and the user tools are in the shell.
- The engine's tool files can be regrouped (one folder per tool, one registry)
  without touching the shell; that is the engine-side follow-up spec.
- Until the moves land, `engine-store.ts` still answers `notify`, `send_file`,
  `show_file`, the scheduler tools and MCP calls.
- Known gap: scheduled runs still need the window to start a task's chat.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Everything in the engine | The engine would need a host protocol for webviews, ptys, the database and notifications that already live in the shell. |
| Leave the webview tools as they are | A hidden window stalls them, and the layer rule would have exceptions. |
| Move only what is easy | The scheduler is the one that matters most (timers in a webview), and it is the hardest. |
