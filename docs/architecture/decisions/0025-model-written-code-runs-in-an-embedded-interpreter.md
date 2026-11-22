# 0025. Model-written code runs in an embedded interpreter in the engine, with no abilities except checked tool calls

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-22 |
| ADD sections | [5.1 Crate layers](../README.md#51-crate-layers), [9. Cross-cutting concepts](../README.md#9-cross-cutting-concepts) |
| Related | [Code mode spec](../../features/code-mode/spec.md), [0010](0010-engine-enforces-approval-mode.md), [0014](0014-agent-tools-by-layer.md), [0022](0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md) |

## Context

[Code mode](../../features/code-mode/spec.md) lets the agent write a
JavaScript program that makes many tool calls in one step. That is a new
trust boundary: Evo will run code written by a model, and the model's input
includes untrusted tool output, web pages and connector results.

Facts at the time of the decision:

- The engine already runs nested tool calls for a running tool (the tool
  program capability, used today by native plugins). Each nested call goes
  through the same `before_tool_call` chain as a direct call: read-only
  guard, self-kill guard, `PreToolUse` hooks, plugin handlers, the approval
  gate ([0010](0010-engine-enforces-approval-mode.md)) and turn checkpoints.
  Nested calls are already journaled in the session.
- Shell commands are sandboxed according to the person's settings. File,
  web and connector tools have their own checks.
- Deno is available as a plugin runtime ([0022](0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md)).
  The shell downloads it on demand, the engine does not ship it, and each
  run would start a process.
- The engine runs in many places: chats, nested sessions, scheduled runs,
  the memory indexer and plugin AI. Some of these are headless or offline.

## Decision

Model-written code runs **inside the engine process, in an embedded
JavaScript interpreter (QuickJS through `rquickjs`) that has no standard
library for files, network, environment, processes or modules**. The only
host functions given to a program are bindings to the tool program
capability. As a result, every effect a program has is an ordinary tool
call that passes the full `before_tool_call` chain, with that call's real
arguments.

Specifics:

- One interpreter context per `run_code` call, thrown away when the call
  ends. No state is kept between calls.
- Hard limits are enforced by the host, not by the program: a memory limit
  on the runtime, an interrupt handler for time and cancellation, a cap on
  tool calls, and output truncation.
- The interpreter runs on a blocking worker, so a busy loop cannot starve
  the engine's async runtime. Tool calls are awaited on the async runtime
  and handed back to the interpreter as resolved promises.
- The tool that hosts the interpreter lives with the engine's tools
  ([0014](0014-agent-tools-by-layer.md)). `evo-agent` keeps the
  provider-agnostic tool program capability and learns nothing about
  JavaScript.

## Consequences

- No download, install or extra process. Code mode works offline, in tests
  with the `faux` provider, and in every engine mode.
- Approvals stay per call and see the real arguments at run time. There is
  no need to judge a whole program up front.
- The program is not a sandbox escape: it cannot do anything a direct tool
  call could not do, and it is checked the same way.
- A C dependency is added to the engine build (QuickJS through
  `rquickjs` 0.14, MSRV 1.87, under the workspace's 1.89). It builds on
  Windows (MSVC) with no extra tools; macOS and Linux are checked by CI.
- The interpreter is ES2023 without TypeScript. The agent writes plain
  JavaScript, and type declarations in its instructions are documentation,
  not checked.
- Rules for contributors: never expose a host function to a program other
  than a tool call, never give a program a path to the file system,
  network or environment, and never bypass the tool program capability
  when dispatching calls from code. A new kind of effect means a new tool,
  not a new binding.
- The ADD's crate-layer table and its trust section describe the
  interpreter.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Deno subprocess with all permissions denied, talking to the engine over stdio | Strong isolation, but the shell downloads Deno on demand and the engine does not have it. It also adds a process start (about 0.25 s) to every run and a download on first use, and it is unavailable in headless and offline engines. |
| V8 embedded through `deno_core` or `rusty_v8` | Fast and complete, but it adds tens of MB to the engine binary and makes the build much heavier. The extra speed does not matter for programs whose time is spent waiting on tools. |
| Boa (pure Rust interpreter) | No C build, but slower and with less complete language support than QuickJS. Its time and memory limits are less mature. |
| Run the code in a shell tool (`node -e` or `deno eval`) | Depends on what the person has installed. The code would have the person's full file and network access outside the per-call checks. |
| A small custom DSL instead of JavaScript | Models write JavaScript well and a DSL poorly. It would also need its own parser, docs and tests for no gain in safety, since effects are tool calls either way. |
