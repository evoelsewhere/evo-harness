# Nested sessions: roles, limits and mail across engines — Spec

| | |
| --- | --- |
| Status | Implemented |
| Area | engine · protocol · desktop shell · desktop UI · crates `evo-mesh` `evo-worktrees` `evo-hooks` `evo-tools` |
| Created | 2026-11-11 |
| Updated | 2026-11-11 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Subagents and nested sessions](../subagents-and-nested-sessions/spec.md), [Session mesh protocol](../session-mesh-protocol/spec.md), [Customize: hooks](../customize-skills-plugins-connectors/spec.md) |

## 1. Summary

Nested sessions and the session mesh work, but they stop short in places
that today's features already run into:

- A nested session always runs on its parent's model, with no role. A parent
  on a large model starts children on the same large model, and project agent
  types work for subagents but not for sessions.
- The running limits (8 nested sessions, 4 subagents) are fixed.
- Mail stops at the engine boundary. A nested session cannot ask its parent
  anything mid-task (it has to end its turn), and sibling sessions cannot talk
  at all. The mesh spec promises this (its FR-32), but it was never built.
- A worktree branch gets a generated name.
- No hook fires when a subagent ends.
- Agent types load from two fixed folders only.

This feature closes those gaps with general primitives, so that one chat can
run several role-specific sessions that coordinate. It takes over the open
tasks T21, T22 and T29 of
[subagents and nested sessions](../subagents-and-nested-sessions/tasks.md)
and T22 (FR-32) of the
[session mesh](../session-mesh-protocol/tasks.md).

## 2. Scope

### 2.1 Goals

- Start a nested session as an agent type, on a chosen model and thinking
  level, and keep that role across turns, reopening and engine restarts.
- Let the person set the running limits for nested sessions and subagents.
- Mail (`send_message`, `ask`, `reply`) between a session and its nested
  sessions in both directions, and between sibling nested sessions, across
  engines, with the same guards as mail inside one engine.
- Let the caller name a worktree's branch.
- A hook event when a subagent ends.
- Extra folders for agent types, like the extra skill folders.
- A session started with a read-only type is read-only for its whole engine,
  guarded like a read-only subagent.

### 2.2 Non-goals

- Deeper nesting. Nested sessions stay one level deep and subagents still
  cannot start subagents.
- Topics or pub/sub channels. Mail still goes from one node to another;
  channels stay deferred (mesh spec 6.2).
- Mail between root trees, and remote engines over TCP (mesh FR-33).
- Committing, merging or an integration branch for worktrees. **Apply** stays
  as it is ([subagents FR-22](../subagents-and-nested-sessions/spec.md)).
- A team or fleet feature (board, tickets, dashboard). That is a separate
  feature built on top of this one.
- An operating-system write restriction for any agent. Read-only stays
  enforced by the command checker (ADR
  [0020](../../architecture/decisions/0020-read-only-agents-stay-guarded-by-the-command-checker.md));
  subagents T30 is dropped.
- Running engines with no window. That is
  [shell-hosted engines](../shell-hosted-engines/spec.md).

## 3. User scenarios

### US-1 Start a session with a role and a cheaper model (priority P1)

A person's chat runs on a large model. The agent wants a read-only reviewer
working in its own session, on a smaller model, so that the review costs less
and cannot change files.

Acceptance scenarios:

1. **Given** a project agent type `reviewer` (`access: read-only`), **when**
   the agent calls `start_session` with that type and a smaller model,
   **then** the card shows the title, type, model and thinking level. After
   **Start**, the session runs on that model with the type's instructions, and
   a write is refused.
2. **Given** that session, **when** the app is restarted and the session gets
   a new message, **then** it still runs as `reviewer` on the same model.
3. **Given** an unknown type or model, **when** the agent calls
   `start_session`, **then** the call is refused and says what is available.

### US-2 A nested session asks its parent mid-task (priority P1)

A nested session reaches a decision it should not make alone. It asks its
parent and keeps working with the answer, without ending its turn.

Acceptance scenarios:

1. **Given** a running nested session, **when** it calls `ask` with `to:
   parent`, **then** the parent gets the question (woken if idle) and its
   `reply` resumes the asker's call.
2. **Given** the parent's engine is stopped, **when** the child asks, **then**
   the parent's engine is started and the question delivered.
3. **Given** the parent waits on an `ask` to that child, **when** the child
   asks the parent back, **then** the second ask is refused with
   `would_deadlock`.

### US-3 Sibling sessions coordinate (priority P2)

Two nested sessions of one chat work on parts of one change (for example an
API and its client) and need to agree on an interface.

Acceptance scenarios:

1. **Given** two sibling sessions, **when** one sends the other a message by
   title, **then** it arrives as a "Message from …" chip in the receiver.
2. **Given** a session in another root tree, **when** a session sends to it,
   **then** the call is refused.

### US-4 Run more sessions at once (priority P2)

A person with enough budget wants a chat to run more than 8 nested sessions
(or more than 4 subagents) at once.

Acceptance scenarios:

1. **Given** the nested-session limit set to 12 in **Settings → Sandbox**,
   **when** a chat starts 14 sessions, **then** 12 run and the rest are
   refused, from the tool and from the card alike.

### US-5 Find a session's work by its branch (priority P3)

The person wants to recognise a worktree's branch in Git tools.

Acceptance scenarios:

1. **Given** `start_session` with a worktree and `branch: "api-auth"`,
   **then** the worktree's branch is named after it, and the toolbar chip and
   the review dialog show it.

### US-6 A hook reacts when a subagent ends (priority P3)

A team wants to log or check each subagent's report.

Acceptance scenarios:

1. **Given** a `SubagentStop` hook, **when** a subagent ends (finished,
   failed, cancelled or stopped), **then** the hook runs with its type,
   description, status and the start of its report.

### ~~US-7 A read-only agent cannot write, even past the checker~~

Dropped 2026-11-11: the owner decided against an operating-system write restriction; read-only stays enforced by the command checker (ADR [0020](../../architecture/decisions/0020-read-only-agents-stay-guarded-by-the-command-checker.md)).

### US-8 A team keeps its agent types in a shared folder (priority P3)

A team keeps agent types in a folder outside `.evo/agents` (for example a
repository of shared roles).

Acceptance scenarios:

1. **Given** that folder listed in the settings, **when** the model next
   calls `agent` or `start_session`, **then** its types are usable and
   **Customize → Agents** lists them with their source.

### Edge cases

- The type's file is changed or removed while its session lives. The
  session keeps the type as it was when it started; a removed type is not an
  error.
- The chosen model's provider loses its credentials. The session fails like
  any chat whose model has no credentials, and does not fall back silently.
- The limit is lowered while more sessions run. The running ones go on, and
  new starts are refused until the count is under the limit.
- An `ask` goes to a sibling that the person closed or archived. It waits
  until the timeout (mesh FR-29), and an archived receiver is not woken.
- A branch name that already exists, or is not a valid Git branch name, is
  refused before anything is created.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | `start_session` takes an optional `agentType` (the names the `agent` tool takes). The session runs as that type for its whole life: the type's instructions come before its brief, its tool allow-list and `access` apply (read-only is enforced as for subagents), and its `model` and `thinking` are the session's defaults. The type is resolved once at start and stored with the chat, so the role is kept across turns, reopening and engine restarts and a later edit of the file does not change it. `maxTurns` does not apply to sessions. An unknown type is refused. | Given a read-only type, when a session started with it tries to write, then the write is refused; after an app restart it still is. |
| FR-2 | `start_session` takes an optional `model` (an id from the model catalog) and `thinking` (a level, clamped to the model). Precedence: the call's arguments, then the type's, then the parent's. An unknown model, or one whose provider has no credentials, is refused. | Given `model` on the call, then the child chat's model is that model, not the parent's. |
| FR-3 | The proposal card and the child chat's header show the type, model and thinking level, so the person sees what they start. With **Start sessions without asking** on, the same is shown on the start event. | Given a proposal with a type and model, then the card shows both. |
| FR-4 | A session started with a type never gets more than its parent: its approval mode is the parent's at start (the person may change it in the child, an agent may not), the same sandbox, and `access: full` cannot lift a read-only parent. | Given a read-only parent, when it starts a session of a `full` type, then the session is read-only. |
| FR-5 | **Settings → Sandbox** sets the number of nested sessions that run at once per chat (default 8) and of subagents that run at once per session (default 4). (nested sessions 1–24; running subagents 1–8; the kept-subagents limit stays 16). A change applies to new starts at once, without a restart; running ones are not stopped. | Given the limit at 12, when 14 starts are made, then 12 sessions run. |
| FR-6 | `send_message`, `ask` and `reply` work across engines between a session and its nested sessions (both ways) and between sibling nested sessions, addressed as today (`parent`, node id, title). A subagent inside a session can mail that session's nested sessions, and the other way round. Mail never crosses root trees. (Mesh FR-32.) | Given an agent in session A and nested session B, when the agent sends B a message, then B receives it. |
| FR-7 | Mail across engines is session mail: delivered at least once, deduplicated, kept across restarts, and it starts a stopped receiver's engine (mesh FR-16, FR-17, FR-22). It reaches the model as a mail chip, never as a person's message. Starting a stopped receiver with no window open needs [shell-hosted engines](../shell-hosted-engines/spec.md) (FR-4 there); until it lands, a window hosts it as today. | Given a stopped receiver, when a sibling sends to it, then its engine starts and it gets the mail once. |
| FR-8 | The guards hold across engines: one wait-for graph across all engines refuses an `ask` that closes a cycle with `would_deadlock`; the limit of 20 messages per directed pair between runs counts mail across engines; `ask` keeps its timeout (default 600 s, max 1800 s). | Given A waiting on an `ask` to B, when B asks A, then B's ask is refused with `would_deadlock`. |
| FR-9 | `start_session` with a worktree, and the `agent` tool with `isolation: "worktree"`, take an optional `branch`. The branch is named `evo/<branch>`, and a name that exists or is not valid in Git is refused before anything is created. The badge, the toolbar chip and the review dialog show the branch. | Given `branch: "api-auth"`, then the worktree's branch carries that name; given an existing name, then the call is refused and nothing is created. |
| FR-10 | A `SubagentStop` hook event fires in the parent's engine when a subagent ends (finished, failed, cancelled, stopped), with its agent type, description, status, the first 2,000 characters of its report and whether it kept a worktree. It cannot block, and its output does not reach the model. | Given a `SubagentStop` hook, when a subagent finishes, then the hook runs once with those fields. |
| FR-11 | Settings take `agentDirs`: extra folders of agent types, in user and project settings, relative paths resolved against the settings' owner (as `skillDirs`). Project folders load only for a trusted project. Types load in this order: project folder, project `agentDirs`, user folder, user `agentDirs`, plugins, built-ins; the first name wins. **Customize → Agents** shows each type's source. (Subagents T21.) | Given a type in an extra folder, when types load, then it is usable and listed with its source; given an untrusted project, then its `agentDirs` are ignored. |
| ~~FR-12~~ | dropped 2026-11-11: the owner decided against an operating-system write restriction; read-only stays enforced by the command checker (ADR [0020](../../architecture/decisions/0020-read-only-agents-stay-guarded-by-the-command-checker.md)). | — |
| FR-13 | A session started with a read-only type (FR-1, FR-4) is read-only for its whole engine: the command checker guards every call, and the shell refuses it every app tool that changes something (terminal input, browser and computer actions, scheduled-task changes, connector tools not marked read-only); engine tools that change something, `send_file` included, are refused by the command checker as for read-only subagents. Reading pages in the browser stays allowed; the `lsp` tool is refused and language servers are not started for it. Connector tools count as changing unless their server marks them read-only. | Given a read-only session, when it calls `terminal_write` or a connector tool that writes, then the shell refuses it. |
| ~~FR-14~~ | dropped 2026-11-11: the owner decided against an operating-system write restriction; read-only stays enforced by the command checker (ADR [0020](../../architecture/decisions/0020-read-only-agents-stay-guarded-by-the-command-checker.md)). | — |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Compatible: a call without the new arguments, an older engine and an older database behave exactly as today (parent's model, limits 8 and 4, generated branch names). | Existing hub, mesh and agent tests pass unchanged. |
| NFR-2 | Reach, limits, guards and permissions are enforced by the hub, the router and the engine, never by the model or a tool description (mesh NFR-2). | Review: every check in this spec has a test outside the model. |
| NFR-3 | Models, their limits and thinking levels come from the model catalog; nothing in this feature hard-codes a model id. | Review; a test with a catalog that lacks the model refuses it. |
| NFR-4 | A role is data the person can see: the type's name and source (project, user, plugin) are recorded with the chat and shown in it. | Given a typed session, then its header names the type and where it came from. |
| ~~NFR-5~~ | dropped 2026-11-11: the owner decided against an operating-system write restriction; read-only stays enforced by the command checker (ADR [0020](../../architecture/decisions/0020-read-only-agents-stay-guarded-by-the-command-checker.md)). | — |
| ~~NFR-6~~ | dropped 2026-11-11: the owner decided against an operating-system write restriction; read-only stays enforced by the command checker (ADR [0020](../../architecture/decisions/0020-read-only-agents-stay-guarded-by-the-command-checker.md)). | — |

## 5. Success criteria

- SC-1 A chat on a large model can start a read-only reviewer on a smaller
  model, and the reviewer's cost is the smaller model's.
- SC-2 A nested session gets a decision from its parent without ending its
  turn and without the person relaying it.
- SC-3 Two sibling sessions settle an interface between themselves, and the
  parent is woken only for their reports.
- SC-4 The person can run more than 8 sessions from one chat without a new
  release.
- SC-5 No existing behaviour changes for chats that do not use the new
  arguments.
- ~~SC-6~~ dropped 2026-11-11: the owner decided against an operating-system write restriction; read-only stays enforced by the command checker (ADR [0020](../../architecture/decisions/0020-read-only-agents-stay-guarded-by-the-command-checker.md)).

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-11 — Q: Does `maxTurns` of an agent type apply to a session? → A: No;
  it bounds subagent runs only (now in FR-1).
- 2026-11-11 — Q: Is a type's file read once at start or at every turn? → A: Once
  at start; the resolved type is stored with the chat (now in FR-1, edge
  cases).
- 2026-11-11 — Q: Should `start_session` also take `workspace`? → A: Not in this
  feature; T29's `workspace` part stays open in the subagents tasks
  (section 2.2).
- 2026-11-11 — Q: Do sibling nested sessions get the siblings capability by
  default? → A: Yes, never across root trees; this changes mesh FR-27 (now in
  FR-6).
- 2026-11-11 — Q: Ranges of the limits? → A: Nested sessions 1–24 (default 8),
  running subagents 1–8 (default 4), kept subagents stay 16 (now in FR-5).
- 2026-11-11 — Q: Branch name as given or prefixed? → A: Prefixed `evo/<name>`
  (now in FR-9).
- 2026-11-11 — Q: Approval mode of a nested session? → A: The parent's at start;
  the person can change it, an agent cannot (now in FR-4).
- 2026-11-11 — Q: Without OS protection, refuse or run with the checker only?
  → A: Refuse (fail closed) (now in FR-14).
- 2026-11-11 — Q: Which app tools does a read-only session lose? → A: Everything
  that changes something outside the agent; reading pages stays; `lsp` is
  refused (now in FR-13).
- 2026-11-11 — Q: Git Bash cannot start at low integrity on Windows (Phase 0
  spike). Should a read-only agent's `bash` be refused there, or should a
  private MSYS2 runtime be used? → A: Refused (fail closed, FR-14); read-only
  agents on Windows use `powershell`, which runs restricted (FR-12).
- 2026-11-11 — Q: Does the OS restriction block the network? → A: No; pushing
  and uploading stay refused by the command checker (now in FR-12 and
  section 2.2).

## 7. History

- 2026-11-11 — Draft. Gathers the gaps of nested sessions and the mesh that
  current features hit: roles and models for sessions (subagents T29), limits
  (subagents T22), mail across engines (mesh FR-32, T22), branch names and a
  `SubagentStop` hook.
- 2026-11-11 — Added extra agent folders (subagents T21, FR-11) and an
  operating-system write restriction for read-only agents (subagents T30,
  FR-12 – FR-14; ADR [0019](../../architecture/decisions/0019-read-only-agents-run-under-an-os-write-restriction.md)).
- 2026-11-11 — Gate A: open clarifications answered by the owner (accepted as
  proposed); spec accepted.
- 2026-11-11 — Gate B: plan and tasks accepted by the owner.
- 2026-11-11 — Windows: a read-only agent's `bash` is refused, `powershell` runs
  restricted (Phase 0 spike, 6.2).
- 2026-11-11 — FR-12, FR-14, NFR-5, NFR-6, US-7 and SC-6 dropped by the owner: no
  operating-system write restriction (ADR 0020 supersedes 0019). FR-13 keeps
  the command checker and the shell gate.
- 2026-11-11 — Implemented: every task ticked or dropped, every verification row
  passed (tasks.md section 2); T29's `workspace` part stays open in the
  subagents tasks.
