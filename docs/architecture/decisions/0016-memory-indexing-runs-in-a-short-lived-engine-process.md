# 0016. Memory indexing runs in a short-lived engine process the shell starts for idle chats

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-10 |
| ADD sections | [4. Process view](../README.md#4-process-view), [7. Data view](../README.md#7-data-view) |
| Related | [Memory topics and chat recall spec](../../features/memory-topics-and-chat-recall/spec.md) (FR-24 to FR-34), [0003](0003-engine-runs-as-a-child-process-per-chat.md), [0007](0007-one-transcript-file-per-chat.md), [0014](0014-agent-tools-by-layer.md) |

## Context

- Automatic indexing reads what is new in an idle chat with a model the
  person chose, updates the memory topics and writes a summary of the chat.
  It is a model call outside any chat's own run.
- The engine owns memory, model selection and credentials
  ([ADD 4](../README.md#4-process-view)); the shell links no engine crate
  and cannot call a model.
- There is one engine per open chat ([ADR 0003](0003-engine-runs-as-a-child-process-per-chat.md)).
  Idle engines beyond four are stopped, and a chat that was never opened in
  this run of the app has none. Chats from before indexing was switched on
  can be indexed on request, dozens at a time.
- A chat's engine loads plugins, hooks, connectors and every tool. Its
  usage recorder and approval state are one per process.
- Indexing must do nothing but change topics and write the summary
  (spec FR-31), and must never delay a chat (spec NFR-5).
- A transcript has one writer, its chat's engine; other processes may read
  it ([ADR 0007](0007-one-transcript-file-per-chat.md)). The shell already
  reads transcripts for search.
- The shell owns cross-chat coordination and sees every engine's events.

## Decision

Indexing runs in its own engine process, one chat per run, started and
watched by the shell.

- **`evo-engine --mode index`** reads one chat's transcript read-only,
  resolves the index model like any model, runs a bare agent loop with two
  tools (`memory` and `set_summary`), applies topic changes through the
  engine's memory code, prints one result line and exits. It loads no
  plugins, hooks, skills, connectors, project instructions or other tools.
- **The shell decides when.** A worker in the shell looks for chats with new
  messages that have been idle long enough, runs one index process at a
  time, stores the summary and the "indexed up to" mark in `evo.db`, and
  tells the windows when topics changed. The timer is in the shell, not the
  webview, so indexing works with the window hidden.
- **Usage** of an index run is recorded by that process with its own source,
  under the chat it read.

## Consequences

- A chat is never delayed by indexing: the two share no process.
- Old chats and chats whose engine was stopped are indexed without starting
  a full chat engine.
- "Nothing but memory changes" holds by construction: the index process has
  no other tool to call, and tool results are not part of what it is given.
- A second kind of engine process exists. It is short-lived, has no
  protocol session, and the shell kills it on a timeout or when its chat is
  deleted.
- The transcript-reading code the shell and the index process both need
  moves to `evo-store`, the leaf crate they share.
- Indexing costs a process start per run; runs are queued one at a time.
- Contributors must keep the index mode free of plugins, hooks and
  connectors; adding a tool to it is a change to this decision.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Index inside the chat's own engine, on an RPC command | Needs that engine running, so old chats and stopped engines would each start a full engine. Plugins and every tool are loaded in that process, which weakens "nothing but memory changes". Its usage recorder is per process and counts as the chat's own. |
| The shell calls the model | The shell has no model code or credentials and must not link the engine; it would also make agent decisions. |
| One long-lived index engine for all chats | Another always-on process and a queue protocol for little gain; a crash loses the queue's state, which the database already holds. |
| The timer in the webview, like the Scheduler's | Stops when no window runs script; [ADR 0014](0014-agent-tools-by-layer.md) moves such logic out of the webview. |
