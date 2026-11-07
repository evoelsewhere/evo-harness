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

- [ ] T1 [FR-1] Write the table tests (every tool the engine registers has a row; class of each; read-only list equals today's `SAFE_TOOLS`; reserved names equal today's lists) — `tools/info.rs` — check: `cargo test -p evo-engine tools::info` fails for the right reason
- [ ] T2 [FR-1] Implement the table and its accessors — `tools/info.rs`, `lib.rs` — check: T1 passes
- [ ] T3 [FR-1] `approval.rs` classifies through the table (`READ_TOOLS` / `EDIT_TOOLS` removed; shell-command and memory-action logic stays) — `approval.rs` — check: `cargo test -p evo-engine approval::` unchanged
- [ ] T4 [FR-1] `read_only.rs` reads the safe flag — `read_only.rs` — check: `cargo test -p evo-engine read_only` unchanged
- [ ] T5 [FR-1] `session.rs` built-in names, `modes.rs` and `extensions_actions.rs` reserved names read the table — check: `cargo test -p evo-engine` unchanged

### Phase 2 — Regroup the engine's tool files

Done when: FR-2 holds; mechanical commits only.

- [ ] T6 [FR-2, NFR-2] Move `docs_tool.rs`, `document_preview_tool.rs`, `interaction.rs`, `lsp_tool.rs`, `skill_tool.rs`, `web_tools.rs` into `tools/` (with their tests and assets) — check: `cargo test -p evo-engine`
- [ ] T7 [FR-2, NFR-2] Move `mesh_tools.rs`, `agent_tool.rs` + `agent_tool/`, `visualize_tool.rs` + `visualize/`, and `memory.rs`'s tool part — check: `cargo test -p evo-engine`
- [ ] T8 [FR-2] `tools::register_all` replaces the per-tool construction in `session.rs` — `tools/mod.rs`, `session.rs` — check: `cargo test -p evo-engine`; `session.rs` has no per-tool code

### Phase 3 — The shell answers the rest

Done when: FR-3 – FR-5 and FR-7 hold.

- [ ] T9 [P] [FR-3] `notify`, `send_file`, `show_file` (schemas and results as the TS) — `agent_tools/user.rs`; webview events for the file cards and the Files panel — check: with the window hidden a notification and a card arrive
- [ ] T10 [P] [FR-4] Connector tools through the MCP pool with permissions and prompts — `agent_tools/mcp.rs` — check: Ask prompts, Block refuses, hidden window works
- [ ] T11 [P] [FR-5] The four scheduler tools over the database, `scheduler://changed` — `agent_tools/scheduler.rs`; `stores/scheduler-store.ts` reloads on the event — check: a created task shows in the Scheduler page
- [ ] T12 [FR-7] Shell classes read / edit / command with the origin rule — `agent_tools/mod.rs`, `gate.rs`, the tool modules — check: `cargo test agent_tools`

### Phase 4 — The webview answers nothing

Done when: FR-6 and FR-8 hold.

- [ ] T13 [FR-6] Delete the external-tool handler and the moved code (`features/chat/user-tools.ts`, `features/browser/show-file.ts`, `features/scheduler/agent-tools.ts`, MCP routing, unused `lib/mcp` helpers) — `stores/engine-store.ts` — check: `bun run typecheck`, `bun test`
- [ ] T14 [FR-8] One map of kinds and labels — `components/ai-elements/tool-kinds.ts`, `tool.tsx` — check: transcript rows unchanged

### Phase 5 — Docs

- [ ] T15 [FR-1 – FR-8] ADD building blocks and tool paragraph, `apps/desktop/README.md` maps, `crates/evo-engine` README — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `cargo test -p evo-engine` (table, approval, read-only, reserved names); `rg` for name lists | not run |
| FR-2 | `cargo test -p evo-engine`; `session.rs` has no per-tool code | not run |
| FR-3 | Hidden window: notification and file card | not run |
| FR-4 | Connector set to Ask / Block / Allow, hidden window | not run |
| FR-5 | Agent creates a task, the page shows it | not run |
| FR-6 | `rg "external_tool_request" apps/desktop/src` finds no dispatch | not run |
| FR-7 | `cargo test agent_tools` | not run |
| FR-8 | Transcript rows for each tool unchanged | not run |
| NFR-1 | Before / after run of each tool; all tests | not run |
| NFR-2 | History review | not run |
