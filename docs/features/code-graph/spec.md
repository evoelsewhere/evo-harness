# Code graph: the agent asks the project's structure instead of grepping — Spec

| | |
| --- | --- |
| Status | Draft |
| Area | new crate `evo-codegraph`, engine, desktop shell, desktop UI |
| Created | 2026-11-22 |
| Updated | 2026-11-22 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Research notes](research.md), [Code intelligence](../code-intelligence/spec.md), [Code mode](../code-mode/spec.md), [Approval modes](../approval-modes/spec.md) |

## 1. Summary

To understand how a project fits together, the agent greps and reads file
after file. Each read costs a turn and context, and grep cannot tell a
definition from a call. Code graph keeps an index of the project's symbols
and how they connect: definitions, references, implementations, calls and
imports. The agent gets a `code_graph` tool to ask "where is this defined",
"who calls this" and "what does this change affect", and a short map of the
project's most connected files and symbols in its instructions. Answers are
precise where a language's semantic indexer has run, and approximate (by
name) elsewhere, and each answer says which. Code graph is experimental and
off until the person turns it on.

## 2. Scope

### 2.1 Goals

- One index per project, kept in Evo's data folder and shared by the
  project's chats.
- A precise layer from semantic indexers (SCIP) for every language that has
  one, and an approximate layer from syntax parsing (tree-sitter) that
  covers the rest and fills in while precise data is missing or stale.
- A `code_graph` tool with a small set of operations, usable from code mode
  and by subagents, and safe for read-only agents.
- A project map in the agent's instructions, within a fixed size.
- Indexers that run project tooling run only in trusted projects, and the
  first time only when the person asks.
- Indexers installed and managed like language servers.
- **Settings → Experimental → Code graph**, with the index's state and
  controls.

### 2.2 Non-goals

- A graph view in the app. A later phase, after the index has proven
  itself.
- Refining answers with the language servers Evo already runs (the `lsp`
  tool stays as it is). A later phase.
- Semantic (embedding) search. Code graph answers structural questions.
- Indexing anything outside the chat's project folder, or projects the
  person has not opened in a chat.
- Bundling indexers with Evo. They are installed on request (FR-6).
- Editing code. Code graph only reads.

## 3. User scenarios

### US-1 The agent finds its way in an unfamiliar project (priority P1)

A person opens a chat in a project the agent has not seen and asks for a
change. They want the agent to start from the right files instead of
exploring for several turns.

Acceptance scenarios:

1. **Given** code graph on and the project indexed, **when** a chat starts,
   **then** the agent's instructions carry a project map: the most
   connected files with their main symbols, within the map's size.
2. **Given** the same chat, **when** the agent asks
   `code_graph({ op: "find", query: "retry" })`, **then** it gets the
   matching symbols with their kind, file and line, without reading files.

### US-2 The agent checks the impact of a change before making it (priority P1)

A person asks the agent to change a function's signature. They want the
agent to update every caller and not miss one.

Acceptance scenarios:

1. **Given** a precisely indexed Rust or TypeScript project, **when** the
   agent asks for `callers` of a function, **then** it gets every call site
   with file and line, marked `precise`, and none of the same-named
   functions elsewhere.
2. **Given** the same project, **when** the agent asks for `impact` of a
   file, **then** it gets the symbols that file defines and the files that
   use them.

### US-3 The person turns code graph on and indexes a project (priority P1)

A person wants to try code graph on a project they trust.

Acceptance scenarios:

1. **Given** a fresh install, **when** the person opens
   **Settings → Experimental**, **then** **Code graph** is off.
2. **Given** they turn it on, **when** a chat is open in a project, **then**
   the approximate layer indexes the project in the background and the
   settings page shows its progress.
3. **Given** a trusted project whose languages have indexers installed,
   **when** the person presses **Index**, **then** the indexers run, the
   page shows each language's progress and result, and later answers are
   `precise` for those languages.
4. **Given** a language whose indexer is not installed, **when** the page
   lists the project's languages, **then** it offers **Install** for that
   indexer, as for language servers.

### US-4 Answers keep coming when precise data is missing (priority P2)

A person works in a project that is not trusted, or on Windows in a C++
project (no native indexer there), or while the first index still runs.

Acceptance scenarios:

1. **Given** an untrusted project, **when** the agent asks `callers`,
   **then** it gets name-based results marked `approximate`, with the
   reason "precise indexing needs a trusted project".
2. **Given** the precise index is still running, **when** the agent asks,
   **then** it gets approximate results and the reason "indexing".

### US-5 The index keeps up with changes (priority P2)

A person and the agent keep editing files during a chat.

Acceptance scenarios:

1. **Given** a file the agent just edited, **when** the agent asks about a
   symbol in it, **then** the answer reflects the edit (approximate until
   the next precise index), never the old content as precise.
2. **Given** edits have stopped for a while, **when** the project is
   trusted and was indexed before, **then** the precise index runs again in
   the background, and that file's answers are precise again.

### US-6 The person manages indexes and indexers (priority P3)

A person wants to see what is indexed, rebuild or delete it, and manage
indexers.

Acceptance scenarios:

1. **Given** the settings page, **when** the person looks at a project,
   **then** they see its files and symbols counted, each language's layer
   (precise or approximate) and last index time, and **Index now**,
   **Rebuild** and **Delete index**.
2. **Given** **Delete index**, **when** the person confirms, **then** the
   index file is removed and nothing is indexed again until a chat opens in
   that project.

### Edge cases

- **Very large project:** over the limits (FR-11), the most recently changed
  files are indexed first, and the page says how many were left out.
- **Syntax errors:** the approximate layer still indexes the rest of the
  file. An indexer that fails on a project that does not build leaves that
  language approximate, with the indexer's error on the page.
- **Missing build data:** scip-clang without `compile_commands.json`, a Java
  project whose build fails: the language stays approximate and the page
  names the missing piece.
- **Several chats on one project:** one index, one indexing at a time; the
  others read it meanwhile.
- **A worktree of the project** (a subagent or nested session in its own
  worktree): it uses the index of the repository it was cut from; files it
  changed answer `approximate` from its own folder (FR-16).
- **Engine restart mid-index:** the approximate layer continues where it
  stopped; an interrupted indexer run starts again at the next idle time.
- **Code graph turned off:** indexing and running indexers stop; the tool
  and the map are gone from the next request; the index files stay until
  deleted.
- **Files outside the project or ignored:** never indexed, never answered.
- **An index from an older Evo:** rebuilt, not migrated (FR-15).

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | Code graph is a person-level setting under **Settings → Experimental**, off by default. A change reaches running chats with no restart: the tool from their next request, the project map from their next session start or reload (FR-10). Project settings cannot turn it on. Chats, nested sessions and subagents follow it. | Fresh install: off, no tool, no map. Turn it on mid-chat: the next request has the tool. |
| FR-2 | Each project has one index in Evo's data folder (`~/.evo/agent/codegraph/<project hash>.db`), shared by its chats and never written inside the project. | After indexing, the project folder has no new files; two chats in it read the same index. |
| FR-3 | With code graph on, the approximate layer indexes the project in the background when a chat opens in it: every file allowed by `.gitignore` and `.ignore`, in a language Evo has a grammar for, within the limits (FR-11), and keeps it fresh from file changes. It only reads files, starts no process and uses no network, in trusted and untrusted projects alike. | A fixture project is indexed after the chat opens; no process is started. |
| FR-4 | The precise layer runs the SCIP indexer of each of the project's languages that has one installed: rust-analyzer (Rust), scip-typescript (TypeScript, JavaScript), scip-python (Python), scip-go (Go), scip-java (Java, Kotlin, Scala), scip-clang (C, C++), scip-ruby (Ruby), scip-dotnet (C#, Visual Basic), scip-dart (Dart), scip-php (PHP). | One fixture project per language indexes precisely where its indexer runs on that platform. |
| FR-5 | Indexers run only in trusted projects. The first precise index of a project starts only when the person presses **Index**: on the settings page, or on a one-time card in the chat the first time the agent uses `code_graph` in a trusted project that was never indexed ("Index this project for precise answers?", **Index** / **Not now**). After that it runs again in the background when the chat is idle and a while after files change. Indexer runs are Evo's, not the agent's: they are not tool calls and never show an approval prompt. They run the project's own tooling with the person's permissions, outside the agent's sandbox, and that tooling may fetch the project's dependencies as a build does. | Untrusted: no indexer process ever starts. Trusted, never indexed: none starts until Index. After Index, an edit leads to a new run once edits stop. |
| FR-6 | Indexers are entries of the code intelligence catalog: one already on the computer is used; others are installed only when the person presses **Install**; they can be uninstalled from the same page. A project's language with no indexer installed is shown with **Install**. | An installed rust-analyzer is found; scip-typescript installs on Install; uninstall removes it. |
| FR-7 | Every item of a result says its source: `precise` (from the language's SCIP index, for a file unchanged since it was indexed) or `approximate` (by name, from the syntax layer), and an approximate item says why (no indexer, untrusted project, never indexed, indexing, file changed, indexer failed, not supported on this platform). One answer may mix both, for example callers in changed and unchanged files. | Change one of three calling files: `callers` returns two `precise` items and one `approximate` with "file changed" until the next precise run. |
| FR-8 | The `code_graph` tool takes `op` and its arguments: `find` (query, optional kind), `definition` (symbol, or file and position), `references` (same), `callers`, `callees`, `implementations` (symbol), `outline` (file), `impact` (symbol or file), `map` (optional focus files). A symbol is named by the id a previous result gave, or by a name; a name that matches several symbols returns them all to choose from. Each result lists symbols or locations with id, kind, file and line, at most 100 items with a note when more exist. | One test per operation on a fixture project; a query with 300 matches returns 100 and says so; a name defined twice returns both ids. |
| FR-9 | `code_graph` only reads: it is classed as a read for approvals, read-only agents may call it, code mode programs and subagents can call it. | It never prompts in "Ask every time"; a read-only agent's call succeeds; a program's `tools.code_graph(...)` works. |
| FR-10 | With code graph on and an index with data, the agent's instructions carry a **Project map**: the project's most connected files, each with its main symbols, ranked by connections, within 1,500 tokens. It is built when the chat's session starts or reloads. The `map` operation gives a map focused on given files. | Map present when on and indexed, absent when off; never over the size; `map` with focus files ranks their neighbours first. |
| FR-11 | Limits for the syntax layer: files over 1 MB are skipped, and in a project over 50,000 files the most recently changed 50,000 are indexed; the settings page and the tool's answers say how many files were left out. The precise layer covers what each indexer covers. | A fixture with 50,010 files indexes 50,000 and reports 10 left out. |
| FR-12 | When an indexer fails (the project does not build, build data is missing, the indexer is not supported on this platform), its languages stay approximate, and the settings page shows the error and the next step. | A Rust fixture that fails to build stays approximate with rust-analyzer's error shown. |
| FR-13 | Turning code graph off stops indexing and any running indexer at once, and removes the tool and the map from the next request. The index stays. **Delete index** removes a project's index after confirmation. | Turn off during an indexer run: the process ends; the index file remains until Delete index. |
| FR-14 | **Settings → Experimental → Code graph** shows, for each project with an index: files and symbols counted, each language's layer and last index time, files left out, the last error, and **Index now**, **Rebuild**, **Delete index**; and the indexers with their state and Install / Uninstall. | Manual check in the running app. |
| FR-15 | The index format carries a version; an index of another version is rebuilt, not migrated. | Change the version in a fixture index: it is rebuilt on next use. |
| FR-16 | A chat in a worktree of a project (a subagent's or nested session's) uses the index of the repository the worktree was cut from. Files the worktree changed answer `approximate` from the worktree's own folder; no indexer runs for the worktree. | A subagent in a worktree edits one file: `callers` gives `precise` items from the source index and an `approximate` item for its edited file; no indexer process starts. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | The syntax layer indexes this repository (about 1,200 source files) in under 30 seconds on a developer laptop, and updates one changed file in under 200 ms. | Benchmark test on this repository. |
| NFR-2 | A `code_graph` query answers in under 100 ms (p95) on this repository's index. | Benchmark test. |
| NFR-3 | Indexing runs in the background at low priority and never blocks the chat, a tool call or the app. | A turn during indexing is as fast as without (within 10%). |
| NFR-4 | Evo itself uses no network for code graph except installing an indexer the person asked for. (An indexer's project tooling may fetch dependencies, FR-5.) | Network check during syntax indexing and queries. |
| NFR-7 | A project's index takes at most five times the size of the files it covers on disk, and is removed with **Delete index**. | Size check on this repository's index. |
| NFR-5 | Works on Windows, macOS and Linux. Where an indexer has no native build (scip-clang, scip-ruby on Windows), its languages are approximate, as FR-12 says. | CI on three platforms; the Windows C++ fixture is approximate with the platform reason. |
| NFR-6 | With code graph off, there is no change in the tool list, the instructions, startup time or memory. | Snapshot of tool list and prompt with it off; startup timing. |

## 5. Success criteria

- SC-1 On a benchmark of exploration tasks in projects the agent has not
  seen, chats with code graph on need at least 30% fewer model turns and at
  least 20% fewer input tokens than with it off, and solve at least as many
  tasks (6.2).
  The benchmark must leave room for that: with code graph off, its tasks
  should take well above the two-turn floor (one call, one answer), or the
  target cannot be reached (lesson from the code mode benchmark).
- SC-2 On fixture projects, `callers` and `references` marked `precise`
  match the language's own find-references for every symbol checked.
- SC-3 No indexer process ever starts in an untrusted project, or before
  the person's first Index in a trusted one.
- SC-4 Turning code graph off gives exactly today's behaviour.

## 6. Clarifications

### 6.1 Open

n/a — none open.

### 6.2 Resolved

- 2026-11-22 — D1: Which approach: a connector, an embedded tool, or Evo's
  own crate? → A: Evo's own crate `evo-codegraph`, following the design of
  sunerpy/codegraph-rust (MIT) ([research](research.md) section 4).
- 2026-11-22 — D2: What does the agent get first? → A: The `code_graph`
  tool and a project map in its instructions (FR-8, FR-10).
- 2026-11-22 — D3: How precise must the first version be? → A: Precise from
  the first version, from SCIP indexers (FR-4). (The recommendation was
  name-based first.)
- 2026-11-22 — D4: Where does the index live? → A: In Evo's data folder,
  never in the project (FR-2).
- 2026-11-22 — D5: Which languages? → A: Every language with a SCIP indexer
  (FR-4), with syntax grammars for the same languages (FR-3).
- 2026-11-22 — D6: What is answered while precise data is missing or
  stale? → A: The syntax layer answers, marked `approximate` with the
  reason (FR-7).
- 2026-11-22 — D7: When may an indexer run? → A: Only in trusted projects,
  first on the person's Index, then in the background when idle and after
  changes (FR-5).
- 2026-11-22 — D8: How are indexers installed? → A: Through the code
  intelligence catalog (FR-6).
- 2026-11-22 — The framing above was played back and confirmed by the
  owner.
- 2026-11-22 — Q1: What is SC-1's target? → A: At least 30% fewer turns and
  20% fewer input tokens, as many tasks solved (raised from the proposed
  15% fewer turns and no more tokens) (SC-1).

- 2026-11-22 — Q (audit): Which index does a chat in a worktree use? → A:
  The source repository's, with its own changed files approximate (FR-16).
- 2026-11-22 — Q (audit): Where is the first Index pressed? → A: On the
  settings page and on a one-time card in the chat (FR-5).

### 6.3 Assumptions

- A-1 Ignore rules are `.gitignore` and `.ignore`, plus vendored and
  generated folders (`node_modules`, `target`, `dist`, `build`, `.git`) —
  the usual set; it changes no requirement.
- A-2 The project map's size is 1,500 tokens (FR-10) — about the size of
  one medium file read, small next to the instructions.
- A-3 Background re-indexing starts after 60 seconds without file changes
  while the chat is idle, and at most once every 10 minutes per language —
  frequent enough to stay fresh, rare enough not to compete with builds.
- A-4 Indexer runs have a time limit of 15 minutes per language; past it
  the language stays approximate with a timeout error (FR-12).
- A-5 The exploration benchmark follows the code mode benchmark's method:
  12 tasks in three open-source projects of different languages, one fixed
  model, each task run with code graph off and on, tasks chosen so that with
  code graph off they take at least four turns.

## 7. History

- 2026-11-22 — Draft, from the confirmed framing in [research.md](research.md).
- 2026-11-22 — SC-1 raised by the owner to 30% fewer turns and 20% fewer
  input tokens.
- 2026-11-22 — Audit: FR-1 says when the map follows a change; FR-5 says
  indexer runs are not tool calls, run outside the sandbox and may fetch
  dependencies; FR-7 marks each item; FR-8 adds symbol ids; FR-11 scopes the
  limits; NFR-1 uses this repository's real size; NFR-4 is scoped to Evo's
  own work; NFR-7 bounds disk use; two questions opened (6.1).
