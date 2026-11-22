# Code graph: the agent asks the project's structure instead of grepping — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-23 |
| ADD sections | [4. Process view](../../architecture/README.md#4-process-view), [5.1 Crate layers](../../architecture/README.md#51-crate-layers), [7. Data view](../../architecture/README.md#7-data-view), [9. Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts) |
| ADRs | [0026 Code graph indexing runs in a short-lived engine process; indexers only in trusted projects](../../architecture/decisions/0026-code-graph-indexing-in-a-short-lived-engine-process.md), [0016 Memory indexing in a short-lived engine process](../../architecture/decisions/0016-memory-indexing-runs-in-a-short-lived-engine-process.md), [0018 The shell hosts engines](../../architecture/decisions/0018-the-shell-hosts-engines-windows-attach.md), [0014 Agent tools by layer](../../architecture/decisions/0014-agent-tools-by-layer.md) |

## 1. Context and constraints

What exists today ([research](research.md) section 2 has the full list):

- **Background indexing precedent:** memory indexing runs in a short-lived
  engine process (`evo-engine --mode index`) that a worker in the shell
  starts and queues (`apps/desktop/src-tauri/src/memory_index.rs`,
  ADR 0016). Code graph indexing follows the same shape.
- **Language servers:** `crates/evo-lsp` has the catalog (`catalog.rs`:
  `Server`, `Install` with npm, `go install`, rustup, `dotnet tool`, `gem`,
  GitHub releases, `System`), detection, installers and the pool. The person
  manages them in **Customize → Languages**
  (`apps/desktop/src/features/customize/languages-section.tsx`).
- **Project trust:** `crates/evo-engine/src/trust.rs` (`is_trusted`,
  decisions in the project's `.evo/trust.json`).
- **A live experimental switch:** code mode's pattern, end to end: engine
  state and RPC (`crates/evo-engine/src/code_mode.rs`, `set_code_mode`), the
  shell's fan-out (`apps/desktop/src-tauri/src/supervisor/settings.rs`
  `Change::CodeMode`), the Experimental page
  (`apps/desktop/src/features/settings/sections/experimental-settings.tsx`).
- **Tools and prompt:** the tool table (`crates/evo-engine/src/tools/info.rs`),
  `register_all` (`tools/mod.rs`), per-tool prompt guidelines and snippets
  (`session.rs` `builtin_tool_guidelines`), the system prompt built at
  session start and reload.
- **Storage:** `rusqlite` is in the tree; the agent dir is
  `config::agent_dir()` (`~/.evo/agent`).
- **No tree-sitter yet.** Grammars since 0.23 depend on
  `tree-sitter-language` (a `LanguageFn`), so grammar crates no longer pin
  the core version; each grammar is C code built with `cc` (already in the
  build for QuickJS).
- **SCIP:** the `scip` crate (Apache-2.0) reads `index.scip` (protobuf).
  Occurrences carry a symbol, a range and roles (definition, reference,
  import, write); many indexers also emit `enclosing_range`. A
  relationship list gives implementations.

Constraints from the spec: no project code runs in untrusted projects, no
network from Evo itself (NFR-4), nothing written inside the project (FR-2),
nothing changes with the feature off (NFR-6).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | The tool, the prompt map, query-time freshness and the index process are the engine's. The shell only decides when an index process starts (as for memory indexing) and lists or deletes index files. |
| Layers point down; library crates stay free of engine/desktop concerns | ok, new crate | `evo-codegraph` is a leaf: tree-sitter, `scip`, `rusqlite`, `ignore`. It runs no process and knows nothing of trust or settings. ADR 0026 and an ADD edit. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | ok | `set_code_graph`, `get_state.codeGraph` and the index process's result lines, all additive; an older engine ignores the command (the shell logs it once). |
| Models come from models.dev; no hardcoded model facts | n/a | No model involved. |
| Real engine in the app; settings and credentials reload live | ok | The setting reaches running engines live (FR-1), like code mode. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok, new boundary | Indexers run project tooling: only in trusted projects, first on the person's Index (FR-5). ADR 0026 records the boundary. Symbol names and snippets in answers are untrusted tool output. |
| Desktop UI structure and `DESIGN.md` | ok | The Experimental page and the chat card use existing components; Customize → Languages lists indexers beside servers. |

## 3. Design

### 3.1 Overview

```text
 window                 shell                                engine (chat)               index process
 ──────                 ─────                                ─────────────               ─────────────
 Experimental page ──►  settings_set experimental.codeGraph
                        └─ set_code_graph ─────────────────► code_graph::set + tools
                        codegraph worker (one queue)
 chat opened        ──► ensure syntax index(project) ─────────────────────────────────► evo-engine --mode codegraph-index
 Index card/button  ──► precise index(project) ──(trusted?)────────────────────────────►   walk + tree-sitter → SQLite
 run settled + idle ──► refresh(project)                                                   SCIP indexers → import
                        progress events ◄────────────────────────────────────────────────   result lines (JSON)
 agent's code_graph call ───────────────────────────────► evo-codegraph queries
                                                           (stat files, re-parse changed,
                                                            mark approximate) ──► SQLite (WAL)
```

Two layers live in one SQLite file per project. The **syntax layer**
(tree-sitter tags) covers every indexed file and is refreshed at once. The
**precise layer** (SCIP) is imported per language and is valid for a file
only while its content hash equals the hash it was indexed at. A query
merges both per file: precise where valid, syntax otherwise, each item
marked (FR-7).

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| crate | `crates/evo-codegraph/src/store.rs` | SQLite schema (3.4), versioned; open, migrate-by-rebuild, WAL, busy timeout | FR-2, FR-15, NFR-7 |
| crate | `crates/evo-codegraph/src/walk.rs` | Project walk with `.gitignore`/`.ignore` (`ignore` crate), skipped folders (A-1), file and project limits, most recently changed first | FR-3, FR-11 |
| crate | `crates/evo-codegraph/src/syntax/` | One module per language: grammar, tags query (the grammar's `tags.scm`, or ours where it lacks one), extra queries for calls and imports, enclosing scopes | FR-3, FR-4 (syntax side) |
| crate | `crates/evo-codegraph/src/scip.rs` | Read `index.scip`; store symbols, occurrences (definition, reference, import), relationships (implementations), enclosing ranges, each with the file hash at indexing time | FR-4, FR-7 |
| crate | `crates/evo-codegraph/src/query.rs` | `find`, `definition`, `references`, `callers`, `callees`, `implementations`, `outline`, `impact`; per-item source and reason; symbol ids; 100-item cap | FR-7, FR-8, NFR-2 |
| crate | `crates/evo-codegraph/src/rank.rs` | File graph from references; personalized PageRank; the map rendered within a token budget | FR-10 |
| crate | `crates/evo-codegraph/src/fresh.rs` | Compare stored mtimes and sizes with the disk; re-parse changed files into the syntax layer; precise rows of changed files stop counting until the next import | FR-7, US-5, NFR-1 |
| engine | `crates/evo-engine/src/code_graph.rs` | Live state (on/off) like `code_mode.rs`; `apply` adds or removes the tool; the project and the index path for the session's folder; worktree → source repository (FR-16) | FR-1, FR-2, FR-16 |
| engine | `crates/evo-engine/src/tools/code_graph.rs` | The `code_graph` tool (3.3), read class, read-only safe; the index offer in its result (3.3) | FR-8, FR-9, FR-5 |
| engine | `crates/evo-engine/src/session.rs` | The "Project map" section when on and the index has data; a prompt guideline for the tool | FR-10 |
| engine | `crates/evo-engine/src/code_graph_index.rs`, `args.rs` (`--mode codegraph-index`) | The index process: syntax pass; with `--precise`, trust check, then each language's indexer from the catalog, import, result lines; time limit per language (A-4) | FR-3, FR-4, FR-5, FR-12, ADR 0026 |
| engine | `crates/evo-engine/src/rpc/protocol.rs`, `modes.rs` | `set_code_graph`, `get_state.codeGraph` | FR-1 |
| crate | `crates/evo-lsp/src/catalog.rs`, `install.rs` | Indexer entries (`Indexer` beside `Server`), new install kinds `Coursier`, `DartPub`, `Composer`; detection | FR-6 |
| desktop shell | `apps/desktop/src-tauri/src/codegraph.rs` | The worker: one queue; triggers (chat opened, Index, run settled and idle 60 s, at most every 10 minutes per language); runs the index process; progress to windows; list, stats and delete through `evo-codegraph`; commands `codegraph_index`, `codegraph_rebuild`, `codegraph_delete`, `codegraph_projects` | FR-3, FR-5, FR-13, FR-14, A-3 |
| desktop shell | `apps/desktop/src-tauri/src/engine.rs`, `supervisor/settings.rs` | `experimental.codeGraph`: `--code-graph` at start, `Change::CodeGraph` fan-out | FR-1, FR-13 |
| desktop UI | `apps/desktop/src/features/settings/sections/experimental-settings.tsx` | The Code graph switch and the per-project list with counts, layers, errors and Index now / Rebuild / Delete index | FR-1, FR-14 |
| desktop UI | `apps/desktop/src/features/customize/languages-section.tsx` | Indexers listed with Install / Uninstall beside the servers | FR-6 |
| desktop UI | `apps/desktop/src/components/ai-elements/` (a `code-graph-offer` card), `tool.tsx` | The one-time Index card under a `code_graph` result that carries an offer | FR-5 |
| tests | `crates/evo-codegraph/tests/fixtures/<language>/`, `scripts/bench/code-graph/` | Fixture projects per language; the exploration benchmark | SC-1, SC-2 |

### 3.3 Interfaces

**Tool `code_graph`** (FR-8):

```json
{ "name": "code_graph",
  "parameters": { "type": "object", "required": ["op"], "properties": {
    "op": { "enum": ["find", "definition", "references", "callers", "callees",
                     "implementations", "outline", "impact", "map"] },
    "query": { "type": "string" },
    "kind": { "type": "string" },
    "symbol": { "type": "string", "description": "An id from an earlier result, or a name" },
    "path": { "type": "string" }, "line": { "type": "integer" }, "column": { "type": "integer" },
    "focus": { "type": "array", "items": { "type": "string" } } } } }
```

Result text: one line per item, `[precise] fn retry_policy  src/net/fetch.rs:84  #s:42`,
or `[approximate: file changed] …`, then a note when capped. `details`:

```json
{ "op": "callers", "items": [ { "id": "s:42", "name": "retry_policy", "kind": "function",
  "path": "src/net/fetch.rs", "line": 84, "source": "precise" },
  { "path": "src/net/client.rs", "line": 12, "source": "approximate", "reason": "file_changed" } ],
  "truncated": false, "leftOut": 0,
  "indexOffer": { "project": "C:/work/app" } }
```

`indexOffer` is present once per project and session, when the project is
trusted, was never precisely indexed, and the person has not answered
**Not now** for it (FR-5).

**Engine RPC:** `{"type":"set_code_graph","enabled":true}` → `{ "codeGraph": true }`;
`get_state` gains `codeGraph: { enabled, project, indexed }`. Argument
`--code-graph`.

**Index process** (`evo-engine --mode codegraph-index --project <dir>
[--precise] [--index <db>]`): JSON lines on stdout:
`{"type":"progress","layer":"syntax","files":812,"total":1218}`,
`{"type":"language","language":"rust","layer":"precise","status":"done","symbols":15234}`,
`{"type":"language","language":"cpp","status":"failed","reason":"no compile_commands.json","next":"…"}`,
`{"type":"done","files":1218,"symbols":40211,"leftOut":0}`. Exit 0 on
success, 1 on failure (the last line says why).

**Shell commands:** `codegraph_projects() -> [{ project, files, symbols, languages: [{ language, layer, indexedAt, error }], leftOut, sizeBytes }]`,
`codegraph_index({ project })`, `codegraph_rebuild({ project })`,
`codegraph_delete({ project })`, `codegraph_dismiss_offer({ project })`.
Event `codegraph://progress { project, … }` (the index process's lines).

**Settings keys:** `experimental.codeGraph` (bool, default false);
`codegraph.offerDismissed` (project paths, list).

**Catalog:** `pub struct Indexer { id, name, languages, extensions, command, args, install, requires, output }`
with `output` the file it writes (`index.scip`); `Install::Coursier { app }`,
`Install::DartPub { package }`, `Install::Composer { package }`.

### 3.4 Data and storage

`~/.evo/agent/codegraph/<sha256(canonical project path)[..16]>.db`, SQLite
in WAL mode, `PRAGMA user_version` = index format version (FR-15).

```sql
files(id INTEGER PRIMARY KEY, path TEXT UNIQUE, language TEXT, size INTEGER,
      mtime INTEGER, hash TEXT, syntax_at INTEGER)
symbols(id INTEGER PRIMARY KEY, file_id INTEGER, name TEXT, kind TEXT,
        line INTEGER, col INTEGER, end_line INTEGER, scip TEXT, layer TEXT)
refs(id INTEGER PRIMARY KEY, file_id INTEGER, symbol_id INTEGER NULL, name TEXT,
     line INTEGER, col INTEGER, role TEXT, enclosing_id INTEGER NULL, layer TEXT)
impls(symbol_id INTEGER, implements_id INTEGER)
precise(file_id INTEGER, language TEXT, hash TEXT, indexed_at INTEGER)
languages(language TEXT PRIMARY KEY, indexer TEXT, status TEXT, error TEXT,
          indexed_at INTEGER)
meta(key TEXT PRIMARY KEY, value TEXT)          -- project path, left out, offer state
symbols_fts USING fts5(name, content='symbols') -- find
```

A row with `layer = 'precise'` counts only while `precise.hash = files.hash`
for its file. Rebuild deletes the file and starts over. The shell lists the
folder and reads `meta` and counts for the settings page.

### 3.5 Behaviour

1. **On, chat opens in a project:** the shell's worker queues a syntax run
   for the project (skipped when the index is fresh). The run walks the
   project, parses new and changed files, stores symbols and refs, resolves
   refs to symbols by name within import scope, and prints progress.
2. **First precise index:** the person presses Index (settings page or the
   chat card). The worker queues a run with `--precise`. The process checks
   trust (untrusted: exits with the reason; FR-5), finds each language's
   indexer through the catalog, runs it in the project folder with a time
   limit, imports `index.scip`, records each file's hash, and reports each
   language. A missing indexer is reported as `not installed` with the
   catalog id, so the page offers Install.
3. **Background refresh:** when a chat's run settles in an indexed project,
   the worker schedules a refresh 60 s later, cancelled by another run
   starting. The refresh is a syntax run, plus a precise run for a language
   whose files changed since its last import and that was imported more than
   10 minutes ago (A-3).
4. **Query:** the tool opens the index read-write, runs `fresh` (stat the
   indexed files; re-parse changed ones into the syntax layer, under 200 ms
   each), then the query. Items from files whose precise hash no longer
   matches come from the syntax layer, marked `file_changed`.
5. **Worktree:** the engine maps a worktree folder to its source repository
   (`git rev-parse --git-common-dir`); the index path is the source's;
   `fresh` reads the worktree's own files for changed ones and never writes
   precise rows for it (FR-16).
6. **Off:** `set_code_graph false` removes the tool; the worker stops its
   queue and kills a running index process (FR-13).

Concurrency: one index process at a time (the worker's queue); chat engines
write only syntax rows for changed files; SQLite WAL with a busy timeout.

### 3.6 Errors and edge cases

| Case | What happens |
| --- | --- |
| Untrusted project, Index pressed | The process exits "needs a trusted project"; the page says so with a link to trust it; no indexer starts (SC-3) |
| Indexer fails or times out | That language stays approximate; `languages.error` and the next step shown (FR-12) |
| Platform without a native indexer | Reported `unsupported` before any run (scip-clang, scip-ruby on Windows) |
| Index process killed (engine restart, app quit) | Files already parsed are kept; the next run continues; a half-imported language is rolled back (one transaction per language) |
| Corrupt or other-version index | Rebuilt (FR-15) |
| Over the limits | Most recently changed files first; `leftOut` reported (FR-11) |
| Disk over NFR-7 | Logged; the page shows the size |

### 3.7 Security and trust

- Untrusted inputs: file contents (parsed, never executed by Evo), SCIP
  output from indexers (bounded reads; symbol strings capped), and every
  name or path in an answer (tool output, untrusted for the model).
- Indexers run project tooling (build scripts, proc macros, Gradle, design-
  time builds). They run only in trusted projects (`trust.rs`), first on the
  person's Index, never from a tool call, with the person's permissions and
  outside the agent's sandbox, which ADR 0026 records. Their environment is
  the engine's minus Evo's own secrets (`EVO_PLUGIN_*`, provider keys).
- The tool only reads; it cannot start an indexer. The index offer is a card
  the person answers; the agent cannot press it.
- Nothing is written inside the project; the index path is derived from the
  canonical path, so two projects never share an index.

### 3.8 Configuration

| Key | Type | Default | Where | Applies |
| --- | --- | --- | --- | --- |
| `experimental.codeGraph` | bool | false | app database settings | live (`set_code_graph`; the map at the next session start or reload) |
| `codegraph.offerDismissed` | list of project paths | [] | app database settings | at once |
| `--code-graph` | flag | absent | engine argument | at start |

Limits, debounce and time limits are constants (A-1 – A-4).

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Index inside each chat engine | Several chats on one project would index at once, and a long indexer run would hold a chat's process; the shell-queued process follows ADR 0016. |
| Index in the shell | The shell would have to link the language-server installers and the trust code, and run project tooling itself; the engine already has both. |
| Indexer catalog inside `evo-codegraph` | It would make the leaf crate run processes and install software; the installers already live in `evo-lsp`. |
| A graph database (Kuzu, Cozo) | A new store and dependency for queries SQLite answers with indexes and FTS5. |
| Name-based only first | The owner chose precise from the first version (spec 6.2, D3). |
| Embedding codanna | See research section 4. |

## 5. As-built notes

Empty until the status is `In progress`.
