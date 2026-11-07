# 0012. A native messaging bridge to the person's own browser

| | |
| --- | --- |
| Status | Proposed |
| Date | 2026-11-07 |
| ADD sections | [4. Process view](../README.md#4-process-view), [8. Interfaces](../README.md#8-interfaces), [9. Cross-cutting concepts](../README.md#9-cross-cutting-concepts) |
| Related | [Browser bridge spec](../../features/browser-bridge/spec.md), [Agent browser spec](../../features/agent-browser/spec.md) |

## Context

Agents need to work in pages where the person is already signed in. The
built-in browser cannot offer that. A bridge to the person's own Chrome or Edge
is a new trust boundary: whatever can talk to it can read and act in pages
that hold the person's sessions. It must not be reachable from web pages or
other local programs, must not open a port, and must work without restarting
the browser.

## Decision

An extension (Manifest V3) attaches the browser's debugging API to tabs the
person shares and relays DevTools commands and events. It talks to a native
messaging host (`evo-bridge-host`, a new small binary) that the browser starts;
the host talks to the app over a local pipe or Unix socket guarded by a token
file readable by the person only. The app drives a shared tab through the same
page driver as the built-in browser; tab ownership, approvals and the
untrusted-content rules are the app's.

## Consequences

- No listener on the network; the browser only lets the extension (named in the
  host manifest's `allowed_origins`) start the host.
- A host registration per browser (registry keys on Windows, manifest files on
  macOS and Linux) is part of setup, removable from Settings.
- The browser shows its own notice that a tab is being debugged: sharing can
  never be silent.
- A new crate and a new extension folder; the ADD's process view and interfaces
  list them; the shell gains a channel server.
- The page driver must be transport-neutral (webview commands or the bridge).
- The extension is loaded unpacked until it is published.

## Alternatives considered

| Option | Why not |
| --- | --- |
| A local WebSocket the extension connects to, guarded by a token | A port is open, and any web page can try to connect to localhost; the check rests on a token and an `Origin` header only. |
| The browser's remote debugging port | Needs a browser restart with a flag, exposes every tab and profile, and cannot be limited to shared tabs. |
| Launching a separate automated browser profile | No signed-in sessions, which is the point; still a possible later "managed profile" option. |
| Content scripts injected into every page | Broad host permissions and a larger attack surface than per-tab debugging the person grants. |
