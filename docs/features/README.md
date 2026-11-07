# Feature catalogue

Every feature built with Spec-Driven Development. Each folder holds
`spec.md` (what and why), `plan.md` (how) and `tasks.md` (the work). Add a
row when you start a `Draft` spec, and keep Status in step with the spec's
header. Work still planned for a feature shows as unticked tasks in its
`tasks.md`, not in this table. Rules and the workflow:
[docs/README.md](../README.md). Templates: [spec](../templates/spec.md),
[plan](../templates/plan.md), [tasks](../templates/tasks.md).

| Feature | Status | Area | Docs |
| --- | --- | --- | --- |
| [Agent terminal: shell choice and interactive tools](agent-terminal/spec.md) — `terminal_*` tools, shell discovery, approvals, shell integration | In progress | desktop UI, shell | [plan](agent-terminal/plan.md) · [tasks](agent-terminal/tasks.md) |
| [Agent tools for working with the person](agent-tools/spec.md) — `ask_user`, `update_todos`, `send_file`, `notify` | Implemented | engine, desktop UI | [plan](agent-tools/plan.md) · [tasks](agent-tools/tasks.md) |
| [Application database and provider settings](app-database/spec.md) — `evo.db`, migrations, models.dev cache | Implemented | `evo-store`, engine, shell | [plan](app-database/plan.md) · [tasks](app-database/tasks.md) |
| [Approval modes enforced in the engine](approval-modes/spec.md) — ask / approve edits / approve for me on every tool call, chat grants | In progress | engine, protocol, shell, desktop UI | [plan](approval-modes/plan.md) · [tasks](approval-modes/tasks.md) |
| [Side panel tabs, browser pages and terminals per chat](session-panels/spec.md) — each chat has its own tabs, pages and shells; a new chat starts with the panel closed | In progress | desktop UI | [plan](session-panels/plan.md) · [tasks](session-panels/tasks.md) |
| [Agent browser and app control: closing the tool gaps](agent-browser/spec.md) — pointer, wait, dialogs, upload, emulation, storage, network bodies, batch; then app control | In progress | desktop UI, desktop shell | [plan](agent-browser/plan.md) · [tasks](agent-browser/tasks.md) |
| [Browser bridge: an agent in the person's own Chrome or Edge](browser-bridge/spec.md) — extension, native host, shared tabs, per-chat grants | Draft | desktop shell, desktop UI, extension, new crate | [plan](browser-bridge/plan.md) |
| [Agent tools by layer: one home, one registry, one class table](agent-tools-layers/spec.md) — app tools to the shell, engine tools regrouped under `tools/`, one tool table | In progress | engine, desktop shell, desktop UI | [plan](agent-tools-layers/plan.md) · [tasks](agent-tools-layers/tasks.md) |
| [Code intelligence](code-intelligence/spec.md) — language servers and the `lsp` tool | Implemented | `evo-lsp`, engine, desktop UI | [plan](code-intelligence/plan.md) · [tasks](code-intelligence/tasks.md) |
| [Customize: skills, connectors, plugins, hooks](customize-skills-plugins-connectors/spec.md) | Implemented | engine, `evo-mcp`, `evo-plugins`, `evo-hooks`, desktop | [plan](customize-skills-plugins-connectors/plan.md) · [tasks](customize-skills-plugins-connectors/tasks.md) |
| [Git in the Files tool](files-git/spec.md) — changed files in the tree, changed lines in the editor | Implemented | `evo-worktrees`, shell, desktop UI | [plan](files-git/plan.md) · [tasks](files-git/tasks.md) |
| [Memory and the Scheduler](memory-and-scheduler/spec.md) | Implemented | engine, desktop UI | [plan](memory-and-scheduler/plan.md) · [tasks](memory-and-scheduler/tasks.md) |
| [Notifications](notifications/spec.md) — toasts, bell, OS notifications | Implemented | desktop UI, shell | [plan](notifications/plan.md) · [tasks](notifications/tasks.md) |
| [Office documents](office/spec.md) — LibreOffice runtime, Files preview, Office skills | Implemented | `evo-office`, engine, desktop | [plan](office/plan.md) · [tasks](office/tasks.md) |
| [Office documents: v3 behaviour](office-v3-parity/spec.md) — native preview, live build, area edits, versions, widgets | Implemented | `evo-office`, engine, protocol, desktop, skills | [plan](office-v3-parity/plan.md) · [tasks](office-v3-parity/tasks.md) |
| [Session mesh protocol](session-mesh-protocol/spec.md) | Implemented | `evo-mesh`, `evo-protocol`, engine, shell | [plan](session-mesh-protocol/plan.md) · [tasks](session-mesh-protocol/tasks.md) |
| [Subagents and nested sessions](subagents-and-nested-sessions/spec.md) | Implemented | engine, `evo-worktrees`, desktop | [plan](subagents-and-nested-sessions/plan.md) · [tasks](subagents-and-nested-sessions/tasks.md) |
| [Usage and cost history](usage-and-cost/spec.md) — every response's tokens and models.dev cost, Usage page | Implemented | `evo-store`, engine, desktop | [plan](usage-and-cost/plan.md) · [tasks](usage-and-cost/tasks.md) |
| [Web search and web fetch](web-search-and-fetch/spec.md) | Implemented | `evo-web`, engine | [plan](web-search-and-fetch/plan.md) · [tasks](web-search-and-fetch/tasks.md) |
