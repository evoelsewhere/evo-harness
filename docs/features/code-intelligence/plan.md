# Code intelligence (language servers) — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-18 |
| ADD sections | [4. Process view](../../architecture/README.md#4-process-view) |
| ADRs | none |

## 1. Context and constraints

- The engine runs each chat in its own process
  ([ADD 4](../../architecture/README.md#4-process-view)), so each engine owns
  the language servers its chat uses.
- Language servers speak LSP over stdio. Positions in LSP are UTF-16 code
  units.
- Servers come from many places: their own installers (npm, `go install`,
  `rustup`, `dotnet tool`, `gem`), GitHub releases, a rustup toolchain, PATH,
  or a copy shipped inside a VS Code-style editor's extensions.
- Installing downloads and runs code from the internet, so it happens only
  when the person presses Install.
- Some servers report readiness late: rust-analyzer reports `serverStatus`,
  others work-done progress; diagnostics may arrive in two passes (a quick
  one, then the slower check).
- The errors after an edit ride on the edit's result, so the check must stay
  short: a cold server start (about 7 s for rust-analyzer) or a full
  `cargo check` cannot be waited for on every edit.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | The `lsp` tool, the server pool and installs live in the engine; the desktop only shows the page and the notice. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | `evo-lsp` is a library with no engine or desktop dependency; the engine wraps it in `lsp_tool.rs`. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | ok | New RPC commands (`list_language_servers`, `install_language_server`, `uninstall_language_server`, `set_language_server_enabled`) ship with their desktop callers. |
| Models come from models.dev; no hardcoded model facts | n/a | No model facts involved. |
| Real engine in the app; settings and credentials reload live | ok | `disabledLanguageServers` is a setting read by the engine; no app restart. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | The tool is read-only toward the project; installs run only on the person's Install press (NFR-1). The edit check runs after `edit` / `write` have passed approval and the sandbox, and never in a read-only session (FR-21). |
| Desktop UI structure and `DESIGN.md` | ok | The page is a Customize section; it reaches the engine through the engine store. |

## 3. Design

### 3.1 Overview

```text
 agent ──lsp tool──► engine (lsp_tool.rs) ──► evo-lsp pool ── one server per (server, root)
                          │                        │
                          │                        └─ client.rs: LSP over stdio
                          │
 desktop Customize → Code intelligence ──RPC──► list / install / uninstall / enable
 engine-store (once per folder) ──list_language_servers──► notice with link to the page
```

```text
 agent ──edit / write──► engine: EditCheck wrapper (tools/lsp.rs)
                            1. before: the server's last errors for the file (no wait)
                            2. run the real edit / write
                            3. after: edit_check::check_edit — sync the file, wait ≤ 4 s,
                               compare the errors, add a text part to the result
                            (no running server → start one, wait for it within the 4 s)
 agent ──read──► engine: WarmOnRead wrapper → start the file's server in the background
```

`evo-lsp` holds the catalog, the installers, project detection, the stdio
client, the pool and the operations. The engine wraps `edit` and `write` with
the edit check, exposes the `lsp` tool (when switched on) and five RPC
commands. The desktop shows the page and a one-time notice.

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| crate | `crates/evo-lsp/src/catalog.rs` | 21 servers; each: file extensions → language ids, root markers, command and arguments, how to install, programs it needs. HTML / CSS / JSON come from `vscode-langservers-extracted`. | FR-5 |
| crate | `crates/evo-lsp/src/install.rs` | Where a server is: evo's own install in `<agent dir>/lsp/<id>`, then (rust-analyzer) the folder's rustup toolchain, then PATH, then a copy shipped with a VS Code-style editor (`.vscode`, `.vscode-insiders`, `.cursor`, `.windsurf` extensions). Installers; no window, 15-minute limit, the output's tail kept for the app. | FR-6, FR-7, FR-8, NFR-1, NFR-2 |
| crate | `crates/evo-lsp/src/detect.rs` | Which languages a project uses: files by extension and root markers, four levels deep, skipping dependency and build folders. | FR-11 |
| crate | `crates/evo-lsp/src/client.rs` | LSP over stdio: one writer task, requests matched by id, the server's requests answered with defaults (`workspace/configuration`, progress tokens, …), published diagnostics keyed by file whatever the URI spelling, work-done progress and rust-analyzer's `serverStatus` so a request waits for indexing; just after start it gives the server a few seconds to begin. Opens (and saves, for servers that check on save) or updates files with their current text before each request. | FR-3, FR-4 |
| crate | `crates/evo-lsp/src/pool.rs` | One server per (server, project root); the root is the chat's folder when it is itself a project (one rust-analyzer for a whole Cargo workspace), else the nearest folder with a root marker. | FR-10 |
| crate | `crates/evo-lsp/src/ops.rs` | Diagnostics (sorted, errors first, waits for the slower check after a quick first report), definition / type definition / implementation / references (with each line's text), hover, document outline, workspace symbols; positions by `line` + `symbol` (whole word) or `character`, converted to UTF-16. | FR-1, FR-2, FR-3, NFR-3 |
| crate | `crates/evo-lsp/src/edit_check.rs` | The check after an edit: `before_edit` (the file's text, the running server and its errors for that text, and its errors for every other file when it is not busy; no wait, no start), `after_edit` (start or wait for a server, sync, wait within a budget, compare by message / code / source, the text for the model). | FR-15–FR-19, FR-23 |
| crate | `crates/evo-lsp/src/client.rs` (`last_diagnostics`, `reports`, `last_report_at`, `work_finished_since`, `starting`, `open_file`) | The last report for one file, or for every file, and when any came, without waiting; whether the server finished work since a change and is idle; whether it is in its first 30 s; open a file without saving it. | FR-16, FR-17, FR-22, FR-23 |
| crate | `crates/evo-lsp/src/install.rs` (`init_options_for`, `problem`), `catalog.rs` | `initializationOptions` by where the server is: `tsserver.fallbackPath` for the TypeScript server; the TypeScript install is pinned to `typescript@5`; why an install will not work as it is (shown on the page, fixed by Reinstall only). | FR-24, FR-25 |
| crate | `crates/evo-lsp/src/client.rs` (`workspace/configuration`) | Settings asked for are answered `null` (defaults), `{}` for the CSS server. | FR-26 |
| crate | `crates/evo-lsp/src/pool.rs` (`server_for`, `running_for`, `warm`) | The server for a file unless switched off; the running server for a file without starting one; start one in the background (one try per server a minute). | FR-17, FR-22 |
| crate | `crates/evo-lsp/examples/lsp-try.rs` | Try any of it from a terminal. | — (developer aid) |
| engine | `crates/evo-engine/src/tools/lsp.rs` | The `lsp` tool, the `EditCheck` wrapper around `edit` / `write`, the `WarmOnRead` wrapper around `read`, the RPC commands; settings `disabledLanguageServers`, `editDiagnostics`, `lspTool`. | FR-1, FR-7, FR-8, FR-9, FR-15–FR-22 |
| engine | `crates/evo-engine/src/tools/mod.rs` | Wraps `read`, `edit` and `write` (also for a child in its own worktree); registers `lsp` only when `lspTool` is on. | FR-1, FR-15, FR-22 |
| desktop UI | `apps/desktop/src/features/customize/languages-section.tsx` | **Customize → Code intelligence**: switches for the edit check and the `lsp` tool; suggested servers for the chat's folder with Install all, then the rest; source, needs, install command, log or error, switch, Uninstall. | FR-12, FR-20 |
| desktop UI | `apps/desktop/src/stores/engine-store.ts` (`suggestCodeIntelligence`) | Once per folder, the notice that links to the page. | FR-13 |
| desktop UI | `apps/desktop/src/components/ai-elements/tool.tsx` | Transcript row labels ("Checked for errors · main.rs", "Found references · add", …). | FR-14 |

### 3.3 Interfaces

Agent tool `lsp` (the model sees it):

```text
lsp {
  operation,   // diagnostics | definition | type definition | implementation |
               // references | hover | document outline | workspace symbols
  path,        // the file
  line?,       // the line
  symbol?,     // whole word on that line
  character?,  // column, instead of symbol
  query?       // for workspace symbols
}
```

RPC commands (engine):

| Command | Params | Effect |
| --- | --- | --- |
| `list_language_servers` | — | the chat's folder, detected languages (server and file count), every server with its state (installed, installing, missing programs, install kind, log) |
| `install_language_server` | `{server}` | installs in the background; progress shows in the list |
| `uninstall_language_server` | `{server}` | removes evo's own install |
| `set_language_server_enabled` | `{server, enabled}` | writes `disabledLanguageServers` |
| `set_code_intelligence_options` | `{editDiagnostics?, lspTool?}` | writes those settings; returns the list |

`list_language_servers` also returns `options: {editDiagnostics, lspTool}`.

What the edit check adds to an `edit` / `write` result (a separate text part;
the result's `details` are left as they are):

```text
rust-analyzer: 1 new error in src/a.rs after this change (2 were already there):
src/a.rs:12:5 error: mismatched types [rustc E0308]
If the change is not finished yet, these may go away with your next edit.
```

### 3.4 Data and storage

- evo's installs: `<agent dir>/lsp/<id>`.
- Settings `disabledLanguageServers`, `editDiagnostics`, `lspTool` in the
  agent settings.
- The pool's running servers live as long as the chat's engine.
- The desktop remembers that a folder was already suggested in browser
  storage (`evo.lsp-suggested:<folder>`).

### 3.5 Behaviour

1. The agent calls `lsp` on a path. The engine picks the server for the
   file's extension, finds the project root (FR-10) and gets or starts that
   server from the pool.
2. Just after start the client gives the server a few seconds to begin, then
   waits on work-done progress or `serverStatus` until indexing ends.
3. The file is opened (or saved) or updated with its current text, and the
   request is sent; positions are converted to UTF-16.
4. For diagnostics, the client waits for the slower check after a quick
   first report, then returns them sorted, errors first.

Edit check (FR-15–FR-21):

0. When the agent reads a file (outside a read-only session), its server is
   started in a background task that also opens the file, so its report for
   the old text is there by the time of the edit (FR-22).
1. Before the real `edit` / `write` runs, the wrapper keeps the file's text,
   asks the pool for a running server for the file (never starting one) and
   keeps that server's last errors for the file when they are for that text.
2. The real tool runs. On failure its error is returned as it is.
3. With no running server, an installed, switched-on server is started (or
   its start from step 0 joined) and waited for within the budget; the
   errors it reported for the old text, if any, become the baseline. When
   it is not ready in time, the result is returned as it is.
4. Otherwise the new text is sent (`didChange` + `didSave`) and the check
   waits, within 4 s in all, for indexing to end and for a report at the new
   version; after a report it waits briefly for a slower check (as for
   diagnostics) while time is left.
5. Errors (severity 1), one per place and code (rust-analyzer reports an
   error from its own check and again from `cargo check`; the longer message
   is kept), are compared with those kept in step 1 as a multiset of
   (message, code, source); line numbers are ignored, since an edit moves
   them. New ones are listed (at most 20), old ones counted.
   The wait ends at a report for the new version, or as soon as the server
   finished some work after the change and is idle (`work_finished_since`):
   rust-analyzer does not resend a file whose diagnostics did not change,
   so the file's last report stands. A server that reports no work (the
   TypeScript server) ends it after a silence while idle (`QUIET`, 1 s, or
   1.5 times the longest the server has taken to report: `slowest_report`),
   an answer that never claims what is left. A server that answers only when
   asked (`diagnosticProvider` and a working `textDocument/diagnostic`) is
   asked right after the change (`Client::pull`), and after a read. In a server's first 30 s both
   count only once it has been idle 1 s in a row (`settle`). The budget is 4 s, or 10 s while the
   server is in its first 30 s (`starting`), which covers the usual "read,
   then edit at once" of a chat's first change.
6. Other files (FR-23): when the server was not busy in step 1, the wrapper
   also kept the server's last errors for every file it had reported on.
   After the wait, each other file's errors are compared with those, the
   same way; new ones are listed after the changed file's, under one
   20-line cap. A server busy in step 1 (say, its first `cargo check`) or
   started during the edit gives no baseline: every error in other files is
   listed, said to perhaps predate the change.
7. Nothing is added when there are no new errors anywhere and the changed
   file had none before (or none were known), when the server was still
   checking and found nothing, or when no report came in time.

Install: the RPC starts the installer in the background — `npm install
--prefix`, `go install …@latest` with `GOBIN`, `rustup component add`,
`dotnet tool install --tool-path`, `gem install --install-dir`, or the latest
GitHub release (zip, tar.gz or a bare binary, picked by platform). No
window, 15-minute limit; the output's tail is kept for the page.

### 3.6 Errors and edge cases

- Server missing a required program: the list names the missing programs;
  the notice is not raised for such servers.
- Install failure or timeout: the page shows the log or error.
- Diagnostics published under a different URI spelling are matched to the
  file.
- Server requests the client does not handle get default answers so the
  server does not stall.

### 3.7 Security and trust

- Installs download and run code from the internet; they run only from the
  person's Install press (NFR-1), never on the agent's request.
- Servers run with the person's permissions on the project's files; the tool
  only reads answers from them.
- Server output reaches the model as tool results and is untrusted data.

### 3.8 Configuration

| Key | Type | Default | Where | Applies |
| --- | --- | --- | --- | --- |
| `disabledLanguageServers` | list of server ids | empty | agent `settings.json` | through `set_language_server_enabled`; edited by the switch on the page |
| `editDiagnostics` | bool | `true` | agent `settings.json` | read at each edit; switch on the page |
| `lspTool` | bool | `false` | agent `settings.json` | read when a chat's tools are built (new chats); switch on the page |

## 4. Alternatives considered

- Keep diagnostics only in the `lsp` tool and tell the model to call it
  after edits: rejected (2026-11-18); the model rarely did, so the check
  depended on its choice.
- Run the check in the `after_tool_call` hook: rejected; that hook carries
  the person's `PostToolUse` hooks, and the check needs the errors from
  before the edit, which a wrapper around the tool can take.
- Wait for a cold server start or a full check on every edit: rejected; an
  edit would take seconds to minutes. The wait is capped at 4 s.
- Start the servers of the project's languages when a chat opens: rejected;
  each chat runs its own engine, so every chat would run (and index with)
  its own servers even when it never edits code. Reading a file is the
  cheaper sign that an edit may follow.

## 5. As-built notes

- Built 2026-11-06 as designed.
- Installing from npm and GitHub was not run as part of the checks: installs
  download and run code from the internet, so they happen when the person
  presses Install.
- The engine module is `crates/evo-engine/src/tools/lsp.rs` (the design
  first named it `lsp_tool.rs`).
- 2026-11-18 trial in the app: with the server started only by the first
  edit, that edit came back unchecked; FR-22 and the wait in FR-17 were
  added, after which the first edit of a chat carried rustc's E0308.
- 2026-11-18 (FR-23): rust-analyzer does not republish a file whose
  diagnostics stayed the same, so an edit that left the file clean waited
  the whole budget for nothing; the wait now also ends when the server has
  finished its work. A chat's first edit comes while rust-analyzer is still
  loading, so a cold server gets 10 s and, with no settled baseline, other
  files' errors are listed with the "may have been there before" note.
  Telling a cold server by "no work finished yet" failed (rust-analyzer
  finishes many small progress items while loading); its first 30 s are
  used instead.
- 2026-11-18 (all installed servers tried): the TypeScript server never
  started ("Could not find a valid TypeScript installation"): evo had
  installed TypeScript 7, which has no `tsserver.js`, and passed no path.
  Fixed by FR-24; an existing install needs Reinstall on the page. The
  TypeScript server neither republishes an unchanged file nor reports
  progress, so clean edits waited the whole budget: the 1 s quiet rule.
  rust-analyzer pauses between loading steps, which ended a cold wait early
  and flakily: the 1 s settle. Opening (not saving) files on `read` keeps
  `cargo check` from running on every read.
- 2026-11-18, ten more servers installed into a scratch folder with the
  person's go-ahead (PHP, YAML, Bash, Dockerfile, Svelte, Go, C#, C/C++,
  Kotlin, Lua): csharp-ls answers only pulls (FR-27); its latest release
  needs .NET SDK 10 (FR-29); Intelephense and Svelte report more than 1 s
  after a change, so a fixed file was reported one edit late until the
  silence was learnt per server; Svelte reports one error under two codes
  (FR-28); Bash checks nothing without ShellCheck (FR-29). Kotlin did not
  report: the Kotlin server could not resolve `kotlin-stdlib` with no Gradle
  or Maven cache on this computer, an environment problem, not evo's. Go's
  first edit came before gopls had loaded (more than the 10 s cold wait);
  later edits were checked. `window/logMessage` lines are now kept with the
  server's stderr, which is how the Kotlin and C# cases were understood.
- 2026-11-18, every server on this computer (no downloads): CSS reported
  nothing because it validates nothing with `null` settings; `{}` fixes it,
  but `{}` stops the HTML server checking scripts, hence the per-server
  answer (FR-26). Marksman reports broken links as warnings, which the edit
  check leaves out by design (FR-15). Servers not on this computer (Go,
  C/C++, C#, Java, Kotlin, PHP, Ruby, YAML, Bash, Dockerfile, Svelte, Lua,
  Swift, Dart) were not tried: trying them means downloading them, which
  waits for the person's go-ahead.
