# 0026. Code graph indexing runs in a short-lived engine process; indexers run only in trusted projects

| | |
| --- | --- |
| Status | Proposed |
| Date | 2026-11-23 |
| ADD sections | [4. Process view](../README.md#4-process-view), [5.1 Crate layers](../README.md#51-crate-layers), [7. Data view](../README.md#7-data-view), [9. Cross-cutting concepts](../README.md#9-cross-cutting-concepts) |
| Related | [Code graph spec](../../features/code-graph/spec.md), [0016](0016-memory-indexing-runs-in-a-short-lived-engine-process.md), [0018](0018-the-shell-hosts-engines-windows-attach.md) |

## Context

[Code graph](../../features/code-graph/spec.md) keeps one index per project
that the project's chats share. The index has two layers: a syntax layer
Evo builds itself by parsing files, and a precise layer imported from
semantic indexers (SCIP). The indexers are separate programs, and many of
them run the project's own tooling: rust-analyzer runs build scripts and
proc macros, scip-java runs the Gradle or Maven build, scip-dotnet runs a
design-time build. Indexing a large project takes from seconds (syntax) to
many minutes (some indexers).

Several chat engines can be open in one project. Memory indexing faced a
similar shape and runs in a short-lived engine process that a worker in the
shell starts and queues (ADR 0016).

## Decision

1. **A leaf crate, `evo-codegraph`,** holds parsing (tree-sitter), the SCIP
   reader, the SQLite store, queries and ranking. It runs no process and
   knows nothing of trust or settings, so the shell can link it to list and
   delete indexes.
2. **Indexing runs in its own engine process,** `evo-engine --mode
   codegraph-index --project <dir> [--precise]`. A worker in the shell
   decides when to start one and queues them, one at a time. It prints
   progress lines and exits. Chat engines only read the index, and write
   syntax rows for files that changed since the last run when a query needs
   them.
3. **Indexers run only in trusted projects, and the first time only on the
   person's Index.** The index process checks trust itself before starting
   any indexer. Indexer runs are Evo's, not the agent's: no tool can start
   one, they are not tool calls and pass no approval gate, and they run with
   the person's permissions outside the agent's sandbox. They get the
   engine's environment without Evo's own secrets.
4. **Indexers are installed and found through the language-server catalog
   (`evo-lsp`)**, which gains indexer entries and three install kinds
   (coursier, `dart pub`, composer).
5. **The index lives in Evo's data folder**
   (`~/.evo/agent/codegraph/<hash of the project path>.db`), never in the
   project.

## Consequences

- A chat is never slowed by indexing, and two chats on one project never
  index it twice at once.
- Trusting a project now also allows its tooling to run for indexing, after
  the person's first Index. The trust prompt and the Index card must say so.
- A third kind of engine process exists. It is short-lived, has no protocol
  session, and the shell kills it when code graph is turned off or the app
  quits.
- Two writers share one SQLite file (the index process and a chat engine
  refreshing changed files); WAL and a busy timeout handle it.
- Contributors must keep the index mode free of plugins, hooks, connectors
  and models, and must never let a tool start an indexer.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Index inside each chat engine | Chats on one project would index at once; a long indexer run would hold a chat's process. |
| Index in the shell | The shell would link the installers and trust code and run project tooling itself; the engine already has both. |
| Run indexers in any project the person opens | Runs the tooling of projects nobody vouched for. |
| Store the index in the project's `.evo/` | Risk of committing it; the owner chose Evo's data folder (spec 6.2, D4). |
