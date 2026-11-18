# Code intelligence (language servers) — Spec

| | |
| --- | --- |
| Status | Implemented |
| Area | crate `evo-lsp`, engine, desktop UI |
| Created | 2026-11-06 |
| Updated | 2026-11-18 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Agent tools](../agent-tools/spec.md), [Office documents](../office/spec.md) (same install-on-demand pattern) |

## 1. Summary

When the agent changes a file, the project's language server checks it and
the compiler's errors come back with the `edit` or `write` result: the agent
sees what it broke without having to ask. evo knows a catalog of language
servers, finds the ones already on the computer, installs the others when the
person asks, and runs one per project. The person manages them on one page
and is told when a project uses a language that has no server yet. An `lsp`
tool for definitions, references, types and outlines is there for those who
switch it on.

## 2. Scope

### 2.1 Goals

- The errors a change causes come back with the `edit` / `write` result,
  without the agent asking and without slowing an edit down for long.
- An opt-in agent tool, `lsp`, that answers code questions from the language
  server of the file's language: diagnostics, definition, type definition,
  implementation, references, hover, document outline, workspace symbols.
- A catalog of common language servers, each with how to find and install it.
- Use a server already on the computer before installing one.
- Install, uninstall and switch servers off from the app.
- Suggest the servers the chat's project needs.

### 2.2 Non-goals

- Installing anything without the person pressing Install.
- Language servers bundled inside plugins (listed as unsupported by
  [Customize](../customize-skills-plugins-connectors/spec.md)).

## 3. User scenarios

### US-1 The agent sees the errors its edit caused (priority P1)

The agent wants the compiler's errors for a file it just changed so that it
fixes them before answering — without having to remember to ask.

Acceptance scenarios:

1. **Given** a project whose language server is running, **when** the agent
   edits or writes a file and the change breaks it, **then** the tool result
   ends with the new errors (line, message, code) from the file's new text.
2. **Given** the file already had errors before the change, **when** the
   change adds none, **then** the result says no new errors and how many
   were already there; old errors are not listed again.
3. **Given** the file had errors and the change removes them all, **when**
   the server has finished checking, **then** the result says no errors are
   left.
4. **Given** a clean file stays clean, **then** nothing is added to the
   result.
5. **Given** no server for the language is running yet, **when** the agent
   reads a file of that language, **then** its server starts in the
   background, so the agent's first edit of it is checked too.
6. **Given** a change to one file that breaks another (a changed signature
   and its callers), **when** the server reports on the other file within
   the wait, **then** the result also lists the other file's new errors.
7. **Given** no server is running or ready when the agent changes a file,
   **then** the edit waits for it at most the check's budget and returns
   without errors added when it is not ready.

### US-1b The agent asks for diagnostics (priority P3)

With the `lsp` tool switched on, the agent can still ask for a file's
diagnostics; the request waits for indexing instead of returning nothing.

### US-2 The agent finds real definitions and uses (priority P2)

The agent wants the definition, references, type and docs of a symbol so
that it reasons about the real code instead of text matches.

Acceptance scenarios:

1. **Given** a symbol on a line, **when** the agent asks for references by
   line and symbol name, **then** it gets the definition and every call with
   each line's text.
2. **Given** a symbol, **when** the agent asks for hover, **then** it gets
   the signature and the doc comment.

### US-3 The person sets up servers for a project (priority P2)

A person wants the servers their project needs, with no manual setup, so
that the agent can use code intelligence there.

Acceptance scenarios:

1. **Given** a chat in a project, **when** the person opens the code
   intelligence page, **then** the servers for that project are listed first
   with an "Install all" action, then the rest.
2. **Given** a project that uses three or more files of a language with no
   server, **when** a chat runs there, **then** evo notifies once for that
   folder with a link to the page.

### Edge cases

- A server needs a program that is missing: the page names the missing
  programs.
- An install fails: the page shows its log or error.
- A server is switched off: the tool does not use it.
- A file's URI is spelled differently by the server: its diagnostics still
  belong to the file.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | With the setting `lspTool: true` (off by default; applies to new chats) the agent has an `lsp` tool with `operation`, `path`, `line`, `symbol`, `character` and `query`; operations: diagnostics, definition, type definition, implementation, references (with each line's text), hover, document outline, workspace symbols. | Given a project with a running server, when the agent calls each operation, then it gets the server's answer. |
| FR-2 | A position is given by `line` plus `symbol` (whole word) or `character`. | Given a line and a symbol name, when the tool runs, then it resolves the symbol on word boundaries. |
| FR-3 | Diagnostics are sorted errors first and wait for the slower check after a quick first report; any request waits while the server is indexing. | Given a fresh server, when diagnostics are asked, then the compiler's errors come back, not an empty list. |
| FR-4 | Before each request the file is opened (and saved, for servers that check on save) or updated with its current text. | Given an edited file, when diagnostics are asked, then they reflect the edit. |
| FR-5 | evo ships a catalog of 21 language servers: rust-analyzer (Rust), TypeScript Language Server (TS/JS), Pyright (Python), gopls (Go), clangd (C/C++), csharp-ls (C#), Kotlin Language Server, jdtls (Java, on PATH), Intelephense (PHP), Ruby LSP, HTML / CSS / JSON, YAML, Bash, Dockerfile, Svelte, Lua, Marksman (Markdown), SourceKit-LSP (Swift, with the toolchain), Dart; each with file extensions, root markers, command, how to install and programs it needs. | The server list names every server. |
| FR-6 | A server is found in this order: evo's own install, then (rust-analyzer) the folder's toolchain, then PATH, then a copy shipped with a code editor's extensions. | Given a server in an editor's extension folder, when evo looks for it, then it is found. |
| FR-7 | The person can install a server from the app; the install runs in the background with progress in the list, and the output's tail is kept for the app. | Given a missing server, when the person presses Install, then it installs and the list shows progress and the log. |
| FR-8 | The person can uninstall a server that evo installed. | Given an evo install, when the person presses Uninstall, then it is removed. |
| FR-9 | The person can switch a server off and on; the choice is saved. | Given a switched-off server, when the agent calls the tool on its language, then that server is not used. |
| FR-10 | One server runs per server and project root; the root is the chat's folder when it is itself a project (one server for a whole multi-package workspace), else the nearest folder with a root marker. | Given a file deep in a project, when the tool runs, then the server starts at the nearest marker folder. |
| FR-11 | evo detects which languages a project uses from file extensions and root markers, four levels deep, skipping dependency and build folders. | Given a mixed project, then each of its languages is detected. |
| FR-12 | The **Customize → Code intelligence** page lists the chat's project servers first ("Suggested for <folder>", Install all), then the rest; each shows where it comes from, what it needs (missing programs named), the install command, its log or error, a switch, and Uninstall for evo's own installs. | Manual check in the running app. |
| FR-13 | Once per folder, a chat whose project uses three or more files of a language without a server notifies with a link to the page. | Manual check in the running app. |
| FR-14 | Transcript rows describe the call in words, e.g. "Checked for errors · main.rs", "Found references · add". | Manual check in the running app. |
| FR-15 | After a successful `edit` or `write`, when a server for the file's language is running and switched on, the file is sent to it and the errors it reports (severity error only, at most 20 lines) are added to the tool's result as a separate text part. Warnings are not added. | Given a running server, when an edit breaks a file, then the result ends with the compiler's error. |
| FR-16 | Errors are compared with those the server reported for the file before the change (by message, code and source, not by line): only new errors are listed; the errors that were already there are counted. When the server had not reported on the file before, every error is listed and the result says some may predate the change. | Given a file with one old error, when an edit adds another, then only the new one is listed and "1 already there" is said. |
| FR-17 | The check waits at most 4 s in total (for indexing and the report), or 10 s while the file's server is in its first 30 s (a cold start, once per language and chat). With no server running for the file, an installed, switched-on server is started and waited for within the 10 s; when it is not ready in time, nothing is added and it keeps starting in the background. A server that finishes its work without republishing the file (rust-analyzer does not resend unchanged diagnostics) ends the wait at once, with the file's last report. With no server, a server switched off, or no report in time, nothing is added. | Given no running server, when the agent edits a file, then the edit returns within the 4 s and the server is running afterwards. |
| FR-18 | When the file had errors before the change and the server reports none after it and has finished checking, the result says no errors are left. A clean file that stays clean adds nothing. When the server is still checking at the end of the wait, errors found so far are listed with a note that the check had not finished, and "no errors" is never claimed. | Given a broken file, when an edit fixes it, then "no errors left" is added. |
| FR-19 | The added text says the errors may come from a change that is not finished yet, so the agent does not stop a multi-step change to fix them. | Review of the text. |
| FR-20 | The setting `editDiagnostics` (on by default, read at each edit) switches the check off; **Customize → Code intelligence** has a switch for it and one for `lspTool`. | Given `editDiagnostics: false`, when the agent edits a file, then nothing is added. |
| FR-21 | A read-only session never starts language servers (its `edit` and `write` are refused before they run, and `read` does not warm them). | Review: the check runs only after a successful `edit` / `write`; `read` warms only outside read-only sessions. |
| FR-23 | The result also lists new errors in other files that the same server reports on within the wait, compared per file with that server's reports from before the change (as in FR-16). When the server was busy before the change (say, its first check), there is no settled baseline: every error in other files is listed and the result says some may have been there before. The 20-line cap covers all files, the changed file first. When only other files have errors, the result says so. | Given a Cargo project, when an edit changes a function's return type and a caller in another file breaks, then the result lists the caller's error. |
| FR-22 | After the agent reads a file, its language server (installed, switched on) starts in the background when it is not running, at most one try per server a minute; nothing is added to the `read` result. Off with `editDiagnostics: false`. | Given a project with rust-analyzer, when a real model reads then edits a file in a new chat, then the first edit's result carries the error. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Installs download and run code from the internet, so they happen only when the person presses Install. | Review: no install path runs without the install command from the app. |
| NFR-2 | Installs open no window and stop after 15 minutes. | Review of the installer; manual install on Windows shows no console window. |
| NFR-3 | Positions sent to servers are converted to UTF-16. | Given a line with non-ASCII text, when a symbol is resolved, then the server gets the right column. |

## 5. Success criteria

- SC-1 On a small project, the agent gets the compiler's error with its
  hints from a cold-started server in seconds (observed: about 7 s).
- SC-2 (retired 2026-11-18) A real model uses the tool on its own, without
  being told to, and answers correctly from its results. It could not be
  shown to help in normal work, which is why errors now come with the edit.
- SC-3 With a running server, an edit that breaks a file returns its error
  within the 4 s wait, and a real model fixes it in its next turns without
  being told to.

## 6. Clarifications

### 6.1 Open

n/a — no open questions.

### 6.2 Resolved

- 2026-11-18 — Q: Should the agent have to call a tool to see the errors
  its edit caused? → A: No; the errors come with the `edit` / `write`
  result (FR-15–FR-20), and the `lsp` tool is off by default (FR-1), since
  the agent rarely chose to call it.
- 2026-11-06 — Q: Should evo test installs from npm and GitHub
  automatically? → A: No; installs download and run code, so they happen
  only when the person presses Install (NFR-1).

## 7. History

- 2026-11-06 — Built and checked with a real language server and a real
  model.
- 2026-11-07 — Moved to the spec / plan / tasks format.
- 2026-11-18 — Errors come with `edit` / `write` results (US-1, FR-15–FR-21,
  SC-3); the `lsp` tool is opt-in (FR-1, US-1b); SC-2 retired.
- 2026-11-18 — The trial in the app showed a chat's first edit unchecked
  (the server was not running yet): `read` warms the server (FR-22) and the
  edit waits for a starting server within its budget (FR-17).
- 2026-11-18 — New errors in other files come with the result too (US-1
  scenario 6, FR-23).
