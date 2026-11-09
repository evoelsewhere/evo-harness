<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/banner-dark.png">
    <img src="docs/assets/readme/banner-light.png" width="800" alt="Evo Harness: a chat that added a retry to web_fetch, next to the edited fetch.rs with its git changes in the Files panel" />
  </picture>

  **A local-first desktop workspace for AI agents.**

  Evo Harness brings agent chats, files, terminal, browser, git, connectors
  and skills into one desktop app. A headless Rust engine runs every chat on your
  machine; the desktop app is a thin shell around it.

  [![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-2563EB.svg)](LICENSE)
  [![Desktop app](https://img.shields.io/badge/Product-Desktop-1764FF)](apps/desktop/)
  [![Rust 1.89+](https://img.shields.io/badge/Rust-1.89+-000000?logo=rust&logoColor=white)](Cargo.toml)
  [![Tauri v2](https://img.shields.io/badge/Tauri-v2-FFC131?logo=tauri&logoColor=white)](apps/desktop/)

  **[Website](https://evoelsewhere.asia/)** · **[Features](#core-features)** · **[Download](#download)** ·
  **[Run from source](#run-from-source)** · **[Documentation](#documentation)**
</div>

## Highlights

- **One engine process per chat.** A crashing chat, tool or plugin does not
  take the window down. Sessions are saved as they run and resume after a
  crash.
- **Bring your own model** from hosted, cloud or local providers. Models,
  limits, costs and thinking options come from models.dev, not from code.
- **Workspace tools beside the chat:** Files with a code editor and Git
  changes, Terminal, a Browser the agent can drive, Memory and a side chat.
- **You stay in control:** approval prompts, a sandbox, project trust, and the
  usage and cost of every response, all kept on your machine.

## Core features

Animated walkthroughs of these four are on the
[website](https://evoelsewhere.asia/#core).

### Customize

Shape how the agent works, per person or per project, from **Customize** in
the sidebar. Changes apply without restarting the app.

- **Skills and slash commands** in the open Agent Skills (`SKILL.md`) format,
  from evo's folders, `.agents/skills` or plugins. Each skill has a switch, a
  "who can use it" setting and a usage count; type `/` in the composer to run
  one.
- **Connectors (MCP servers)** over stdio, streamable HTTP or SSE, with OAuth
  sign-in for remote servers. One shared connection pool serves every chat.
  Each tool is set to Allow, Ask or Block, and connectors can be switched off
  per chat from the composer's `+` menu.
- **Plugins and marketplaces** bundle skills, commands, subagents, hooks and
  connectors. Install them from a GitHub `owner/repo`, any git URL or a
  folder.
- **Hooks** run a command or a web request on session, prompt and tool events,
  and can block, allow or rewrite a tool call or add context.
- **Instructions** for every chat (Personal) and for one project.

A project's skills, hooks, instructions and `.mcp.json` servers are only used
after you trust the folder and approve them.
[Spec](docs/features/customize-skills-plugins-connectors/spec.md)

### Browser

The agent gets its own browser inside the app and works in it while you keep
chatting.

- **Floating cards.** Each page the agent uses floats over its chat as a live
  card (up to three per chat), so you do not have to open a panel. The card
  shows what the agent is doing, and you can minimise it, drag it, resize it
  or open it in the side panel.
- **A virtual cursor** shows where the agent points and clicks. You can click
  and type in the card yourself, for example to sign in.
- **Desktop-sized pages.** Pages the agent drives always lay out at
  1280 × 800 and are scaled to fit.
- **Real browser tools:** navigate, read the page as an outline, text or
  Markdown, click, type, drag, wait, handle dialogs, upload files and run up
  to 20 steps in one batch. The agent can also set the viewport, colour
  scheme, locale and time zone, edit cookies and storage, mock requests, read
  the console and network, save a PDF and record performance traces.
- **Approvals by site.** Reading a page never asks. Clicks and typing on a
  site that is not on your computer ask once per site per chat in Ask every
  time. Uploads, storage writes, mocks and scripts ask in Ask every time and
  Approve edits.

Trusted input, screenshots and page cards are Windows-only for now; on macOS
and Linux, clicks and typing are scripted.
[Spec](docs/features/agent-browser/spec.md) ·
[Floating cards](docs/features/floating-cards/spec.md)

**WebBridge** (in development) will let the agent work in a tab of your own
Chrome or Edge, where you are already signed in. Nothing is shared until you
pick a tab, a shared tab belongs to one chat, and you can stop sharing at any
time. [Spec](docs/features/browser-bridge/spec.md)

### Computer App Control (preview, Windows)

Ask for work in a desktop app you already use, such as a spreadsheet, a
document or a chat app. The app's window moves off your screen while the agent
works in it (it is not minimised or hidden), and a live card shows it with a
virtual cursor.

- **Never your mouse or keyboard.** evo does not move your pointer, change
  your keyboard focus or front window, or use the clipboard.
- **Every action asks first** by default, even in Approve for me. The prompt
  lists each click and keystroke as it will run.
- **It checks its own work.** The agent acts on identified UI elements first,
  reads the app back after each step and says what changed. When something
  cannot be done in the background, it says so.
- **You stay in charge.** **Stop** ends the current action until you choose
  **Allow again**. **Show the app** brings the window back and ends control
  for the turn. **Close** hands the app back. The window returns exactly where
  it was when control ends.

Turn it on in **Settings → Computer apps** (off by default), where you can
also allow or block apps. evo itself, system processes, apps running as
administrator and password fields are never controlled.
[Spec](docs/features/computer-app-control/spec.md)

### Office documents

Word, PowerPoint and Excel files open in **Files**, rendered natively with no
download, network access or scripts. Bundled `docx`, `pptx` and `xlsx` skills
let the agent create and edit them.

- **A real viewer:** zoom, search and thumbnails. Decks have Normal, Sorter
  and Reading views with speaker notes. Workbooks have sheet tabs, a name
  box, the formula bar and a print layout.
- **Live build.** A deck or workbook fills in slide by slide or sheet by sheet
  while the agent builds it, and new files open by themselves.
- **Point, then ask.** Use **Select an area to edit** on a slide, or **Select
  cells to edit** in a sheet, and write what should change. The agent changes
  only what you marked.
- **Every version kept.** Undo, Redo and History step through the last 50
  saves.

For exact print pages, install the optional LibreOffice runtime from the
viewer's banner. [Spec](docs/features/office/spec.md) ·
[Viewer and editing](docs/features/office-v3-parity/spec.md)

## More features

| Feature | What it does |
| --- | --- |
| [Subagents and nested sessions](docs/features/subagents-and-nested-sessions/spec.md) | Delegate work to a child agent with a fresh context: general, read-only explore or plan, or your own types in `.evo/agents`. Background agents report back when they finish. Nested sessions are full chats under their parent. Either can work in its own Git worktree, which you review and then apply or discard. |
| [Code intelligence](docs/features/code-intelligence/spec.md) | Language servers for 21 languages give the agent diagnostics, definitions, references and symbols. They install on demand from **Customize → Code intelligence**, and servers you already have are reused. |
| [Git in Files](docs/features/files-git/spec.md) | Changed files are marked in the tree and changed lines in the editor gutter. Click a gutter marker to see the committed version. |
| [Terminal](docs/features/agent-terminal/spec.md) | Every shell on your machine, including PowerShell, Git Bash, WSL and Visual Studio developer shells. The agent can run commands, wait for output and send keys in its own tabs, which are marked with a bot icon. |
| [Approval modes](docs/features/approval-modes/spec.md) | Choose Ask every time, Approve edits or Approve for me from the composer. Reads never ask. A change applies to every running chat at once. |
| [Memory and Scheduler](docs/features/memory-and-scheduler/spec.md) | Memories persist across chats; project memories are kept outside the repository. Scheduled prompts run as new chats, manually, once, hourly, daily, weekly or on a cron schedule. |
| [Usage and cost](docs/features/usage-and-cost/spec.md) | The tokens and models.dev cost of every response, broken down by model, chat and source, with a price check against other models. Nothing leaves your computer. |
| [Web search and web fetch](docs/features/web-search-and-fetch/spec.md) | Search that works with any model and needs no key, and public pages read as Markdown. |
| [Notifications](docs/features/notifications/spec.md) | A notification bell, toasts and OS notifications for finished runs, approvals and scheduled tasks, plus Do not disturb. |
| [Model providers](docs/features/app-database/spec.md) | Connect providers in **Settings → Providers** with an API key or by signing in. Model lists come from models.dev, refresh without a restart and have a bundled snapshot for offline use. Credentials stay in an owner-only file, not in the app database. |

The full list, with the status of each feature, is in the
[feature catalogue](docs/features/README.md).

## Download

Download the latest desktop release from
[GitHub Releases](https://github.com/evoelsewhere/evo-harness/releases).
Packages are available for macOS (Intel and Apple Silicon), Windows and
Linux. Each package includes the `evo-engine` sidecar.

Linux packages should be installed through the system package manager, for
example:

```sh
sudo apt install ./Evo*_amd64.deb
```

The LibreOffice runtime for Office document previews is optional. The app
offers to install it the first time you open an Office file.

## Run from source

### Requirements

- Rust (the toolchain is pinned in `rust-toolchain.toml`; MSRV 1.89)
- [Bun](https://bun.sh/)
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your
  operating system
- On Windows, [NASM](https://www.nasm.us/) for a clean build

### Setup

```sh
git clone https://github.com/evoelsewhere/evo-harness.git
cd evo-harness/apps/desktop

bun install
```

Start the development app with:

```sh
# Build evo-engine, stage it as the sidecar, then run the Tauri app
bun run dev:app
```

Use bun only in `apps/desktop`; do not use npm, pnpm or yarn there.
`bun run build:app` builds a release with installers for the current
platform. The engine and library crates build and test from the repository
root:

```sh
cargo build --workspace
cargo test --workspace
```

On first launch, connect a model provider in **Settings → Providers**, then
open a folder and start a chat.

## Documentation

Each feature above links to its spec. Detailed contracts and guides live under
[`docs/`](docs/):

- [Documentation index](docs/README.md)
- [Architecture (ADD) and decisions (ADRs)](docs/architecture/README.md)
- [Feature catalogue: specs, plans and tasks (Spec-Driven Development)](docs/features/README.md)
- [Desktop UI map and source layout](apps/desktop/README.md)
- [Visual design language](DESIGN.md)
- [Contributor and agent guide](AGENTS.md)

## Architecture at a glance

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/architecture-dark.png">
  <img src="docs/assets/readme/architecture.png" width="960" alt="Evo Harness architecture: the React UI talks to the Tauri shell over IPC; the shell starts one evo-engine process per chat and relays JSONL to it, and hosts the shared connector pool; the engine works on project folders, saves sessions to local state, calls model providers over HTTPS and reads model data from models.dev." />
</picture>

The engine owns agent behaviour: the agent loop, tools, sessions, compaction,
skills, hooks, memory, model selection and credentials. The shell owns native
features (windows, terminals, files, browser webviews, the MCP connection
pool, plugins) and relays the protocol. Model providers are replaceable, while
the harness handles context, tools, permissions, memory and verification.

## Project layout

```text
crates/         Rust workspace: engine, protocol, agent loop, tools, providers
apps/desktop/   Tauri 2 shell (src-tauri/) and React 19 UI (src/)
examples/       minimal native plugin used by extension tests
scripts/        Office runtime build, README screenshots
docs/           architecture (ADD, ADRs) and feature spec / plan / tasks
```

The crates are released together and depend only on the layers below them:

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/crate-layers-dark.png">
  <img src="docs/assets/readme/crate-layers.png" width="960" alt="Evo Harness crate layers, top to bottom: App (evo-engine, apps/desktop/src-tauri); Protocol (evo-protocol, evo-mesh); Runtime (evo-harness, evo-durable, evo-runtime, evo-extensions, evo-plugin-sdk); Agent (evo-agent, evo-tools, evo-worktrees); LLM (evo-ai); Leaf services with no internal dependencies (evo-store, evo-mcp, evo-plugins, evo-hooks, evo-lsp, evo-web, evo-office, evo-telemetry)." />
</picture>

Diagram and banner sources are in [`docs/assets/readme/`](docs/assets/readme/);
[`scripts/readme-shots/`](scripts/readme-shots/) regenerates the screenshots.

## Contributing

Issues and pull requests are welcome. Keep changes focused and include the
smallest relevant test run. [AGENTS.md](AGENTS.md) lists the checks for each
layer and the design-document workflow. Report vulnerabilities privately
through GitHub Security Advisories.

## License

Evo Harness is released under the [Apache License 2.0](LICENSE). Parts of the
`evo-*` crates are derived from MIT-licensed upstream work; see
[NOTICE](NOTICE).
