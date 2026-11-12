# Nested sessions: roles, limits and mail across engines — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-11 |

## 1. Phases

Order and dependencies:

- **Phase 0** (spikes) and **Phase 4** (OS write restriction) are dropped
  (ADR 0020): only the Windows spike ran, and its findings stay in the plan.
  Gate B for those requirements is conditional on its results, which are
  recorded in the plan's as-built notes.
- **Phases 1–3** do not depend on Phase 0 or on
  [shell-hosted engines](../shell-hosted-engines/spec.md), and can start at
  once.
- **Phase 5** follows Phase 1.
- **Phase 6** (mail across engines) works with a window present. It works
  with no window once shell-hosted engines land (ADR 0018); it does not wait
  for that feature.
- **Phase 7** holds the documentation edits.

How the checks run:

- Engine tests: `cargo test -p evo-engine <name>` (faux provider).
- Crate tests: `cargo test -p <crate> <name>`.
- Shell tests: `bun run test:shell <filter>` in `apps/desktop`.

### Phase 0 — OS restriction spikes (dropped)

Done when: on each platform, a throwaway helper restricts itself without
administrator rights, a write outside scratch fails, and reading tools work.
The findings (what failed, timings, chosen fallback) are written into the
plan's section 5.

- [x] T1 [P] [FR-12] [NFR-5] Windows spike: lower the helper's own token to
  low integrity, label a scratch folder low, then run Git Bash and PowerShell
  through it. Try `git status`, `git log`, `rg`, `ls`, `Get-ChildItem` and a
  write to the workspace and to `%USERPROFILE%`. If self-lowering fails, try
  the `CreateProcessAsUserW` fallback. — scratch crate outside the workspace —
  check: written findings, start overhead measured — done 2026-11-11: works for
  PowerShell, cmd and native programs (+3–35 ms); Git Bash cannot start at low
  integrity (plan section 5)
- ~~T2~~ dropped 2026-11-11: no operating-system write restriction (ADR 0020).
- ~~T3~~ dropped 2026-11-11: no operating-system write restriction (ADR 0020).
- ~~T4~~ dropped 2026-11-11: no operating-system write restriction (ADR 0020).

### Phase 1 — Roles, model and thinking for sessions

Done when: a session started with a type and a model runs as that role on
that model, keeps it after an app restart, and the card and header show it.

- [x] T5 [FR-1] [NFR-1] Migration `0013_chat_roles.sql` and `role` on
  `Chat`, `NewChat` and `ChatPatch`. Test first: an old database upgrades,
  and the role round-trips. — `crates/evo-store/src/migrations/0013_chat_roles.sql`, `crates/evo-store/src/chats.rs`, `crates/evo-store/src/lib.rs` — check: `cargo test -p evo-store chat_role`
- [x] T6 [FR-1] [FR-6] `SessionRole` in the mesh wire types, plus `role` and
  `branch` on `SessionSpec`. Serde tests for the old shape and the new one. —
  `crates/evo-mesh/src/wire.rs`, `crates/evo-mesh/src/hub.rs` — check:
  `cargo test -p evo-mesh session_spec`
- [x] T7 [FR-1] [FR-2] [FR-4] [NFR-3] Tests for `session_roles::enrich`:
  - an unknown type or model is refused;
  - a model without credentials is refused;
  - thinking is clamped;
  - the order is the argument, then the type, then the parent;
  - a read-only parent caps access;
  - a model-written `role` is stripped.

  — `crates/evo-engine/src/session_roles.rs` — check: `cargo test -p evo-engine session_roles` fails
- [x] T8 [FR-1] [FR-2] [FR-4] Implement `enrich` and call it from the
  external tool path for `start_session`. — `crates/evo-engine/src/session_roles.rs`, `crates/evo-engine/src/rpc/external_tools.rs`, `crates/evo-engine/src/lib.rs` — check: T7 passes
- [x] T9 [FR-1] [FR-3] [NFR-1] Hub changes:
  - the schema gains `agentType`, `model`, `thinking` and `branch`;
  - `call_start` carries the role;
  - the proposal and start payloads carry the role summary;
  - a call with role fields but no `role` is refused.

  Hub tests first. — `crates/evo-mesh/src/hub.rs` — check: `cargo test -p evo-mesh role`
- [x] T10 [FR-1] [FR-2] [NFR-4] `session_row` uses the role's provider and
  model and stores the role. `engine_start` writes `<chat>.role.json` and
  passes `--role-file`. — `apps/desktop/src-tauri/src/mesh.rs`, `apps/desktop/src-tauri/src/engine.rs` — check: `bun run test:shell mesh::a_session_with_a_role`
- [x] T11 [FR-1] Engine applies `--role-file`: instructions in the system
  prompt, tool narrowing, and thinking at the first start. Tests:
  - a typed session has the instructions and only its tools;
  - after a restart it keeps them.

  — `crates/evo-engine/src/args.rs`, `crates/evo-engine/src/session.rs`, `crates/evo-engine/src/session_roles.rs` — check: `cargo test -p evo-engine role_file`
- [x] T12 [P] [FR-3] [NFR-4] Card shows type, model, thinking and branch;
  child header shows the role and its source. — `apps/desktop/src/components/ai-elements/session-proposal.tsx`, `apps/desktop/src/features/shell/workspace-header.tsx`, `apps/desktop/src/engine/protocol.ts` — check: `bun run typecheck`; run in the app

### Phase 2 — Limits and agent folders

Done when: both limits are set in Settings → Sandbox and apply without a
restart, and types from an extra folder load and show their source.

- [x] T13 [FR-5] Hub: `max_children` replaces the constant, set through
  `mesh_configure`. Tests: a raised limit admits 12; a lowered one stops
  nothing that runs. — `crates/evo-mesh/src/hub.rs`, `apps/desktop/src-tauri/src/mesh.rs` — check: `cargo test -p evo-mesh changed_limit`
- [x] T14 [FR-5] Engine: a counted `Slots` type replaces the fixed semaphore,
  with the limit taken from `SandboxConfig` and from the RPC `set_limits`.
  Tests: six agents with a limit of 6 run at once; lowering the limit cancels
  nothing. — `crates/evo-engine/src/tools/agent.rs`, `crates/evo-tools/src/sandbox.rs`, `crates/evo-engine/src/modes.rs`, `crates/evo-engine/src/rpc/protocol.rs` — check: `cargo test -p evo-engine running_limit`
- [x] T15 [FR-5] Shell defaults and range checks for both keys; send
  `set_limits` to running engines on change; the Settings → Sandbox inputs. —
  `apps/desktop/src-tauri/src/engine.rs`, `apps/desktop/src/features/settings/sections/sandbox-settings.tsx`, `apps/desktop/src/stores/engine-store.ts` — check: `bun run typecheck`; run in the app
- [x] T16 [P] [FR-11] Tests for `agentDirs`:
  - load order;
  - relative paths are resolved against the settings' owner;
  - an untrusted project's `agentDirs` are ignored;
  - `source` is set on each type.

  — `crates/evo-engine/src/agents.rs` — check: `cargo test -p evo-engine agent_dirs` fails
- [x] T17 [FR-11] Implement the `agentDirs` setting (alias `agents`) and
  `ResourceKind::Agents`. Add the `source` field and send it to Customize →
  Agents. — `crates/evo-engine/src/settings.rs`, `crates/evo-engine/src/resource_dirs.rs`, `crates/evo-engine/src/agents.rs`, `crates/evo-engine/src/customize.rs`, `apps/desktop/src/features/customize/customize-page.tsx` — check: T16 passes; run in the app

### Phase 3 — Branch names and `SubagentStop`

Done when: a named worktree branch shows in the badge, the chip and the
dialog, and a `SubagentStop` hook runs once per ended subagent.

- [x] T18 [P] [FR-9] `Worktrees::create_named`. Tests:
  - `evo/<name>` is created;
  - a name that exists or is invalid is refused before anything is made;
  - removal deletes only a branch the manager recorded.

  — `crates/evo-worktrees/src/lib.rs` — check: `cargo test -p evo-worktrees named_branch`
- [x] T19 [FR-9] Pass `branch` through the `agent` tool's isolation and
  through `start_session` worktrees. The badge, the chip and the review dialog
  already show the branch; confirm in the app. — `crates/evo-engine/src/tools/agent.rs`, `crates/evo-engine/src/child_agents.rs`, `apps/desktop/src-tauri/src/mesh.rs` — check: `cargo test -p evo-engine named_worktree`; run in the app
- [x] T20 [P] [FR-10] `SubagentStop` in `EVENTS`. Tests: it fires once for
  each of finished, failed, cancelled and stopped, with the payload fields; it
  cannot block; its output is dropped. — `crates/evo-hooks/src/lib.rs`, `crates/evo-engine/src/hooks.rs`, `crates/evo-engine/src/tools/agent.rs` — check: `cargo test -p evo-engine subagent_stop_hook`
- [x] T21 [P] [FR-10] Event list in Customize → Hooks. — `apps/desktop/src/features/customize/hooks-section.tsx` — check: `bun run typecheck`

### Phase 4 — OS write restriction for read-only subagents (dropped)

Needs Phase 0. Done when: a read-only subagent's command that the checker
lets through cannot write on any platform in CI, and a `full` sibling still
writes.

- ~~T22~~ dropped 2026-11-11: no operating-system write restriction (ADR 0020).
- ~~T23~~ dropped 2026-11-11: no operating-system write restriction (ADR 0020).
- ~~T24~~ dropped 2026-11-11: no operating-system write restriction (ADR 0020).
- ~~T25~~ dropped 2026-11-11: no operating-system write restriction (ADR 0020).
- ~~T26~~ dropped 2026-11-11: no operating-system write restriction (ADR 0020).

### Phase 5 — Read-only sessions

Needs Phase 1. Done when: a session with a read-only role is refused every
writing tool and app tool by the command checker and the shell gate, and
reading still works.

- [x] T27 [FR-13] Engine tests: a read-only role puts the guard first in the
  `before_tool_call` chain, narrows external tools to its role, and has no
  `lsp`. —
  `crates/evo-engine/src/session.rs`, `crates/evo-engine/src/session_roles.rs`, `crates/evo-engine/src/tools/mod.rs` — check: `cargo test -p evo-engine read_only_session`
- [x] T28 [FR-13] The shell refuses app tools whose class is not `Read`, and
  connector tools without `readOnlyHint`, to read-only chats, before any
  prompt. Tests first. — `apps/desktop/src-tauri/src/agent_tools/mod.rs`, `apps/desktop/src-tauri/src/mcp.rs` — check: `bun run test:shell agent_tools::read_only_chat`

### Phase 6 — Mail across engines

Done when: an agent in session A messages and asks nested session B, a
nested session asks its parent mid-turn, and siblings mail each other.
Cycles are refused, and the pair limit holds across engines.

- [x] T29 [FR-6] [FR-8] Wire: `MailBody` gains `kind`, `ask`, `reply_to` and
  `waits`; mail ops `send`, `ask`, `reply` and `cancel_ask`; the `peers`
  command. `MAX_PAIR_MAIL` moves to `evo-mesh`. Old envelopes still parse. —
  `crates/evo-mesh/src/wire.rs`, `crates/evo-mesh/src/lib.rs`, `crates/evo-engine/src/mesh.rs` — check: `cargo test -p evo-mesh mail_ops`
- [x] T30 [FR-6] [FR-7] [FR-8] Hub tests for `route_mail`:
  - reach covers the parent, children and siblings, and never another tree;
  - a title that is ambiguous or unknown is refused, with the reachable list;
  - the pair limit holds, with its reset;
  - a remote-only cycle and a mixed local/remote cycle are both refused;
  - delivery is durable, and a stopped receiver is hosted;
  - a receiver without the capability gets `unsupported`.

  — `crates/evo-mesh/src/hub.rs` — check: `cargo test -p evo-mesh route_mail` fails
- [x] T31 [FR-6] [FR-7] [FR-8] Implement `route_mail` and `peers` in the hub
  and feed the hub from `mesh::on_line`. — `crates/evo-mesh/src/hub.rs`, `apps/desktop/src-tauri/src/mesh.rs` — check: T30 passes
- [x] T32 [FR-6] [FR-8] Engine tests:
  - an unknown target is forwarded with its `waits`;
  - a remote ask blocks and resolves on its reply;
  - the timeout sends `cancel_ask`;
  - a reply with no waiting call becomes a chip;
  - a received ask is answered by `reply` or by the end of the run;
  - `list_nodes` shows peers;
  - the capability is announced.

  — `crates/evo-engine/src/tools/mesh.rs`, `crates/evo-engine/src/mesh.rs` — check: `cargo test -p evo-engine cross_engine_mail` fails
- [x] T33 [FR-6] [FR-8] Implement the engine side so that T32 passes. —
  `crates/evo-engine/src/tools/mesh.rs`, `crates/evo-engine/src/mesh.rs` — check: T32 passes
- [x] T34 [FR-6] [FR-7] End-to-end run in the app:
  - parent, two siblings and a subagent exchange messages and an ask;
  - one receiver is stopped and is hosted;
  - restart the app with mail pending.

  — check: run in the app (WebView2 CDP), recorded in the verification table

### Phase 7 — Documentation

Done when: every other document this feature changes says what is built and
points here.

- [x] T35 [FR-1] – [FR-13] Fill the plan's as-built notes and tick the
  verification table. — `docs/features/session-roles-and-mail/plan.md`, `tasks.md` — check: review
- [x] T36 [FR-1] [FR-5] [FR-13] Subagents spec:
  - FR-3 (the OS layer sits behind the checker);
  - FR-7 (settable running limit);
  - FR-15 (role, model, thinking, branch);
  - FR-18 (settable session limit);
  - a History line.

  — `docs/features/subagents-and-nested-sessions/spec.md` — check: review
- [x] T37 [FR-1] [FR-5] [FR-11] Subagents tasks: T21, T22 and T29 (its
  `model` part) struck out with a pointer to this feature, T30 struck out as
  dropped (ADR 0020);
  T29's `workspace` part stays open. — `docs/features/subagents-and-nested-sessions/tasks.md` — check: review
- [x] T38 [FR-6] Mesh spec, plan and tasks:
  - spec: FR-27 (sessions reach siblings) and FR-32 restated as built, plus
    a History line;
  - plan: the components row of FR-32;
  - tasks: T22 struck out with a pointer.

  — `docs/features/session-mesh-protocol/spec.md`, `plan.md`, `tasks.md` — check: review
- [x] T39 [FR-10] Customize spec: the hook event list gains `SubagentStop`. —
  `docs/features/customize-skills-plugins-connectors/spec.md` — check: review
- ~~T40~~ dropped 2026-11-11: no operating-system write restriction (ADR 0020).
- [x] T41 [FR-3] [FR-5] [FR-11] UI map: the card, the header,
  Settings → Sandbox and Customize → Agents. — `apps/desktop/README.md` —
  check: review
- [x] T42 [FR-1] [FR-11] Spec links in the module doc comments of the new and
  changed modules. — the code files above — check:
  `git grep 'docs/features/session-roles-and-mail'`
- [x] T43 [FR-1] – [FR-13] Status `Implemented` in the spec and the
  catalogue once every row below has passed. — `docs/features/session-roles-and-mail/spec.md`, `docs/features/README.md` — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `cargo test -p evo-engine session_roles role_file`; `cargo test -p evo-mesh role`; `cargo test -p evo-store chat_role`; run in the app (restart keeps the role) | passed (2026-11-11): `session_roles` 10, `role_file` 2, evo-mesh `role` 2, evo-store `chat_role` 2; run in the app (T12: role stored, `.role.json` written, thinking Low applied) |
| FR-2 | `cargo test -p evo-engine session_roles` (precedence, unknown model, no credentials) | passed (2026-11-11): `session_roles` (argument → type → parent, unknown model, thinking clamped) |
| FR-3 | `bun run typecheck`; run in the app (card and header show type, model, thinking) | passed (2026-11-11): `bun run typecheck`; run in the app: the card read "Runs as reviewer · … · thinking low · read-only", the header showed the role chip |
| FR-4 | `cargo test -p evo-engine session_roles` (read-only parent caps access) | passed (2026-11-11): `session_roles_of_a_read_only_parent_are_read_only` |
| FR-5 | `cargo test -p evo-mesh changed_limit`; `cargo test -p evo-engine running_limit`; run in the app | passed (2026-11-11): evo-mesh `changed_limit` 1, engine `running_limit` 2; run in the app: limit 2 refused the third of three starts |
| FR-6 | `cargo test -p evo-mesh route_mail`; `cargo test -p evo-engine cross_engine_mail`; T34 run | passed (2026-11-11): `cargo test -p evo-mesh route_mail`, `cargo test -p evo-engine cross_engine_mail`; run in the app: sibling `send_message`, a subagent of the parent to a nested session |
| FR-7 | `cargo test -p evo-mesh route_mail` (durable, hosted receiver); T34 restart run | passed (2026-11-11): run in the app: mail to a crashed receiver restarted its engine; mail waiting at quit was delivered after the app started again; shown as "Messages arrived" chips |
| FR-8 | `cargo test -p evo-mesh route_mail` (cycles, pair limit); `cargo test -p evo-engine cross_engine_mail` (timeout) | passed (2026-11-11): `cargo test -p evo-mesh route_mail_refuses_a_remote_cycle route_mail_holds_the_pair_limit_until_the_sender_runs_again`; run in the app: a nested session's `ask` to its parent resolved by the parent's `reply` mid-turn |
| FR-9 | `cargo test -p evo-worktrees named_branch`; `cargo test -p evo-engine named_worktree`; run in the app | passed (2026-11-11): evo-worktrees `named_branch` 1, engine `named_worktree` 1; run in the app: branch `evo/review-readme` made and shown on the chip |
| FR-10 | `cargo test -p evo-engine subagent_stop_hook` | passed (2026-11-11): `subagent_stop_hook` 2 |
| FR-11 | `cargo test -p evo-engine agent_dirs`; run in the app (Customize → Agents source) | passed (2026-11-11): `agent_dirs` 3; run in the app: Customize → Agents showed "Personal · agentDirs" |
| ~~FR-12~~ | dropped 2026-11-11: no operating-system write restriction (ADR 0020) | — |
| FR-13 | `cargo test -p evo-engine read_only_session`; `bun run test:shell agent_tools::read_only_chat` | passed (2026-11-11): engine `read_only_session` 2, shell `agent_tools::read_only_chat` and `read_only_chat_keeps_the_servers_read_only_hint` |
| ~~FR-14~~ | dropped 2026-11-11: no operating-system write restriction (ADR 0020) | — |
| NFR-1 | Existing `cargo test -p evo-mesh`, `cargo test -p evo-engine --lib tools::agent`, `bun run test:shell mesh::` unchanged and passing; an old database upgrades (`chat_role`) | passed (2026-11-11): evo-mesh 48/48, engine `tools::agent` 30/30, shell `mesh::` 5/5 |
| NFR-2 | Review: every reach, limit and access check is in the hub, router or engine, each with a test above | passed (2026-11-11): review; reach, limits, pair limit and cycles in the hub, the read-only guard in the engine and the shell gate, each tested above |
| NFR-3 | `cargo test -p evo-engine session_roles` (a catalog without the model refuses it); review for hard-coded ids | passed (2026-11-11): `session_roles_refuse_unknown_types_models_and_levels`; no model id in the new code (grep) |
| NFR-4 | Run in the app: a typed session's header names its type and source | passed (2026-11-11): run in the app: the header chip names the type, its tooltip the source (`project: .evo/agents/reviewer.md`) |
| ~~NFR-5~~ | dropped 2026-11-11: no operating-system write restriction (ADR 0020) | — |
| ~~NFR-6~~ | dropped 2026-11-11: no operating-system write restriction (ADR 0020) | — |
