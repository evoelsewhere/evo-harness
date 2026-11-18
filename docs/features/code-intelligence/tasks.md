# Code intelligence (language servers) — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-18 |

## 1. Phases

### Phase 1 — Language server library

Done when: `evo-lsp` finds, installs, starts and queries servers, and
`examples/lsp-try.rs` answers from a real server.

- [x] T1 [FR-5] Server catalog — `crates/evo-lsp/src/catalog.rs` — check: `cargo test -p evo-lsp files_map_to_servers`
- [x] T2 [P] [FR-11] Project language detection — `crates/evo-lsp/src/detect.rs` — check: `cargo test -p evo-lsp a_mixed_project_is_detected`
- [x] T3 [P] [FR-6, FR-7, FR-8, NFR-1, NFR-2] Locate and install servers — `crates/evo-lsp/src/install.rs` — check: `cargo test -p evo-lsp binaries_are_found_in_unpacked_folders`, `missing_programs_are_reported`
- [x] T4 [FR-3, FR-4] LSP client over stdio with indexing waits and diagnostics — `crates/evo-lsp/src/client.rs` — check: manual run with rust-analyzer
- [x] T5 [FR-10] Server pool per (server, root) — `crates/evo-lsp/src/pool.rs` — check: `cargo test -p evo-lsp roots_are_the_nearest_marker_folder`
- [x] T6 [FR-1, FR-2, FR-3, NFR-3] Operations and positions — `crates/evo-lsp/src/ops.rs` — check: `cargo test -p evo-lsp positions_find_symbols_on_word_boundaries`, `diagnostics_read_well`, `locations_take_every_shape`
- [x] T7 Terminal tryout — `crates/evo-lsp/examples/lsp-try.rs` — check: runs against a scratch Cargo project

### Phase 2 — Engine tool and RPC

Done when: a chat's agent can call `lsp`, and the RPC commands list,
install, uninstall and switch servers.

- [x] T8 [FR-1, FR-2, FR-9] The `lsp` tool and `disabledLanguageServers` — `crates/evo-engine/src/lsp_tool.rs` — check: manual run with a real model
- [x] T9 [FR-7, FR-8, FR-9] RPC `list_language_servers`, `install_language_server`, `uninstall_language_server`, `set_language_server_enabled` — `crates/evo-engine/src/lsp_tool.rs` — check: `cargo test -p evo-engine the_list_names_every_server`

### Phase 3 — Desktop

Done when: the page manages servers, the notice appears once per folder, and
transcript rows read in words.

- [x] T10 [P] [FR-12] **Customize → Code intelligence** page — `apps/desktop/src/features/customize/languages-section.tsx` — check: manual in the running app
- [x] T11 [P] [FR-13] Once-per-folder notice — `apps/desktop/src/stores/engine-store.ts` — check: manual in the running app
- [x] T12 [P] [FR-14] Transcript row labels — `apps/desktop/src/components/ai-elements/tool.tsx` — check: manual in the running app

### Phase 4 — Errors with the edit (2026-11-18)

Done when: an `edit` / `write` that breaks a file returns the server's new
errors within the wait, and the `lsp` tool is opt-in.

- [x] T13 [FR-16, FR-18, FR-19] Compare errors and word the result, tests first — `crates/evo-lsp/src/edit_check.rs` — check: `cargo test -p evo-lsp edit_check`
- [x] T14 [FR-15, FR-17] `errors_before` / `check_edit` with the 4 s budget; `Pool::running_for`, `Pool::warm`; `Client::last_diagnostics` — `crates/evo-lsp/src/{edit_check,pool,client}.rs` — check: `cargo run -p evo-lsp --example edit-check` against a scratch Cargo project
- [x] T15 [FR-15, FR-17, FR-20, FR-21] `EditCheck` wrapper; settings `editDiagnostics`, `lspTool`; `lsp` registered only when switched on — `crates/evo-engine/src/tools/{lsp,mod}.rs`, `crates/evo-engine/src/settings.rs` — check: `cargo test -p evo-engine edit_check`
- [x] T16 [FR-20] RPC `set_code_intelligence_options`, `options` in the list — `crates/evo-engine/src/{modes,rpc/protocol}.rs` — check: `cargo test -p evo-engine customization_and_mesh_commands_pass_payload_validation`
- [x] T18 [FR-17, FR-22] Warm the server on `read`; the edit waits for a starting server within its budget — `crates/evo-lsp/src/{edit_check,pool}.rs`, `crates/evo-engine/src/tools/{lsp,mod}.rs` — check: example `edit-check`; real model in the app
- [x] T19 [FR-17, FR-23] New errors in other files: per-file baseline when the server is idle, compared after the wait, one cap for all — `crates/evo-lsp/src/{edit_check,client}.rs` — check: `cargo test -p evo-lsp edit_check`; example `edit-check` with a two-file Cargo project
- [x] T20 [FR-17, FR-22, FR-24] TypeScript: `typescript@5` and `tsserver.fallbackPath`; quiet and settle rules; `read` opens the file in a running server — `crates/evo-lsp/src/{catalog,install,client,pool,edit_check}.rs` — check: example `edit-check` with rust-analyzer, Pyright, TypeScript and JSON servers
- [x] T21 [FR-25, FR-26] Settings answer per server; an install that cannot work is flagged for Reinstall — `crates/evo-lsp/src/{client,install}.rs`, `crates/evo-engine/src/tools/lsp.rs`, `apps/desktop/src/features/customize/languages-section.tsx` — check: `cargo test -p evo-lsp a_typescript_server_without_typescript_5_needs_reinstall`; example `edit-check` with CSS / HTML / JSON; the page in the running app
- [x] T22 [FR-17, FR-27, FR-28, FR-29] Pull diagnostics; silence learnt per server; duplicates by message; csharp-ls fallback; ShellCheck note; server log messages kept — `crates/evo-lsp/src/{client,edit_check,pool,install,catalog}.rs`, `apps/desktop/src/features/customize/languages-section.tsx` — check: example `edit-check` on ten more languages; `cargo test -p evo-lsp`
- [x] T17 [FR-20] Two switches on **Customize → Code intelligence** — `apps/desktop/src/features/customize/languages-section.tsx`, `apps/desktop/README.md` — check: `bun run typecheck`; manual in the running app

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Manual: real rust-analyzer on a scratch Cargo project — diagnostics, references (definition and three calls with their lines), hover (signature and doc comment), definition (jumped to `math.rs`) | passed (2026-11-06) for those four operations; the others not run |
| FR-2 | `cargo test -p evo-lsp positions_find_symbols_on_word_boundaries` | not run |
| FR-3 | Manual: diagnostics returned the compiler's E0308 with its hints from a cold start | passed (2026-11-06) |
| FR-4 | Manual: edit a file in a scratch Cargo project, then ask for diagnostics | not run |
| FR-5 | `cargo test -p evo-lsp files_map_to_servers`; `cargo test -p evo-engine the_list_names_every_server` | not run |
| FR-6 | `cargo test -p evo-lsp binaries_are_found_in_unpacked_folders` | not run |
| FR-7 | Manual: Install from npm and GitHub in the running app | not run (installs download and run code; done only on the person's press) |
| FR-8 | Manual: Uninstall an evo install in the running app | not run |
| FR-9 | Manual: switch a server off on the page, then call `lsp` | not run |
| FR-10 | `cargo test -p evo-lsp roots_are_the_nearest_marker_folder` | not run |
| FR-11 | `cargo test -p evo-lsp a_mixed_project_is_detected` | not run |
| FR-12 | Manual: **Customize → Code intelligence** in the running app | not run |
| FR-13 | Manual: open a chat in a project with three or more files of a language without a server | not run |
| FR-14 | Manual: transcript rows after `lsp` calls | not run |
| NFR-1 | Review: installs start only from `install_language_server` | not run |
| NFR-2 | Review of `install.rs`; manual install on Windows | not run |
| NFR-3 | `cargo test -p evo-lsp positions_find_symbols_on_word_boundaries` | not run |
| SC-1 | Manual: cold-start diagnostics on a scratch Cargo project | passed (2026-11-06, about 7 s) |
| SC-2 | Manual: a real model used diagnostics and references in parallel on its own and answered correctly | passed (2026-11-06); retired 2026-11-18 |
| FR-15 | Example `edit-check` with real rust-analyzer and Pyright on scratch projects: a breaking change returned the compiler's error with the write (rustc E0308 in 0.9 s; Pyright reportArgumentType in 0.6 s) | passed (2026-11-18) |
| FR-16 | `cargo test -p evo-lsp edit_check`; example `edit-check` with Pyright (one new error listed, "1 other error was already there") | passed (2026-11-18) |
| FR-17 | `cargo test -p evo-engine edit_check`; example `edit-check`: with rust-analyzer a clean edit now returns in 0.2–0.7 s (was the whole 4 s); a write with no server waits for the start within 10 s (Pyright: answered within it; rust-analyzer: 4.0 s, nothing added, running after it) | passed (2026-11-18) |
| FR-18 | `cargo test -p evo-lsp edit_check`; example `edit-check` (fixing the error said "no errors left" with both servers) | passed (2026-11-18) |
| FR-19 | `cargo test -p evo-lsp edit_check` | passed (2026-11-18) |
| FR-20 | `cargo test -p evo-engine the_list_names_every_server`, `customization_and_mesh_commands_pass_payload_validation`; `bun run typecheck`; in the running app (test copy, scratch data): both switches shown, "Code navigation tool" flipped on and off at once and `lspTool` saved | passed (2026-11-18) |
| FR-21 | Review: the check runs only after a successful `edit` / `write` (`EditCheck::execute` returns the inner error first); `WarmOnRead` checks `current_is_read_only()` | passed (2026-11-18) |
| FR-23 | `cargo test -p evo-lsp edit_check`; example `edit-check` with rust-analyzer on a two-file Cargo project (changing `add` to return `i64` listed `src/main.rs:4:22 … E0308` in 0.7 s; with `READ_FIRST_MS=2500`, a cold server, it was listed in 4.3 s); running app, real model, new chat, "make `add` return i64": the edit came back with the caller's error and the model read and fixed `src/main.rs` on its own (two runs before FR-23 left it broken) | passed (2026-11-18) |
| FR-24 | Example `edit-check` with the TypeScript server installed by `lsp-try install typescript` into a scratch dir (a project without `node_modules`): `Cannot find name 'missingValue' [typescript 2304]` in 0.7–1.4 s; with `src/main.ts` read, changing `add` to return `string` listed `src/main.ts:3:7 … [typescript 2322]`; clean edits 1.1–1.5 s | passed (2026-11-18) |
| FR-25 | `cargo test -p evo-lsp a_typescript_server_without_typescript_5_needs_reinstall`; running app (test copy) with a copy of the TypeScript 7 install: the row read "Installed by evo · Needs reinstall" with the reason, and nothing was installed (`installing: false`) | passed (2026-11-18) |
| FR-26 | Example `edit-check`: CSS `} expected [css css-rcurlyexpected]`, HTML `Expression expected. [javascript]` (embedded script), JSON `Trailing comma [json 519]`, each fixed edit "no errors left"; Rust, Python, TypeScript unchanged | passed (2026-11-18) |
| FR-27 | Example `edit-check`, C# (csharp-ls 0.16.0, .NET 8): `Cannot implicitly convert type 'string' to 'int' [lsp CS0029]`, then "no errors left" | passed (2026-11-18) |
| FR-28 | `cargo test -p evo-lsp edit_check_reports_one_error_per_place_and_code`; Svelte showed one "Block was left open" | passed (2026-11-18) |
| FR-29 | `lsp-try install csharp-ls` with .NET SDK 8: the latest failed, 0.16.0 installed; Bash without ShellCheck reported nothing (the note explains it) | passed (2026-11-18); the page note not seen in the app |
| Languages | Example `edit-check` (broken, fixed, clean edit) with warm servers: Go, C/C++, C#, PHP, YAML, Dockerfile, Svelte, Lua, Rust, Python, TypeScript, JSON, HTML, CSS all reported the error, "no errors left", then nothing; Kotlin nothing (no `kotlin-stdlib` on this computer); Bash nothing (no ShellCheck); Markdown only warnings; Ruby, Java, Swift, Dart not tried (no Ruby, jdtls, Swift or Dart SDK here) | 14 of 21 passed (2026-11-18) |
| FR-22 | Running app, real model (Copilot claude-sonnet-5.5), new chat in a scratch Cargo project: read, then an edit making `sum` return `u64` came back with `src/main.rs:8:22 error: mismatched types … [rustc E0308]` | passed (2026-11-18) |
| SC-3 | Running app, real model: the error came with the first edit (the server was warmed by the `read` before it); told to change only `sum`, the model quoted the error (line 8, E0308) and offered the fix instead of claiming success. Without the check (earlier build) it could only reason that `main` would break. Unconstrained, it changed both in one edit and nothing was added. | passed (2026-11-18), small trial |
