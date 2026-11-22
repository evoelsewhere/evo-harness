# Code graph (experimental) — Research notes

| | |
| --- | --- |
| Spec | [spec.md](spec.md) (Draft) |
| Updated | 2026-11-22 |
| Related | [Code intelligence](../code-intelligence/spec.md), [Code mode](../code-mode/spec.md), [Customize: connectors](../customize-skills-plugins-connectors/spec.md) |

These notes frame an experimental **code graph** for Evo: an index of a
project's symbols and how they connect (definitions, references, calls,
imports). The agent can then ask structural questions ("who calls this?",
"what breaks if I change it?", "which files matter for this task?") instead
of grepping and reading file after file. The notes cover what Evo has today,
the open-source projects weighed (Rust first), the options, the big
questions, and a recommendation. They become the spec's background once the
open decisions are settled.

## 1. Why

- To understand structure, the agent greps and reads many files. Each read
  costs a model turn and context tokens, and grep cannot tell a definition
  from a call or a comment.
- The `lsp` tool answers precise questions, but it is opt-in. It needs a
  language server per language, which may need installing, starting it is
  slow, and it answers one file or one position at a time
  ([code intelligence spec](../code-intelligence/spec.md)). It has no
  whole-project view such as "the 20 most connected files" or "everything
  that depends on this module".
- The system prompt carries no map of the project. A model starting in an
  unfamiliar repository spends its first turns exploring.
- Agents doing multi-file changes need impact analysis: callers,
  implementers and dependents of what they change.

## 2. What Evo has today (facts)

| Fact | Source |
| --- | --- |
| Language servers per project, the opt-in `lsp` tool (definition, references, hover, symbols), errors returned with each edit | `crates/evo-lsp/`, `crates/evo-engine/src/tools/lsp.rs`, [code intelligence](../code-intelligence/spec.md) |
| Code mode lets a program call any tool many times in one step; a graph tool would be callable from programs with no extra work | `crates/evo-engine/src/tools/code/`, [code mode](../code-mode/spec.md) |
| Connectors (MCP servers) can be added by the person; an external code graph server works today as a connector | `crates/evo-mcp/`, Customize → Connectors |
| SQLite is already in the tree (`rusqlite`); no tree-sitter dependency yet | `Cargo.lock` |
| The code intelligence catalog installs with npm, `go install`, rustup, `dotnet tool`, `gem` and GitHub releases. That covers rust-analyzer, scip-typescript, scip-python, scip-go, scip-dotnet, scip-ruby and scip-clang; scip-java (coursier, JVM), scip-dart (`dart pub global activate`) and scip-php (composer) need new install kinds | `crates/evo-lsp/src/catalog.rs` (`Install`) |
| Project trust exists per folder (`TrustDecision`, kept in `.evo/trust.json`) | `crates/evo-engine/src/trust.rs` |
| This repository has about 1,200 source files (`.rs`, `.ts`, `.tsx`, `.js`, `.py`, `.go`) | `git ls-files` |
| File changes are watched by polling modification times (no native watcher) | `crates/evo-engine/src/fs_watch.rs` |
| A C compiler is already part of the build (`cc` for QuickJS) | `crates/evo-engine/Cargo.toml` (`rquickjs`) |
| Evo is Apache-2.0; MIT and Apache-2.0 code can be reused with attribution | `LICENSE`, `NOTICE` |
| Experimental features live under **Settings → Experimental**, off by default, reaching engines live | `apps/desktop/src/features/settings/sections/experimental-settings.tsx` |

## 3. The landscape

Status as of 2026-11-22, from the projects' repositories, crates.io and
docs.rs. Star counts are rough.

### 3.1 Building blocks

| Project | Language, license | What it gives | Fit for Evo |
| --- | --- | --- | --- |
| [tree-sitter](https://github.com/tree-sitter/tree-sitter) + grammar crates | Rust/C, MIT | Fast, incremental, error-tolerant parsing for 100+ languages; each grammar is a crate | The base of nearly every tool below. Each grammar adds C code and binary size. |
| [tree-sitter-tags](https://docs.rs/tree-sitter-tags) | Rust, MIT | Definitions and references from each grammar's `queries/tags.scm` ([GitHub code navigation](https://github.com/github/code-navigation) uses the same approach) | The cheapest way to a cross-file symbol graph. Name-based ("search-based"), not precise: two functions with the same name are not told apart. |
| [stack-graphs](https://github.com/github/stack-graphs) | Rust, MIT/Apache | Precise name resolution from tree-sitter, file by file | **Archived** (last push 2025-09-09). Not a base to build on. |
| [tree-sitter-graph](https://github.com/tree-sitter/tree-sitter-graph) | Rust, MIT/Apache | A DSL to build graphs from syntax trees | Not archived, but little activity (last push late 2024). |
| [SCIP](https://github.com/sourcegraph/scip) (`scip` crate) | Rust bindings, Apache-2.0 | A precise index format (definitions, references, implementations), produced by language-specific indexers: `rust-analyzer scip`, scip-typescript, scip-python, scip-java and more | The precise layer. Each indexer is a separate install (Node, JVM, Python), runs slowly, and needs a project that builds. `rust-analyzer scip` has had recent regressions. |
| rust-analyzer as a library (`ra_ap_*` crates) | Rust, MIT/Apache | Full semantic analysis | Rust projects only, and very heavy. |
| [ast-grep-core](https://docs.rs/ast-grep-core) | Rust, MIT | Structural search and rewrite over tree-sitter trees (`$X.unwrap()`) | A query tool, not a graph. Could be a later addition for structural search. |
| A repository map from an open-source coding assistant (`repomap.py`) | Python, Apache-2.0 | tags.scm definitions and references, then a personalized PageRank ranks files and symbols for the task, cut to a token budget | The best-known way to give a model a project map. Rust ports exist ([ripmap](https://docs.rs/crate/ripmap/latest), [repo-mapper](https://docs.rs/crate/repo-mapper/latest)) and describe the method. |

### 3.2 Complete code graph tools in Rust

| Project | License, activity | Approach | Interfaces | Fit for Evo |
| --- | --- | --- | --- | --- |
| [sunerpy/codegraph-rust](https://github.com/sunerpy/codegraph-rust) (fork of 0717wuwang/codegraph-rust) | MIT, frequent releases (v0.55) | Deterministic tree-sitter + SQLite/FTS5 graph of symbols, calls and dependencies. No LLM or embeddings; byte-stable output | CLI, MCP server, local browser viewer | **Closest to Evo's stack and needs.** Same storage (SQLite) and no model inside. A design to follow and code to borrow with attribution. |
| [bartolli/codanna](https://github.com/bartolli/codanna) | Apache-2.0, ~750 stars, v0.9.x | tree-sitter for 13 languages, call graph, impact analysis, semantic search with embeddings | CLI, MCP (`find_callers`, `get_calls`, `analyze_impact`, `find_symbol`) | Mature agent-facing tools. Brings a search index and embedding models; Windows support is marked experimental. Works today as a connector. |
| [postrv/narsil-mcp](https://github.com/postrv/narsil-mcp) | MIT/Apache, v1.7 | Symbols, call graph, control and data flow, taint analysis, SBOM, optional RDF/SPARQL graph | MCP with 75–90 tools, 16–32 languages | Very broad. Far more surface than Evo needs, and too many tools for an agent's tool list. |
| [Muvon/octocode](https://github.com/muvon/octocode) | Apache-2.0, ~470 stars | Semantic search plus a tree-sitter graph | CLI, MCP | Leans on embeddings and semantic search. |
| symgraph (Rust port of [colbymchenry/codegraph](https://github.com/colbymchenry/codegraph), which is TypeScript/WASM) | MIT | tree-sitter + SQLite graph | MCP | Young port; the original is not Rust. |
| phuetz/code-explorer | **BSL 1.1** (not open source) | Graph over 14 languages | MCP | Excluded by its license. |

### 3.3 SCIP indexers (the precise layer)

| Indexer | Languages | Runs on | Notes |
| --- | --- | --- | --- |
| `rust-analyzer scip` | Rust | Windows, macOS, Linux (rustup component) | Runs `cargo metadata`, build scripts and proc macros; recent regressions reported |
| scip-typescript | TypeScript, JavaScript | all (Node) | The reference indexer; type-checks with the TypeScript compiler, executes no project code |
| scip-python | Python | all (Node, based on pyright) | Static analysis only |
| scip-go | Go | all (Go toolchain) | Loads packages with the Go toolchain |
| scip-java | Java, Kotlin, Scala | all (JVM) | Runs the project's build (Gradle, Maven, sbt) to compile |
| scip-clang | C, C++ | macOS, Linux; **no native Windows build** | Needs a compilation database (`compile_commands.json`) |
| scip-ruby | Ruby | macOS, Linux; **no native Windows build** | Based on Sorbet |
| scip-dotnet | C#, Visual Basic | all (.NET SDK) | Runs a design-time build |
| scip-dart | Dart | all (Dart SDK) | |
| scip-php | PHP | all (PHP) | |

[SCIP-IO](https://github.com/GlitterKill/scip-io) (Rust) installs and manages these indexers and merges indexes of several languages; a reference for the install and merge parts. Where an indexer cannot run (scip-clang and scip-ruby on Windows, a project that does not build, an indexer not installed), the tree-sitter layer answers (D6).

What they share: tree-sitter for parsing, tags-style queries or per-language
extractors for definitions, references and calls, a local store (SQLite or a
search index), and a small set of agent tools (find symbol, callers,
callees, impact, outline, map). The precise tools (SCIP, stack graphs) are
either heavy or abandoned. Every agent-oriented tool listed chose
name-based resolution plus ranking because it is fast, works on code that
does not build, and needs no per-language install.

## 4. Options for Evo

| Option | What it is | For | Against |
| --- | --- | --- | --- |
| **A. Connector only** | Recommend codanna or narsil-mcp as a connector (Customize → Connectors); no Evo code | Works today; zero maintenance | A second process with its own index and update cycle; no project map in the prompt; no Evo UI; tool names and answers outside Evo's control; Windows support uneven |
| **B. Embed a tool as a library** | Link codanna (or narsil) into the engine | Mature extraction for many languages | Not built to be embedded; heavy dependency trees (search index, embedding runtimes); a large surface to keep up with; still not shaped for Evo's prompt and UI |
| **C. Own crate on tree-sitter** (recommended) | A leaf crate `evo-codegraph`: tree-sitter grammars + tags queries into SQLite, incremental by file hash; personalized PageRank for a project map; a `code_graph` tool; later an optional precise layer from SCIP indexes or the language servers Evo already runs | Fits the stack (SQLite, polling watcher, `cc`); deterministic and offline; small agent surface Evo controls; works in code mode for free; borrows the design of sunerpy/codegraph-rust (MIT) and the ranked repository map (Apache-2.0) | Evo owns it; tags-based answers are approximate until the precise layer exists; each grammar adds binary size |

## 5. Big questions

Facts are cited in section 2. **D** marks a decision for the owner (with
the recommended answer), **A** an assumption recorded with its default.

| # | Question | Answer |
| --- | --- | --- |
| 1 | Why now, and what outcome shows it works? | Fewer exploring turns and tokens on unfamiliar or large projects, and correct impact lists for multi-file changes. Measured with a benchmark like code mode's (`scripts/bench/`). |
| 2 | **D1** Which approach: connector, embedded tool, or own crate? | **Decided (2026-11-22): C, own crate `evo-codegraph`**, borrowing sunerpy/codegraph-rust's design. A stays possible for anyone today. |
| 3 | **D2** What does the agent get in the first version? | **Decided (2026-11-22): a `code_graph` tool plus a ranked project map in the system prompt**, cut to a token budget. |
| 4 | **D3** How precise must the first version be? | **Decided (2026-11-22): precise from the first version**, from SCIP indexes (the recommendation was name-based first). This brings D6–D8 (round 2): which indexers, what answers while an index is not ready, and trust, since indexers run project tooling. |
| 5 | **D4** Where does the index live? | **Decided (2026-11-22): under Evo's data folder** (`~/.evo/agent/codegraph/<project hash>.db`), never in the repository. |
| 6 | **D5** Which languages first? | **Decided (2026-11-22): every language with a SCIP indexer** (table 3.3), with tree-sitter grammars for the same languages as the fallback (D6). Each grammar adds roughly 0.5–3 MB to the engine. |
| 6a | **D6** What does `code_graph` answer while an index is not ready, for files changed since the last index, or for a language with no indexer on this computer? | **Decided (2026-11-22): it falls back to tree-sitter tags**, and every answer names its source: `precise` (SCIP) or `approximate` (tags). |
| 6b | **D7** When may an indexer run? Indexers run project tooling (rust-analyzer runs build scripts and proc macros). | **Decided (2026-11-22): only in trusted projects; the first index starts when the person presses Index**, then it re-indexes in the background when idle and after files change. |
| 6c | **D8** How are indexers installed? | **Decided (2026-11-22): through the code intelligence catalog**: an indexer already on the computer is used, others are installed when the person presses Install, all managed on one page. |
| 7 | Who turns it on, and when does indexing run? | **A:** **Settings → Experimental → Code graph**, off by default. When on, the engine indexes the chat's project in the background on first use and keeps it fresh from file changes. |
| 8 | Which files are indexed? | **A:** Files `.gitignore` and `.ignore` allow, under a size limit per file (1 MB) and per project (50,000 files), skipping generated and vendored folders. |
| 9 | Edges: very large repos, binary files, syntax errors, several chats on one project, engine restart? | **A:** Indexing is incremental and resumable, so a restart continues where it stopped. tree-sitter tolerates syntax errors. One index per project is shared by its chats, with SQLite's locking. A project over the limit indexes its most recently changed files first and says so. |
| 10 | Trust: does it read untrusted content or run anything? | Reads files only; never runs project code (unlike SCIP indexers, which build the project, and which are therefore a later, opt-in step). Symbol names and snippets in answers are untrusted data, like any tool output. |
| 11 | Layers and compatibility | A new leaf crate (an ADR), an engine tool and prompt section, a setting and its fan-out in the shell, and an Experimental switch in the UI. No protocol break: older clients ignore the new state field. A graph view in the desktop is a later phase. |
| 12 | Done: how is it checked? | Unit tests on fixture projects per language (definitions, references, callers, ranking); the tool's answers against a small known repository; a benchmark of turns and tokens with the graph on and off; indexing time and memory on a large repository (this one). |
| 13 | Cost of being wrong | The index format (the SQLite schema) and the tool's schema. Both are versioned: an index with an older version is rebuilt, not migrated. |

## 6. Shape after the decisions (input to the spec and plan)

- **Crate `evo-codegraph`** (leaf), two layers in one SQLite store
  (`~/.evo/agent/codegraph/<project hash>.db`, FTS5 for names):
  - **Precise layer:** run the SCIP indexers of the project's languages,
    read their `.scip` output with the `scip` crate, and store symbols,
    definitions, references, implementations and the call edges derived
    from them, each tied to the file hash it was indexed at.
  - **Fallback layer:** tree-sitter tags for every file, kept fresh from
    file changes at once; answers for files with no precise data, or
    changed since the last index, come from it, marked `approximate`.
  - Personalized PageRank over the merged graph for the project map.
- **Engine:** a `code_graph` tool with a few operations (`find`,
  `definition`, `references`, `callers`, `callees`, `outline`, `impact`,
  `map`), class read, safe for read-only agents. A "Project map" section in
  the system prompt (top files and symbols within a token budget). Indexing
  runs on a background task with progress in the chat's state.
- **Code mode:** nothing to add; `tools.code_graph(...)` works in programs.
- **Settings → Experimental → Code graph**, plus the index's state (files,
  symbols, last update, Rebuild) on the same page.
- **Indexers:** entries in the code intelligence catalog (detect, Install,
  Uninstall), run only in trusted projects, first run on the person's Index
  (in the chat or on the settings page), then in the background when idle
  and after changes, debounced; progress and the last result per language
  on the settings page.
- **Later phases:** refine answers with the language servers Evo already
  runs; a graph view in the side panel.

## 7. Sources

- tree-sitter-tags: <https://docs.rs/tree-sitter-tags>; GitHub code navigation: <https://github.com/github/code-navigation>
- stack-graphs (archived): <https://github.com/github/stack-graphs>; tree-sitter-graph: <https://github.com/tree-sitter/tree-sitter-graph>
- SCIP: <https://github.com/sourcegraph/scip>, <https://docs.rs/crate/scip/latest>; rust-analyzer `scip`: <https://docs.rs/crate/ra_ap_rust-analyzer/latest/source/src/cli/scip.rs>
- SCIP indexers (list): <https://github.com/scip-code/scip>, <https://sourcegraph.com/docs/code-navigation/writing-an-indexer>; SCIP-IO: <https://github.com/GlitterKill/scip-io>
- ast-grep-core: <https://docs.rs/ast-grep-core>
- codegraph-rust: <https://github.com/sunerpy/codegraph-rust>, <https://github.com/0717wuwang/codegraph-rust>
- codanna: <https://github.com/bartolli/codanna>
- narsil-mcp: <https://github.com/postrv/narsil-mcp>
- octocode: <https://github.com/muvon/octocode>
- colbymchenry/codegraph: <https://github.com/colbymchenry/codegraph>; symgraph: <https://docs.rs/crate/symgraph/latest>
- code-explorer: <https://github.com/phuetz/code-explorer>
- Ranked repository map, Rust ports: <https://docs.rs/crate/ripmap/latest>, <https://docs.rs/crate/repo-mapper/latest>
