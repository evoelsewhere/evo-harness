# Browser bridge: let an agent work in the person's own Chrome or Edge — Spec

| | |
| --- | --- |
| Status | Draft |
| Area | desktop shell, desktop UI, browser extension, new crate `evo-bridge-host` |
| Created | 2026-11-07 |
| Updated | 2026-11-07 |
| Plan | [plan.md](plan.md) |
| Tasks | not yet (after Gate B) |
| Related | [Agent browser](../agent-browser/spec.md), [Approval modes](../approval-modes/spec.md), [ADR 0012](../../architecture/decisions/0012-native-messaging-bridge-to-the-persons-browser.md) |

## 1. Summary

The built-in browser has no logins, extensions or history of the person's own.
Many tasks need them: an admin console, a dashboard behind single sign-on, a
form on a site the person is already signed in to. This feature adds a small
extension for Chrome and Edge, a native messaging host and a local channel to
the app, so an agent can work in a tab the person chose to share, with the same
`browser_*` tools it uses in the built-in browser.

## 2. Scope

### 2.1 Goals

- An extension for Chrome and Edge, installed from the app's settings, that
  shares chosen tabs with Evo and nothing else.
- The agent uses the same browser tools on a shared tab (`target: "chrome"` or
  `"edge"`).
- The person decides what is shared, can see that a tab is shared, and can stop
  it at any time from either side.
- No open network port; only the extension can reach the app through the bridge.

### 2.2 Non-goals

- Reading or driving tabs the person did not share, or the person's browsing
  history, bookmarks, passwords or saved forms.
- Firefox and Safari (a later feature; they lack the same debugging API).
- Remote machines: the bridge is local only.
- Publishing to a browser store in this feature (the extension loads unpacked
  first).

## 3. User scenarios

### US-1 Connect a browser (priority P1)

Acceptance scenarios:

1. **Given** Settings → Browser bridge, **when** the person chooses "Set up
   Chrome", **then** the app registers the native host for that browser and
   shows the folder of the extension to load and the steps to do it.
2. **Given** the extension loaded, **when** the browser starts or the extension
   is enabled, **then** Settings shows "Chrome connected".

### US-2 Share a tab and let the agent work in it (priority P1)

Acceptance scenarios:

1. **Given** a chat whose agent calls `browser_tabs {target:"chrome"}`, **when**
   nothing is shared, **then** the agent is told no tab is shared and the person
   gets a prompt in the chat: "Share a Chrome tab with this chat?" with the
   open tabs (title and site) to choose from.
2. **Given** the person picks a tab, **then** the browser shows its debugging
   notice on that tab, the tab is listed to that chat's agent only, and the
   agent can read, click and type in it.
3. **Given** a shared tab, **when** the person stops sharing in the app or in
   the extension, or closes the tab, **then** the agent's next call says the tab
   is no longer shared.

### US-3 Stay safe (priority P1)

Acceptance scenarios:

1. **Given** "Ask every time" or "Approve edits", **when** the agent clicks,
   types or changes something in a shared tab, **then** it asks first (once per
   site and chat for clicks and typing, every time for uploads, cookie writes
   and request mocking).
2. **Given** a page that tells the agent to do something, **then** the page is
   data: nothing in it changes what the agent was asked.

### Edge cases

- Two browsers connected (Chrome and Edge): tabs show with their browser's
  name; `target` picks one.
- The browser closes or the extension reloads mid-call: the call returns and
  says the browser went away.
- The app is not running: the host tells the extension, which shows "Evo is not
  running" and shares nothing.
- Another debugger (DevTools, another extension) is attached to the tab: the
  share fails with the browser's message, shown to the person.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | An extension (Manifest V3) for Chrome and Edge with only the permissions `debugger`, `tabs` (titles and URLs of tabs, for the picker) and `nativeMessaging`; no content scripts and no host permissions. | Review of `manifest.json`. |
| FR-2 | A native messaging host `evo-bridge-host` (a small binary shipped with the app) relays messages between the extension and the app over a local channel (named pipe on Windows, Unix socket with mode 0600 elsewhere). | Run the host by hand: it connects to the app's channel. |
| FR-3 | Settings → Browser bridge registers the host for Chrome and Edge (Windows: current-user registry keys; macOS and Linux: per-user manifest files), shows the extension folder and whether each browser is connected, and can remove the registration. | Set up Chrome, restart it: "Chrome connected". Remove: the host is gone. |
| FR-4 | The extension's `allowed_origins` names only its own id; the host accepts a connection only from it; the app accepts the host only with a token kept in a file readable by the person only. | A web page cannot connect; a wrong token is refused. |
| FR-5 | Nothing is shared until the person picks a tab: from a prompt in the chat (opened by an agent's call) or from the extension's popup ("Share this tab with Evo", choosing the chat). | With nothing shared, an agent call lists no tabs and prompts. |
| FR-6 | A shared tab belongs to one chat (as built-in pages do): only that chat's agent sees it; the app lists it under the chat's Browser tool, marked as a shared Chrome or Edge tab. | Chat B cannot name chat A's shared tab. |
| FR-7 | The browser tools run on a shared tab through the same page driver as the built-in browser, over DevTools commands relayed by the extension; `target` selects the browser; tab ids read `chrome:<id>` or `edge:<id>`. | `browser_read_page`, `browser_click`, `browser_screenshot` work on a shared tab. |
| FR-8 | Stopping a share, closing the tab, disabling the extension or quitting the browser ends the share and every pending call. | Close the tab during a wait: the call returns "tab closed". |
| FR-9 | The approval rules of the [agent browser](../agent-browser/spec.md) FR-22 apply, and for a shared tab acts always ask in "Ask every time" and "Approve edits" (once per site and chat). | In "Ask every time", the first click on a site asks; the second does not. |
| FR-10 | Navigation of a shared tab and new tabs it opens stay in the share only for the tab itself; other tabs the page opens are not shared. | A link with `target=_blank` opens an unshared tab. |
| FR-11 | The app shows which tabs are shared and offers "Stop sharing" for each, and "Stop all"; the extension's popup shows the same. | Stop all: every share ends and the browser's debugging notices go. |
| FR-12 | Every tool result from a shared tab marks page content as untrusted data. | Review of the result wording. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | No port is open on the machine for the bridge. | `netstat` shows no listener added by Evo for it. |
| NFR-2 | A command reaches the page and answers in under 300 ms for a simple call on a local machine. | Time `browser_read_page` on a shared tab. |
| NFR-3 | The bridge adds no work when nothing is connected, and the app starts without it. | Start the app with no host registered. |
| NFR-4 | Windows 10/11 with Chrome and Edge first; macOS and Linux (Chrome, Chromium, Edge) follow in the same code. | Manual check on Windows; macOS later. |

## 5. Success criteria

- SC-1 An agent reads and fills a page in a tab the person is already signed in
  to, after the person shared that tab and nothing else.
- SC-2 Closing the browser, the tab or the share ends the agent's access at once.
- SC-3 No other program or web page can use the bridge.

## 6. Clarifications

### 6.1 Open

- [NEEDS CLARIFICATION: Native messaging (needs a registered host) or a token-
  protected local WebSocket (no registration, but reachable from web pages).
  Proposed: native messaging, as in [ADR 0012](../../architecture/decisions/0012-native-messaging-bridge-to-the-persons-browser.md).]
- [NEEDS CLARIFICATION: Where does the extension's id come from while it loads
  unpacked? Proposed: a fixed `key` in `manifest.json` so the id is stable and
  `allowed_origins` can name it.]
- [NEEDS CLARIFICATION: Should a share outlive an app restart? Proposed: no; the
  person shares again (the browser's debugging notice makes a silent share
  impossible anyway).]
- [NEEDS CLARIFICATION: Publish to the Chrome Web Store and Edge Add-ons?
  Proposed: later, not in this feature.]

### 6.2 Resolved

- 2026-11-07 — Q: Is the bridge wanted? → A: Yes; the person asked for an
  extension bridge to control their own Chrome or Edge, designed and built.

## 7. History

- 2026-11-07 — Draft.
