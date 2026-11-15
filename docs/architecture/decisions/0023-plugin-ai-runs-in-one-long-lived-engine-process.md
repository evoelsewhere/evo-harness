# 0023. Plugin AI runs in one long-lived engine process

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-14 |
| ADD sections | [4. Process view](../README.md#4-process-view) · [7. Data view](../README.md#7-data-view) · [9. Cross-cutting concepts](../README.md#9-cross-cutting-concepts) |
| Related | [Plugin AI spec](../../features/plugin-ai/spec.md) · [0016](0016-memory-indexing-runs-in-a-short-lived-engine-process.md) · [0021](0021-plugin-views-run-in-native-webviews.md) · [0022](0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md) |

## Context

Plugins (views, backends, hooks) need model answers without going through
a chat: streamed into their own UI, with models chosen by need among the
person's connected providers, agent runs with tools they define at call
time, evo's own tools under the approval mode, threads, and per-plugin
caps. Model calls belong to engines: the shell never talks to providers,
and credentials, provider settings and usage accounting live with the
engine (`evo-runtime`, `usage_log`). Chat engines are tied to a chat; the
memory index process is one-shot and starts in seconds. Plugins are
untrusted code: this gives them the person's money and, optionally, evo's
tools.

## Decision

Plugin AI runs in **one engine process of its own**,
`evo-engine --mode plugin-ai`, which the shell starts on the first call,
keeps while calls come and stops after 5 idle minutes. It runs many runs at
once, each an in-memory `ChildHarness` with the model, tools and limits of
that run, and speaks a JSONL protocol of its own (`evo-protocol::plugin_ai`).
The shell routes calls from views (bridge) and backends and hooks (host API)
to it, and owns the gates: the `ai` and `ai.tools:*` permissions, caps,
concurrency, the run registry and its followers, and approvals in the main
window. The engine owns the rest: choosing a model from models.dev data,
the agent loop, tools, structured output, fallback, threads and usage rows.

## Consequences

- Plugin runs never touch a chat, wait for one, or need a chat engine.
- A warm run starts in well under a second; the first after idle pays one
  engine start.
- Usage gains a `plugin_id`; `evo.db` gains run and thread tables
  (migration 0014). Older releases cannot open a database migrated by this
  one.
- A new trust boundary: untrusted plugins can spend money and act through
  evo tools. Rules: `ai` and each tool group are consent permissions with
  reasons; evo tools in plugin runs go through the same gate chain as in
  chats, and approvals name the plugin; tool results and attached context
  are data; no credential reaches a plugin.
- The shell's settings fan-out sends `reload_providers` to this engine too.
- Plugin authors learn one API (`window.evo.ai`, `/v1/ai/*`), documented in
  the evo-creator skill.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Use the open chat's engine | Tied to a chat's life and turns; usage and approvals land in that chat. |
| A short-lived engine per run | Seconds per run; no shared queue, follows or approvals. |
| The shell calls providers | Breaks the engine's ownership of agent logic and duplicates credential handling. |
| One engine per plugin | More processes for no isolation the shell's limits do not already give. |
