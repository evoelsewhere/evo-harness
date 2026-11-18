# 0012. A native messaging bridge to the person's own browser

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-18 |
| ADD sections | [4. Process view](../README.md#4-process-view), [7. Data view](../README.md#7-data-view), [8. Interfaces](../README.md#8-interfaces), [9. Cross-cutting concepts](../README.md#9-cross-cutting-concepts) |
| Related | [Evo Webbridge spec](../../features/browser-bridge/spec.md), [Agent browser spec](../../features/agent-browser/spec.md), [ADR 0013](0013-app-side-agent-tools-run-in-the-rust-shell.md), [ADR 0018](0018-the-shell-hosts-engines-windows-attach.md) |

## Context

Agents need to work in pages where the person is already signed in, and the
person wants to talk to Evo from the browser they are using. The built-in
browser offers neither. Letting Evo work in the person's own Chrome or Edge is
a new trust boundary: whatever can talk to the bridge can read and act in
pages that hold the person's sessions. It must not be reachable from web
pages, other extensions, other users or the network, must not open a port, and
must work without restarting the browser. The browser tools already run in the
shell behind one page driver ([ADR 0013](0013-app-side-agent-tools-run-in-the-rust-shell.md)),
and the shell hosts engines that clients attach to ([ADR 0018](0018-the-shell-hosts-engines-windows-attach.md)).

## Decision

An Evo extension (Manifest V3) for Chrome and Edge is a thin client of the
shell. It reaches the shell through a native messaging host
(`evo-bridge-host`) that the browser starts, and the host reaches the shell
over a per-user named pipe (a 0600 Unix socket elsewhere).

- The extension drives tabs through the browser's debugging API, only tabs in
  the tab groups Evo keeps per chat. It holds no host permissions and declares
  no content scripts.
- The shell owns every decision: which chat owns which tab, the approval gate
  with per-site permissions, takeover and stop. The browser tools reach a
  Webbridge tab through a second implementation of the page driver's tab host.
- The extension's side panel is one more client attached to a shell-hosted
  engine, like a window. It has no model and no agent logic.
- Trust: the browser lets only Evo's extension ids start the host. The host
  only talks to a pipe owned by Evo's process. The shell only accepts the host
  from its install folder and with an allowed extension origin.

## Consequences

- No network listener. The browser shows its own notice while a tab is being
  debugged, so agent work in the browser is never silent.
- Setup registers the host per browser (current-user registry keys on Windows,
  manifest files on macOS and Linux) and can remove it. The host ships next to
  the engine sidecar.
- The page driver gets a tab host abstraction: the WebView2 code becomes one
  implementation, the bridge another. Tools that only make sense for the
  built-in browser refuse Webbridge tabs.
- A new store, `~/.evo/agent/webbridge/` (site permissions and known
  profiles), and a new client of hosted engines. The ADD's process, data,
  interface and trust sections list them.
- A page could hide Evo's in-page indicator. The browser notice and the tab
  group still show, and Evo's page helper runs in an isolated world that page
  scripts cannot call.
- Browser policy can turn the debugging API off. Webbridge then does not work
  in that browser and says so; there is no fallback.
- The extension is loaded unpacked with a fixed id until it is published to
  the Chrome Web Store and Edge Add-ons. Each id goes into the host's
  `allowed_origins` and the shell's allow-list.

## Alternatives considered

| Option | Why not |
| --- | --- |
| A local WebSocket the extension connects to, guarded by a token | A port is open, and any web page can try to connect to localhost; the check rests on a token and an `Origin` header only. |
| The browser's remote debugging port | Needs a browser restart with a flag, exposes every tab and profile, and cannot be limited to Evo's tabs. |
| Launching a separate automated browser profile | No signed-in sessions, which is the point. |
| Content scripts on every site instead of the debugging API | Broad host permissions; no trusted input, file inputs or request interception; Evo's script in every page even when no agent works. |
| Share tabs one by one, the extension only relays (this ADR's first draft) | The agent cannot open the pages it needs, and the person cannot talk to Evo from the browser. |
| The side panel talks to the engine, or to a model, directly | Splits engine ownership from the shell and agent logic from the engine. |
