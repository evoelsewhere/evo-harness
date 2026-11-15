# Plugin UI: plugins with their own views in evo — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-14 |

## 1. Phases

### Phase 0 — Spikes

Done when: ADR 0021 records how plugin storage is kept apart on Windows and
that `plugin-*` webviews reach only `plugin_bridge`.

- [x] T1 [FR-9, FR-13] Spike: two `evo-plugin://` views on Windows. Their URL and origin, whether `localStorage` is shared, and what keeps it apart (per-host URLs, `use_https_scheme`, per-plugin data folders) — `apps/desktop/src-tauri/src/plugin_ui/` (spike code) — check: CDP in the dev app
- [x] T2 [FR-9, NFR-1] Spike: what IPC a `plugin-*` view reaches (none: the ACL refuses every command), and a bridge through the scheme (`POST /__bridge` knows the caller's label) — `apps/desktop/src-tauri/src/lib.rs`, `plugin_ui/` — check: CDP in the dev app

### Phase 1 — Manifest, consent and storage

Done when: a plugin's `ui` and `permissions` are read, validated, approved at install and revocable.

- [x] T3 [FR-1, FR-2] Tests: the `ui` slots, paths, ids, permissions and `network:` origins (valid and invalid) — `crates/evo-plugins/src/ui.rs` tests — check: `cargo test -p evo-plugins` fails before T4
- [x] T4 [FR-1, FR-2] Parse and validate `ui` and `permissions` — `crates/evo-plugins/src/{manifest.rs,ui.rs}` — check: T3 passes
- [x] T5 [FR-3, FR-14, FR-16] `approvedPermissions`, `developerMode` and `pending` in `installed.json`. Install, enable and update keep consent — `crates/evo-plugins/src/store.rs`, `apps/desktop/src-tauri/src/plugins.rs` — check: tests; old files still read
- [x] T6 [FR-3, FR-16] The consent dialog, the views and permissions list, revoke, the Developer mode switch — `apps/desktop/src/features/customize/plugins-section.tsx`, `src/lib/plugins.ts` — check: running app

### Phase 2 — Hosting and the bridge

Done when: a view opens in a slot, follows it, and calls the bridge under its permissions.

- [x] T7 [FR-9] The `evo-plugin://` scheme: files from the plugin folder only, CSP, MIME — `apps/desktop/src-tauri/src/plugin_ui/scheme.rs` — check: tests for path escapes
- [x] T8 [FR-4 – FR-7, FR-15] Plugin views: open, bounds, visible, reload, close; card cap — `plugin_ui/views.rs` — check: running app
- [x] T9 [FR-10 – FR-13, NFR-1] The bridge behind `POST /__bridge`: label and host key → plugin, schema, permission, rate limit; `chat.*`, `tools.call`, `ui.*`, `composer.insert`, `theme.*`, `storage.*`, `view.*`; event fan-out — `plugin_ui/bridge.rs`, `supervisor.rs` — check: unit tests for the checks; running app
- [x] T10 [FR-10] The injected `window.evo` client and theme variables — `plugin_ui/client.js` — check: running app

### Phase 3 — Slots

Done when: all four slots show enabled plugins' views.

- [x] T11 [FR-4] Sidebar pages from plugins; `pluginPage` in the chat area — `src/stores/plugin-ui-store.ts`, `features/shell/sidebar-items.ts`, `app/app.tsx`, `stores/ui-store.ts` — check: running app
- [x] T12 [FR-5] Panels from plugins: the tool rail, the side panel, the palette and `app_panel` — `features/panels/*`, `stores/ui-store.ts`, `src-tauri/src/agent_tools/app/panel.rs` — check: running app
- [x] T13 [FR-6] Composer actions — `features/chat/chat-composer.tsx` — check: running app
- [x] T14 [FR-7, FR-8] `plugin_show_card` and `plugin_open`; the card renderer — `src-tauri/src/agent_tools/app/plugin.rs`, `components/ai-elements/tool.tsx` — check: running app
- [x] T15 [FR-14] Developer mode watch and reload — `plugin_ui/watch.rs` — check: running app

### Phase 4 — Verify

- [x] T16 A sample plugin covering all four slots, and a check of every requirement — `examples/plugin-ui-sample/` — check: section 2
- [x] T17 UI map, ADR 0021 Accepted, ADD trust boundaries — `apps/desktop/README.md`, `docs/architecture/` — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 – FR-2 | `cargo test -p evo-plugins` (ui: 6, store: 2 new); running app: the sample plugin's sidebar page and panel listed, and an unknown slot reported | passed (2026-11-14) |
| FR-3 | `cargo test -p evo-plugins` (consent, pending update); running app: Add from folder leaves it off with three permissions to approve; turning it on without them is refused; Review → Allow and turn on enables it | passed (2026-11-14); the update-with-more-permissions path by test only |
| FR-4 – FR-8 | Running app with the sample plugin | passed on Windows (2026-11-14): FR-4 (sidebar row with its icon, the page in the chat area), FR-5 (rail button after the divider, the panel, hidden while a popover covers it), FR-6 (composer button, popover, `composer.insert` into the message box); FR-7 with a real model: `plugin_show_card` drew the board card in the transcript with its data, kept in sight when the turn folds, sized to its content, and again after the window reloaded; FR-8: `plugin_open` from the agent opened the wall page. At most 6 live cards per window and card data over 64 KB refused (commands); webview tests (`plugin-ui-store.test.ts`) and shell tests (`agent_tools::app::plugin`) |
| FR-9 | Spikes T1, T2 (Windows): per-plugin origins keep storage apart; views reach no IPC command; cross-plugin and external fetches fail. Tests: path escapes, links out, navigation. Running app: no file outside the folder | passed on Windows (2026-11-14): `bun run test:shell plugin_ui` (scheme 4, views 1); running app: IPC refused, `%2e%2e` and `%5c` escapes 404/400, another plugin's origin and example.com blocked, inline script blocked by the CSP. Slots following their area come with Phase 3 |
| FR-10 – FR-13 | Unit tests and the running app | passed on Windows (2026-11-14): bridge tests (permissions, call shape, rate, storage, events); running app: `window.evo` present and frozen; with a real model the panel followed a streaming reply (`chat.onEvent`) and kept the last reply as a note (`chat.messages`, 106 ms); `tools.call` in Ask mode showed "sticky-wall (plugin): Allow browser_download_folder?" and Deny came back to the view as an error (FR-11); storage kept in `storage.json`; calls without their permission refused; the theme followed light and dark |
| FR-14 | Running app: the Developer mode switch is saved; editing a file reloads the open view | passed (2026-11-14): `watch` test; running app: an edit to `notes.js` reloaded the panel within a second |
| FR-15 – FR-16 | Running app: Take back moves a permission to Not allowed; a call needing it fails at once; turning the plugin off closes its views and removes its slots | passed (2026-11-14): off removed the sidebar row, rail button and composer button and closed every view; the open panel tab says it is not available. A card of an old chat with the plugin off shows a line instead (code, not run in the app) |
| NFR-1 | Review; a fuzzed bridge call is rejected | passed (2026-11-14): 400 random bridge calls from a view (bad JSON, wrong shapes, unknown calls, huge values) were each answered with a code or a valid result, and the app kept working; a burst of 150 calls: 106 answered, 44 `rate_limited`; nothing a view sends is run in the app (review of `bridge.rs`) |
| NFR-2 | Measured on the developer machine | partly (2026-11-14): a bridge call takes 3.5 ms (median of 20; p90 4.1 ms); scrolling the transcript with a card holds 60 fps (p95 frame 16.9 ms); a page reopened within 5 minutes shows in about 120 ms from the click. A first open takes about 600 ms from the click (the page itself loads in about 100 ms; WebView2 creating the view takes the rest): over the 300 ms target |
| NFR-3 | Review | passed (2026-11-14): theme tokens and fonts reach views as `--evo-*` (the sample uses them in light and dark); headers, tabs, popovers and placeholders around views are the app's own (screenshots of the sample) |
| NFR-4 | Windows and macOS | partly: Windows passed (2026-11-14); macOS not run |
