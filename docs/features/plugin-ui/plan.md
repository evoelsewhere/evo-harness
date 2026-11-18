# Plugin UI: plugins with their own views in Evo — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Status | In progress |
| Updated | 2026-11-14 |

## 1. Context and constraints

- **Plugin bundles** (`crates/evo-plugins`):
  - `manifest.rs` reads `.evo-plugin/plugin.json`: `name`, `version`,
    `userConfig`, and paths for skills, commands, agents, hooks and MCP
    servers. Paths that leave the folder are rejected.
  - `store.rs` keeps `<agent dir>/plugins/installed.json`
    (`{id, name, marketplace, version, path, enabled, installedAt, config}`)
    and a `data/<id>/` folder per plugin.
  - The desktop manages bundles in `src-tauri/src/plugins.rs`. After every
    change, `changed()` emits `plugins://changed` and has the supervisor
    reload resources. The UI is `features/customize/plugins-section.tsx`
    with `lib/plugins.ts`.
- **Native child webviews already exist for browser tabs**
  (`src-tauri/src/browser.rs`: `WebviewBuilder` and `window.add_child`,
  labels `browser-N`).
  - The UI keeps each webview over a placeholder. `PageArea` in
    `features/browser/browser-panel.tsx` measures the placeholder's rect
    (times the app zoom) and sends it with `browser_bounds`.
  - The webview is hidden (`browser_visible`) while an app overlay covers
    it. The `OVERLAYS` selector covers menus, popovers, selects, hover
    cards and dialogs.
- **Fixed lists:**
  - sidebar pages: `SIDEBAR_ITEMS` (`features/shell/sidebar-items.ts`),
    each with a boolean and an opener in `ui-store`, and the page is picked
    by a ternary in `app/app.tsx`;
  - side panel tools: `panels` (`features/panels/panel-registry.ts`), the
    closed `PanelId` union in `ui-store.ts`, and an if-chain in
    `right-panel.tsx`;
  - the agent's `app_panel` tool keeps its own `PANELS` list.
- **IPC:**
  - `capabilities/default.json` grants core and plugin permissions to the
    `main` and `chat-*` windows only.
  - App commands from `generate_handler!` are not restricted by
    capability, so any webview with IPC injected could call them.
  - There is no custom URI scheme. The CSP allows `frame-src 'self' data:`
    only, which does not matter for native webviews.
- **App tools** run in the shell (ADR 0013, ADR 0014):
  - the static `registry()` plus the `Provider` trait (MCP connectors) in
    `agent_tools/mod.rs`;
  - approvals through `ask_person`;
  - the class gate (`Read/Edit/Command/Control`) and read-only roles.
- **Engines:** the supervisor (`supervisor.rs`) hosts every chat's engine.
  It sees every engine line (`on_line`) and can send commands
  (`supervisor/rpc.rs`); the Scheduler already starts and prompts chats
  through it.
- **Platforms:** on Windows, WebView2 serves a custom scheme `x://host/p`
  at `http://x.localhost/…` (with Tauri's default `use_https_scheme:
  false`), so every plugin would share one origin. WKWebView keeps
  `x://host/` as the origin. Storage isolation between plugins therefore
  needs checking (T1).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | Views are native UI hosted by the shell. The bridge relays to engines and app tools; no agent decision moves to a plugin. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | `evo-plugins` only parses and stores `ui` and `permissions`. Hosting lives in the shell. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | No engine protocol change. The bridge uses commands the supervisor already sends (`get_messages`, `prompt`). |
| Models come from models.dev; no hardcoded model facts | n/a | |
| Real engine in the app; settings and credentials reload live | ok | Enabling, disabling or approving a plugin applies at once (`plugins://changed`). |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | deviation → new ADR | A new trust boundary (plugin views) and a new URI scheme. ADR 0021 records the hosting, the label-checked bridge and the permission model. Tool calls keep approval modes and the class gate; project plugins need project trust. |
| Desktop UI structure and `DESIGN.md` | ok | Registries move from fixed lists to stores. The chrome around views stays the app's own. |

## 3. Design

### 3.1 Overview

```text
 plugin folder ──(evo-plugin:// scheme)──▶ native child webview "plugin-<n>"
   .evo-plugin/plugin.json                    │  init script: window.evo (bridge client), theme vars
   ui/*.html, icons                           │
                                              ▼  fetch POST /__bridge (its own origin; no IPC) → scheme handler
 webview (React) ── slot placeholder ──▶ shell: plugin_views.rs (create/bounds/visible/reload)
   PluginSlot measures its rect,            shell: plugin_ui/bridge.rs (behind the scheme handler)
   overlays hide it (OVERLAYS)                 label → view → plugin, permission check,
                                                 rate limit, dispatch:
                                                 chat.* → supervisor (DB rows, get_messages, prompt, line fan-out)
                                                 tools.call → agent_tools (registry/Provider, approvals)
                                                 ui.* / composer.* → event to the main window
                                                 storage.* → data/<id>/storage.json
   events to a view: webview.eval("window.__evo_dispatch(…)")
```

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| crate | `crates/evo-plugins/src/manifest.rs`, `ui.rs` (new) | Parse and validate `ui` (four slots, ids, entry and icon paths inside the plugin) and `permissions` (the fixed list, `network:<https origin>`). Problems go to the plugin's `unsupported`/errors list. | FR-1, FR-2 |
| crate | `crates/evo-plugins/src/store.rs` | `InstalledPlugin` gains `approvedPermissions: Vec<String>`, `developerMode: bool`, and `pending: Option<{version, path, permissions}>` for an update that waits for consent. | FR-3, FR-14, FR-16 |
| shell | `src-tauri/src/plugin_ui/scheme.rs` (new) | `evo-plugin://<host-key>/<path>`. The host key comes from the plugin id: a hostname label (`[a-z0-9-]`, ≤ 63 characters, with a short hash), one origin per plugin. It serves files from the plugin folder after canonical-path checks, with CSP and MIME headers. `POST /__bridge` goes to the bridge. | FR-9 |
| shell | `src-tauri/src/plugin_ui/views.rs` (new) | Child webviews `plugin-<n>`: create (with the init script and theme), set bounds, show or hide, reload, close. View key ↔ label ↔ plugin. Live card views are capped. | FR-4 – FR-7, FR-9, FR-15 |
| shell | `src-tauri/src/plugin_ui/bridge.rs` (new) | Bridge calls arrive from the scheme handler with the caller's webview label and host. It checks that the label belongs to a view of that host's plugin, then the call's schema, the permission and the rate, and dispatches. Event fan-out to subscribed views. | FR-10, FR-11, FR-13, NFR-1 |
| shell | `src-tauri/src/plugin_ui/watch.rs` (new) | Developer mode: watch the plugin folder (`notify` crate) and reload its views. | FR-14 |
| shell | `src-tauri/src/lib.rs` | Register the scheme and the commands. Wrap the invoke handler so `plugin-*` webviews reach no command (a second guard behind the ACL). | FR-9 |
| shell | `src-tauri/src/plugins.rs` | Install, enable and update apply `pending` and consent. New commands `plugin_approve`, `plugin_revoke`, `plugin_set_developer_mode`. Views close on disable or uninstall. | FR-3, FR-15, FR-16 |
| shell | `src-tauri/src/agent_tools/app/plugin.rs` (new) | `plugin_show_card` (class Read, ≤ 64 KB of data) and `plugin_open`. `app_panel` lists plugin panels. | FR-7, FR-8 |
| desktop UI | `src/stores/plugin-ui-store.ts` (new) | The enabled plugins' views by slot, from `plugins_state` and refreshed on `plugins://changed`. | FR-4 – FR-6 |
| desktop UI | `src/features/plugin-ui/plugin-slot.tsx` (new) | The placeholder a view fills. It opens the view, tracks its bounds (the `PageArea` pattern) and hides it under overlays. Loading and error states. | FR-4 – FR-7, NFR-2 |
| desktop UI | `features/shell/sidebar-items.ts`, `app/app.tsx`, `ui-store.ts` | Sidebar items are fixed items plus plugin pages. `ui-store.pluginPage` opens one in the chat area. | FR-4 |
| desktop UI | `features/panels/panel-registry.ts`, `right-panel.tsx`, `ui-store.ts` (`PanelId` → `string`), command palette | Panels are fixed panels plus plugin panels. `right-panel` renders a `PluginSlot` for `plugin:` ids. | FR-5 |
| desktop UI | `features/chat/chat-composer.tsx` | Composer actions open a popover holding a `PluginSlot`. `composer.insert` comes back as an event. | FR-6 |
| desktop UI | `components/ai-elements/tool.tsx` (or a renderer next to the session card) | A `plugin_show_card` call renders a `PluginSlot` with its data. A placeholder when the plugin is off. | FR-7, FR-15 |
| desktop UI | `features/customize/plugins-section.tsx`, `lib/plugins.ts` | The consent dialog on install, enable and update; views and permissions per plugin; revoke; the Developer mode switch. | FR-3, FR-16 |

### 3.3 Interfaces

```jsonc
// plugin.json (additions)
"ui": {
  "sidebar":  [{ "id": "wall",  "title": "Sticky Wall", "entry": "ui/wall.html",  "icon": "ui/icon.svg", "defaultShown": false }],
  "panels":   [{ "id": "notes", "title": "Notes",       "entry": "ui/notes.html", "icon": "ui/notes.svg", "shortcut": "K" }],
  "cards":    [{ "id": "chart", "title": "Chart",       "entry": "ui/chart.html" }],
  "composer": [{ "id": "snip",  "title": "Insert snippet", "entry": "ui/snip.html", "icon": "ui/snip.svg" }]
},
"permissions": ["chat:read", "chat:send", "tools:call", "composer:write", "storage", "network:https://api.example.com"]
```

Tauri commands, for the main window only (`plugin-*` views reach no
command: no capability covers them, and the invoke wrapper refuses them):

```ts
plugin_view_open({ plugin, slot, view, key, bounds, theme, data? }): string   // label
plugin_view_bounds({ label, x, y, width, height }); plugin_view_visible({ label, visible });
plugin_view_close({ label }); plugin_view_theme({ tokens, dark });            // broadcast
plugin_approve({ id, permissions }); plugin_revoke({ id, permission }); plugin_set_developer_mode({ id, on });
```

The bridge, the only way a view reaches evo:

```ts
// in the view (injected): window.evo.*(…) →
//   fetch("/__bridge", { method: "POST", body: JSON.stringify({ id, call: "chat.messages", args: { chat } }) })
//   → { ok: true, value } | { ok: false, code, message }
// shell → view events: webview.eval("window.__evo_dispatch(…)") with { kind: "chat.event" | "theme" | "view.data", … }
```

Events to the main window: `plugin-ui://open {plugin, view}`,
`plugin-ui://composer-insert {text}` and `plugin-ui://notify`.

### 3.4 Data and storage

- `installed.json` gains three fields per plugin: `approvedPermissions`,
  `developerMode` and `pending`. Older files read them as empty or off.
- Storage lives in `<agent dir>/plugins/data/<id>/storage.json`:
  - one JSON object, written atomically;
  - up to 10 MB;
  - kept across updates, removed on uninstall (the data folder already
    is).
- Webview profile: plugin webviews share the app's WebView2 profile.
  Each plugin's host key is its own origin, so web storage stays apart
  (spike T1). A per-plugin data folder is not needed: a second WebView2
  environment would also start more processes and lose the debugging port.
- A card's data is stored in the tool call's arguments in the transcript,
  as the session card's is.

### 3.5 Behaviour

- **Open:** a `PluginSlot` mounts and calls `plugin_view_open`. The shell
  creates the webview hidden, loads `evo-plugin://<key>/<entry>`, and
  shows it at the slot's bounds after the first load. The slot shows a
  spinner until then.
- **Follow the slot:** the `PageArea` pattern. A `ResizeObserver` and
  `MutationObserver` measure on animation frames, and bounds are scaled by
  the app zoom. Split view pane motion and the side panel slide move the
  slot, and the view follows. Covered by an overlay, the view is hidden
  and the slot shows a still placeholder (the plugin icon and title).
- **Cards:** at most 6 live card views per window. A card scrolled off by
  more than one screen is closed and opens again when it comes back.
- **Bridge call** (a `POST /__bridge` to the view's own origin, which the
  scheme handler receives with the caller's label):
  1. The label is mapped to its view and plugin; an unknown label, or one
     whose host key does not match, is refused.
  2. The call's schema is checked.
  3. The permission is checked against `approvedPermissions`.
  4. A token bucket limits calls (100/s per view).
  5. The call is dispatched. Errors return `{code, message}`.
- **chat.*:**
  - `list` reads chat rows from the DB;
  - `messages` goes through the supervisor to the chat's engine
    (`get_messages`, starting it if needed), or reads the saved transcript
    when the chat has no engine;
  - `onEvent` subscribes the view to that chat's engine lines (assistant
    text, turn start and end, tool start and end), which the supervisor
    fans out;
  - `send` uses the Scheduler's path: ensure the engine, then prompt.
- **tools.call** builds an `external_tool_request`-like call for the
  shell's registry or MCP provider. The chat is the active one, and the
  approval text names the plugin.
- **Consent:**
  1. `plugin_install` and `plugin_update` return the manifest's views and
     permissions.
  2. When the permissions are not all approved, the plugin is installed
     disabled (or, on update, the new version is kept as `pending`), and
     the UI shows the consent dialog.
  3. `plugin_approve` enables the plugin, or swaps in the pending version.
- **Developer mode:** a `notify` watcher on the plugin path. A change is
  debounced 150 ms, then the plugin's views reload (`webview.reload()`).

### 3.6 Errors and edge cases

- A missing entry, or a load error: the slot shows "<plugin> could not
  load this view" with Reload.
- A plugin is disabled while its views are open: they close (FR-15), and
  old cards show a placeholder.
- The engine is down when `chat.messages` is called: the saved transcript
  is read instead.
- A view that keeps failing (5 reloads in a minute) is not reloaded again
  until it is opened again.

### 3.7 Security and trust

- **IPC:** views get `window.__TAURI_INTERNALS__`, but no capability
  covers `evo-plugin.*` origins. Tauri's ACL refused every app, core and
  plugin command in spike T2. The invoke-handler wrapper also refuses all
  of them from `plugin-*` labels. The bridge needs no IPC: it is the
  scheme's `/__bridge`.
- **The scheme handler:**
  - serves only files under the plugin folder, after canonicalising, and
    refuses symlinks that escape it;
  - sends a CSP of `default-src 'self'; script-src 'self'; style-src
    'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src
    <network origins>; frame-src 'none'`.
- **Navigation:** off-scheme navigation and new windows are refused, and
  `network:` origins must be https.
- **Bridge:** the shell checks every call. The view's claims (plugin id,
  chat) are never trusted beyond the label mapping and the permissions.
- **Tool calls** keep approval modes, read-only roles and the class gate.
  The engine's own tools are not reachable (FR-11).
- **Trust:** project-scoped plugins need project trust, like their hooks
  and MCP servers.

### 3.8 Configuration

- Per plugin, in `installed.json`: `approvedPermissions`,
  `developerMode`.
- No app settings.

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| A sandboxed iframe per view (the `show_widget` way) | Proposed first. The person chose native webviews for stronger isolation (spec Q1). |
| Restrict app commands through Tauri's app manifest (`build.rs`) | It would mean listing every command (around 200) and every capability. One wrapper on the invoke handler gives the same "only `plugin_bridge`" guarantee. |
| `postMessage` between a view and the main webview | Native webviews are separate documents with no shared parent. IPC to the shell is the only channel. |
| A `plugin_bridge` IPC command, allowed for `plugin-*` | It needs a capability for remote-like origins and an app manifest in `build.rs` to name the command, and it opens IPC to plugin views. The scheme bridge (chosen after spike T2) needs neither. |
| One WebView2 data folder per plugin | Not needed, because per-key origins already keep storage apart (spike T1). It would also start a WebView2 environment per plugin. |

## 5. Risks and spikes

- **T1, done 2026-11-13: origins on Windows.** `evo-plugin://<key>/` is
  served at `http://evo-plugin.<key>/`, one origin per key. Storage set by
  one key was invisible to another key and to the app. Keys must be valid
  hostname labels.
- **T2, done 2026-11-13: IPC.** Scheme views get `__TAURI_INTERNALS__`, but
  the ACL refuses every command from them. A `POST /__bridge` reaches the
  handler with the caller's label and host. Fetches to another plugin's
  origin or to an external site fail.
- **Open:** the same checks on macOS (WKWebView), as part of NFR-4.
- **Many webviews:** a page with several cards costs memory. Hence the
  cap of 6 live cards and the unloading of off-screen cards.

## 6. As-built notes

Phase 2 (hosting and the bridge), 2026-11-14:

- **Modules.** `plugin_ui/mod.rs` keeps the enabled plugins with views by
  host key (`Entry`: root, data folder, views, approved permissions) and
  the open views (`views::Live`). `plugins.rs`'s `changed()` re-reads them
  (`plugins_changed`), so install, approve, revoke, disable and update all
  reach the views at once.
- **Scheme.** The handler is async (its own task). A view loads only its
  own plugin's files; app windows load nothing from it (Phase 3 below). The CSP also sets `font-src`, `media-src`,
  `object-src 'none'`, `base-uri 'none'` and `form-action 'none'`, and
  `connect-src` starts with `'self'` (the bridge). Inline scripts do not
  run (`script-src 'self'`): a plugin ships its scripts as files.
- **Views.** Commands as in 3.3, plus `plugin_view_reload`;
  `plugin_view_theme` takes `{tokens, dark}` and only from `main`. A view
  whose window is not the caller's is refused. Views get
  `disable_drag_drop_handler` (HTML drag and drop works inside them) and
  DevTools in debug builds or in Developer mode. Events to the owner
  window: `plugin-ui://loaded`, `plugin-ui://closed {reason: off | cap}`,
  `plugin-ui://resize`, `plugin-ui://open`, `plugin-ui://notify`,
  `plugin-ui://composer-insert`. The card cap closes the oldest card of
  the window (`reason: cap`).
- **Bridge.** `chat.onEvent` is `chat.subscribe`/`chat.unsubscribe` on the
  wire; `""` follows the window's open chat. Events are plain
  (`turn_start`, `turn_end`, `text_delta`, `message`, `tool_start`,
  `tool_end`), and messages are `{role, text, at, tools?, tool?,
  isError?}`. Incognito chats are invisible to plugins.
  `chat.messages` on a chat with no engine starts one (it reopens the
  saved session, new reason `plugin`) instead of reading the transcript
  file: the shell has no transcript reader; the idle limit stops it again.
- **tools.call** runs through `agent_tools::call_for_plugin`: hidden and
  unoffered tools are refused, read-only chats keep their rule, approvals
  read "<plugin> (plugin): …", and "Allow for this chat" given to a plugin
  is kept apart from the agent's.
- **Client.** `window.evo` is frozen; theme tokens become `--evo-<name>`
  on the root element with `data-evo-theme`. Card data and height are
  `evo.view.data()` and `evo.view.resize()`.

Phase 3 (slots), 2026-11-14:

- **Icons** reach the app's windows as data URLs in `plugins_state`
  (`icons` by path), so the scheme serves nothing to app windows at all
  (the main window's CSP does not cover `evo-plugin` hosts).
- **Hosting.** The view commands are async (off the main thread), and views
  are created one at a time (`creating` lock): a sync command arriving on
  the main thread while it was creating a view (a slot mounted twice under
  React's StrictMode) hung every IPC call.
- **Slots.** `PluginSlot` hides its view unless the slot is wholly inside
  its scroll areas and uncovered (menus, popovers, dialogs, tooltips);
  a card more than a screen away closes its view and opens it again when it
  comes back. `plugin-ui://closed {reason: cap}` shows "Show again".
- **Panels** are `plugin:<id>/<view>` tab ids (`PanelId` is a template
  type); saved tabs of a plugin that is off stay and say so. A plugin
  shortcut a built-in tool has is dropped. The macOS menu bar lists only
  the built-in tools.
- **Agent tools.** `plugin_show_card` (class Read) and `plugin_open` (class
  Edit, never asks) are offered only while an enabled plugin has cards, or
  pages and panels; the tools are set up again when that changes. A plugin
  is named by id or by a unique name. `app_panel` keeps `plugin:` tabs in
  its status but does not open them: `plugin_open` does.
- **Developer mode** polls the plugin folder every 500 ms while it has an
  open view (`watch.rs`: names, sizes and times, `node_modules`, `.git` and
  `target` skipped) instead of a `notify` watcher: no new dependency.

Phase 4 (verify), 2026-11-14:

- **Sample:** `examples/plugin-ui-sample/` (Sticky Wall) uses all four slots
  and documents `window.evo`.
- **Cards** stand on their own in a turn, like `show_widget`, so they stay
  in sight when the turn folds; a card is 36rem wide at most (the trace
  around tool calls fits its children to their content).
- **Card data** a model sends as a JSON string is read as JSON
  (`card_data`); the schema now says `data` is an object.
- **Kept views.** A sidebar page or panel the person leaves stays open,
  hidden, for 5 minutes (`keepAlive` in `PluginSlot`): coming back takes
  about 120 ms instead of about 600 ms and keeps the page's state.
- **Window reload.** A window that starts closes the views an earlier page
  of it left (`plugin_view_reset`) before it lists its own: a reload keeps
  native webviews, which would otherwise sit over the new page.
- **Open:** NFR-2's 300 ms for a first open is not met on Windows
  (WebView2 creation); NFR-4 on macOS is not checked.
