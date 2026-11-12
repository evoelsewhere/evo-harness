# Subagents and nested sessions — Spec

| | |
| --- | --- |
| Status | Implemented |
| Area | engine · crate `evo-worktrees` · desktop UI |
| Created | 2026-11-06 |
| Updated | 2026-11-12 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Session mesh](../session-mesh-protocol/spec.md), [Usage and cost](../usage-and-cost/spec.md), [App database](../app-database/spec.md) |

## 1. Summary

A chat's agent can hand work to other agents in two ways. A **subagent** is a
child agent inside the same engine, run as one tool call: it starts with only
the brief it is given, works on its own, and hands back one report, so wide
searches and file dumps stay out of the parent's context. A **nested
session** is a full chat started by another chat: the person can open it,
read it, type into it and stop it like any chat, and the parent hears from it
each time it finishes a turn. Both can work in their own Git worktree, and the
person reviews what a worktree changed before it reaches the workspace.

The two look alike but are different things:

| | Subagent | Nested session |
| --- | --- | --- |
| What it is | A child agent **inside one engine**, run as a tool call of the parent | A **full chat** (own engine process, own transcript, own sidebar row) started by another chat |
| Context | Fresh: sees only the task it was given | Fresh, or a fork of the parent's conversation |
| Who sees the work | The parent gets one final report; the person sees the child's steps nested under the tool call | The person can open it, read it, type into it, like any chat |
| Lifetime | Bounded: ends with its report (or runs in the background until then) | Open-ended: lives on after the parent's turn, can be resumed |
| Back to the parent | The tool result (foreground) or a wake-up note (background) | A note each time the child finishes a turn; the parent may be woken |
| Cost of a mistake | A few model turns | A whole extra session that keeps running |

## 2. Scope

### 2.1 Goals

- Let the model delegate a bounded task to a child agent with a fresh
  context and get one report back, in the foreground or the background.
- Let projects and people define their own agent types in Markdown files,
  beside three built-in types (`general`, `explore`, `plan`).
- Enforce read-only access for agent types that declare it.
- Show a child's work live in the transcript and count its cost in the turn.
- Let an agent start, message and list full child chats (nested sessions),
  with the person deciding whether a proposed session starts.
- Bring a nested session's finished turns back to its parent, waking an idle
  parent within guard rails.
- Show nested sessions under their parent in the sidebar.
- Let subagents and nested sessions work in their own Git worktree, and let
  the person review, apply or discard a kept worktree.

### 2.2 Non-goals

- Recursion: a subagent cannot start subagents, and a nested session cannot
  start its own sessions (one level).
- Merging or committing a worktree's changes: **Apply** puts them in the
  workspace uncommitted; committing stays with the person.
- Transport of notes and session requests between windows and engines: that
  is the [session mesh](../session-mesh-protocol/spec.md).
- Plugin-owned child agents (`delegate_task`): their action API is unchanged
  and documented with plugins.

## 3. User scenarios

### US-1 Delegate a search to a subagent (priority P1)

A person asks a question that needs a wide search; the model wants the answer
without filling its own context with search output.

Acceptance scenarios:

1. **Given** a chat, **when** the model calls the `agent` tool with type
   `explore`, **then** the person sees the child's steps live under the tool
   call, the parent receives only the child's report, and the turn's cost
   includes the child's usage.
2. **Given** an `explore` or `plan` agent, **when** it tries to write a file or
   run a writing command, **then** the call is refused and the refusal tells
   it to retry with a plainer command.

### US-2 Run a subagent in the background (priority P2)

The model starts a long task and keeps working, or the person goes idle.

Acceptance scenarios:

1. **Given** a background agent, **when** it finishes while the parent is
   idle, **then** the parent continues with its report on its next run
   without the person relaying it.
2. **Given** a finished agent, **when** the model sends it a follow-up,
   **then** it resumes with its own context.
3. **Given** the parent waits on foreground agents, **when** the person (or
   another session) steers the parent, **then** the wait ends at once and the
   agents report later like background agents.

### US-3 An agent proposes a nested session (priority P1)

The model sees work that deserves its own chat.

Acceptance scenarios:

1. **Given** the default setting, **when** the model calls `start_session`,
   **then** a card **Start session: {title}** appears with **Start** and
   **Dismiss**, and nothing starts until the person clicks **Start**.
2. **Given** the session was started, **then** it appears under the parent in
   the sidebar, and when it finishes a turn the parent gets a note with the
   start of its report.

### US-4 Review a kept worktree (priority P2)

A subagent or nested session changed files in its own worktree.

Acceptance scenarios:

1. **Given** a kept worktree, **when** the person opens the review dialog
   (from the subagent's tool call, the session's toolbar chip, or
   **Settings → Sandbox**), **then** they see the changes and can **Apply**
   or **Discard** them.
2. **Given** a patch that does not apply cleanly, **when** the person clicks
   **Apply**, **then** nothing in the workspace changes.

### US-5 Define a project agent type (priority P3)

A team wants an agent type with its own instructions, tools and limits.

Acceptance scenarios:

1. **Given** a new agent file in the project's agents folder, **when** the
   model next calls the `agent` tool, **then** the type is usable at once
   and listed in the tool's description.

### Edge cases

- The engine exits (the app closes or crashes) while background agents run:
  they are lost, and the next run is told each one stopped with no report.
- A foreground agent finishes in the same instant it is steered into the
  background: it reports to its call as before.
- A parent chat is deleted: its children stay and move to the top level.
- The person is typing in the parent, or the parent was woken five times in a
  row without the person, or the parent is archived: notes wait instead of
  waking it.
- An unknown agent type or model is refused.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | Agent types are Markdown files with frontmatter in a project folder and a user folder (project first). Frontmatter sets `name`, `description` (required; the parent reads it to choose), `tools` (allow-list, default the parent's), `model` (`inherit` or a model id), `thinking` (`inherit` or a level, clamped to the model), `maxTurns` (1–100), `access` (`read-only` or `full`, default `full`) and `isolation` (`worktree`, optional). The body is put in front of the task. | Given project and user files with the same name, when types load, then the project's wins and the fields are read and range-checked. |
| FR-2 | Three built-in types exist, each with a working style and a fixed report format: `general` (parent's tools, full access, inherited thinking, 80 turns; Done / Changes / Checks / Not done), `explore` (`read`, `bash`, `powershell`; read-only; low thinking; 40 turns; Answer / Evidence (`path:line`) / Not found), `plan` (`read`, `bash`, `powershell`; read-only; high thinking; 40 turns; Goal / Approach / Changes / Steps / Risks / Verification / Open questions). A file may replace a built-in type. | Given no files, then `explore` and `plan` are read-only and `general` is not; given a file named like a built-in, then it replaces it. |
| FR-3 | `access: read-only` is enforced before any approval prompt (also for a whole session started with a read-only type: [session roles](../session-roles-and-mail/spec.md) FR-13) and again right before the call runs: reading tools and the mesh tools pass; `write`, `edit` and unknown tools (connector tools included) are refused; shell commands pass only when every simple command is on a read-only allow-list without its writing options; redirection to a file, here-documents, command substitution in double quotes, PowerShell method calls, static .NET calls, property assignment and `&` are refused. So is whatever could turn an allowed command into another one: a program named by its path, an argument that reaches a command through a variable, a substitution or a file-name pattern (unless it stands after `--` or behind a literal prefix such as `./`), setting variables other than lowercase shell variables and a few such as `LC_ALL`, builtins that evaluate their operands, brace expansion, .NET conversions outside a short list of value types, and text the shells could read differently from the check (`$'…'`, here-strings, block comments, typographic quotes). A refusal tells the model to retry with a plainer command. | Given a read-only agent, when it calls `write` or a writing shell command, then the call is refused; reading commands pass. |
| FR-4 | The model can call a built-in `agent` tool with `description` (3–5 words, shown in the UI), `prompt` (the whole brief), optional `agentType` (default `general`), `model`, `isolation: "worktree"` and `background`. In the foreground the tool returns the child's final text as its result, followed by a line naming the agent's id (what `message_agent` takes); several calls in one assistant message run at the same time. The child's intermediate steps never enter the parent's context. | Given a foreground `agent` call, when the child finishes, then its report is the tool result and names the agent's id. |
| FR-5 | While a child runs, the person sees under the tool call its type and description, a live list of its tool calls, the tail of its latest text, counts (tool uses, tokens) and elapsed time, then the report (Markdown, collapsed when long). A reopened chat shows the finished child the same way, a background child with the report it sent later. A child that failed, was cancelled or was stopped keeps its steps and counts, live and reopened. A `message_agent` call shows the same view for that run. | Given a running child, when it calls tools, then rows appear live; given a reopened chat, then the finished child shows, also one that failed or reported in the background. |
| FR-6 | A child's usage is counted in the parent chat's cost: a foreground child's with its call (also when it failed or was cancelled), a background child's when it ends — in the running turn, or in the chat's next one. No run is counted twice when an agent is continued. | Given a turn with a subagent, then the turn's cost includes the child's usage; given a cancelled child, then its cost is still counted. |
| FR-7 | Limits: a subagent cannot start subagents; at most 4 children run at once per session (the person can set 1 – 8 in **Settings → Sandbox**: [session roles](../session-roles-and-mail/spec.md) FR-5) — a further foreground call waits for a free place (until it is cancelled, the session closes or the parent is steered), a background call is refused — and 16 are kept per session (plugin children have a limit of 16 of their own); `maxTurns` bounds each child; the Sandbox policy (environment, timeouts, denied patterns) applies to children. | Given a child, when it tries to start a subagent, then it is refused; given six foreground calls in one message, then all six run, four at a time. |
| FR-8 | A report longer than 16 KiB is cut, and the cut tells the parent to ask the agent for the rest with `message_agent` (or that the rest is lost, when the agent can no longer continue). A child that stops without a report (failure, timeout, turn limit) hands over the last text it wrote. | Given a long report, then the result is cut with that pointer; given a child that ran out of turns, then its last text reaches the parent. |
| FR-9 | With `background: true` the tool returns at once with `{agentId, status: "running"}`; the child outlives the parent's run and is cancelled when the session closes or switches. When it finishes, its report reaches a running parent in the same run, or wakes an idle parent for a new run. | Given a background child that finishes while the parent is idle, then the parent continues with its report. |
| FR-10 | A finished child is kept for a while; `message_agent {agentId, message}` resumes it with its own context. `agentId` is the id its result names; `a_<id>`, the node address and the agent's description (when only one agent has it) are taken too. A background child can be cancelled. | Given a finished child, when the parent messages it, then it answers with its earlier context. |
| FR-11 | A steer sent to the parent (by the person or by another session's `message_session`) while it waits on foreground agents ends every waiting call at once ("moved to the background, its report comes as a message"); the children run on with the session's lifetime and report like background agents. A child that finishes in the same instant reports to its call. Background agents' own notes release nothing. | Given a parent waiting on a foreground agent, when it is steered, then the call returns and the report arrives later; without a steer, the agent reports to its call. |
| FR-12 | When an engine opens a chat whose background agents never reported (the engine exited), each is reported once to the next run as stopped with no report (status `error`), without starting a run; the desktop shows those calls — the ones the engine lists — as stopped, not running, and an agent that finished while its report still waits for the next run as finished. When an engine exits while the app runs, its running agents show as stopped at once. | Given a saved chat with an unreported background call, when its engine starts, then the next run gets one error report and the UI shows the call stopped. |
| FR-13 | A subagent can work in its own Git worktree, cut from the last commit (the workspace's uncommitted changes are not in it): an unchanged worktree is removed, a changed one is kept and its path and branch reported — also when the child failed, was cancelled or timed out; the tool call shows a badge with the branch and actions **Open in…**, **Show changes** and **Discard**, which stays in sight under the turn when its steps fold, until the changes are applied or discarded. | Given a child with `isolation: "worktree"` that changed files, then the worktree is kept and the badge shows. |
| FR-14 | A chat can have a parent chat and an origin (`user` or `agent`). Deleting a parent keeps its children (they move to the top level); archiving a parent archives nothing else. A child started as a fork of the parent's conversation keeps the transcript lineage too. | Given a parent with children, when it is deleted, then the children remain at the top level. |
| FR-15 | The agent can call `start_session {title, prompt, context: fresh\|fork, worktree?}` (and, since [session roles](../session-roles-and-mail/spec.md) FR-1, FR-2, FR-9, `agentType`, `model`, `thinking` and `branch`: the session then runs as that role, on that model): it creates the child chat (parent link; the parent's workspace, model and memory flags), starts its engine in the background without showing it, sends the prompt and returns `{sessionId, title, status}`. | Given an allowed start, then a child chat exists, runs the prompt on its parent's model, and the call returns its id. |
| FR-16 | By default `start_session` starts nothing: it returns `{status: "proposed", proposalId}` and the tool row shows a card **Start session: {title}** with **Start** and **Dismiss**. The proposal is recorded: **Start** starts it once (a second click gets the same session), a dismissed proposal cannot be started, and the parent is told by a note — read with its next turn, without waking it — that the person started it (with the session's id) or dismissed it. **Settings → Sandbox** has **Start sessions without asking** (off by default). | Given the default setting, then the call is a proposal; given the setting on, then the session starts at once; given two clicks on **Start**, then one session exists. |
| FR-17 | The agent can call `message_session {sessionId, message}` (a running child reads it at its next step; an idle one starts a turn) and `list_sessions {}` (its children: id, title, status — `starting`, `running`, `waiting` for the person, `idle` — unread, start of the last report). | Given a child, when the parent messages it, then it receives the message; `list_sessions` lists it. |
| FR-18 | Nested sessions are one level deep: a child session does not get `start_session`. At most 8 children run or are starting at once per parent (the person can set 1 – 24 in **Settings → Sandbox**: [session roles](../session-roles-and-mail/spec.md) FR-5): a start beyond that is refused, from the tool and from the card alike. | Given a child session, then `start_session` is not offered to it; given ten starts at once, then eight sessions start. |
| FR-19 | When a child session finishes a turn, the parent gets a note `{sessionId, title, status, report}` with the first 400 characters of the report; a longer report says so in the note, and how to read the rest (`session_read`, or asking the session). A busy parent reads it at its next step; an idle parent is woken, after a 2 s window that gathers notes, by one automated turn shown as an event chip (**Session finished a turn · {title}**), never as a person's message. The shell decides the wake, once, whether or not a window shows the parent; windows only report drafts. No automated wake while a draft sits in that chat's composer in any window or a question is open there; at most 5 automated wakes in a row without a person's message, after which notes wait for the person's next prompt; an archived parent is never woken. Notes reach the parent in any window or none, or after its engine starts. | Given an idle parent, when a child finishes a turn, then the parent is woken once with the note shown as an event chip; given a draft in its composer, then it is woken only once the draft is sent or cleared. |
| FR-20 | The sidebar draws children under their parent's row with a guide line, not in the top-level list. Up to 3 children show (running and unread first, then most recent); a footer row such as "26 more nested sessions · 2 running · 22 unread" expands the rest. Child rows show the title (shimmering while running) and a status dot (amber = waiting for approval, blue = unread, red = stopped). A collapsed workspace group shows a roll-up over its chats and their nested sessions: a spinner if any runs, otherwise a dot if any is unread. A child's chat header shows "Parent title › Child title" (clicking the parent opens it; in a separate chat window it is plain text); the chat menu has **Open parent** and **Detach from parent**. | Given a parent with five children, then three show with a footer for the rest. |
| FR-21 | `start_session` with `worktree: true` creates the child's worktree before its engine starts — from the last commit, without the workspace's uncommitted changes — and uses it as the child's working folder. A start that fails afterwards removes the worktree again. | Given `worktree: true`, then the child runs in a new worktree. |
| FR-22 | A review dialog for kept worktrees (subagents and sessions) is reached from a subagent's kept worktree, a worktree session's toolbar chip, and **Settings → Sandbox**. It shows the changes since the commit the worktree started from. **Apply** puts exactly the reviewed changes into the folder the worktree was cut from, uncommitted (no merge, no commit), and removes the worktree; it changes nothing when the patch does not apply cleanly, when it fails part-way (what it touched is put back) or when the worktree changed after the dialog showed it. **Discard** removes the worktree. Both are disabled while an agent still works there; a session's engine is stopped first and restarted in the workspace afterwards. A folder that is still in use is removed later: the changes count as applied. Deleting a chat removes its worktree when it has no changes and keeps a changed one, telling the person. | Given a conflicting patch, when the person applies it, then the workspace is unchanged; given a file that cannot be replaced, then the workspace is as it was. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Progress updates stay small: about 4 per second, the last ~50 tool rows and ~4 KB of text. | A long-running child's updates never carry more than 50 rows and 4 KB of text. |
| NFR-2 | The plugin child-agent action API stays unchanged; plugin children and the `agent` tool share limits, cancellation, worktrees and usage roll-up. | Existing plugin child-agent tests keep passing. |
| NFR-3 | An agent file added or changed while the app runs is usable at once, without restarting the app. | Given a file added after the session started, when the model calls `agent` with it, then it runs. |
| NFR-4 | Only managed worktrees are ever removed; an apply never leaves the workspace half-changed. | Removing a folder that is not a managed worktree is refused; a conflicting apply changes nothing. |

## 5. Success criteria

- SC-1 A wide search delegated to a subagent adds only its report to the
  parent's context, not the child's tool output.
- SC-2 No nested session starts without the person's click unless they
  turned on **Start sessions without asking**.
- SC-3 A child's finished turn reaches its parent without the person
  relaying it, and automated wakes never pass for the person.
- SC-4 After an engine restart no subagent shows as running forever.
- SC-5 A worktree's changes reach the workspace only through **Apply**, and
  a failed apply leaves the workspace as it was.

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-07 — Q: Keep both the built-in `agent` and the plugin's `delegate_task`,
  or have the built-in replace the plugin? → A: Keep both; plugins wrap the
  shared runner (as built).
- 2026-11-07 — Q: Should `start_session` wait for the person's click by default?
  → A: Yes, a proposal the person clicks, with a setting to start without
  asking (as built).
- 2026-11-07 — Q: May a child session start its own sessions? → A: No, one level
  (as built; section 2.2).
- 2026-11-07 — Q: Should an idle parent be woken, or should notes only queue until
  the person's next message? → A: Wake, within the guard rails of FR-19
  (as built).
- 2026-11-07 — Q: Is `general` / `explore` / `plan` the right first set of
  built-in types? → A: Yes (as built).

## 7. History

- 2026-11-06 — Design written; phases 1–4 built (agent types and the `agent`
  tool; background agents and `message_agent`; nested sessions; worktree
  review).
- 2026-11-06 — Read-only access enforced for `explore` and `plan`; built-in
  types tuned. Steering releases a foreground wait.
- 2026-11-06 — Background-agent polling replaced by session mesh push events
  and wake requests; notes and the session tools move to the shell's mesh hub
  ([session mesh](../session-mesh-protocol/spec.md)).
- 2026-11-07 — Moved to the spec / plan / tasks format.
- 2026-11-07 — Open clarifications answered by the owner (accepted as proposed).
- 2026-11-11 — Read-only guard hardened after an audit: commands passed
  because the check read them differently from the shell, or trusted a
  variable, a path or another spelling of an option. FR-3 lists what else is
  refused.
- 2026-11-11 — The whole feature audited (code review and runs in the app)
  and its defects fixed in the engine, the worktree crate, the hub and the
  desktop (tasks phase 6). FR-4 to FR-8, FR-10, FR-12, FR-13 and FR-15 to
  FR-22 restated as built: `start_session`'s `workspace` and `model` were
  never built (a nested session follows its parent; follow-up T29), the
  running limit is per session, and the chip is labelled by what happened.
- 2026-11-11 — Runs in the app: a kept worktree's badge stays in sight when
  the turn folds (FR-13); the review dialog no longer overflows its width.
- 2026-11-11 — [Session roles, limits and mail](../session-roles-and-mail/spec.md) built the follow-ups T21, T22 and
  T29's `model` part: agent types from `agentDirs`, settable running limits,
  `start_session` with a role, model, thinking and branch, and read-only
  sessions. T30 (an operating-system write restriction) was dropped (ADR
  [0020](../../architecture/decisions/0020-read-only-agents-stay-guarded-by-the-command-checker.md)).
- 2026-11-12 — FR-19 amended by [shell-hosted engines](../shell-hosted-engines/spec.md):
  wakes are decided by the shell, with or without a window; windows report
  drafts.
