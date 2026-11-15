# 0021 — Plugin views run in native child webviews with no IPC; their bridge is their own scheme

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-13 |
| Feature | [Plugin UI](../../features/plugin-ui/spec.md) |

## Context

Plugins are to bring their own views: sidebar pages, side panel tools,
chat cards and composer actions, written in HTML with TS or JS. Those views
are third-party code inside the app, which is a new trust boundary. Until
now the app's UI has been only its own code. Its one exception is
`show_widget`, which runs model-written HTML in a sandboxed iframe with a
`postMessage` bridge that can only send a prompt.

Views need much more than that: reading and sending in chats, calling
tools through approvals, storage, theme. They must not be able to reach
the app's ~200 IPC commands or another plugin's data.

The two spikes of 2026-11-13 (plugin-ui tasks T1, T2), run in a debug
build on Windows (WebView2), found:

- **Origins.** `evo-plugin://<key>/<path>` is served at
  `http://evo-plugin.<key>/<path>` (`https://` with `use_https_scheme`).
  Each key is its own origin: `localStorage`, cookies and IndexedDB set by
  one view were not visible to another key's view, nor to the app.
- **IPC.** Child webviews served from the scheme get
  `__TAURI_INTERNALS__`, but no capability covers their origin. Tauri's
  ACL refused every command:
  - app commands such as `engine_running`, `settings_get` and a test
    `plugin_bridge` ("not allowed. Plugin not found");
  - core and plugin commands (`window.minimize`, `event.listen`,
    `opener.open_url`).
- **Bridge.** A `POST` to the view's own origin (`/__bridge`) reaches the
  scheme handler, which knows the calling webview's label and host. A
  fetch to another plugin's origin failed, and so did a fetch to an
  external site (CSP).

## Decision

1. **Hosting.** Each view is a native child webview (`plugin-<n>`) of the
   window. It is laid over a placeholder in the UI and follows it, as
   browser tabs do. The person chose this over sandboxed iframes, for
   isolation.
2. **Serving and isolation.** Files come from the plugin folder through
   the `evo-plugin://` scheme.
   - Each plugin gets one host key, derived from its id as a valid
     hostname label (lowercase `[a-z0-9-]`, ≤ 63 characters, with a short
     hash). That key is its origin, which keeps plugins' web storage
     apart.
   - The handler canonicalises paths, refuses escapes and sets a strict
     CSP (`default-src 'self'`, `connect-src` only for approved
     `network:` origins).
   - No remote scripts are loaded, and all views share the app's WebView2
     profile.
3. **No IPC.** Plugin views get no capability, so Tauri's ACL refuses
   every command from them. A wrapper on the app's invoke handler also
   refuses every command from `plugin-*` labels, as a second guard.
4. **Bridge.** The view's client (`window.evo`) posts JSON to
   `/__bridge` on its own origin. The scheme handler checks the call:
   - it maps the calling webview's label to its view and plugin, and
     refuses a label it does not know or whose host key does not match;
   - it checks the call's schema;
   - it checks the permission the person approved;
   - it applies a rate limit.

   It then dispatches in the shell. Tool calls keep approval modes,
   read-only roles and the tool class gate. Events reach views by
   evaluating a dispatch function in their webview.
5. **Consent.** Permissions come from a fixed list and are approved once
   per install, and again when an update adds any. The person can revoke
   them.

## Consequences

- Plugins cannot see or call app internals. Everything they do goes
  through one audited handler that never trusts what the view says about
  itself.
- No capability, app manifest or `build.rs` change is needed. Adding app
  commands later does not widen what plugins reach.
- Native webviews sit above app content. Overlays hide them while open,
  and many cards cost memory, so live cards are capped.
- Not yet checked on macOS: WKWebView keeps `evo-plugin://<key>` as the
  origin, which keeps the same per-key isolation. That check is part of
  the feature's NFR-4.
- Later steps (plugin backends in four languages, the creator skill) reuse
  this bridge and its permission list.
