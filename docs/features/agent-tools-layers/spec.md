# Agent tools by layer: one home, one registry, one class table — Spec

| | |
| --- | --- |
| Status | Accepted |
| Area | engine, desktop shell, desktop UI |
| Created | 2026-11-08 |
| Updated | 2026-11-08 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [ADR 0014](../../architecture/decisions/0014-agent-tools-by-layer.md), [ADR 0013](../../architecture/decisions/0013-app-side-agent-tools-run-in-the-rust-shell.md), [Agent browser](../agent-browser/spec.md), [Approval modes](../approval-modes/spec.md) |

## 1. Summary

Agent tools were added one at a time and ended up in three layers with seven
repeated name lists. This feature applies ADR 0014: every tool has one home (the
engine for workspace, process, network and session work; the shell for anything
that needs a native app resource; the webview for none), the engine's tool files
sit in one `tools/` module with one metadata table, and the webview stops
answering tools. Behaviour does not change: a person and an agent see the same
tools, schemas, prompts and approvals as before.

## 2. Scope

### 2.1 Goals

- `notify`, `send_file`, `show_file`, the MCP connector tools and the scheduler
  tools (create, list, update, delete) are answered by the shell.
- The engine's tool code moves into `crates/evo-engine/src/tools/`, one module per
  tool (or tool family), registered in one place.
- One table of tool facts in the engine (name, class read / edit / command,
  safe for read-only agents, built-in / reserved) replaces the lists in
  `approval.rs`, `read_only.rs`, `session.rs`, `modes.rs` and
  `extensions_actions.rs`.
- The shell's tools use the same class vocabulary (read / edit / command) in their
  approval gate, with the origin rule for browser pages on top.
- The webview has no tool logic: `engine-store.ts` only registers the shell's
  definitions and shows what tools raise.

### 2.2 Non-goals

- Changing a tool's name, schema, description or result.
- Moving anything out of the engine.
- The scheduler's timers and the runner that starts a task's chat: starting an
  engine is still the window's job (ADR 0014; a later decision).
- A plugin-defined tool ABI change.

## 3. User scenarios

### US-1 Everything still works (priority P1)

Acceptance scenarios:

1. **Given** any chat, **when** the agent uses a file, shell, web, memory, skill,
   sub-agent, session, browser, terminal, notify, send_file, show_file, scheduler
   or connector tool, **then** it behaves as before.
2. **Given** the app window hidden to the tray, **when** an agent calls `notify`,
   `send_file`, a scheduler tool or a connector tool, **then** it is answered.

### US-2 A maintainer adds a tool (priority P2)

Acceptance scenarios:

1. **Given** a new engine tool, **when** the maintainer registers it, **then**
   one table row gives its approval class and read-only flag, and nothing else in
   the engine lists tool names.
2. **Given** a new app tool, **when** the maintainer adds it to the shell's
   registry, **then** its definition, class, gate and result need no webview
   code.

### Edge cases

- A plugin or connector tool that shares a name with a built-in: refused as today
  (the reserved names come from the table).
- A tool the table does not know (a plugin's): classified as a command, as today.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | The engine has one table of tool facts (`name`, `class` read / edit / command, `read_only_safe`, `reserved`) in `crates/evo-engine/src/tools/info.rs`; `approval.rs`, `read_only.rs`, `session.rs`, `modes.rs` and `extensions_actions.rs` read it and keep no tool-name lists. | `rg` finds no tool-name list outside the table; the approval and read-only tests pass unchanged. |
| FR-2 | The engine's tool modules live under `crates/evo-engine/src/tools/` (`docs`, `document_preview`, `interaction`, `lsp`, `mesh`, `skill`, `visualize`, `web`, `agent`, plus `memory`'s tool), each with its definition, handler and tests; `session.rs` registers them through one `tools::register_all`. | `session.rs` has no per-tool construction code; `cargo test -p evo-engine` passes. |
| FR-3 | `notify`, `send_file` and `show_file` are shell tools in `agent_tools/` with the same schemas and results; the webview file `features/chat/user-tools.ts` and the show-file code are gone. | A chat with the window hidden gets a notification and a file card. |
| FR-4 | The connector tools (`mcp__*` and the search / call pair) are answered by the shell through its MCP pool with the same permissions (Allow / Ask / Block) and prompts. | A connector set to Ask prompts; Block refuses; with the window hidden a call works. |
| FR-5 | The scheduler tools `create_scheduled_task`, `list_scheduled_tasks`, `update_scheduled_task`, `delete_scheduled_task` are shell tools over the app database; the webview scheduler reloads on a `scheduler://changed` event. | A task created by the agent shows in the Scheduler page without a restart. |
| FR-6 | `engine-store.ts` answers no tool: its external-tool handler is removed; it merges the shell's definitions and shows the shell's questions. | `rg "external_tool_request" apps/desktop/src` finds no tool dispatch. |
| FR-7 | The shell's tools classify calls as read / edit / command for the approval mode (observe = read; act and change per the agent-browser rules), sharing the vocabulary with the engine. | The gate tests in `agent_tools/gate.rs` use the same class names. |
| FR-8 | The desktop's transcript map of tool names to kinds and labels lives in one file (`components/ai-elements/tool-kinds.ts`) fed by nothing else. | `tool.tsx` imports it; adding a tool changes that file only. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | No behaviour change: every existing test passes, and a recorded run of each tool before and after gives the same result text. | `cargo test`, `bun test`, and the checks of FR-3 – FR-5. |
| NFR-2 | The moves are mechanical commits (file moves with their tests, no logic edits) separate from behaviour commits. | Review of the history. |

## 5. Success criteria

- SC-1 `rg` for a tool name finds its definition, its table row, its test and its
  transcript label, and nowhere else.
- SC-2 The window can be hidden and every tool still answers.

## 6. Clarifications

### 6.1 Open

n/a — none open.

### 6.2 Resolved

- 2026-11-08 — Q: Which layer for which tool? → A: ADR 0014 (engine for
  workspace, process, network, session; shell for native resources; webview none).
- 2026-11-08 — Q: Move the scheduler's timers too? → A: Not now; the runner needs
  the window to start a chat's engine (ADR 0014).
- 2026-11-08 — Q: One class vocabulary? → A: Yes: read / edit / command, with the
  browser origin rule on top.

## 7. History

- 2026-11-08 — Accepted by the person ("agree with this split, refactor it clean").
