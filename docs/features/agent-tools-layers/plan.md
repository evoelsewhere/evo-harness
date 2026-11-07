# Agent tools by layer: one home, one registry, one class table — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-08 |
| ADD sections | [4. Process view](../../architecture/README.md#4-process-view), [5. Building blocks](../../architecture/README.md#5-building-blocks), [9. Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts) |
| ADRs | [0014 Where each agent tool lives](../../architecture/decisions/0014-agent-tools-by-layer.md), [0013](../../architecture/decisions/0013-app-side-agent-tools-run-in-the-rust-shell.md) |

## 1. Context and constraints

- The engine's tool files are flat in `crates/evo-engine/src/` and registered by
  hand in `session.rs` (about 2820 – 3010). The built-ins are in
  `crates/evo-tools/src/tools/` (unchanged by this feature).
- Name lists today: `session.rs` `BUILTIN_TOOL_NAMES`; `approval.rs`
  `READ_TOOLS` / `EDIT_TOOLS` and `classify`; `read_only.rs` `SAFE_TOOLS`;
  `modes.rs` reserved names in `rpc_configure_external_tools`;
  `extensions_actions.rs` reserved names; the desktop's `kinds` and label maps in
  `components/ai-elements/tool.tsx`.
- The shell's `agent_tools` registry exists (agent browser plan); the in-flight
  agent-browser work owns `agent_tools/browser`, `agent_tools/terminal.rs` and
  `agent_tools/app`. This feature starts after it lands, so the two do not edit the
  same files.
- Webview tool code to remove: `features/chat/user-tools.ts`,
  `features/browser/show-file.ts`, `features/scheduler/agent-tools.ts`, the MCP tool
  routing in `stores/engine-store.ts` (`runMcpTool` and the search / call pair) and
  `lib/mcp` helpers that only served it.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | Engine tools stay in the engine; app tools move to the shell (ADR 0014). |
| Layers point down; library crates stay free of engine/desktop concerns | ok | Only `evo-engine` and the shell change. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | No protocol change; tool definitions still travel in `configure_external_tools`. |
| Models come from models.dev; no hardcoded model facts | n/a | |
| Real engine in the app; settings and credentials reload live | ok | The approval mode is read per call as before. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Classes and gates are unchanged in effect; connector permissions keep their rules. |
| Desktop UI structure and `DESIGN.md` | ok | The webview loses logic, not UI. |

## 3. Design

### 3.1 Overview

```text
engine                                          shell (agent_tools/)                       webview
tools/info.rs   one table: name · class · read-only · reserved     registry: Tool { name, class, run }     shows tools' output and questions
tools/<tool>.rs definition + handler + tests                        browser · terminal · app ·              (no tool logic)
tools/mod.rs    register_all(ctx) -> Vec<HarnessTool>               notify · send_file · show_file ·
approval.rs / read_only.rs / session.rs / modes.rs                  mcp · scheduler
   read the table, keep no lists
```

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| engine | `crates/evo-engine/src/tools/info.rs` (new) | The tool table, `info(name)`, `class_of(name)`, `read_only_safe(name)`, `reserved_names()`. | FR-1 |
| engine | `crates/evo-engine/src/tools/mod.rs` (new) | `register_all`: builds every engine `HarnessTool` (moved out of `session.rs`). | FR-2 |
| engine | `crates/evo-engine/src/tools/{docs,document_preview,interaction,lsp,mesh,skill,visualize,web,agent}.rs` and folders (moved from `docs_tool.rs`, `document_preview_tool.rs`, `interaction.rs`, `lsp_tool.rs`, `mesh_tools.rs`, `skill_tool.rs`, `visualize_tool.rs` + `visualize/`, `web_tools.rs`, `agent_tool.rs` + `agent_tool/`) | One tool or family each: definition, handler, tests, prompt assets next to it. `memory.rs` keeps its storage; its tool part moves to `tools/memory.rs`. | FR-2 |
| engine | `approval.rs`, `read_only.rs`, `session.rs`, `modes.rs`, `extensions_actions.rs` | Read the table; keep shell-command analysis and per-call logic (memory actions, bash read-only check) where they are. | FR-1 |
| shell | `agent_tools/user.rs` (new) | `notify`, `send_file`, `show_file`. | FR-3 |
| shell | `agent_tools/mcp.rs` (new) | Connector tools through `mcp.rs`'s pool with the permission prompts. | FR-4 |
| shell | `agent_tools/scheduler.rs` (new) | The four scheduler tools over the database; emits `scheduler://changed`. | FR-5 |
| shell | `agent_tools/gate.rs`, `agent_tools/mod.rs` | `Class` becomes read / edit / command plus the origin rule. | FR-7 |
| desktop UI | `stores/engine-store.ts` | Drops the external-tool handler. | FR-6 |
| desktop UI | `components/ai-elements/tool-kinds.ts` (new), `tool.tsx` | One map of kinds and labels. | FR-8 |

### 3.3 Interfaces

No wire change. Rust: `tools::info::{info, class_of, read_only_safe, reserved_names}`
and `tools::register_all`. Shell: the registry's `Tool` gains the three classes;
event `scheduler://changed`.

### 3.4 Data and storage

None new.

### 3.5 Behaviour

Moves are behaviour-preserving. Each tool's result text, schema and description
stay byte-for-byte; the check is a before / after run of each tool.

### 3.6 Errors and edge cases

| Case | Result |
| --- | --- |
| A name not in the table (a plugin or connector tool) | Classified as a command, as today |
| A connector prompt with no window | Waits as other asks do (agent browser plan 3.6) |

### 3.7 Security and trust

No change in what is gated. Connector permissions (Allow / Ask / Block) and the
untrusted-content rules move with the code.

### 3.8 Configuration

None.

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Generate the table from the tool registrations | The class and the read-only flag are decisions, not facts of the tool; they belong in a reviewed table. |
| Keep per-tool metadata on each tool | Spreads the decision again; the table is the single place to review approvals. |
| Move the scheduler runner too | Needs the shell to host engines (ADR 0014). |

## 5. Phases (to become tasks)

1. Engine table: `tools/info.rs`, then switch the five consumers (tests first).
2. Engine regrouping: mechanical file moves into `tools/`, `register_all` out of `session.rs`.
3. Shell: `user.rs`, `mcp.rs`, `scheduler.rs`; the shell's classes.
4. Webview: remove the external-tool handler and the moved code; `tool-kinds.ts`.
5. Docs: ADD building blocks and tool paragraph; README maps.

## 6. As-built notes

Recorded 2026-11-08.

- **Classes.** The shell's `Class` is `Read` / `Edit` / `Command` (was `Observe` /
  `Act` / `Change`); an edit asks only in "Ask every time" and only off this
  machine, a command asks in "Ask every time" and "Approve edits".
- **tool-kinds.ts** holds the tool-name → kind map and the `Kind` type only; the
  per-kind icons, verbs and the label functions (`terminalLabel`, `browserLabel`)
  stay in `tool.tsx` because they depend on its `ToolPart` type. FR-8 holds for
  adding a tool's kind; its row wording still lives in `tool.tsx`.
- **Webview.** `engine-store.ts` no longer answers external tool requests at all
  (the early return is deliberate: the shell answers every one, an unknown name
  included); `external_tool_request_closed` still drops a pending approval.
- **Order of commits.** The shell move and the webview removal landed as two commits
  (`feat(desktop): agent tools run in the Rust shell…`, `refactor(desktop): the
  webview no longer runs agent tools`); the engine tool table and regrouping
  landed earlier.
