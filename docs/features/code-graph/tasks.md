# Code graph: the agent asks the project's structure instead of grepping — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-23 |

## 1. Phases

### Phase 0 — Spikes

The two risks the plan rests on, checked before product code. Results go to
[research.md](research.md); a finding that changes the plan reopens it.

Done when: the grammar set builds on Windows, macOS and Linux with one
tree-sitter core, its size is known, and SCIP output of rust-analyzer and
scip-typescript reads with the `scip` crate on this repository.

- [ ] T1 [FR-3, FR-4, NFR-5] Build a throwaway binary with tree-sitter and the grammar crates of all FR-4 languages (Rust, TypeScript, TSX, JavaScript, Python, Go, Java, Kotlin, Scala, C, C++, Ruby, C#, Dart, PHP); record versions, which ship `tags.scm`, and the size each adds — scratch crate, `research.md` — check: builds on the three CI platforms; table in research
- [ ] T2 [FR-4, FR-7] Run `rust-analyzer scip` and scip-typescript on this repository on Windows; read both with the `scip` crate; record time, size, roles and whether `enclosing_range` is present — scratch, `research.md` — check: both read; numbers in research

### Phase 1 — The crate: syntax layer for Rust and TypeScript/JavaScript

Done when: on fixture projects the syntax layer answers every operation,
fresh after edits, within NFR-1 and NFR-2 on this repository.

- [ ] T10 [FR-2, FR-15, NFR-7] Write store tests: schema created, version mismatch rebuilds, WAL, two writers — `crates/evo-codegraph/src/store.rs` — check: `cargo test -p evo-codegraph store` fails for the right reason
- [ ] T11 [FR-2, FR-15, NFR-7] Implement the store (plan 3.4) and add the crate to the workspace — `crates/evo-codegraph/`, `Cargo.toml` — check: T10 passes
- [ ] T12 [FR-3, FR-11] Write walk tests: ignore files, skipped folders, 1 MB files, the 50,000-file limit with most recent first and `leftOut` — `crates/evo-codegraph/src/walk.rs` — check: fails for the right reason
- [ ] T13 [FR-3, FR-11] Implement the walk — `crates/evo-codegraph/src/walk.rs` — check: T12 passes
- [ ] T14 [FR-3, FR-8] Write syntax tests on fixtures for Rust and TS/JS: definitions, references, calls, imports, enclosing scopes, syntax errors tolerated — `crates/evo-codegraph/tests/fixtures/{rust,ts}/` — check: fails for the right reason
- [ ] T15 [FR-3] Implement the syntax modules for Rust, TypeScript, TSX and JavaScript (tags plus call and import queries) — `crates/evo-codegraph/src/syntax/` — check: T14 passes
- [ ] T16 [FR-7, FR-8] Write query tests: every operation, symbol ids, a name defined twice, the 100-item cap, per-item source — `crates/evo-codegraph/src/query.rs` — check: fails for the right reason
- [ ] T17 [FR-7, FR-8] Implement the queries — `crates/evo-codegraph/src/query.rs` — check: T16 passes
- [ ] T18 [FR-7, US-5] Write and implement freshness: changed files re-parsed at query time, items marked `file_changed` — `crates/evo-codegraph/src/fresh.rs` — check: tests pass
- [ ] T19 [FR-10] Write and implement ranking and the map within a token budget; focus files rank their neighbours first — `crates/evo-codegraph/src/rank.rs` — check: tests pass
- [ ] T20 [NFR-1, NFR-2] Benchmark tests on this repository: full syntax index, one-file update, query p95 — `crates/evo-codegraph/benches/` — check: within NFR-1 and NFR-2

### Phase 2 — Engine and shell, syntax layer only

Done when: with the switch on, a chat's agent has `code_graph` and a project
map from the syntax layer, indexing runs in the index process the shell
starts, and with it off nothing changes.

- [ ] T30 [FR-1, NFR-6] Write engine tests: off by default, `--code-graph`, `set_code_graph` live, `get_state.codeGraph`, tool list and prompt unchanged when off — `crates/evo-engine` — check: fails for the right reason
- [ ] T31 [FR-1, FR-9] Implement `code_graph.rs` (state, apply), the tool row (read, read-only safe), `--code-graph`, the RPC and state field — `crates/evo-engine/src/code_graph.rs`, `tools/info.rs`, `args.rs`, `rpc/protocol.rs`, `modes.rs` — check: T30 passes
- [ ] T32 [FR-8, FR-9] Implement the `code_graph` tool over `evo-codegraph` (result text and details, plan 3.3) — `crates/evo-engine/src/tools/code_graph.rs` — check: tool tests on a fixture; a code mode program calls it
- [ ] T33 [FR-10] Add the Project map section and the tool's prompt guideline — `crates/evo-engine/src/session.rs` — check: prompt snapshot with it on, unchanged with it off
- [ ] T34 [FR-3] Implement `--mode codegraph-index` (syntax pass, result lines) — `crates/evo-engine/src/code_graph_index.rs`, `args.rs`, `host.rs` — check: run on a fixture prints progress and done
- [ ] T35 [FR-1, FR-3, FR-13] Shell: `experimental.codeGraph`, `--code-graph`, `Change::CodeGraph`; the worker with its queue and triggers (chat opened, run settled + idle, off stops it) — `apps/desktop/src-tauri/src/codegraph.rs`, `engine.rs`, `supervisor/settings.rs` — check: `bun run test:shell` worker tests
- [ ] T36 [P] [FR-1] UI: the Code graph switch on the Experimental page — `apps/desktop/src/features/settings/sections/experimental-settings.tsx` — check: `bun run typecheck`

### Phase 3 — Precise layer for Rust and TypeScript/JavaScript

Done when: in a trusted project, Index produces precise answers for Rust and
TS/JS that match find-references on the fixtures (SC-2), and no indexer
ever starts otherwise (SC-3).

- [ ] T40 [FR-4, FR-7] Write SCIP import tests from recorded `index.scip` files of the fixtures — `crates/evo-codegraph/src/scip.rs` — check: fails for the right reason
- [ ] T41 [FR-4, FR-7] Implement the import (symbols, occurrences, implementations, enclosing scopes from SCIP or the syntax layer, per-file hashes, one transaction per language) — `crates/evo-codegraph/src/scip.rs` — check: T40 passes
- [ ] T42 [FR-6] Add `Indexer` entries for rust-analyzer and scip-typescript to the catalog, with detection — `crates/evo-lsp/src/catalog.rs`, `detect.rs` — check: catalog tests
- [ ] T43 [FR-5, FR-12, SC-3] Write index-process tests: untrusted refuses, missing indexer reported, failing indexer reported, time limit, killed mid-run rolls back — `crates/evo-engine` — check: fails for the right reason
- [ ] T44 [FR-5, FR-12] Implement `--precise` in the index process (trust, catalog, run, import, environment without Evo's secrets) — `crates/evo-engine/src/code_graph_index.rs` — check: T43 passes
- [ ] T45 [FR-5] The index offer: `indexOffer` in results, the chat card (Index / Not now), `codegraph_index`, `codegraph_dismiss_offer` — `crates/evo-engine/src/tools/code_graph.rs`, `apps/desktop/src/components/ai-elements/`, `apps/desktop/src-tauri/src/codegraph.rs` — check: card tests; manual in the running app
- [ ] T46 [SC-2] Precise answers match find-references on the Rust and TS fixtures for every symbol checked — `crates/evo-codegraph/tests/` — check: passes

### Phase 4 — Every language with an indexer

Done when: each FR-4 language answers from the syntax layer, and precisely
where its indexer runs on the platform.

- [ ] T50 [P] [FR-3] Syntax modules and fixtures for Python, Go, Java, Kotlin, Scala — `crates/evo-codegraph/src/syntax/`, `tests/fixtures/` — check: syntax tests per language
- [ ] T51 [P] [FR-3] Syntax modules and fixtures for C, C++, Ruby, C#, Dart, PHP — same paths — check: syntax tests per language
- [ ] T52 [FR-6] Install kinds `Coursier`, `DartPub`, `Composer` — `crates/evo-lsp/src/catalog.rs`, `install.rs` — check: install tests with a fake toolchain
- [ ] T53 [FR-4, FR-6, FR-12, NFR-5] Indexer entries for scip-python, scip-go, scip-java, scip-clang (needs `compile_commands.json`), scip-ruby, scip-dotnet, scip-dart, scip-php; `unsupported` on platforms without a native build — `crates/evo-lsp/src/catalog.rs`, `crates/evo-engine/src/code_graph_index.rs` — check: precise fixture run per language on the platforms that support it

### Phase 5 — Settings, worktrees and the edges

Done when: the person can see and manage indexes and indexers, worktrees
share the source index, and every edge case of the spec behaves as written.

- [ ] T60 [FR-14] Shell commands `codegraph_projects`, `codegraph_rebuild`, `codegraph_delete`, progress events — `apps/desktop/src-tauri/src/codegraph.rs` — check: shell tests
- [ ] T61 [FR-14, FR-13] The per-project list on the Experimental page (counts, layers, errors, Index now, Rebuild, Delete index with confirmation) — `apps/desktop/src/features/settings/sections/experimental-settings.tsx` — check: `bun run typecheck`; manual in the running app
- [ ] T62 [P] [FR-6] Indexers in Customize → Languages with Install / Uninstall — `apps/desktop/src/features/customize/languages-section.tsx` — check: manual in the running app
- [ ] T63 [FR-16] Worktree → source index; the worktree's changed files approximate; no indexer for it — `crates/evo-engine/src/code_graph.rs` — check: test with a git worktree fixture
- [ ] T64 [FR-11, FR-13] Limits reported in answers and on the page; turning off kills a running index process and keeps the file — check: tests
- [ ] T65 Update the UI map — `apps/desktop/README.md` — check: review

### Phase 6 — Benchmark and verification

Done when: every row of the verification table has a result.

- [ ] T70 [SC-1] The exploration benchmark: 12 tasks in three open-source projects of different languages, each needing at least four turns with code graph off; off and on with one model — `scripts/bench/code-graph/` — check: report with turns, input tokens and tasks solved
- [ ] T71 Run every check below and record the results — this file — check: the table is filled

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | T30; switch on mid-chat in the running app | not run |
| FR-2 | T10; no file written in the project after indexing | not run |
| FR-3 | T12, T14, T34, T50, T51 | not run |
| FR-4 | T41, T44, T53 | not run |
| FR-5 | T43, T45 | not run |
| FR-6 | T42, T52, T53, T62 | not run |
| FR-7 | T16, T18, T40 | not run |
| FR-8 | T16, T32 | not run |
| FR-9 | T31, T32 (read-only agent, code mode program) | not run |
| FR-10 | T19, T33 | not run |
| FR-11 | T12, T64 | not run |
| FR-12 | T43, T53 | not run |
| FR-13 | T35, T64 | not run |
| FR-14 | T60, T61 in the running app | not run |
| FR-15 | T10 | not run |
| FR-16 | T63 | not run |
| NFR-1 | T20 | not run |
| NFR-2 | T20 | not run |
| NFR-3 | a turn timed during indexing | not run |
| NFR-4 | network check during syntax indexing and queries | not run |
| NFR-5 | CI on three platforms; T53 unsupported platforms | not run |
| NFR-6 | T30 | not run |
| NFR-7 | size of this repository's index | not run |
| SC-1 | T70 | not run |
| SC-2 | T46 | not run |
| SC-3 | T43 | not run |
| SC-4 | T30 | not run |
