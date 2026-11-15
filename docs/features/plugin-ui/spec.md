# Plugin UI: plugins with their own views in evo — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop shell · desktop UI · crate `evo-plugins` |
| Created | 2026-11-13 |
| Updated | 2026-11-14 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Customize: skills, plugins, connectors](../customize-skills-plugins-connectors/spec.md) · [Sidebar items](../sidebar-items/spec.md) · [Session panels](../session-panels/spec.md) · [Split view](../chat-split-view/spec.md) |

## 1. Summary

evo plugins today bundle skills, commands, subagents, hooks and connectors,
but they cannot show anything. Every page in the sidebar and every side panel
tool is fixed in the app's code.

This feature lets a plugin bring its own views, written in HTML with
TypeScript or JavaScript. A view appears in four places:

- a page in the sidebar;
- a tool in the side panel;
- a card in the chat;
- an action on the composer.

Each view runs in a native webview of its own, isolated from the app. It
talks to evo through one permission-checked bridge (`window.evo`): it can
read and use chats, call tools through the usual approvals, keep its own
storage and follow the theme. The person
approves a plugin's permissions once, when they install it.

This is step 1 of the plugin layer. Its research note covers DeepSeek
Harness's Creator Mode and evo's two plugin systems. Later features add:

- the plugin creator: a built-in skill and `/create-plugin`;
- plugin backends in Rust, TypeScript, JavaScript and Python, run with the
  Bun and uv that evo ships;
- forwarding the UI of native extensions to the desktop.

## 2. Scope

### 2.1 Goals

- A `ui` section in `.evo-plugin/plugin.json` declares views for the four
  slots. Each view has an HTML entry inside the plugin folder.
- The views of enabled plugins appear in the app with no restart:
  - sidebar pages join the sidebar's items (under More by default, and
    shown via Edit sidebar);
  - panel tools join the tool rail, the side panel and the command palette;
  - composer actions join the composer;
  - chat cards appear when the agent or a view shows one.
- Each view runs in its own native webview, placed over the slot it fills.
  It is served from the plugin's own folder and has no access to the app's
  internals.
- The `window.evo` bridge covers:
  - chats: list them, read their messages and events, send a message;
  - tools: call one, through approvals;
  - the UI: notify, open a view, insert text in the composer;
  - the theme: read the tokens, follow the theme;
  - a per-plugin storage.

  Each call is allowed only by a permission the plugin declared and the
  person approved.
- Install-time consent: installing or enabling a plugin with views lists its
  permissions in plain words and asks once. An update that adds permissions
  asks again before it takes effect.
- The agent can show a plugin's card in the chat, and can open a plugin's
  page or panel.
- With a plugin's Developer mode on, editing its view files reloads them.

### 2.2 Non-goals

- Plugin backends (processes in Rust, TS, JS or Python), the bundled Bun and
  uv, and `evo.backend.*` belong to the next feature.
- The plugin creator skill and `/create-plugin` are their own feature. This
  one only provides the dev reload it relies on.
- Native extensions' `UiComponent` and `SetStatus` on the desktop.
- MCP Apps (`ui://` resources).
- Themes as plugins. A view may override theme tokens inside itself only.
- A public plugin store. Plugins come from marketplaces and folders, as
  today.

## 3. User scenarios

### US-1 Install a plugin that adds a page (priority P1)

A person adds a marketplace and installs "Sticky Wall". It brings a sidebar
page for notes pinned around a chat.

1. **Given** the plugin declares a sidebar page and `chat:read`, `storage`,
   **when** it is installed, **then** a dialog lists "Read your chats" and
   "Keep its own data", and installs on Confirm.
2. **Given** it is installed, **then** "Sticky Wall" is in the sidebar's More
   menu with its icon. Choosing it opens its page in the chat area. Edit
   sidebar can pin it.
3. **Given** the page is open, **when** it calls `evo.chat.messages()`,
   **then** it gets the active chat's messages. A call needing an unapproved
   permission fails with a clear error.

### US-2 A panel tool beside the chat (priority P1)

1. **Given** a plugin with a panel "Notes", **then** its icon is on the tool
   rail and in the command palette. Opening it shows the view as a side
   panel tab that follows the active chat.

### US-3 The agent shows a plugin card (priority P2)

1. **Given** a plugin with a card "Chart", **when** the agent shows it with
   some data, **then** the card renders in the transcript with that data,
   and is drawn again when the chat is reopened.

### US-4 A composer action (priority P2)

1. **Given** a plugin with a composer action "Insert snippet", **then** a
   button appears on the composer. Its view opens as a small popover and can
   put text into the composer with `evo.composer.insert()`.

### US-5 Develop a plugin with live reload (priority P2)

1. **Given** a plugin with Developer mode switched on in Customize › Plugins,
   **when** a file under it changes, **then** its open views reload within a
   second, keeping their storage.

### Edge cases

- A plugin is disabled or uninstalled while its page is open: the view
  closes, and the chat area shows the active chat.
- Two plugins declare the same view id: ids are namespaced
  `<plugin>/<view>`, so they do not collide.
- A view's entry is missing or fails to load: its slot shows a short error
  with the plugin's name. Nothing else breaks.
- A view floods the bridge: calls are rate-limited per view.
- A plugin from a project folder that is not trusted: it is not shown, like
  its hooks.
- A plugin's view is open in two places (a page and a card): each is its own
  webview, and they share the plugin's storage.
- An app menu, popover or dialog opens over a plugin view: native webviews
  sit above the app's own content, so the view is hidden (its last frame
  shown in its place) while the overlay is open, and comes back after.
- A chat card scrolls out of sight: its webview is hidden. Cards far out of
  sight are unloaded and loaded again when they scroll back.
- A view opens before its plugin's backend runs (the first start downloads
  a runtime and installs dependencies): the slot waits for the backend
  and says which step it is on, so the view's first `backend.call` does not
  fail (FR-17).

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | `plugin.json` accepts `ui` with `sidebar[]`, `panels[]`, `cards[]` and `composer[]`. Each entry has an `id`, a `title`, an `entry` (HTML path inside the plugin) and an optional `icon` (SVG/PNG inside the plugin). A sidebar page also has `defaultShown`, and a panel has an optional `shortcut`. Paths that leave the plugin folder, and unknown slots, are rejected with a message in Customize › Plugins. | A manifest with a `../` entry shows an error and adds no view. |
| FR-2 | `plugin.json` accepts `permissions[]` from a fixed list (section 4.3). An unknown permission is an error. | An unknown permission is refused. |
| FR-3 | Installing or enabling a plugin that declares views or permissions shows them in plain words and asks once. An update that adds permissions keeps the old version running until the person approves the new list. | Installing asks once; an update with a new permission asks again. |
| FR-4 | Enabled plugins' sidebar pages join the sidebar items: shown under More unless `defaultShown`, pinnable in Edit sidebar. They open as a page in the chat area, like Scheduler. | US-1. |
| FR-5 | Enabled plugins' panels join the side panel: a tool-rail icon, a command-palette entry, the optional shortcut, and a side panel tab that follows the active chat. | US-2. |
| FR-6 | Enabled plugins' composer actions add a button to the composer, in a "Plugins" group when there are several. The button opens the view in a popover above the composer. | US-4. |
| FR-7 | An app tool `plugin_show_card {plugin, card, data}` lets the agent show a plugin card in the transcript with `data` (JSON, up to 64 KB). The card is stored with the turn and drawn again when the chat is reopened. A disabled plugin's card shows a placeholder. | US-3. |
| FR-8 | An app tool `plugin_open {plugin, view}` lets the agent open a plugin's sidebar page or panel. | The agent opens a panel. |
| FR-9 | Every view runs in its own native child webview, laid over its slot and kept in step with it (scrolling, resizing, split view, the side panel opening). It is served from its plugin's folder under its own origin. It can call only the plugin bridge, never the app's other IPC. Its network access is limited to the origins its permissions name, and it loads no remote scripts. | A view cannot reach the app's other commands or another plugin's files. |
| FR-10 | `window.evo` gives views: `chat.active()`, `chat.list()`, `chat.messages(chat?)`, `chat.onEvent(fn, chat?)`, `chat.send(text, chat?)` (the active chat when none is named), `tools.call(name, args)`, `ui.notify()`, `ui.open(view)`, `composer.insert(text)`, `theme.tokens()`, `theme.onChange()`, `storage.get/set/delete/list`, `view.data()` (a card's data) and `view.resize()`. Each call is checked against the plugin's approved permissions in the shell, not in the view. | A call without its permission is refused with a clear error. |
| FR-11 | `tools.call` runs an app tool or a connector tool as the agent's call would: approval mode, read-only roles and per-tool permissions apply, and the approval names the plugin. The engine's own tools (shell, edit and the like) are not callable from a view in this step. | A connector tool call from a view in Ask mode shows an approval naming the plugin. |
| FR-12 | Views get the app's theme tokens (light and dark) and follow theme changes. A view may override tokens inside itself only. | A view follows a theme switch. |
| FR-13 | A plugin's storage is a key–value store in its data folder, up to 10 MB, kept across updates and removed on uninstall. | Data survives an update; uninstall removes it. |
| FR-14 | A plugin's Developer mode (a switch in Customize › Plugins, off by default) watches its folder. A changed file reloads its open views and keeps its storage. | US-5. |
| FR-15 | Disabling or uninstalling a plugin closes its views and removes its slots at once. Cards in old chats show a placeholder. | Edge case 1. |
| FR-16 | Customize › Plugins shows each plugin's views and permissions, and lets the person revoke a permission. A revoked permission makes calls that need it fail. | Revoking `chat:send` makes `chat.send` fail. |
| FR-17 | A view whose plugin has a backend to run (not set never to start, nothing waiting on the person) waits for it in its slot. While the backend's runtime is downloaded, its dependencies installed or it starts, the slot shows the plugin's icon with a soft ring and the steps — get its runtime, install its dependencies, start its backend — with the current one marked; a card or composer action shows the current step on one line. The view opens once the backend is in the pool (the opening wakes it) and shows once it runs. A failed preparation or start shows why, a Try again button and where its log is. A backend already running shows nothing extra: the words come after 300 ms. The ring stops for reduced motion. | A Python plugin's page opened right after install shows the steps, then the view; a backend that exits shows the error and Try again. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Security: views are untrusted content. Nothing they send is run as code in the app. Bridge calls are validated against JSON schemas and rate-limited (100 calls/s per view). | Review; a fuzzed bridge call is rejected. |
| NFR-2 | A plugin view opens within 300 ms of its first paint on the developer machine. The bridge adds less than 10 ms per call. A view keeps up with its slot while the page scrolls or resizes, with no visible lag at 60 fps. | Measured. |
| NFR-3 | A plugin's views look at home: theme tokens and the DESIGN.md fonts are available to them, and the app's chrome around them (tabs, headers, popovers) is the app's own. | Review. |
| NFR-4 | Works on Windows and macOS. | Both. |

### 4.3 Permissions

| Permission | Allows | Shown to the person as |
| --- | --- | --- |
| `chat:read` | `chat.active`, `chat.list`, `chat.messages`, `chat.onEvent`, for every chat | Read all your chats |
| `chat:send` | `chat.send`, in any chat | Send messages in your chats |
| `tools:call` | `tools.call` for app and connector tools (approvals still apply) | Use evo's app and connector tools, with your approval |
| `composer:write` | `composer.insert` | Put text in the message box |
| `storage` | `storage.*` | Keep its own data |
| `network:<origin>` | Fetch from that origin (https only) | Connect to `<origin>` |

`ui.*`, `theme.*`, `view.*` need no permission.

## 5. Success criteria

- SC-1 A plugin made of one `plugin.json` and one HTML file shows a sidebar
  page after install, with no restart.
- SC-2 No view can reach the app's other IPC, the file system or another
  plugin's data.
- SC-3 Every slot works in light and dark, and inside split view.

## 6. Clarifications

### 6.1 Open

n/a — all answered.

### 6.2 Resolved

- 2026-11-13 — Manifest: extend `.evo-plugin/plugin.json` (FR-1).
- 2026-11-13 — The first version covers all four slots: sidebar page, side
  panel tool, chat card and composer action (FR-4 – FR-7).
- 2026-11-13 — Permissions are approved once at install, and asked again
  when they grow (FR-3).
- 2026-11-13 — Q1: how is a view hosted? → A: a native child webview per
  view, like browser tabs. It is served by an `evo-plugin://` scheme, and
  app overlays hide it while open (FR-9; edge cases).
- 2026-11-13 — Q2: remote scripts? → A: no. Code ships in the plugin, and
  fetches need `network:<origin>` (FR-9).
- 2026-11-13 — Q3/Q4: reach → A: every chat, under `chat:read` and
  `chat:send`. Chat cards come from the agent's `plugin_show_card`, and
  tool-result renderers wait for plugin backends (FR-7, FR-10).
- 2026-11-13 — Q5: dev reload → A: an explicit Developer mode switch per
  plugin (FR-14).
- 2026-11-13 — Later features: Bun and uv ship with evo for plugin
  backends, and the plugin creator is a built-in skill with
  `/create-plugin`.

## 7. History

- 2026-11-13 — Draft, from the plugin research and the person's answers.
- 2026-11-13 — Q1–Q5 answered: native webviews, bundled code, every chat,
  and a Developer mode switch. Accepted (Gate A).
- 2026-11-13 — FR-11: `tools.call` reaches app and connector tools. The
  engine's own tools need a new engine command and come with plugin
  backends.
- 2026-11-15 — FR-17: a view waits for its plugin's backend, showing the
  steps, instead of opening before the backend runs.
