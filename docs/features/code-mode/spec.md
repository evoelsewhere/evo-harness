# Code mode: the agent runs many tool calls from one piece of code — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | engine, protocol, desktop UI, hooks |
| Created | 2026-11-22 |
| Updated | 2026-11-22 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [ADR 0025](../../architecture/decisions/0025-model-written-code-runs-in-an-embedded-interpreter.md), [Approval modes](../approval-modes/spec.md), [Agent tools by layer](../agent-tools-layers/spec.md), [Customize: hooks](../customize-skills-plugins-connectors/spec.md), [Source control: turn checkpoints](../source-control/spec.md), [Subagents and nested sessions](../subagents-and-nested-sessions/spec.md) |

## 1. Summary

Today the agent calls tools one at a time. For each call it waits for the
result, reads all of it, and only then decides on the next call. A change
repeated across twenty files therefore takes twenty model turns, and a
search that returns 5,000 lines puts all 5,000 into the context. Code mode
gives the agent one more tool, `run_code`. With it, the agent writes a short
JavaScript program that calls tools, runs them in parallel, passes one
result into the next call and returns only what the agent needs to read.
Every call the program makes is checked exactly like a direct call: same
approvals, hooks, guards, sandbox and checkpoints. The program itself can do
nothing except call tools. Code mode is experimental and off until the
person turns it on.

## 2. Scope

### 2.1 Goals

- One extra tool, `run_code`, offered next to the agent's usual tools when
  code mode is on. The agent picks whichever fits the step.
- A small JavaScript API for the agent: every allowed tool as a function,
  plus shortcuts for shell commands and common file work.
- No program can reach anything except through tool calls: no files,
  network, environment variables or processes of its own.
- Each call from a program passes the same checks as a direct call, using
  that call's real arguments at the moment it runs.
- The transcript shows what a program did: the code, and each call as its
  usual tool card.
- Off by default, switched on by the person, never by a project.

### 2.2 Non-goals

- Replacing the direct tools or a code-only mode (resolved 2026-11-22,
  section 6.2).
- Code that keeps running after its turn, or past the time limit (FR-13).
  Long-running background code is a later spec.
- Code mode in plugin AI runs and memory indexing. Those keep the direct
  tools only. Subagents, nested sessions and scheduled task chats do get
  code mode (FR-20).
- Calls from code to tools that need the person or a native app surface:
  `ask_user`, browser pages, computer app control, agent terminals, and
  session and subagent tools (FR-12).
- Importing packages or modules, TypeScript type checking, or Node and
  Deno style system APIs.
- Keeping variables from one `run_code` call to the next. Each program
  starts fresh (6.1 Q3).
- Changing what a direct tool call does, how it is classified or how it is
  shown.

## 3. User scenarios

### US-1 Repeat a change across many files (priority P1)

A person asks the agent to rename an option in every file that uses it.
They want this done in a few steps instead of one turn per file, without
giving up any approval they would normally see.

Acceptance scenarios:

1. **Given** code mode on and 20 files that use `retryPolicy`, **when** the
   person asks for the rename, **then** the agent finds the files and edits
   them from one program, and the transcript shows 20 edit cards inside one
   **Code** row.
2. **Given** the mode "Ask every time", **when** the program edits the
   first file, **then** the approval panel asks for that edit as it would
   for a direct edit, labelled as coming from code.
3. **Given** the person picks "Allow edits for this chat" on the first
   prompt, **when** the program edits the other 19 files, **then** no more
   edit prompts appear.

### US-2 Search, then read only what matters (priority P1)

A person asks which links in the project's Markdown files point to local
files that don't exist. They want an answer without the agent reading
every file into the chat.

Acceptance scenarios:

1. **Given** code mode on, **when** the agent answers the question, **then**
   it reads the files from a program, checks the links in code, and gets
   back only the broken links.
2. **Given** a search with very large output, **when** a program filters
   it, **then** only the program's result reaches the agent, never the
   full search output.

### US-3 Run independent work at the same time (priority P2)

A person asks the agent to run the linter, the type checker and the tests
and summarise what failed.

Acceptance scenarios:

1. **Given** code mode on and the mode "Approve for me", **when** the agent
   runs the three commands from one program, **then** they run at the same
   time and the run takes about as long as the slowest one.

### US-4 Stay in control of every call (priority P1)

A person relies on approvals, project hooks, the sandbox and read-only
agent types. They want code mode to take nothing away from them.

Acceptance scenarios:

1. **Given** a project `PreToolUse` hook that blocks `git push`, **when** a
   program runs `` $`git push` ``, **then** the hook blocks that call, the
   call fails inside the program, and the program can go on.
2. **Given** a prompt for a call from code, **when** the person picks
   **Deny this call**, **then** only that call fails ("The user declined
   this command.") and the program continues.
3. **Given** a prompt for a call from code, **when** the person picks
   **Stop code** or presses Esc, **then** the program stops. Calls that
   already finished keep their effects, and the result tells the agent
   which calls ran.
4. **Given** a program that tries to read a file, open a network
   connection or start a process without a tool, **when** it runs,
   **then** it fails because the program has no such ability.
5. **Given** a hook that blocks `run_code` itself, **when** the agent tries
   to run code, **then** nothing runs and the agent falls back to direct
   tools.

### US-5 See what the code did (priority P2)

A person reviewing the chat wants to know exactly what a program changed
and ran, also after restarting the app.

Acceptance scenarios:

1. **Given** a finished program, **when** the person expands its **Code**
   row, **then** they see the code, and each call it made as the usual tool
   card in order: diffs for edits, output for commands.
2. **Given** the app restarted, **when** the person opens the chat,
   **then** the **Code** rows and their calls look the same as before.
3. **Given** a turn whose program edited files, **when** the person uses
   **Revert this turn**, **then** those edits are reverted too.

### US-6 Turn code mode on and off (priority P1)

A person wants to try code mode and to go back to today's behaviour at any
time.

Acceptance scenarios:

1. **Given** a fresh install, **when** the person starts a chat, **then**
   the agent has no `run_code` tool.
2. **Given** the person turns code mode on in Settings, **when** they send
   the next message in any chat, **then** the agent has `run_code`. No
   restart is needed.
3. **Given** a chat with earlier code runs, **when** the person turns code
   mode off and continues the chat, **then** the earlier runs stay readable
   and the agent goes on with direct tools.
4. **Given** a cloned repository whose project settings try to turn code
   mode on, **when** a chat runs in it, **then** code mode stays as the
   person set it.

### US-7 Subagents and nested sessions use code too (priority P2)

A person whose chat hands work to a subagent or a nested session wants that
agent to work as efficiently as the main one, under the same control.

Acceptance scenarios:

1. **Given** code mode on, **when** the agent starts a subagent or a nested
   session, **then** that agent has `run_code` as well.
2. **Given** the mode "Ask every time", **when** a subagent's program wants
   to run a command, **then** the prompt appears in the parent chat's panel
   as "<agent name> wants to run …", labelled **From code**.
3. **Given** a subagent that ran code, **when** the person opens its
   transcript, **then** they see its **Code** rows and their calls, as in a
   main chat.

### Edge cases

- **Agent type with an explicit tool list:** if the list leaves out
  `run_code`, that agent gets no `run_code`. Its programs can only call
  tools on its list.
- **Syntax error:** nothing runs. The result gives the error and its line.
- **Error thrown halfway:** calls that already finished keep their effects.
  The result gives the error, its line and the list of calls that ran.
- **Infinite loop or runaway memory:** stopped at the limit (FR-13). The
  chat and the engine keep working.
- **Two calls edit the same file at the same time:** they run one after the
  other, as two direct edits to one file do today.
- **A call to an excluded or unknown tool:** that call fails with a clear
  reason. The program can catch the error and go on.
- **A read-only agent type runs code:** each call is checked by the
  read-only guard, so only reads succeed.
- **A connector (MCP) tool called from code:** the app gates it with the
  same connector permissions as a direct call.
- **The turn is stopped, or the engine exits, during a run:** the program
  and its in-flight calls stop. The **Code** row shows it was interrupted,
  with the calls that finished.
- **Very large result:** cut to the usual tool output limit, with a note
  saying so (FR-13).
- **A model without tool calls:** gets no tools at all, as today.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | Code mode is a person-level setting, off by default, changed in **Settings** (6.1 Q1). Project settings, repository files and plugins cannot turn it on. An engine without a UI has it off unless started with code mode on. | Fresh install: no `run_code`. A project setting that turns it on is ignored. |
| FR-2 | A change to the setting applies from the next model request in every running chat, with no restart (as for other settings). | Turn it on while a chat is idle; the next message offers `run_code`. |
| FR-3 | With code mode on, the agent is offered one extra tool, `run_code`, next to its usual tools, which stay unchanged. With it off, the tool list and the instructions are exactly as today. | Snapshot of the tool list and system prompt with the mode off equals today's. |
| FR-4 | `run_code` takes `code` (JavaScript source, required) and `description` (one line saying what the program is for, shown in the transcript). | Schema test. |
| FR-5 | In a program, each allowed tool is an async function `tools.<name>(args)`. It resolves to the tool's text output and rejects with an `Error` holding the tool's error text when the call fails, is blocked or is declined. `tools.list()` gives the names and descriptions of the allowed tools, and `tools.describe(name)` gives one tool's parameters. | A program calling `tools.read({path})` gets the file text; a blocked call rejects and can be caught. |
| FR-6 | Shortcuts: `` $`cmd` `` runs a command through the chat's shell tool (`bash` or `powershell`) and fails on a non-zero exit. Its result offers `.text()`, `.json()`, `.lines()` and `.nothrow()`, where `.nothrow()` gives `{exitCode, output, ok}` instead of failing (the shell tools return standard output and errors together). `` $.timeout(seconds)`cmd` `` sets the shell tool's timeout for a long command. Values put into the template are quoted as single arguments. `read(path)`, `write(path, content)` and `edit(path, oldText, newText)` and `files(dir, ext?)` (relative paths, through one shell call) are also available; `grep(pattern, options)` and `glob(pattern)` call the `grep` and `find` tools when the chat has them, and say to use `$` otherwise. Each shortcut is exactly one ordinary tool call. | Each shortcut shows as one card of the matching tool. A value with spaces passed to `$` reaches the command as one argument. |
| FR-7 | Calls the program starts together run at the same time, up to 8 at once; further calls wait their turn. Calls that the tool itself says must not overlap still run one at a time. | Three 2-second commands with `Promise.all` finish in under 3 seconds. |
| FR-8 | The program's result is the value of its last expression (strings as is, other values as JSON), followed by what it printed with `console.log` / `console.error`, the names of the tools it used and, when it failed, the error and its line. Images and files returned by its calls are attached to the result. | A program ending in `({count: 3})` returns `{"count":3}` with its printed lines and `toolsUsed`. |
| FR-9 | A program has no way to read files, use the network, read environment variables, start processes or load modules, except through tool calls. | Programs that try each of these fail. Unit tests in the interpreter. |
| FR-10 | Every call from a program passes the same checks as a direct call, in the same order and under the tool's own name: read-only guard, self-kill guard, `PreToolUse` hooks, plugin handlers, approval mode (classified with the call's real arguments), chat grants, turn checkpoints. Shell commands keep the sandbox settings. Connector tools keep the app's connector permissions. | Test matrix: for each mode and each check, a call from code gives the same outcome as the same direct call. |
| FR-11 | A prompt for a call from code looks like the prompt for the direct call, labelled **From code**, with the options Allow, Allow <kind> for this chat, **Deny this call** and **Stop code** (Esc). Deny this call fails only that call with the usual declined message. Stop code ends the program, keeps the effects of finished calls and tells the agent which calls ran. Time spent waiting on a prompt does not count toward the time limit. | Deny one edit: the program continues and the next edit runs. Esc: the program stops and the result lists the finished calls. |
| FR-12 | A program can call every tool of the chat except `run_code`, `ask_user`, browser page tools, computer app control tools, agent terminal tools, and session and subagent tools. `tools.list()` leaves these out, and calling one fails with "<name> cannot be called from code". | Calling `ask_user` from code fails with that message. |
| FR-13 | Limits per run: 10 minutes of running time, 128 tool calls, 64 MB of program memory. A shell command from code follows the shell tool's own timeout, not a shorter one. The result given to the agent is cut to the usual tool output limit, with a note. Reaching a limit stops the program with an error that names the limit. | One test per limit; a 5-minute `npm test` from code completes. |
| FR-14 | Stopping the turn stops the program and cancels its in-flight calls. Nothing it started keeps running after the turn ends. | Stop during a program running `sleep 60`; the command is gone and the turn ends. |
| FR-15 | The transcript shows each `run_code` call as one collapsible **Code** row: description, status (running, done, failed, stopped, interrupted), duration and number of calls. Expanded, it shows the code and each call as its usual tool card, in start order. The row looks the same after the chat is reloaded or the app restarts. | Reload a chat with a finished program; the row and its cards match. |
| FR-16 | Edits made from code are part of the turn's checkpoint, and **Revert this turn** reverts them. | Program edits two files; Revert this turn restores both. |
| FR-17 | `PreToolUse` and `PostToolUse` hooks can match `run_code`. `PreToolUse` gets the code in `tool_input.code` and can block it or ask. `PostToolUse` gets `{code, result, stdout, stderr, toolsUsed, error}` and can return a changed copy in `hookSpecificOutput.updatedToolOutput` (for example to hide a secret). A malformed copy is ignored and the original output is used. | A hook that replaces `result` changes what the agent sees; a malformed one leaves the output unchanged. |
| FR-22 | An `edit` or `write` from a program does not wait for the language server's diagnostics after each change (a program edits many files in a row; each wait can last the diagnostics timeout). The program runs its own check. Direct edits keep the check. | A program that edits 40 files finishes in seconds, not minutes; a direct edit still reports the language server's errors. |
| FR-21 | For every tool, not only `run_code`, a `PostToolUse` hook can replace what the agent sees with `hookSpecificOutput.updatedToolOutput`. A string replaces the tool's text output and keeps its images. A tool with a structured response (`run_code`, FR-17) also accepts an object of that shape. Anything else is ignored. `PostToolUseFailure` cannot replace output. Calls from code get the same treatment as direct calls (6.1 Q4). | A `PostToolUse` hook on `bash` that returns `updatedToolOutput: "[hidden]"` makes the agent see `[hidden]`. A number is ignored. The same hook on a `bash` call from code hides that call's output. |
| FR-18 | With code mode on, the agent's instructions say when to use `run_code` (many related calls, filtering large output, independent work in parallel) and when not to (a single call, anything that needs the person). They also include typed declarations of the built-in tools' functions. | Prompt snapshot with the mode on. |
| FR-19 | Code mode is offered to every model that supports tool calls according to models.dev. There is no list of allowed models. | A models.dev model with tool calls gets `run_code` when the mode is on. |
| FR-20 | When code mode is on, subagents and nested sessions get `run_code` too, with the same API, limits and checks (FR-5 to FR-17). The exception is an agent type whose explicit tool list leaves `run_code` out. A program in such an agent can call only the tools that agent has. Its prompts ask in the parent chat's panel, labelled with the agent's name and **From code**. Its **Code** rows show in its own transcript. Starting sessions or subagents from code stays excluded (FR-12). | A subagent in "Ask every time" runs a command from code: the parent's panel shows "<agent name> wants to run …" labelled **From code**. A read-only agent type's program can only read. An agent type that lists tools without `run_code` has no `run_code`. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | A program starts running within 50 ms of the call, with nothing downloaded, installed or started as a separate process. | Benchmark test. |
| NFR-2 | Works the same on Windows, macOS and Linux, offline, with no extra install. | CI runs the code-mode tests on all three. |
| NFR-3 | A call from code adds under 5 ms over the same direct call. | Benchmark test. |
| NFR-4 | A program that loops forever, allocates without bound or throws inside a callback is stopped or reported without crashing the engine, blocking other tool calls of the turn, or affecting other chats. | Stress tests with a busy loop, an allocation loop and an unhandled rejection. |
| NFR-5 | With code mode off there is no change in behaviour, tool list, prompt or performance. | FR-3 snapshot; the existing test suite passes unchanged. |

## 5. Success criteria

- SC-1 On a fixed set of multi-step coding tasks (benchmark set B), chats
  with code mode on need at least 10% fewer model turns and no more input
  tokens than with it off, and solve at least as many tasks (6.2, Q5 and
  its revision).
- SC-2 No call from code runs without the approval, hook or guard decision
  it would have had as a direct call. The FR-10 matrix shows zero
  differences.
- SC-3 A person can tell from the transcript alone every file a program
  changed and every command it ran, also after a restart.
- SC-4 Turning code mode off gives exactly today's behaviour, and chats
  with earlier code runs reopen and continue.

## 6. Clarifications

### 6.1 Open

n/a — none open. All five questions were answered on 2026-11-22
(section 6.2).

### 6.2 Resolved

- 2026-11-22 — Q: Should code mode replace the direct tools or sit next to
  them? → A: Next to them. The agent picks per step (FR-3, section 2.2).
- 2026-11-22 — Q: In "Ask every time", is there one prompt per program
  (showing the code) or one prompt per call? → A: One per call that needs
  approval, using its real arguments, labelled **From code** (FR-10,
  FR-11).
- 2026-11-22 — Q: Does the program run in a separate process (a
  downloadable runtime) or inside Evo, with no install? → A: Inside Evo,
  with no install and no abilities of its own (FR-9, NFR-1, NFR-2;
  [ADR 0025](../../architecture/decisions/0025-model-written-code-runs-in-an-embedded-interpreter.md)).
- 2026-11-22 — Q2: Do subagents and nested sessions get `run_code` when
  code mode is on? → A: Yes, with the same API, limits and checks. An agent
  type's explicit tool list can leave it out (FR-20, US-7).
- 2026-11-22 — Q1: Where does the switch live? → A: A new
  **Settings → Experimental** group with one switch, **Code mode**. The
  setting reaches running engines live, like the approval mode, instead of
  the start-time-only experimental variable (FR-1, FR-2).
- 2026-11-22 — Q3: Do variables persist between `run_code` calls? → A: No.
  Each program starts fresh (section 2.2).
- 2026-11-22 — Q4: Does `updatedToolOutput` work for every tool or only
  for `run_code`? → A: Every tool. The hook output path is refactored once
  for all tools, and `run_code` adds its structured shape on top (FR-21,
  FR-17).
- 2026-11-22 — Q5: Are the SC-1 targets right? → A: Yes: 40% fewer turns
  and 20% fewer input tokens, measured on 12 tasks from this repository's
  history with one fixed model (SC-1).
- 2026-11-22 — Q5 revised after six benchmark rounds: the targets were not
  reachable. Each task needs at least two turns (one call, one answer), the
  model already batches through the shell with code mode off (about 3.7
  turns per task), and it tends to split a task into a program that looks
  and one that acts. → A (owner): SC-1 is at least 10% fewer turns, no more
  input tokens and at least as many tasks solved, on set B (SC-1). The value
  of code mode is control (each call approved, Stop code), not mainly
  savings.

## 7. History

- 2026-11-22 — Draft.
- 2026-11-22 — Q2 answered: subagents and nested sessions get code mode
  (FR-20, US-7).
- 2026-11-22 — Q1, Q3, Q4 and Q5 answered. FR-21 added: `updatedToolOutput`
  works for every tool. The owner accepted the spec and asked to implement
  the refactors it needs (Gate A).
- 2026-11-22 — FR-7 lowered from 16 to 8 calls at once: the tool program's
  existing limit for active calls, per program and per tree, is 8. Phase 1
  refactors implemented; status In progress.
- 2026-11-22 — FR-6 as built: `.nothrow()` gives `{exitCode, output, ok}`,
  because the shell tools return output and errors together; `$.timeout`
  added for long commands; `grep` / `glob` need the `grep` / `find` tools,
  which chats do not register by default.
- 2026-11-22 — SC-1 lowered by the owner to at least 10% fewer turns, no more
  input tokens and as many tasks solved, on set B (6.2). Round 6 meets it.
- 2026-11-22 — Benchmark rounds 2–6 (tasks.md SC-1): best −11% turns and
  −6% input tokens; not met. As built from them: FR-22 (no diagnostics wait
  for edits from code), a top-level `return` is the result (FR-8),
  `files(dir, ext?)` lists files through the shell (FR-6), and a shorter
  description with one-line tool signatures and a worked example (FR-18).
- 2026-11-22 — First benchmark run (SC-1): not met. The model never chose
  `run_code` on the 12 tasks; turns and tokens did not drop (tasks.md).
- 2026-11-22 — Scheduled task chats follow the setting like any chat (the
  shell starts them as chats); only plugin AI and memory indexing are left
  out (section 2.2).
