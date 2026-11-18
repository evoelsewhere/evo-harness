# Evo — Architecture Design Document (ADD)

| | |
| --- | --- |
| Status | Living document — describes the system as built |
| Updated | 2026-11-15 |
| Decisions | [ADR index](decisions/README.md) |
| Features | [Feature catalogue](../features/README.md) |

This document describes how Evo is put together as a whole: its processes,
crates, interfaces, where state lives, and the rules every feature must fit
into. Each feature is specified, planned and broken into tasks under
`docs/features/<slug>/` (`spec.md`, `plan.md`, `tasks.md` — Spec-Driven
Development); the
reasons behind the structure live in the ADRs under `decisions/`. Rules for
keeping this document current are in [docs/README.md](../README.md).

## 1. Purpose and scope

Evo is a local-first desktop agent workspace. A person chats with agents that
read and change files, run commands, browse, use connectors (MCP), plugins and
skills, delegate to subagents and start nested sessions. Everything runs on
the person's machine except the model providers, connectors and web pages the
agent reaches.

In scope: the Rust engine and library crates under `crates/`, the wire
protocol, and the desktop app under `apps/desktop/`. Out of scope: the
visual language ([DESIGN.md](../../DESIGN.md)) and per-feature specs, plans
and tasks (`docs/features/`).

## 2. System context

```text
                 ┌──────────────────────────── person's machine ─────────────────────────────┐
  person ──UI──► │  evo desktop app  ──spawns──►  evo-engine (one per open chat)              │
                 │       │                             │                                     │
                 │       │ MCP servers (stdio/HTTP)    │ tools: files, shell, git worktrees, │
                 │       │ language servers            │ LSP, web fetch/search               │
                 │       ▼                             ▼                                     │
                 │  ~/.evo/agent/ (config, evo.db)   project folders (user-owned)           │
                 └──────────────┬────────────────────────────┬──────────────────────────────┘
                                │                            │
                     model providers (HTTPS)       models.dev (model database),
                                                   web pages, plugin marketplaces (git),
                                                   GitHub Releases (app updates),
                                                   Deno and uv releases (plugin runtimes),
                                                   services plugins sign in to
```

| External | Used for | Trust |
| --- | --- | --- |
| Model providers | completions (streaming) | trusted transport; output is untrusted |
| models.dev | model list, limits, costs, thinking options | data only, cached in `evo.db` |
| MCP servers | connector tools, prompts, resources | per-tool Allow / Ask / Block; project servers need approval |
| Web | `web_fetch`, `web_search`, the in-app browser | untrusted content; public addresses only for fetch |
| Plugin marketplaces | plugin bundles | installed by explicit user action; a plugin's views are untrusted pages that reach the app only through the plugin bridge, under the permissions the person approved ([ADR 0021](decisions/0021-plugin-views-run-in-native-webviews.md)); its code runs only as Evo launches it, with the permissions the person approved ([ADR 0022](decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md)) |
| Deno and uv releases (GitHub) | the runtimes of plugin backends | downloaded on first need and used only when their SHA-256 matches the one pinned in the app ([ADR 0022](decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md)) |
| Services plugins sign in to | OAuth, device-code and web sign-ins of plugin accounts | the person signs in in their own browser or an isolated login window; tokens and cookies stay in the plugin's sealed vault |
| GitHub Releases (Evo's own) | the update manifest and new app versions | installed only when signed with the project's release key, after the person chooses to restart ([ADR 0017](decisions/0017-app-updates-from-signed-github-releases.md)) |

## 3. Architectural drivers

Quality goals, in priority order when they conflict:

1. **Safety and trust.** The person stays in control of what agents do:
   permission prompts, a sandbox config, project trust, untrusted-data
   handling. A new capability never bypasses them.
2. **Crash isolation and durability.** A crashing engine, plugin or tool does
   not take the window down; sessions survive process death and resume.
3. **Provider independence.** Nothing above `evo-ai` knows which provider
   serves a model; model facts come from data (models.dev), not code.
4. **Local-first.** State is on the device; the app works with any reachable
   provider and degrades gracefully offline.
5. **Cross-platform.** Windows, macOS (Intel and Apple Silicon) and Linux
   are first-class; platform-only features are marked as such.
6. **Small, testable layers.** Library crates are usable and testable without
   the engine or the desktop.

Constraints: Rust (toolchain pinned in `rust-toolchain.toml`, MSRV 1.89),
Tauri 2, React 19 + TypeScript, bun for the desktop's JavaScript. There is no
CLI or TUI front end ([ADR 0001](decisions/0001-desktop-app-is-the-only-front-end.md)).

## 4. Process view

```text
┌──────────────────────── evo-desktop (Tauri, Rust) ────────────────────────┐
│  windows (WebView2 / WKWebView / WebKitGTK) — React UI, zustand stores    │
│     ▲ Tauri events            │ invoke(commands)                          │
│  shell: engine supervisor · mesh hub (evo-mesh) · MCP pool (evo-mcp) ·    │
│         plugins (evo-plugins) · app DB (evo-store) · pty · files ·        │
│         browser and plugin-view child webviews · worktrees · tray ·       │
│         plugin host API (127.0.0.1) · plugin login windows                │
└──────┬───────────────────────────┬───────────────────────┬───────────────┘
       │ stdin/stdout JSONL        │                       │ stdio (MCP), from the pool
   evo-engine --mode rpc           evo-engine --mode rpc     plugin backends:
   --session-id <chat>             --session-id <chat>         deno run (approved net, files)
   --session-db                    --session-db                uv's Python in <data>/venv
     transcripts/<chat>.db           transcripts/<chat>.db   connector servers of plugins
   --sandbox-config …              --sandbox-config …
   (one child process per chat with work or a viewer)

   evo-engine --mode plugin-ai     one process for every plugin's model calls,
                                   started on the first, stopped after 5 idle minutes
```

- **Plugin views are child webviews of a window** (`plugin-<n>`), served
  from their plugin's folder by the `evo-plugin://` scheme at one origin
  per plugin. They reach no Tauri command; their only way in is
  `POST /__bridge` on their own origin, which the scheme handler answers
  for the calling view (`src-tauri/src/plugin_ui/`,
  [ADR 0021](decisions/0021-plugin-views-run-in-native-webviews.md),
  [plugin UI spec](../features/plugin-ui/spec.md)).

- **Plugin code runs only as Evo launches it**
  ([ADR 0022](decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md),
  [plugin settings spec](../features/plugin-settings/spec.md)). A plugin's
  backend is an MCP server in the shell's pool: TypeScript/JavaScript under
  Deno with `--allow-net`/`--allow-read`/`--allow-write` from the person's
  approvals and no subprocesses, Python through uv in the plugin's own
  environment (not confined). Each starts on demand (or with Evo, or
  never), sleeps after its idle minutes with its tools still offered, and
  restarts after a crash, 5 times a minute at most. It and the plugin's
  hooks reach Evo through the **plugin host API** on `127.0.0.1` with a
  key that names the plugin (`src-tauri/src/plugin_host/`): an account's
  token or cookies, its settings, a log line. Sign-ins open the person's
  browser (OAuth, device code) or a **login window** with storage of its
  own (`plugin-login-<n>`, no Tauri command; `src-tauri/src/plugin_auth/`).

- **Each chat's transcript is a file of its own**, written only by that
  chat's engine, so engines never wait on one another's write lock
  ([ADR 0007](decisions/0007-one-transcript-file-per-chat.md)). The shell
  reads those files (search, fork) and removes them with their chat after
  stopping its engine. `evo.db` is shared: the shell writes the app's data,
  engines read it and write provider settings and the models.dev cache.

- **One engine process per chat with work or a viewer**
  ([ADR 0003](decisions/0003-engine-runs-as-a-child-process-per-chat.md),
  [ADR 0018](decisions/0018-the-shell-hosts-engines-windows-attach.md)).
  The shell's engine supervisor (`src-tauri/src/supervisor.rs`) owns every
  engine's life ([shell-hosted engines](../features/shell-hosted-engines/spec.md)):
  - it starts one when a window shows the chat, the mesh hub needs a host for
    mail or a new session, the Scheduler runs a task, or the app starts with
    mail waiting; it is the only code that spawns an engine;
  - it sets the engine up on `ready` (model, tools, the plugin host for
    plugins' hooks, mesh subscription, the hub's waiting mail; see
    [6.3](#63-hosting-an-engine));
  - it restarts an engine that exits with pending work, at most 3 times in
    2 minutes, then notifies and leaves the mail waiting;
  - it stops engines with no work and no viewer beyond 4 idle ones over the
    whole app, least recently used first, and stops every engine on quit.

  "Work" is a running turn, running agents, mail or a prompt waiting for the
  chat, or an open question. Windows are viewers: they attach to the engine
  of the chat they show (`engine_attach`) and detach when they show another
  chat, close or reload, which never stops an engine that has work. The
  person's **Stop** (`engine_stop`) still stops a chat and its work.
  Settings, credential and customization changes reach every engine, viewed
  or not; idle engines reload or restart, and the app itself never restarts
  for that ([ADR 0005](decisions/0005-real-engine-and-live-reload.md)).
- **Evo keeps working with no window** while it runs. With the tray icon on,
  closing every window keeps the engines going; the tray's menu counts the
  chats working and waiting for the person and opens a waiting one, and the
  main window is built again when it was destroyed. Quitting, or closing the
  last window with the tray off, asks once while chats work ("2 chats are
  working. Quit anyway?"), then stops every engine. Work never outlives the
  Evo process.
- **Plugins use the person's models through one long-lived engine
  process** (`evo-engine --mode plugin-ai`, [ADR 0023](decisions/0023-plugin-ai-runs-in-one-long-lived-engine-process.md),
  [plugin AI spec](../features/plugin-ai/spec.md)). The shell starts it on a
  plugin's first AI call and stops it after 5 minutes without runs; it
  speaks its own JSONL (`evo-protocol::plugin_ai`). The shell gates every
  run (`ai` and the tool groups the person approved, the plugin's caps,
  3 runs per plugin and 6 overall, a 30 s queue), routes its events to
  the views and backend streams that follow it, runs view functions and
  backend tools the model calls, and shows approvals for Evo's own tools in
  the main window. The engine chooses the model from models.dev facts and
  the person's rules, calls the provider directly, falls back once,
  executes Evo's tools in the run's folder under the approval mode and
  sandbox config, and records each call's usage against the plugin. A run
  goes on when the view that started it closes.
- **Memory indexing is a short-lived engine process**
  (`evo-engine --mode index`,
  [ADR 0016](decisions/0016-memory-indexing-runs-in-a-short-lived-engine-process.md)).
  A worker in the shell starts one for an idle chat with new messages, at
  most one at a time. It reads that chat's transcript read-only, changes
  memory topics with the model the person chose for indexing, prints one
  result line and exits. It loads no plugins, hooks, skills or connectors
  and has two tools (docs/features/memory-topics-and-chat-recall).
- **Evo Webbridge reaches the person's own Chrome or Edge** without a port.
  The browser starts `evo-bridge-host` (one per profile) for the Evo
  extension through native messaging; the host joins the shell over a
  per-user named pipe (a 0600 Unix socket elsewhere), and each side checks the
  other's executable. The extension only relays DevTools commands for tabs in
  Evo's tab groups and hosts the side panel; the shell decides which chat owns
  a tab and what the gate allows
  ([ADR 0012](decisions/0012-native-messaging-bridge-to-the-persons-browser.md),
  [Evo Webbridge](../features/browser-bridge/spec.md)).
- **The shell owns native capabilities and cross-chat coordination**: windows,
  pseudo-terminals, file access for the Files tool, browser webviews, the MCP
  connection pool, plugin installs, the session mesh hub, the app database,
  engine lifecycles, wakes, turn records and the Scheduler's runner
  ([ADR 0018](decisions/0018-the-shell-hosts-engines-windows-attach.md)).
  These are lifecycle decisions: what the agent does stays in the engine.
- **The shell owns app updates** ([app updates](../features/app-updates/spec.md),
  [ADR 0017](decisions/0017-app-updates-from-signed-github-releases.md)):
  it checks the latest release, downloads and verifies the new version, and
  installs it only when the person chooses to restart, stopping every
  engine first. The webview shows the state; the agent cannot trigger it.
- **The engine owns agent behaviour**: the agent loop, tools, sessions,
  compaction, skills, hooks, memory, model selection and credentials.
- **The webview owns presentation only**: it renders state and sends user
  intent (prompts, steering, answers, cancels). It tells the shell which chat
  it shows and when a draft sits in a composer; it decides no wake, record or
  start. Only `stores/engine-store.ts` talks to the engine client.
- **A page an agent uses lives on a stage in the shell, not in the panel**: a
  visible desktop-size webview parked outside the window whose frames the shell
  streams (`browser://frame`) to whatever shows it, a floating card over the chat
  or the side panel; input goes back as DevTools events in page coordinates
  (docs/features/floating-cards).
- **Another app the agent controls stays the person's program, moved off
  screen** (docs/features/computer-app-control): the shell drives it
  only through background channels. These are a DevTools pipe for web-based
  apps it started, the OS accessibility API, and posted messages. The shell
  never uses the person's pointer, keyboard focus or foreground. It streams the
  window's pictures to a floating card the same way as a page, and puts the
  window back when control ends
  ([ADR 0015](decisions/0015-agent-drives-desktop-apps-in-the-background-only.md)).
- Some agent tools are implemented by the app and offered to the engine as
  **external tools** over the protocol; the engine still runs the loop and
  the permission checks. Every one of them runs in the Rust shell, which
  answers the requests itself, so they work with the window hidden: the
  browser, terminal, app-control, session, scheduler and connector tools and
  `show_file`, `send_file` and `notify`
  ([ADR 0013](decisions/0013-app-side-agent-tools-run-in-the-rust-shell.md),
  [ADR 0014](decisions/0014-agent-tools-by-layer.md)). The webview only shows
  what they open and ask. The shell's tools share one registry
  (`agent_tools/mod.rs`), the engine's classes (read / edit / command) and one
  approval gate (`agent_tools/gate.rs`).

## 5. Building blocks

### 5.1 Crate layers

```text
 app        evo-engine (bin evo-engine)                apps/desktop/src-tauri (own workspace)
              │                                          │ links: evo-store, evo-mesh, evo-mcp,
              │                                          │        evo-plugins, evo-worktrees
 protocol   evo-protocol ──► evo-mesh (wire)             │ never links evo-engine
              │
 runtime    evo-harness ──► evo-durable     evo-runtime     evo-extensions ──► evo-plugin-sdk
              │                                │
 agent      evo-tools ──► evo-worktrees        │
              │                                │
            evo-agent                          │
              │                                │
 llm        evo-ai ◄───────────────────────────┘
              │
 leaf       evo-telemetry   evo-store  evo-hooks  evo-lsp  evo-web  evo-mcp  evo-plugins  evo-office
```

| Crate | Responsibility |
| --- | --- |
| `evo-ai` | Messages, content, `Provider` trait, streaming events, HTTP providers, `faux` test provider |
| `evo-agent` | The provider-agnostic agent loop, `AgentTool`, events, hooks, steering/follow-up queues |
| `evo-tools` | Built-in tools over `ExecutionEnv` (real FS, in-memory, remote), the sandbox |
| `evo-harness` | Durable session tree, JSONL/SQLite persistence, compaction, crash recovery |
| `evo-durable` | Checkpoint/recovery primitives for the harness |
| `evo-runtime` | models.dev catalog, provider-scoped credentials, model and thinking selection |
| `evo-plugin-sdk` / `evo-extensions` | Stable `#[repr(C)]` plugin ABI / the host that loads `cdylib` plugins |
| `evo-protocol` | Engine ↔ client wire contract (JSONL default, CBOR frames optional) |
| `evo-mesh` | Session mesh wire types and the hub that routes mail between sessions |
| `evo-store` | `evo.db`: schema, migrations, chats and their memory flags, chat search (also ranked, for agents), chat summaries, the list of memory changes made by indexing, logs, mail, settings, usage and cost history; the transcript files' location, identity and life cycle, and reading one as messages |
| `evo-mcp` | MCP client and connection pool |
| `evo-plugins` | Plugin bundles and marketplaces |
| `evo-hooks` | Hook commands and web requests on agent events |
| `evo-lsp` | Language-server catalog, installers, LSP client |
| `evo-web` | Guarded page fetching as Markdown, web search |
| `evo-office` | Office documents: the native preview renderer (DOCX/PPTX/XLSX → script-free HTML, package preflight, cache, inspection; [ADR 0009](decisions/0009-native-office-preview-renderer.md)), document versions, and the optional pinned LibreOffice runtime (verified install, headless Office to PDF conversion) |
| `evo-worktrees` | Managed Git worktrees for subagents and nested sessions |
| `evo-telemetry` | Span/event contracts with a no-op default |
| `evo-engine` | Composes everything into the headless application and its RPC server |
| `evo-bridge-host` | Native messaging host for Evo Webbridge: frames between the browser's stdio and the shell's pipe; depends on no other Evo crate |

**Dependency rules**

- Dependencies point down the layers above; a lower crate never depends on a
  higher one.
- `evo-ai` is the only place that knows provider wire formats.
- `evo-plugin-sdk` never depends on the host crates; plugins share only the ABI.
- The desktop shell never links `evo-engine`; anything both need lives in a
  leaf crate (`evo-store`, `evo-mesh`, `evo-worktrees`, …).
- A new leaf crate is preferred over growing `evo-engine` when the code is
  reusable by the shell or testable on its own.

### 5.2 Desktop app

`apps/desktop/src` is split into `app/` (shell layout, bootstrap),
`components/` (`ui/` generated shadcn primitives, `ai-elements/` chat
patterns), `features/<feature>/` (one folder per feature), `engine/`
(protocol types and IPC client, no React), `stores/` (zustand) and `lib/`
(pure helpers). Rules: components use store selectors and actions; only
`stores/engine-store.ts` talks to `engine/client.ts`; features use
`components/*` but not each other's internals. The full UI map is in
[apps/desktop/README.md](../../apps/desktop/README.md).

## 6. Runtime view

### 6.1 A prompt

```mermaid
sequenceDiagram
  participant UI as Webview (engine-store)
  participant Shell as Shell (engine.rs)
  participant Eng as evo-engine
  participant P as Model provider
  UI->>Shell: invoke engine_send {prompt}
  Shell->>Eng: JSONL command (stdin)
  Eng->>P: stream request (evo-ai)
  P-->>Eng: deltas
  Eng-->>Shell: events (stdout): message updates, tool calls
  Shell-->>UI: event engine://message
  Eng->>Shell: permission / dialog request (when needed)
  Shell->>UI: show approval in the composer
  UI->>Shell: answer
  Shell->>Eng: response
  Eng-->>UI: turn finished (via Shell)
```

Tool calls run in the engine (built-in tools) or are forwarded to the app
(external tools); either way the engine applies permissions and records the
result in the session.

### 6.2 Sessions talking to sessions

Subagents run inside their parent's engine; nested sessions are separate
engines. All of them are nodes on the **session mesh**: the hub in the shell
routes control, observation and mail between engines and windows; each engine
has a local router for its in-process nodes. When the hub needs an engine
that does not run (a new session, mail for a stopped chat), it asks the
engine supervisor, not a window ([6.3](#63-hosting-an-engine)). Design:
[session mesh spec](../features/session-mesh-protocol/spec.md).

### 6.3 Hosting an engine

Whoever needs a chat's engine (a window that shows the chat, the hub's
`Effect::Host`, the Scheduler's runner, the app's start for chats with
undelivered mail) calls the supervisor's `ensure`. Windows do so through
`engine_ensure` and `engine_attach`. The set-up is the same whoever asked
([shell-hosted engines plan, 3.5](../features/shell-hosted-engines/plan.md#35-behaviour)).

```mermaid
sequenceDiagram
  participant R as Window / hub / Scheduler / app start
  participant S as Shell (supervisor.rs)
  participant H as Mesh hub (mesh.rs)
  participant Eng as evo-engine
  participant W as Windows
  R->>S: ensure(chat, reason)
  S->>Eng: spawn evo-engine --mode rpc (folder, flags, sandbox, approval mode)
  S-->>W: engine://state starting
  Eng-->>S: ready (capabilities)
  S->>Eng: set_model (a new chat: its row's model or the default)
  S->>Eng: configure_external_tools (session, app and connector tools)
  S->>Eng: mesh subscribe at summary, from the shell
  S->>Eng: settings changed while it started
  S->>H: on_hosted(chat)
  H->>Eng: waiting mail: a brief, a scheduled prompt, notes
  S-->>W: engine://state ready
```

- **Wakes.** The shell follows each engine's session node through its
  subscription. When notes, background reports or mail wait for an idle
  chat, it gathers them for 2 s and wakes the chat with one automated turn,
  sent as hub mail to the chat itself. It does not wake a chat with a draft
  in a composer (windows report drafts), an open question or an archived
  row, and stops after 5 wakes in a row until the person's next prompt.
- **Turn records.** Each `agent_settled` carries the run's totals (`turn`:
  model, tokens, cost, the cost of subagents settled since the last turn,
  a preview). The shell records the turn once, whatever number of windows
  are open; it is unread unless a focused window shows the chat. Windows
  record turns only for an engine whose `ready` lacks `turnTotals` (a stale
  sidecar).
- **Scheduled runs.** The runner (`agent_tools/scheduler/run.rs`) runs missed
  tasks once at start, also when Evo starts hidden, and checks for due tasks
  every 20 s. It creates the task's chat, ensures its engine and posts the
  prompt as hub mail, so a window reload loses nothing. It is the only writer
  of `scheduler.tasks`: the Scheduler page and the agent's tools edit through
  it.

## 7. Data view

| Where | What | Owner |
| --- | --- | --- |
| `~/.evo/agent/evo.db` (SQLite, WAL) | chats, workspaces, chat search text, app settings, run log, mail, provider settings, models.dev cache, usage and cost history, windows the agent moved off screen (put back on the next start, [ADR 0015](decisions/0015-agent-drives-desktop-apps-in-the-background-only.md)), each chat's memory flags, summary and "indexed up to" mark, the list of memory changes made by indexing | `evo-store` (schema + migrations); shell and engines both open it |
| `evo.db` tables `plugin_ai_runs`, `plugin_ai_threads`, `plugin_ai_messages`, and `usage_events.plugin_id` (migration 0014) | plugins' AI runs (status, model, cost, kept result and last events; 7 days), their threads per subject (the provider messages, without reasoning, images or attached context), and each model call's usage counted against its plugin; removed with the plugin (usage stays) | written by the plugin-AI engine; read by the shell (caps, Models tab) ([ADR 0023](decisions/0023-plugin-ai-runs-in-one-long-lived-engine-process.md)) |
| `~/.evo/agent/transcripts/<chat id>.db` (SQLite, WAL) | one chat's transcript: evo-harness's `sessions` row and progress frames | written only by that chat's engine; the shell reads, forks and removes them (`evo_store::transcripts`, [ADR 0007](decisions/0007-one-transcript-file-per-chat.md)) |
| `~/.evo/agent/auth.json` | credentials (owner-only on Unix) | engine |
| `~/.evo/agent/settings.json`, `models.json`, `trust.json` | defaults, user model catalog, project trust | engine |
| `~/.evo/agent/mcp.json`, `mcp-auth.json` | connectors and their OAuth tokens | shell |
| `~/.evo/agent/webbridge/{sites.json,profiles.json}`, the host manifest `dev.evo.webbridge.json` and its per-user browser registration | Evo Webbridge site permissions (Allow / Block; others Ask), known browser profiles; where Chrome and Edge find the host | shell (`webbridge/`) |
| `~/.evo/agent/{skills,plugins,lsp,memory}`, `projects/<folder>/memory` | personal skills, plugins, language servers, memory | engine / shell |
| `~/.evo/agent/plugins/installed.json`, `plugins/data/<plugin>/storage.json` | installed plugins with the permissions approved (views and code), optional answers, Developer mode, an update waiting for consent, plain settings (personal and per workspace), which secrets are set, when its code starts, its sign-ins' names and states (never a secret), its AI rules (providers and models allowed, default, tier pins, caps); each plugin's own key–value storage (10 MB, removed on uninstall) | shell (`plugins.rs`, `plugin_settings.rs`, `plugin_auth/`, `plugin_ui/bridge.rs`) |
| OS keychain, service `evo.plugins` (Credential Manager, Keychain, Secret Service), and `~/.evo/agent/plugins/vault/<plugin>.vault` | one 32-byte key per plugin in the keychain; the plugin's secret settings, tokens, refresh tokens and cookies sealed with it (AES-256-GCM); both removed on uninstall | shell only (`evo-vault`, [ADR 0022](decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md)) |
| `~/.evo/agent/runtimes/{deno-<v>,uv-<v>,deno-cache,python}`, `plugins/data/<plugin>/` | Deno and uv for plugin backends, Deno's module cache, uv's Python; each plugin's data folder (with a Python plugin's `venv/`) | shell (`plugin_runtime/`) |
| `~/.evo/agent/AGENTS.md` | personal instructions for every chat | engine |
| `~/.evo/agent/{runtimes,cache/office,bundled-skills,document-versions/<chat>}` | LibreOffice runtime, Office preview cache (native HTML, exact PDFs), extracted Office skills, copies of Office files saved in a chat (versions) | engine (`office.rs`, `evo-office`); the shell removes a chat's versions with the chat |
| `<project>/AGENTS.md`, `<project>/.evo/`, `.mcp.json` | project instructions, commands, agents, settings, connectors | the user (read by the engine after trust) |
| managed worktrees | delegated work in isolation | `evo-worktrees` |
| `<app cache dir>/updates/<version>/` | a downloaded, verified app update waiting for the person's restart; settings `updates.autoCheck` and `updates.lastCheckedAt` in `evo.db` | shell (`updates.rs`, [ADR 0017](decisions/0017-app-updates-from-signed-github-releases.md)) |

Rules:

- `EVO_CODING_AGENT_DIR` relocates `~/.evo/agent`; never hardcode the path.
- Schema changes go through numbered migrations in
  `crates/evo-store/src/migrations/`; old databases must keep upgrading.
- One writer per transcript file: only the chat's engine writes it; nothing
  else opens it for writing while that engine may run.
- Secrets stay out of `evo.db`, the transcript files and logs. A plugin's
  secrets live only in its sealed vault; `installed.json` says which are set.
- Repository and project files are user-owned: app cleanup never deletes them.
- Models are data from models.dev
  ([ADR 0002](decisions/0002-models-dev-is-the-model-database.md)); no model is
  hardcoded.

Details: [app database spec](../features/app-database/spec.md).

## 8. Interfaces

| Interface | Between | Defined in | Compatibility rule |
| --- | --- | --- | --- |
| Engine protocol | shell/clients ↔ engine | `crates/evo-protocol` | additive changes; capabilities announced in `ready`; old shapes keep working |
| Mesh envelope | engines ↔ hub ↔ windows | `crates/evo-mesh` (`wire`) | versioned (`v`); messages without an envelope are legacy and still accepted |
| Tauri commands / events | webview ↔ shell | `apps/desktop/src-tauri/src/*.rs`, `apps/desktop/src/lib`, `src/engine` | changed together with their TypeScript callers |
| External tools | engine ↔ app | `crates/evo-engine/src/rpc/external_tools.rs` | tool schemas are part of the contract |
| Plugin ABI | engine ↔ `cdylib` plugins | `crates/evo-plugin-sdk` | stable `#[repr(C)]`; breaking changes need a new ABI version |
| MCP | shell ↔ connectors | `crates/evo-mcp` | follows the MCP spec |
| Webbridge wire | Evo extension ↔ `evo-bridge-host` ↔ shell | `apps/desktop/src-tauri/src/webbridge/wire.rs`, `extensions/evo-webbridge/background/port.js`, [plan 3.3](../features/browser-bridge/plan.md#33-interfaces) | versioned (`v` in `hello` / `welcome`); messages added, never changed in meaning; a newer peer's unknown message is ignored |
| Hooks | engine ↔ user commands / URLs | `crates/evo-hooks` | documented exit codes and JSON |
| Plugin bridge (`window.evo`) | plugin views ↔ shell | `apps/desktop/src-tauri/src/plugin_ui/{bridge.rs,client.js}`, [sample plugin](../../examples/plugin-ui-sample/README.md) | calls are added, never changed in meaning; every call is checked against the plugin's approved permissions |
| Plugin host API (`/v1/…` on `127.0.0.1`, `EVO_PLUGIN_HOST` + `EVO_PLUGIN_KEY`) | plugin backends and hooks ↔ shell | `apps/desktop/src-tauri/src/plugin_host/`, [sample plugin](../../examples/plugin-settings-sample/README.md) | versioned path (`/v1`); endpoints added, never changed in meaning; each call checked against the key's plugin, its part's permission and `secrets` |
| Plugin AI (`window.evo.ai`, `/v1/ai/*` on the host API) | plugin views and backends ↔ shell | `apps/desktop/src-tauri/src/plugin_ai/`, `plugin_ui/client.js`, [evo-creator `references/ai.md`](../../crates/evo-engine/bundled-skills/evo-creator/references/ai.md), [sample plugin](../../examples/plugin-settings-sample/README.md) | as the bridge and the host API: calls and options added, never changed in meaning; every run checked against `ai`, its tool groups and the person's caps |
| Plugin-AI protocol | shell ↔ `evo-engine --mode plugin-ai` | `crates/evo-protocol/src/plugin_ai.rs` (`PROTOCOL`, announced in `ready`) | additive; the shell speaks it as JSON without linking the crate, and its tests pin the shapes |
| Plugin manifest (`settings`, `accounts`, `runtime`, permissions) | plugins ↔ Evo | `crates/evo-plugins` (`settings.rs`, `accounts.rs`, `runtime.rs`, `manifest.rs`) | fields added as optional; a wrong entry is left out with its problem, never fatal for the plugin |

## 9. Cross-cutting concepts

- **Permissions and sandbox.** Tool calls pass the permission check (Allow /
  Ask / Block) and the sandbox config (`crates/evo-tools/src/sandbox.rs`,
  passed as `--sandbox-config`). Approvals are shown in the composer.
  The approval mode (`ask` · `auto-edits` · `auto`, an app setting passed as
  `--approval-mode` and changed live with `set_approval_mode`) is enforced by
  the engine as the last link of `before_tool_call`: read-only guard →
  self-kill guard → `PreToolUse` hooks → plugin handlers → approval mode. It
  classifies each call as a read, an edit or a command and asks for the kinds
  the mode says ([ADR 0010](decisions/0010-engine-enforces-approval-mode.md)).
  App-side tools — terminal, scheduler, connectors — ask through the same
  approval panel, reading the same mode. The computer app tool has
  a gate class of its own: with its setting on "Ask every time" it asks in
  every mode ([ADR 0015](decisions/0015-agent-drives-desktop-apps-in-the-background-only.md)).
- **Questions that wait for the person.** An engine's approvals and
  `ask_user` questions, and the questions of the shell's own tools, are kept
  by the shell per chat (`supervisor/questions.rs`), whether or not a window
  shows the chat. When no focused window shows it, the shell sends an OS
  notification (clicking it opens the chat) and a bell entry, and the tray
  lists the chat as waiting. A window that attaches shows every open
  question. Nothing answers a question for the person and none times out; a
  question ends when it is answered or its call or engine ends. A chat with
  an open question counts as working, so its engine is never trimmed.
- **Untrusted data.** Tool results, web pages, MCP output, plugin content,
  documents and memories are data, never instructions to the app.
- **The person's own browser.** Evo Webbridge acts only in tabs of a chat's
  tab group; per-site permissions (Ask, Allow, Block) sit inside the approval
  gate, and Block holds in every mode. The browser's debugging notice, the tab
  group and Evo's in-page indicator (with Stop and Take over) show agent work.
- **Plugin views.** A plugin's views are untrusted pages. Each runs in its
  own webview at its plugin's own origin, with a CSP (no remote or inline
  scripts, no frames, fetches only to its `network:` origins), no Tauri
  command (no capability covers it, and the invoke handler refuses
  `plugin-*` webviews), and one way in: the bridge. The bridge maps the
  calling webview to its plugin, then checks the call's shape, the
  permission the person approved (at install, and again when an update asks
  for more) and a rate of 100 calls a second. Tool calls from a view go
  through the same approval mode and gates as the agent's, and the approval
  names the plugin. Incognito chats are invisible to plugins
  ([ADR 0021](decisions/0021-plugin-views-run-in-native-webviews.md)).
- **Plugin code and secrets.** A plugin's hooks run only with `hooks`, its
  backend and connector servers only with `runtime`, and any of its code
  that uses secrets or accounts only with `secrets`; a required setting not
  filled holds its code back. Taking a permission back stops that part at
  once. Network and file limits are enforced where Evo can enforce them
  (Deno) and marked "not enforced" elsewhere (Python, hooks, connector
  servers). Secrets reach code only as environment variables of its own
  processes, or on request through the host API; never in arguments, URLs,
  logs or the windows. Views see accounts' states, never tokens
  ([ADR 0022](decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md)).
- **Plugins and the person's models.** A plugin never holds a provider
  key: the plugin-AI engine reads Evo's credentials, and a plugin learns
  only models, abilities and prices. `ai` is a consent permission that
  says it may cost money; each of Evo's tool groups (`ai.tools:read`,
  `edit`, `command`, `web`) is one more. Caps per day and month (none by
  default) are checked when a run starts and on every usage event. Evo's
  tools in a plugin's run go through the person's approval mode as a
  chat's do, with questions that name the plugin; attached context and
  tool results are marked as data to the model and change no tool or
  permission; runs, threads and follows are checked against the calling
  plugin ([ADR 0023](decisions/0023-plugin-ai-runs-in-one-long-lived-engine-process.md)).
- **Project trust.** Project hooks, `.mcp.json` servers and settings apply only
  after the user trusts or approves them.
- **Configuration.** Settings changes apply without restarting the app. The
  shell passes each change to every engine, viewed or not: the approval mode
  at once, provider and resource reloads to idle engines and to busy ones
  when their run settles, a sandbox change by restarting engines with no work
  (the others when they settle). Settings the shell needs live in the app
  database, not in a window.
- **Errors.** Errors reach the user in plain words in the chat or a
  notification. An engine that exits is reported in its chat; the transcript
  is durable in `evo.db`, so the chat continues in a new engine.
- **Notifications.** One pipeline for toasts, the bell and OS notifications
  ([notifications spec](../features/notifications/spec.md)). The shell
  announces questions and the ends of runs in chats no focused window shows,
  once for the whole app, with titles only (`supervisor/notices.rs`); bell
  entries made with no window open are kept until a window opens.
- **Telemetry.** `evo-telemetry` contracts with a no-op default; nothing leaves
  the device unless a real context is configured.
- **Testing.** Library crates test against `faux` providers and in-memory
  environments; engine tests drive RPC; the desktop is verified by type-check,
  build and driving the real app (no mocked engine).

## 10. Deployment view

- The engine is built (`cargo build -p evo-engine`) and staged as a Tauri
  sidecar by `apps/desktop/scripts/prepare-sidecar.ts`
  (`src-tauri/binaries/evo-engine-<target-triple>`), installed next to the app.
- Installers (`.github/workflows/desktop-packages.yml`): NSIS for Windows x64,
  DMG for macOS Intel and Apple Silicon, DEB for Linux x64.
- App updates ([ADR 0017](decisions/0017-app-updates-from-signed-github-releases.md)):
  tag builds sign the update files with the release key (GitHub secrets
  only), and every release carries a `latest.json` that the installed apps
  read. Windows and macOS builds signed with the project's Apple identity
  install updates in the app; Linux packages and other macOS builds are only
  told about them.
- CI (`.github/workflows/ci.yml`): rustfmt, clippy, build and test on
  Linux/Windows/macOS, MSRV check, desktop type-check and build.
- One running copy of the app (single-instance); a second launch focuses the
  first.
- Plugin runtimes are not shipped: Deno and uv are downloaded into
  `~/.evo/agent/runtimes` the first time a plugin needs them, from their
  GitHub releases, at versions and SHA-256 sums pinned in the app
  (`src-tauri/src/plugin_runtime/install.rs`); uv brings its own Python.
  A new pin ships with an app update ([ADR 0022](decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md)).

## 11. Architecture decisions

The index of decisions, their status and links is in
[decisions/README.md](decisions/README.md).

## 12. Risks and technical debt

| Item | Impact | Where tracked |
| --- | --- | --- |
| Some crate READMEs and `embedded-docs/` still describe the removed CLI/TUI | misleading docs for contributors and the agent | — |
| Browser screenshots and trusted input are Windows-only | platform gap | [apps/desktop/README.md](../../apps/desktop/README.md) "Known gaps" |
| Background-only control of other apps cannot do hover-only UI, drag and drop between apps, key tips or raw-input apps | some app tasks need the person | [computer app control spec](../features/computer-app-control/spec.md) FR-19, [ADR 0015](decisions/0015-agent-drives-desktop-apps-in-the-background-only.md) |
| The release key is one key pair: lost, installed apps cannot update in the app; leaked, it could sign a malicious update | every installed copy | [ADR 0017](decisions/0017-app-updates-from-signed-github-releases.md) |
| Engines work with no window: nested sessions, background agents, wakes and scheduled runs go on while every window is closed | work, cost and tool calls the person does not watch as they happen. Mitigations: the sidebar marks chats working or waiting with no window, the tray counts them and opens a waiting one, questions are notified and never answered for the person, quitting asks once while chats work. The macOS app menu's own Quit still quits without asking | [shell-hosted engines](../features/shell-hosted-engines/spec.md) NFR-5, FR-11, FR-12, [ADR 0018](decisions/0018-the-shell-hosts-engines-windows-attach.md) |
| The desktop has no automated UI test runner | regressions found late | — |
| Plugin views are checked on Windows only; a first open takes about 0.6 s (WebView2 creates the view), a reopen within 5 minutes about 0.1 s | macOS behaviour and isolation unproven; slow first open | [plugin UI spec](../features/plugin-ui/spec.md) NFR-2, NFR-4 |
| A plugin granted `chat:read` reads every chat but incognito ones | a trusted plugin sees a lot; consent is the only gate | [plugin UI spec](../features/plugin-ui/spec.md) FR-3, 4.3 |
| Python backends, hooks and connector servers of plugins are not confined (network, files, subprocesses) | an approved plugin's code can do what the person can; the consent says "not enforced" | [plugin settings spec](../features/plugin-settings/spec.md) FR-15, FR-16, [ADR 0022](decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md) |
| The keychain protects plugin secrets at rest, not from the person's other programs (Windows Credential Manager entries are readable by them) or the agent's own commands | a local program running as the person can read a plugin's vault key | [ADR 0022](decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md), plugin settings plan 3.7 |
| Plugin settings, the host API and sign-ins are checked on Windows only; a plugin's `SessionStart` hook may run before the engine has its host key | macOS and Linux unproven | [plugin settings spec](../features/plugin-settings/spec.md) NFR-3 |
| Plugin AI spends the person's money: with no cap set, an approved plugin may spend without limit; with `ai.tools:apps` its AI browses in Evo's browser (with the person's sites' sessions) and runs terminals, under the approval mode | cost and actions the person does not watch as they happen; the consent and the Models tab show spend, caps stop runs, the approval mode asks before edits and commands, and "auto" asks for none | [plugin AI spec](../features/plugin-ai/spec.md) FR-13, FR-14, T23, [ADR 0023](decisions/0023-plugin-ai-runs-in-one-long-lived-engine-process.md) |
| Main JS bundle ~1.2 MB | startup time | [apps/desktop/README.md](../../apps/desktop/README.md) "Known gaps" |
| A transcript's `state_json` is the whole reducer snapshot, rewritten on every write | slow writes in very long chats | [app database plan, "Planned tables"](../features/app-database/plan.md#planned-tables) |

## 13. Glossary

| Term | Meaning |
| --- | --- |
| Chat / session | One conversation, backed by one engine process and one transcript |
| Engine supervisor | The part of the shell that starts, sets up, wakes, records, restarts and stops chats' engines |
| Viewer | A window that shows a chat; it attaches to the chat's engine and detaches without stopping it |
| Subagent | A child agent inside its parent's engine, run as a tool call |
| Nested session | A full chat started by another chat, with its own engine |
| Node | Anything on the session mesh: a session, an agent, a side question |
| External tool | A tool implemented by the app and offered to the engine over the protocol |
| Connector | An MCP server |
| Skill | A folder with `SKILL.md` the agent can load on demand |
| Sidecar | The `evo-engine` binary shipped next to the desktop app |
| Topic | A memory: one short document about one subject, changed in place |
| Index run | A short-lived `evo-engine --mode index` process that reads what is new in one idle chat, updates memory topics and writes the chat's summary |
| Incognito chat | A chat that uses no memory and leaves no history: it is removed when it ends |
| Plugin backend | A plugin's own MCP server that Evo runs under Deno or uv, with the permissions the person approved |
| Plugin AI run | One request of a plugin to the person's models: a model chosen by need, a stream of events, optional tools, a thread, a result; run by `evo-engine --mode plugin-ai` |
| Thread | A plugin's conversation about one subject, kept in `evo.db` and continued by runs that name it |
| Vault | A plugin's secrets, sealed with a key kept in the OS keychain |
| Host API | The shell's loopback HTTP API where a plugin's backend and hooks ask for tokens, cookies and settings |
