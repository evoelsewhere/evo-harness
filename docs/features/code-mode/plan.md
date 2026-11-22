# Code mode: the agent runs many tool calls from one piece of code — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-22 |
| ADD sections | [5.1 Crate layers](../../architecture/README.md#51-crate-layers), [8. Interfaces](../../architecture/README.md#8-interfaces), [9. Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts) |
| ADRs | [0025 Model-written code runs in an embedded interpreter](../../architecture/decisions/0025-model-written-code-runs-in-an-embedded-interpreter.md), [0010 The engine enforces the approval mode](../../architecture/decisions/0010-engine-enforces-approval-mode.md), [0014 Agent tools by layer](../../architecture/decisions/0014-agent-tools-by-layer.md) |

## 1. Context and constraints

What exists today:

- **Tool program capability**: `crates/evo-agent/src/tool_program.rs`.
  - While a tool executes, it can `list`, `start`, `poll`, `read_result`
    and `release_result` other tools in its effective tool set, through
    `ToolProgram::handle` (JSON).
  - Limits: 8 active calls (per invocation and per invocation tree), 16
    retained requests, 128 starts per invocation,
    1 MiB frames, a 16-level ancestor chain with no recursion, and a
    per-call `timeoutMs` (default 30 s, maximum 120 s).
  - Each nested call goes through `agent_loop::run_one_tool_call`, which
    runs the whole `before_tool_call` / `after_tool_call` chain.
  - `nested_parent()` tells a hook that the current call is nested, and
    which root tool started it (`root_name`).
  - Today only native plugins use it, through ABI action 30
    (`crates/evo-extensions/src/actions.rs`).
- **Journal of nested calls**: `crates/evo-harness/src/tool_program_journal.rs`
  stores nested calls in the session as custom entries
  (`evo.tool_program.child_begin.v1` / `child_phase.v1`).
- **Approval gate**: `crates/evo-engine/src/approval.rs` (ADR 0010).
  - It is the last link of `before_tool_call`.
  - Prompts go through the UI dialog mailbox with an `options` list. The
    panel (`apps/desktop/src/features/chat/approval-panel.tsx`) draws any
    options and maps Esc to the first rejecting option.
- **Hooks**: `crates/evo-hooks/src/run.rs` parses hook output into
  `Outcome`. `crates/evo-engine/src/hooks.rs` applies it. `PostToolUse`
  can only add feedback and context today; it cannot replace output.
- **Experimental gate**: `crates/evo-engine/src/experimental.rs` reads
  `EVO_EXPERIMENTAL` once per process, so it cannot change live.
- **Engine tools**: `crates/evo-engine/src/tools/` with one table of facts
  (`tools/info.rs`) and `register_all` (ADR 0014).

External constraint: QuickJS through `rquickjs` (ADR 0025). Its MSRV must
be at most the workspace's 1.89, and it must build on the three CI
platforms (task T20).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | `run_code`, the interpreter and every nested dispatch live in the engine. The shell only carries the setting. The webview only draws. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | `evo-agent` gains only generic tool program knobs (timeout limit, stop, state). `evo-hooks` gains one output field. JavaScript stays in `evo-engine`. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | ok | `set_code_mode` and `--code-mode` change together with the shell and the store (Phase 3). The approval payload gains an optional `origin` field that older UIs ignore. |
| Models come from models.dev; no hardcoded model facts | ok | `run_code` is offered when the model supports tool calls (FR-19). No model list. |
| Real engine in the app; settings and credentials reload live | ok | The code mode setting reaches running engines live (FR-2), like the approval mode. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Deviation covered by ADR 0025 (a new trust boundary). Every effect is a tool call through the same chain (FR-10). Project settings cannot enable code mode (FR-1). |
| Desktop UI structure and `DESIGN.md` | ok | The **Code** row and the Experimental group use existing components and store selectors. |

## 3. Design

### 3.1 Overview

```text
model ──run_code{code}──► engine: CodeTool (Sequential)
                            │ worker thread: QuickJS context (no std, no modules)
                            │   tools.x(args) / $`…` / read() …
                            ▼
                     host bridge (async) ──► ToolProgram::start_native
                                                │
                                   run_one_tool_call (nested)
                                   before_tool_call chain:
                                   read-only guard → self-kill → PreToolUse
                                   → plugin handlers → approval gate → checkpoints
                                                │
                                          tool executes
                                                │
                                   after_tool_call: PostToolUse (+updatedToolOutput)
                                                ▼
                     promise resolves in QuickJS ◄── result text / error
                            │
                            ▼
                  result: last value + console + toolsUsed + error
```

The work comes in four phases. **Phase 1** refactors the existing pieces
`run_code` depends on: hook output replacement for every tool, approval
prompts that know about calls from code, and tool program knobs for a
trusted host. **Phase 2** adds the interpreter and the tool. **Phase 3**
adds the setting and the protocol. **Phase 4** adds the desktop
transcript and the benchmark.

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| crate | `crates/evo-hooks/src/run.rs` | Parse `hookSpecificOutput.updatedToolOutput` into `Outcome::updated_output`, for `PostToolUse` only. | FR-21, FR-17 |
| engine | `crates/evo-engine/src/hooks.rs` | Apply `updated_output` in `after_tool_call`. A string replaces the text blocks and keeps the images. An object goes to the tool's registered output view (`register_output_view`). Anything else is ignored. | FR-21, FR-17 |
| crate | `crates/evo-agent/src/tool_program.rs` | Trusted-host knobs: `set_timeout_limit_ms` (only native code can raise the 120 s cap), `stop` with a reason plus `stopped()` (the person's **Stop code**), `is_active()`. | FR-11, FR-13, FR-14 |
| engine | `crates/evo-engine/src/approval.rs` | For a call whose `nested_parent().root_name` is `run_code`: `origin: "code"` in the payload and the options Allow · Allow <kind> for this chat · Stop code · Deny this call. **Stop code** stops the parent program and blocks the call. | FR-10, FR-11, FR-20 |
| desktop UI | `apps/desktop/src/features/chat/approval-panel.tsx` | Show **From code** for `approval.origin === "code"`. | FR-11 |
| engine | `crates/evo-engine/src/tools/code/` (`mod.rs`, `runtime.rs`, `bridge.rs`, `prelude.js`, `declarations.rs`) | `run_code`: the schema, a QuickJS context per call on a blocking worker, limits, the bridge to `ToolProgram`, the `$` / `read` / `edit` … prelude, the result shape, the output view for hooks, and declarations generated from tool schemas. | FR-3–FR-9, FR-12–FR-14, FR-17, FR-18, NFR-1–NFR-4 |
| engine | `crates/evo-engine/src/tools/info.rs`, `tools/mod.rs` | A table row for `run_code` (class `command`, not safe for read-only agents to skip checks: each call is checked on its own). Register it when code mode is on and the model supports tool calls. | FR-3, FR-19, FR-20 |
| engine | `crates/evo-engine/src/code_mode.rs` | Live state `CodeMode` (on/off), set by `--code-mode` and `set_code_mode`. Read when building the next request's tool list. | FR-1, FR-2, FR-3 |
| engine | `crates/evo-engine/src/rpc/protocol.rs`, `modes.rs`, `args.rs` | `--code-mode`, `set_code_mode {enabled}`, and `codeMode` in `get_state`. | FR-1, FR-2 |
| engine | `crates/evo-engine/src/child_agents.rs`, `session_roles.rs` | Children inherit code mode. An agent type with an explicit tool list without `run_code` does not get it. | FR-20 |
| desktop shell | `apps/desktop/src-tauri/src/engine.rs` | Read `experimental.codeMode` from the app database, pass `--code-mode` and broadcast `set_code_mode` on change. | FR-1, FR-2 |
| desktop UI | `apps/desktop/src/features/settings/` (Experimental group), `stores/ui-store.ts`, `stores/engine-store.ts` | The **Code mode** switch, the setting and the broadcast. | FR-1, FR-2 |
| desktop UI | `apps/desktop/src/features/chat/transcript.ts` and a `code-run` tool card | The **Code** row: description, status, duration, number of calls, the code, and the nested calls as their usual cards, from the journal entries. | FR-15, FR-16 |
| tests | `crates/evo-engine/tests/code_mode*.rs`, `scripts/bench/code-mode/` | The FR-10 check matrix, limits, faux-provider runs, and the SC-1 benchmark. | FR-10, SC-1 |

### 3.3 Interfaces

**Hook output (FR-21).** `PostToolUse` hook JSON:

```json
{ "hookSpecificOutput": { "hookEventName": "PostToolUse",
  "updatedToolOutput": "[hidden]" } }
```

Rust:

```rust
// evo-hooks
pub struct Outcome { /* … */ pub updated_output: Option<Value> }
// evo-engine hooks.rs — a tool with a structured response registers a view.
pub struct OutputView {
    pub view: fn(&AgentToolResult) -> Option<Value>,   // what the hook sees as tool_response
    pub apply: fn(&AgentToolResult, &Value) -> Option<AgentToolResult>, // None = malformed, ignored
}
pub fn register_output_view(tool: &'static str, view: OutputView);
```

For a tool without a view, `tool_response` stays the text, as today.

**Tool program knobs (FR-11, FR-13, FR-14).**

```rust
impl ToolProgram {
    /// Raise the per-call `timeoutMs` cap. Native callers only; plugins keep 120 s.
    pub fn set_timeout_limit_ms(&self, max: u64);
    /// End the program: no new starts, active children cancelled, `stopped()` returns the reason.
    pub fn stop(&self, reason: &str);
    pub fn stopped(&self) -> Option<String>;
}
```

**Approval payload (FR-11).** The `ui` object gains `approval.origin`:

```json
{ "kind": "confirm", "title": "Run a command?", "description": "npm test",
  "approval": { "kind": "command", "tool": "bash", "origin": "code" },
  "options": [ { "title": "Allow" },
               { "title": "Allow commands for this chat", "permission": "allow-chat" },
               { "title": "Stop code", "permission": "reject-stop" },
               { "title": "Deny this call", "permission": "reject" } ] }
```

Esc picks the first rejecting option, which is **Stop code**. Older UIs
ignore `origin` and still draw the four options.

**`run_code` (FR-4, FR-8).**

```json
{ "name": "run_code",
  "parameters": { "type": "object", "required": ["code"],
    "properties": { "code": { "type": "string" },
                    "description": { "type": "string" } } } }
```

Result: one text block with the last value, then `--- console ---` with the
printed lines, then `--- error ---` if the program failed. `details` holds
`{code, result, stdout, stderr, toolsUsed, error?, calls}`, which is also
the hook view (FR-17).

**JavaScript prelude (FR-5, FR-6).** `tools.<name>(args)`, `tools.list()`,
`tools.describe(name)`, `` $`…` `` (`.text()`, `.json()`, `.lines()`,
`.nothrow()`), `read`, `write`, `edit`, `grep`, `glob`, `console.log`,
`console.error`.

**Protocol (FR-1, FR-2).** Argument `--code-mode` (off when absent).
Command `{"type":"set_code_mode","id":n,"enabled":true}` returns
`{"ok":true}`. `get_state` gains `codeMode: bool`. An older engine answers
an unknown command, and the store logs it once (as for `set_approval_mode`).

### 3.4 Data and storage

- App setting `experimental.codeMode` (bool, default false) in the app
  database's settings table. No migration: settings are key/value.
- Nested calls are already journaled. `run_code`'s own result `details`
  goes in the transcript like any tool result.
- Interpreter state lives only for one call (FR-1 of ADR 0025, spec 2.2).

### 3.5 Behaviour

1. The model calls `run_code`. The tool is Sequential, which the tool
   program requires.
2. The tool gets `tool_program::current()`, raises the timeout cap, and
   starts a QuickJS runtime on a blocking worker. The runtime has a 64 MB
   memory limit and an interrupt handler that checks the deadline and
   cancellation.
3. Each binding call sends `(name, args)` to the async side. A semaphore of
   8 bounds concurrency (the tool program's active limit). The async side calls `start`, waits, reads and
   releases the result, then resolves or rejects the JS promise.
4. Nested calls run the normal chain. The approval gate sees
   `root_name == "run_code"` and asks with the code options. **Deny this
   call** blocks only that call. **Stop code** calls `program.stop()`, so
   the next binding call and the interpreter see `stopped()`.
5. The deadline clock pauses while a prompt for one of its calls is open.
   The gate reports "waiting" through the tool program's progress.
6. At the end, the last value is serialised, the output is cut to the
   usual limit, and `details` is filled.

### 3.6 Errors and edge cases

| Case | What happens |
| --- | --- |
| Syntax error | Compiled before anything runs. The error and its line come back, with `toolsUsed: []`. |
| Throw halfway | The error and its line, with the list of finished calls. Their effects stay. |
| Time, memory or call limit | The interrupt or memory limit stops the program. The error names the limit (FR-13). |
| Stop turn | The cancellation token interrupts QuickJS and cancels children (FR-14). |
| Excluded tool | The bridge refuses it before `start`: "<name> cannot be called from code" (FR-12). |
| Engine exit mid-run | The tool result is lost like any in-flight call. The UI shows the row as interrupted from the journal. |
| Large output | Cut to the usual tool output limit, with a note. |

### 3.7 Security and trust

- The program's input is model output, which can be shaped by untrusted
  tool output. It is never trusted.
- The interpreter has no std, no modules, no `eval` of host strings, and no
  host functions except the bridge (ADR 0025).
- Every effect is a tool call through the whole chain: guards, hooks,
  plugin handlers, the approval gate (with real arguments), checkpoints,
  the sandbox for shell commands, and connector permissions in the shell.
- **Stop code** and **Deny this call** come only from the person's answer
  in the UI dialog.
- `set_timeout_limit_ms` and `stop` are native-only. The plugin JSON
  surface cannot reach them.
- The setting is person-level. Project `.evo/settings.json` has no key for
  it (FR-1).

### 3.8 Configuration

| Key | Type | Default | Where | Applies |
| --- | --- | --- | --- | --- |
| `experimental.codeMode` | bool | false | app database settings | live (`set_code_mode`), from the next model request |
| `--code-mode` | flag | absent | engine argument | at start |

UI: **Settings → Experimental → Code mode**.

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| `updatedToolOutput` only for `run_code` | Owner's answer to Q4: the hook path is refactored once for all tools. |
| One prompt per program, showing the code | Owner's answer: one prompt per call, with real arguments. |
| Keep the 120 s nested timeout | `npm test` and builds would fail from code (FR-13). The cap stays for plugins. |
| Stop code via the result's `terminate` flag | It would also end the turn, because `terminate` merges into the outer result. A separate `stop` ends only the program. |
| Deno subprocess, V8, Boa | See ADR 0025. |

## 5. As-built notes

- 2026-11-22 — Phase 1 (refactors) implemented first, at the owner's
  request, before the interpreter (T1–T9).
  - The hook output path is split into pure functions
    (`replaced_output`, `post_tool_result`, `tool_response`) so it is
    tested without running hook processes.
  - The approval gate takes the origin lookup as a parameter (`gate`);
    `before_tool_call` passes `origin`, which reads
    `tool_program::nested_parent()`. Unit tests use a fixed `Origin::Code`.
    Stopping the real program from a prompt is covered by the T28 matrix.
  - The stop answer's reason reads "The user stopped the code at this
    <kind>." to match the gate's existing "The user declined this <kind>."
  - Concurrency is 8, not 16: that is the tool program's existing active
    limit (spec FR-7 updated).
- 2026-11-22 — Phase 2 (interpreter and `run_code`, T20–T29).
  - `rquickjs` 0.14 with only `std`. `Context::full` has no `require`,
    `process`, `fetch`, timers or module loader. Programs run with
    `eval_promise` (top-level await), whose completion is `{ value }`.
  - The interpreter runs on `spawn_blocking`; the bridge is a Tokio task.
    Tool calls cross on an unbounded channel. Replies come back on a
    `std::sync::mpsc` channel the worker waits on in 20 ms ticks, so time and
    cancellation are checked even while no JavaScript runs.
  - `ToolProgram` gained `wait(id)` (a `Notify`, no polling),
    `hold_clock()` / `paused()` (the approval gate holds the clock of the
    program it asks for) and `cancellation()`.
  - A nested call's own timeout is 24 h. The interpreter's limit, which
    leaves out prompt time, ends every call with the program. With the
    program's 10 minutes as the call timeout, a call waiting at a prompt was
    cancelled.
  - `ToolError` carries the call site's frames, taken while the call is
    still synchronous. QuickJS keeps no frames across `await`, so an uncaught
    tool error would otherwise have no line.
  - Once the person picks Stop code, the result's error is their reason,
    even when the program caught the refused call and failed later.
  - The `run_code` row is `read` and safe for read-only agents (each call is
    checked on its own). `NOT_FROM_CODE` and `NOT_FROM_CODE_PREFIXES` in the
    tool table list what programs may not call.
  - Shell results merge output and errors, so `.nothrow()` gives
    `{exitCode, output, ok}` (spec FR-6 updated). `grep` / `glob` need the
    `grep` / `find` tools, which chats do not register by default.
  - Children need no change (T29): a subagent's tools are the parent's
    active tools filtered by its agent type, wrapped by its guard.
  - The engine registers `run_code` only in Phase 3 (setting and protocol).
    Until then it is built in tests only.
- 2026-11-22 — Phase 3 (setting and protocol, T30–T33).
  - `code_mode.rs` keeps the state in an `AtomicBool`; `register_all`
    appends `run_code` (declaring the tools built before it) when it is on,
    and `set_code_mode` calls `code_mode::apply`, which adds or removes the
    tool in the harness registry and the active set (`set_tools`), so the
    next request follows. `settings.json#defaultTools` does not filter it:
    code mode is its own switch.
  - The wire contract for the command lives in
    `crates/evo-engine/src/rpc/protocol.rs` (as for `set_approval_mode`), not
    in `evo-protocol`.
  - The shell passes `--code-mode` to every chat engine it starts, nested
    sessions and scheduled task chats included (they are chats; the shell
    cannot tell a scheduled one apart there). The supervisor sends
    `set_code_mode` to set-up engines and keeps it as pending for one still
    starting, like the approval mode. The plugin-AI and memory-index engines
    never get it.
  - The Experimental page reads and saves `experimental.codeMode` itself
    (like Computer apps); `ui-store.ts` only gains the section id and
    `engine-store.ts` is unchanged.
- 2026-11-22 — Phase 4 (transcript and benchmark, T40–T42).
  - The Code row reads its calls from the `run_code` result, not from the
    tool program journal: the journal's live events carry identities and
    phases only, and arguments and results sit in chunked bodies behind
    three more RPC commands. The bridge keeps a bounded copy of each call
    instead (string arguments cut at 1,000 characters, output at 4,000,
    details up to 8 KB, at most 256 KB per program; past that only names and
    outcomes), so the row comes back with the transcript after a restart.
  - While a program runs, the row lists each call's latest phase from the
    tool's progress updates (`details.toolProgram`), kept by the transcript
    reducer as `details.live`.
  - `code-run-model.ts` turns each call into an ordinary `ToolPart`, so
    `Tool` draws it as its usual card (terminal, diff, file). `Tool` returns
    `CodeRun` for `run_code`.
  - The benchmark is `scripts/bench/code-mode/` (bun): a generated sample
    project and 12 tasks with checks, run against the real engine in RPC
    mode. Its run is the owner's call, since it spends real tokens.
- 2026-11-22 — Benchmark rounds (tasks.md SC-1).
  - The tool description is short (about 1,600 characters) with one-line
    signatures (`tools.read({path, offset?, limit?})`) instead of typed
    declarations, which added about 5,500 tokens to every request. A prompt
    rule makes `run_code` the default for work that needs more than one call.
  - The system prompt is built at session start and on reload; turning code
    mode on live adds the tool at once and its prompt rule at the next
    reload.
  - A program may `return` at the top level: on a "return not in a
    function" syntax error it runs again as the body of an async function on
    the same lines.
  - `files(dir, ext?)` lists files with `find` (bash) or `Get-ChildItem`
    (PowerShell), so programs do not depend on `rg`.
  - `EditCheck` (`tools/lsp.rs`) skips the diagnostics wait for calls from
    code (FR-22): 40 edits had taken 420 s.
  - `scripts/bench/code-mode/tasks-b.ts` adds set B, multi-step tasks;
    set A sits near the two-turn floor with code mode off.
- 2026-11-22 — `read(path)` reads the whole file (FR-6). The read tool cuts a
  file at one page and ends the text with a "Use offset=N to continue"
  notice; its `details.truncation` carries the same fact, but the prelude
  returned only the text, so a program could save a cut file. The prelude
  now reads page by page (offset from the notice), joins the pages without
  the notices, and rejects with a `ToolError` for a line over the page
  limit or a file over 64 pages. A call with `offset` or `limit` is one call,
  as the tool answers. `.nothrow()` reports `truncated` from the same
  `details`. Each page is one call against the 128-call cap (FR-13).
- 2026-11-23 — Typed declarations (FR-18, T44). `declarations.rs` turns
  each callable tool's JSON Schema into a method of one
  `declare const tools: { ... }` block in the `run_code` description, in a
  `ts` fence after the prose. Mapping: `string` / `number` (and `integer`) /
  `boolean` / `null`; `enum` and `const` as literal unions; `type` lists,
  `anyOf` and `oneOf` as unions, `allOf` as an intersection; arrays as `T[]`
  and `items` / `prefixItems` lists as tuples; objects as type literals with
  `?` on fields not in `required` and `[key: string]: T` for
  `additionalProperties`; `$ref` to `#/$defs/…` or `#/definitions/…` resolved
  against the tool's schema; `nullable` adds `| null`; anything else, and
  nesting past six levels, is `unknown`. A tool with no fields takes `()`,
  one with only optional fields `args?`. Doc comments carry only the first
  sentence of each description (at most 160 characters), since the direct
  tools already carry the full text; an object whose fields have no docs
  stays on one line. The previous attempt with full descriptions cost about
  5,500 tokens per request. With first sentences, the engine's 16 callable
  built-in tools take 5,944 bytes (about 1,500 tokens; the one-line
  signatures took 752 bytes); `declarations::tests` holds them to 8,000
  bytes. Connector tools add their own share. Not yet measured with the
  SC-1 benchmark (it spends real tokens; the owner starts it).
