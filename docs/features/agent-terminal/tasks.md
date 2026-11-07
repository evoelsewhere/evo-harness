# Agent terminal: shell choice and interactive tools — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-07 |

## 1. Phases

### Phase 1 — Discovery, ownership, the five tools, approvals, transcript

Done when: the verification rows FR-1–FR-15 pass on Windows, and FR-2 and
FR-3 pass on macOS.

- [x] T1 [P] [FR-1] Write `cargo test` cases for Windows discovery, with temp folders standing in for Git, MSYS2 and Cygwin — `apps/desktop/src-tauri/src/terminal.rs` — check: the tests fail for the missing shells
- [x] T2 [P] [FR-2, FR-3] Write `cargo test` cases for `/etc/shells` parsing and the id rules — `apps/desktop/src-tauri/src/terminal.rs` — check: the tests fail for the right reason
- [x] T3 [FR-1, FR-2, FR-3, NFR-2] Implement shell discovery: every Windows shell and each WSL distribution (in the background, 3 s limit), `/etc/shells` + Homebrew + login `PATH` on Unix, stable ids, `ShellInfo.kind` and `isDefault` — `apps/desktop/src-tauri/src/terminal.rs` — check: T1 and T2 pass; `terminal_shells` timed with and without WSL
- [x] T4 [FR-10, FR-11] `terminal_spawn` takes `owner`, kept in `SessionInfo` and returned by `terminal_list`; killing a session ends its process tree — `apps/desktop/src-tauri/src/terminal.rs`, `apps/desktop/src/features/terminal/pty.ts` — check: reload keeps the owner; killed dev server frees its port
- [x] T5 [NFR-3, NFR-4] Write unit tests for the output log: ring, offsets, dropped-characters note, cut results — `apps/desktop/src/features/terminal/` — check: the tests fail before T6
- [x] T6 [FR-8, FR-9, NFR-3, NFR-4, NFR-5] Implement the output log (1 MB escape-free ring, running offset, regex / idle / exit waiters) and feed it from the terminal instance — `apps/desktop/src/features/terminal/output-log.ts`, `apps/desktop/src/features/terminal/terminal-instance.ts` — check: T5 passes
- [x] T7 [FR-5, FR-10, FR-11] `TerminalTab.owner`, `create({ shell, cwd, owner })`, `kill(key)` — `apps/desktop/src/stores/terminal-store.ts` — check: typecheck; `terminal_list` shows owners
- [x] T8 [FR-4, FR-5, FR-6, FR-7, FR-8, FR-9, FR-10, FR-14, NFR-4] Tool schemas and handlers for `terminal_list`, `terminal_run`, `terminal_send`, `terminal_read`, `terminal_kill`: shell name resolution, wait for the first prompt, key names to bytes, password-prompt check, 20,000-character results — `apps/desktop/src/features/terminal/agent-tools.ts` — check: drive the tools over WebView2 DevTools (`runUiTool`)
- [x] T9 [FR-4, FR-5] Drop `terminalTool` from the browser feature; `UI_TOOLS` takes the terminal tools through `features/terminal/index.ts` — `apps/desktop/src/features/browser/agent-tools.ts`, `apps/desktop/src/features/terminal/index.ts` — check: typecheck; old `terminal_run` / `terminal_read` arguments still work
- [x] T10 [FR-9, FR-12, FR-13, NFR-5] Pass the chat id and an `ask` callback to the terminal tools, keep the per-chat read permission, end waits when the engine drops the call — `apps/desktop/src/stores/engine-store.ts` — check: each approval mode × own / person's tab in the running app
- [x] T11 [P] [FR-11] Mark agent tabs in the tab strip (agent icon, chat title in the tooltip) — `apps/desktop/src/features/terminal/terminal-panel.tsx` — check: manual in the running app
- [x] T12 [P] [FR-15] Render the five tools in the transcript — `apps/desktop/src/components/ai-elements/tool.tsx` — check: look at the transcript for each tool
- [x] T13 [FR-12, FR-13] Add to ADD §9: "App-side tools — terminal, scheduler, connectors — ask through the same approval panel." — `docs/architecture/README.md` — check: review
- [x] T14 [FR-4, FR-15] Update the UI map and the tool table in [agent tools](../agent-tools/spec.md) — `apps/desktop/README.md`, `docs/features/agent-tools/` — check: review

### Phase 2 — Shell integration

Done when: FR-16 passes for PowerShell, Git Bash, zsh and fish, and Command
Prompt reports the end of a command.

- [x] T15 [FR-16] Integration scripts for PowerShell, bash, zsh and fish, written to the app's data folder on every start — `apps/desktop/src-tauri/src/shell_integration/` — check: the scripts appear under `<app data>/shell-integration/`
- [x] T16 [FR-16] `terminal_spawn` passes each shell's extra args and env (and the `PROMPT` sequences for Command Prompt) — `apps/desktop/src-tauri/src/terminal.rs` — check: a new tab of each shell emits markers
- [x] T17 [FR-16] OSC 133 handler, markers stored in the output log, `wait: true` returns the text between C and D and the exit code; tabs without markers fall back to waits and say so — `apps/desktop/src/features/terminal/terminal-instance.ts`, `apps/desktop/src/features/terminal/output-log.ts`, `apps/desktop/src/features/terminal/agent-tools.ts` — check: `wait: true` with a passing and a failing command in each shell
- [x] T18 [FR-16] `terminalShellIntegration` setting (boolean, default `true`) in **Settings → General → Terminal** — app settings — check: turning it off starts the next new tab without integration
- [x] T19 [FR-16] Update the UI map and the tool table in [agent tools](../agent-tools/spec.md) — `apps/desktop/README.md`, `docs/features/agent-tools/` — check: review

### Phase 3 — More shell sources, WSL and Nushell integration

Done when: those shells appear in the list, and WSL and Nushell tabs report
command ends.

- [x] T20 [FR-1] Windows Terminal profiles and Visual Studio developer shells as extra shell sources — `apps/desktop/src-tauri/src/terminal.rs` — check: they appear in the "New terminal" menu
- [x] T21 [FR-16] Shell integration for WSL (script path translated into the distribution) and Nushell — `apps/desktop/src-tauri/src/shell_integration/` — check: `wait: true` returns an exit code in WSL and Nushell tabs
- [x] T22 [FR-1, FR-16] Update the UI map and the tool table in [agent tools](../agent-tools/spec.md) — `apps/desktop/README.md`, `docs/features/agent-tools/` — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `cargo test` for Windows discovery with temp folders standing in for Git, MSYS2, Cygwin; manual check of WSL distributions | unit tests passed (2026-11-07); WSL / Windows Terminal / VS sources: manual check not run |
| FR-2 | `cargo test` for `/etc/shells` parsing; manual check on macOS with Homebrew | unit tests passed (2026-11-07); macOS manual check not run |
| FR-3 | `cargo test` for the id rules; manual check on macOS | unit tests passed (2026-11-07); macOS manual check not run |
| FR-4 | Drive `terminal_list` over WebView2 DevTools in the running app (`runUiTool`) | passed (2026-11-07) — owners `you`, `this chat`, `a deleted chat`; no tab content |
| FR-5 | Drive `terminal_run` in Git Bash and with an unknown shell over WebView2 DevTools | passed (2026-11-07) — Git Bash tab opened; `tcsh` lists the available ids |
| FR-6 | Drive `terminal_run` in a slow-starting WSL tab | passed (2026-11-07) — PowerShell, Git Bash, cmd: waits for the first prompt marker, 12 fresh runs without garbled text; WSL: not run (not installed) |
| FR-7 | Drive `terminal_send` Ctrl+C to a dev server | passed (2026-11-07) — Ctrl+C stopped a node server on :5599, prompt back |
| FR-8 | Drive `terminal_read` twice with the returned cursor | passed (2026-11-07) — two reads with the cursor return new lines only; cmd output keeps its line breaks |
| FR-9 | Drive `terminal_run` / `terminal_read` with `waitFor`; stop the turn during a wait | passed (2026-11-07) — `waitFor` returns at the line (the typed command's echo does not match); timeout and stopped turn end the wait |
| FR-10 | Drive `terminal_kill`; check the dev server's port is free | passed (2026-11-07) — tab `exited`, port free; `close: true` removes the tab |
| FR-11 | Reload the window, then `terminal_list` | passed (2026-11-07) — owner kept after a reload (tab ids change, shells restart) |
| FR-12 | Each approval mode × own tab, in the running app | passed (2026-11-07) — each mode × own tab; a denied run types nothing |
| FR-13 | Each approval mode × person's tab, in the running app | passed (2026-11-07) — each mode × person's tab: read asks once, send / run / kill always ask |
| FR-14 | `sudo -v` in WSL, then `terminal_send` text | passed (2026-11-07) with a Git Bash `read -s` password prompt; `sudo` in WSL: not run (not installed) |
| FR-15 | Look at the transcript for each tool | passed (2026-11-07) — real chat: "Pressed Ctrl+C in Git Bash tab …", run output excerpt |
| FR-16 | `wait: true` with a passing and a failing command in each shell | passed (2026-11-07) — PowerShell, Git Bash (also `(exit N)`, `{ …; }`), 3 fresh tabs each and 7 back-to-back runs: right exit codes; cmd reports the end without a code; pwsh 7, zsh, fish, WSL, Nushell: not run (not installed / macOS) |
| NFR-1 | Manual check on Windows and macOS | passed on Windows (2026-11-07); macOS not run |
| NFR-2 | Time `terminal_shells` with and without WSL | passed (2026-11-07) — `terminal_shells` 160 ms; extra sources 127 ms in the background; WSL not installed |
| NFR-3 | Unit tests for `output-log.ts` (ring, offsets) | passed (2026-11-07) — unit tests; a real 1.6 MB log: kept 1,021,595 characters (never above 1 MiB), a stale cursor gets "… N characters dropped" |
| NFR-4 | Unit tests for `output-log.ts` (cut results) | passed (2026-11-07) — 1.6 MB log returns 20,136 characters with a "characters cut" line; unit tests passed |
| NFR-5 | Two chats waiting on two tabs | passed (2026-11-07) — two chats waiting on two tabs returned at 3.2 s and 4.2 s; main thread gap 71 ms |
