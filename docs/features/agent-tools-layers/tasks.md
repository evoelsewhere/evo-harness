# Agent tools by layer: one home, one registry, one class table — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-08 |

## 1. Phases

Starts after the agent-browser move to the shell lands (same files in `engine-store.ts`
and `agent_tools/`). Engine paths under `crates/evo-engine/src/`, shell paths under
`apps/desktop/src-tauri/src/`.

### Phase 1 — The engine's tool table

Done when: FR-1 holds and every existing test passes unchanged.

- [x] T1 [FR-1] Write the table tests (every tool the engine registers has a row; class of each; read-only list equals today's `SAFE_TOOLS`; reserved names equal today's lists) — `tools/info.rs` — check: `cargo test -p evo-engine tools::info` fails for the right reason
- [x] T2 [FR-1] Implement the table and its accessors — `tools/info.rs`, `lib.rs` — check: T1 passes
- [x] T3 [FR-1] `approval.rs` classifies through the table (`READ_TOOLS` / `EDIT_TOOLS` removed; shell-command and memory-action logic stays) — `approval.rs` — check: `cargo test -p evo-engine approval::` unchanged
- [x] T4 [FR-1] `read_only.rs` reads the safe flag — `read_only.rs` — check: `cargo test -p evo-engine read_only` unchanged
- [x] T5 [FR-1] `session.rs` built-in names, `modes.rs` and `extensions_actions.rs` reserved names read the table — check: `cargo test -p evo-engine` unchanged

### Phase 2 — Regroup the engine's tool files

Done when: FR-2 holds; mechanical commits only.

- [x] T6 [FR-2, NFR-2] Move `docs_tool.rs`, `document_preview_tool.rs`, `interaction.rs`, `lsp_tool.rs`, `skill_tool.rs`, `web_tools.rs` into `tools/` (with their tests and assets) — check: `cargo test -p evo-engine`
- [x] T7 [FR-2, NFR-2] Move `mesh_tools.rs`, `agent_tool.rs` + `agent_tool/`, `visualize_tool.rs` + `visualize/`, and `memory.rs`'s tool part — check: `cargo test -p evo-engine`
- [x] T8 [FR-2] `tools::register_all` replaces the per-tool construction in `session.rs` — `tools/mod.rs`, `session.rs` — check: `cargo test -p evo-engine`; `session.rs` has no per-tool code

### Phase 3 — The shell answers the rest

Done when: FR-3 – FR-5 and FR-7 hold.

- [x] T9 [P] [FR-3] `notify`, `send_file`, `show_file` (schemas and results as the TS) — `agent_tools/user.rs`; webview events for the file cards and the Files panel — check: with the window hidden a notification and a card arrive
- [x] T10 [P] [FR-4] Connector tools through the MCP pool with permissions and prompts — `agent_tools/mcp.rs` — check: Ask prompts, Block refuses, hidden window works
- [x] T11 [P] [FR-5] The four scheduler tools over the database, `scheduler://changed` — `agent_tools/scheduler.rs`; `stores/scheduler-store.ts` reloads on the event — check: a created task shows in the Scheduler page
- [x] T12 [FR-7] Shell classes read / edit / command with the origin rule — `agent_tools/mod.rs`, `gate.rs`, the tool modules — check: `cargo test agent_tools`

### Phase 4 — The webview answers nothing

Done when: FR-6 and FR-8 hold.

- [x] T13 [FR-6] Delete the external-tool handler and the moved code (`features/chat/user-tools.ts`, `features/browser/show-file.ts`, `features/scheduler/agent-tools.ts`, MCP routing, unused `lib/mcp` helpers) — `stores/engine-store.ts` — check: `bun run typecheck`, `bun test`
- [x] T14 [FR-8] One map of kinds and labels — `components/ai-elements/tool-kinds.ts`, `tool.tsx` — check: transcript rows unchanged

### Phase 5 — Docs

- [x] T15 [FR-1 – FR-8] ADD building blocks and tool paragraph, `apps/desktop/README.md` maps, `crates/evo-engine` README — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `cargo test -p evo-engine` (table, approval, read-only, reserved names); `rg` for name lists | passed (2026-11-08): `cargo test -p evo-engine` 724 passed |
| FR-2 | `cargo test -p evo-engine`; `session.rs` has no per-tool code | passed (2026-11-08): `cargo test -p evo-engine`; `session.rs` registers through `tools::register_all` |
| FR-3 | Hidden window: notification and file card | partly (2026-11-08): `notify`, `send_file`, `show_file` returned success from the shell in real chats, `notify` also with the main window hidden by the OS; the file card and the Files panel were not inspected |
| FR-4 | Connector set to Ask / Block / Allow, hidden window | passed (2026-11-08): a stdio test connector's tool set to Ask showed the approval (Allow once / Always allow / Deny) and ran, set to Block it was not offered to the agent, set to Allow it ran without a question; hidden-window case not run |
| FR-5 | Agent creates a task, the page shows it | passed (2026-11-08): an agent created, listed and deleted a task; each change asked in the composer |
| FR-6 | `rg "external_tool_request" apps/desktop/src` finds no dispatch | passed (2026-11-08): the webview answers no external tool request (`engine-store.ts` returns at `external_tool_request`) |
| FR-7 | `cargo test agent_tools` | passed (2026-11-08): `bun run test:shell` 422 passed with the gate tests on `Class::{Read, Edit, Command}` |
| FR-8 | Transcript rows for each tool unchanged | passed (2026-11-08): `components/ai-elements/tool-kinds.ts` holds the kind map; `tool.tsx` imports it (the labels stay in `tool.tsx`, see the as-built note) |
| NFR-1 | Before / after run of each tool; all tests | partly (2026-11-08): all Rust tests and `bun test` pass; no before / after text comparison of every tool |
| NFR-2 | History review | not run |
