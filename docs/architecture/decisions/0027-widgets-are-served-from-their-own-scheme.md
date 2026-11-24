# 0027 — `show_widget` documents are served from their own scheme with their own CSP

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-25 |
| ADD sections | [9. Cross-cutting concepts](../README.md#9-cross-cutting-concepts) |
| Related | [Office v3 parity](../../features/office-v3-parity/spec.md) FR-13, NFR-2, NFR-3; [ADR 0021](0021-plugin-views-run-in-native-webviews.md) |

## Context

`show_widget` shows model-written HTML in the chat. The widget frame loaded it
as a `data:` URL in an `<iframe sandbox="allow-scripts">`, with the widget
CSP as a `<meta>` tag, on the belief that a `data:` document starts with a
fresh policy.

It does not. A `data:`, `blob:` or `srcdoc` document is a local-scheme
document: it inherits the policy of the page that created it, and the meta
policy only adds to it. The app's policy (`tauri.conf.json`) has no
`script-src`, and Tauri adds `'self'` and a hash of each of the app's own
scripts to it at build time. Checked on 2026-11-25 in the release app
(4.13.2, WebView2): no script ran in a `data:`, `data:` with its own CSP,
`srcdoc` or `blob:` frame. So in every release build no widget script has
run: no auto height, no `sendPrompt`, no theme updates, no Chart.js. The
debug build loads the Vite dev server, which sends no CSP, so it worked
there.

Adding `'unsafe-inline' 'unsafe-eval'` to the app's `script-src` does not
help: with hashes in the directive, CSP ignores `'unsafe-inline'`. Checked
with real headers in Chromium 148 and Edge 158: still blocked. It works only
once Tauri stops adding its hashes (`dangerousDisableAssetCspModification`),
and then inline script runs in the app itself: one injection anywhere in the
UI would reach the IPC commands (files, shell, engines).

A document fetched from another origin gets the policy of its own response
instead. Plugin views already use this (`evo-plugin://`, ADR 0021), and in
the same check a frame from another origin with the widget policy ran its
script while the app's own inline script stayed blocked.

## Decision

The shell serves widget documents from a scheme of its own, `evo-widget`
(`http://evo-widget.localhost/<id>` on Windows, `evo-widget://localhost/<id>`
elsewhere), with the widget CSP as the response header.

- The window gives the shell the wrapped document (`widget_open`) and gets an
  id; the frame loads `evo-widget` with that id. `widget_close` drops it.
  Documents live in memory only, capped in number and size, oldest dropped
  first.
- A document is served only to the window that opened it; plugin views get
  nothing.
- The frame keeps `sandbox="allow-scripts"` without `allow-same-origin`: the
  document's origin is opaque, so it reaches neither the app's storage nor
  the scheme's other documents, and no capability covers it.
- The app's CSP changes only in `frame-src`, which names the scheme. Its
  `script-src` stays as Tauri builds it.
- Outside Tauri (the Vite preview, tests) the frame still uses a `data:` URL;
  there is no app policy to inherit.

## Consequences

- Widgets work in release builds as they did in development: scripts, the
  height report, `sendPrompt`, theme updates, the CDN allowlist.
- The widget policy now lives in two places that must agree: the header
  (`widgets.rs`, which decides) and the `<meta>` tag (`widget-document.ts`,
  kept for the `data:` fallback). Tests pin both.
- A widget's HTML crosses IPC once (`widget_open`) and is kept by the shell
  while its frame shows it; replaying a long chat opens one document per
  widget.
- Any later feature that shows untrusted HTML with script follows this rule:
  a document of its own origin and policy, never a local-scheme frame inside
  the app's page.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Add `'unsafe-inline' 'unsafe-eval'` and the CDNs to the app's `script-src` | Ignored while Tauri adds hashes; works only by turning Tauri's hashes off, which lets inline script run in the app itself |
| `srcdoc` or a `blob:` URL instead of `data:` | Local schemes too: they inherit the app's policy (checked, blocked) |
| A native child webview per widget, as plugin views | A widget sits in the scrolling transcript, many per chat; child webviews are positioned over the page by the shell and suit fixed panels, not inline content |
| Hash each widget's scripts into the app policy | The policy is built with the app; widgets are written at run time |
