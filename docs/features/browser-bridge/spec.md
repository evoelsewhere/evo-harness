# Evo Webbridge: Evo in the person's own Chrome or Edge — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | browser extension, desktop shell, desktop UI, new crate `evo-bridge-host` |
| Created | 2026-11-07 |
| Updated | 2026-11-18 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Agent browser](../agent-browser/spec.md), [Approval modes](../approval-modes/spec.md), [Shell-hosted engines](../shell-hosted-engines/spec.md), [ADR 0012](../../architecture/decisions/0012-native-messaging-bridge-to-the-persons-browser.md) |

## 1. Summary

The built-in browser has none of the person's sign-ins, extensions or
history, and many tasks need them: an admin console, a dashboard behind
single sign-on, a form on a site the person already uses. Evo Webbridge is an
extension for Chrome and Edge that brings Evo into the person's own browser:
a side panel to chat with Evo next to the page, and an agent that opens and
works in tabs of its own (kept in a tab group per chat), in the person's
signed-in profile, with the same browser tools it uses in the built-in
browser. The person sees which tabs the agent works in, decides per site
what it may do, and can stop it or take over at any moment.

## 2. Scope

### 2.1 Goals

- One extension for Chrome and Edge, connected to the Evo app on the same
  machine, set up from the app's settings.
- The agent opens and drives tabs in the person's browser profile, in a tab
  group that belongs to one chat, without the person having to share each
  tab first.
- The person can hand an existing tab to a chat and take it back.
- A side panel in the browser to talk to Evo: pick or start a chat, send the
  current tab as context, watch the agent work, answer its questions.
- The side panel's composer does what the app's composer does for a chat:
  choose the model and its thinking effort, switch the approval mode, attach
  files, images and a screenshot of the page, and dictate by voice.
- Permissions per site (ask, allow, block) on top of the chat's approval
  mode; the person can review and change them in the browser and in the app.
- Always visible: the tab group, an in-page indicator with **Stop** and
  **Take over**, and the browser's own debugging notice.
- It works while the app's window is closed: the browser wakes Evo in the
  tray.
- Local only: no network port, no cloud service in between.

### 2.2 Non-goals

- Tabs outside the agent's groups: the agent can list their titles and sites
  when it asks to choose one, never read or drive them.
- The person's history, bookmarks, passwords, saved forms or other
  extensions.
- Running without the Evo app: the extension has no model and no agent of
  its own.
- Firefox and Safari (they lack the same debugging API); a later feature.
- Remote machines.
- Recording the agent's work as a video or GIF; a later feature.
- Downloads managed by Evo: files the agent downloads in the person's
  browser go where that browser puts them.

## 3. User scenarios

### US-1 Connect a browser (priority P1)

The person wants Evo to work in their Chrome or Edge so that the agent can
use the sites they are signed in to.

Acceptance scenarios:

1. **Given** **Settings → Browser**, **when** the person chooses **Set up
   Chrome**, **then** Evo registers itself with that browser and opens the
   extension's install page (or shows how to load it while the extension is
   not published).
2. **Given** the extension installed, **when** the browser starts or the
   extension is enabled, **then** Settings shows "Chrome connected" with the
   profile's name, and the extension shows "Connected to Evo".
3. **Given** the browser's policy forbids the debugging API, **when** the
   extension connects, **then** both sides say that the browser's
   administrator has turned it off, and nothing else is offered.

### US-2 The agent works in its own tabs (priority P1)

The person asks a chat to do something on a site they are signed in to, and
the agent does it in the person's browser.

Acceptance scenarios:

1. **Given** a connected Chrome, **when** the agent opens a page with
   `target: "chrome"`, **then** a tab opens in the background in a tab group
   named after the chat, and the agent can read, click, type and take
   screenshots in it.
2. **Given** a tab in a chat's group, **when** the page opens a new tab,
   **then** the new tab joins the same group and the agent can use it.
3. **Given** two chats working in Chrome, **then** each has its own group and
   neither can name the other's tabs.
4. **Given** a chat's group, **when** the person closes the group or a tab of
   it, **then** the agent's next call on that tab says the tab is gone.

### US-3 Chat from the side panel (priority P1)

The person wants to ask Evo about the page in front of them without leaving
the browser.

Acceptance scenarios:

1. **Given** the side panel open, **when** the person picks a chat (or starts
   a new one) and sends a message, **then** the reply streams in the panel,
   and the same chat shows the same messages in the Evo app.
2. **Given** **Include this tab** on, **when** the person sends a message,
   **then** the current tab joins the chat's group and the agent is told its
   title and address.
3. **Given** the agent asks a question (an approval, a choice), **then** the
   panel shows it and the answer given in either the panel or the app
   settles it in both.
4. **Given** the app's window is closed, **then** the side panel still works.

### US-4 Hand a tab over and take it back (priority P1)

Acceptance scenarios:

1. **Given** a tab the person has open, **when** they drag it into a chat's
   group, or choose **Let Evo use this tab** in the panel or the extension's
   menu, **then** the chat's agent can use it.
2. **Given** a tab in a chat's group, **when** the person drags it out of the
   group or chooses **Take this tab back**, **then** the agent can no longer
   use it and its next call on it says so.
3. **Given** the agent asks to work in a tab the person already has open,
   **then** the person gets a choice of open tabs (title and site) and only
   the one they pick joins the group.

### US-5 Decide per site (priority P1)

Acceptance scenarios:

1. **Given** a site set to **Ask** (the default) and the chat in "Ask every
   time" or "Approve edits", **when** the agent first clicks, types or
   navigates on that site, **then** it asks once for that site and chat, with
   **Allow once**, **Allow for this site** and **Deny**.
2. **Given** a site set to **Allow**, **then** acts on it do not ask, in any
   mode; uploads, storage and cookie changes, request mocking and scripts
   still ask as the approval mode says.
3. **Given** a site set to **Block**, **then** the agent cannot open, read or
   act on it, in every mode including "Approve for me", and is told the
   person blocked it.
4. **Given** a page that tells the agent to do something, **then** the page is
   data: nothing in it changes what the agent was asked.

### US-6 Stop and take over (priority P1)

Acceptance scenarios:

1. **Given** the agent working in a tab, **then** the tab shows an indicator
   with **Stop** and **Take over**, and the browser shows its debugging
   notice.
2. **Given** **Stop** pressed in the page, the panel or the app, **then** the
   chat's turn stops and the agent's pending browser calls end.
3. **Given** **Take over** pressed, **then** the agent's calls on that tab
   wait and report that the person is in control, until the person presses
   **Hand back**; the agent is told when control returns.
4. **Given** **Stop all** in the app or the extension, **then** every chat's
   browser work in that browser ends and the debugging notices go.

### US-7 Several browsers and profiles (priority P2)

Acceptance scenarios:

1. **Given** Chrome and Edge connected, or two Chrome profiles, **then** each
   shows in Settings with its browser and profile name, and the agent picks
   one with `target`; when one profile of a browser is connected, `target:
   "chrome"` means it.
2. **Given** two profiles of one browser connected, **when** the agent uses
   `target: "chrome"` without naming a profile, **then** it is told which
   profiles exist.

### US-8 Compose in the side panel as in the app (priority P2)

The person wants to steer the chat from the browser without opening the app.

Acceptance scenarios:

1. **Given** the side panel open on a chat, **then** its header has the
   chat's title, **History**, **New chat**, a **⋯** menu and **Close**, and
   the composer shows the chat's model, thinking effort and approval mode.
2. **Given** the composer, **when** the person picks another model or effort,
   **then** the chat uses it from its next turn, and the app shows the same.
3. **Given** "Ask every time", **when** the person switches the panel's mode
   control to "Approve for me", **then** the mode changes in the panel and the
   app (it is the app's mode), and **Block** sites stay blocked.
4. **Given** **+**, **when** the person adds a screenshot of the page, a file,
   a pasted image or another open tab, **then** each shows as a chip above the
   field and goes with the next message.
5. **Given** voice input turned on, **when** the person presses the
   microphone and speaks, **then** the words appear in the field to edit, and
   nothing is sent until they send it.
6. **Given** a new or empty chat, **then** the panel shows an empty state with
   the field ready and the current site's permission.

### Edge cases

- The browser closes or the extension reloads during a call: the call
  returns and says the browser went away; the chat's group is gone.
- The app is not running when the extension starts: the extension starts Evo
  in the tray and connects; if that fails it shows "Evo is not running".
- Another debugger (DevTools, another extension) is attached to the tab: the
  agent is told the browser refused, and the person sees why.
- Pages the browser never lets an extension debug (its own settings pages,
  the extension store): the agent is told it cannot use them.
- A page dialog (alert, confirm, leave-page) opens during a call: the call
  returns and names it, as in the built-in browser.
- The person switches to an agent tab and clicks in it while the agent works:
  the agent's next result notes that the page changed; the person can press
  **Take over** to keep the agent out.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | One Manifest V3 extension for Chrome and Edge, with only these permissions: the debugging API, tabs (titles and addresses), tab groups, the side panel, native messaging, its own storage, context menus and the active tab (for a screenshot the person asks for). No content scripts declared and no host permissions. | Review of the manifest. |
| FR-2 | **Settings → Browser** sets up each of Chrome and Edge (registers Evo with the browser for the current user), opens the extension's install page or shows how to load it, shows each connected browser and profile, and can remove the set-up. | Set up Chrome, install the extension: "Chrome connected". Remove: the browser can no longer reach Evo. |
| FR-3 | Only the Webbridge extension can reach Evo, and only the person's own processes on this machine: web pages, other extensions, other users and the network cannot. | A web page and another extension cannot connect; another Windows user cannot; `netstat` shows no listener. |
| FR-4 | Each connected browser profile shows with its browser, a profile name the person can change, the extension's version, and whether the browser allows the debugging API. | Two Chrome profiles show as two entries. |
| FR-5 | The agent's browser tools take `target` (`builtin`, `chrome`, `edge`, or `chrome:<profile>` / `edge:<profile>`); every tool of the [agent browser](../agent-browser/spec.md) works on a Webbridge tab except the download folder and downloads list. Tab ids say which browser they belong to. | `browser_navigate`, `browser_read_page`, `browser_click`, `browser_type`, `browser_screenshot`, `browser_dialog`, `browser_mock` work on a Chrome tab. |
| FR-6 | Each chat has at most one tab group per browser profile, named after the chat. Tabs the agent opens go there, in the background, without taking focus from the person. | The agent opens three pages: one group, three tabs, the person's tab stays active. |
| FR-7 | A tab belongs to a chat while it is in that chat's group. Only that chat's agent sees and uses it. Tabs opened from a group's tab join that group. | Chat B cannot name chat A's tab; a `target=_blank` link opens in A's group. |
| FR-8 | The person hands a tab to a chat by dragging it into the chat's group, from the panel (**Let Evo use this tab**) or the extension's menu; and takes it back by dragging it out, closing it, or **Take this tab back**. | Drag out: the agent's next call on it says the tab is no longer the chat's. |
| FR-9 | `browser_tabs` lists the chat's tabs; with `action: "choose"` it asks the person to pick one of their other open tabs (title and site only) and adds only the picked tab to the group. | With nothing picked, the agent gets the titles and sites only, never page content. |
| FR-10 | The side panel lists the person's recent chats, starts a new chat in a workspace set in Settings, sends messages (optionally with the current tab), streams replies and tool activity, stops a turn, and shows and answers the chat's open questions. | US-3 scenarios pass with the app's window closed. |
| FR-11 | A message sent from the side panel is the person's own prompt in that chat, the same as one typed in the app; the app and the panel show the same chat. | Send from the panel: the app's chat shows the message and reply. |
| FR-12 | Each site has a permission: **Ask** (default), **Allow** or **Block**, kept by Evo and editable in **Settings → Browser** and in the extension. | Set Block in the extension: Settings shows Block. |
| FR-13 | On a Webbridge tab, reads never ask; acts (click, type, key, scroll, navigate) ask once per site and chat in "Ask every time" and "Approve edits" unless the site is **Allow**; uploads, storage and cookie changes, request mocking and scripts ask as a command does in the chat's mode; nothing on a **Block** site runs in any mode. | US-5 scenarios. |
| FR-14 | A question about a Webbridge act shows in the side panel when it is open on that browser, and in the app; an answer in one place settles it in both. | Answer in the panel: the app's question goes. |
| FR-15 | While the agent works in a tab, the page shows an indicator naming the chat, with **Stop** and **Take over**. | Visible on every page the agent acts on, also after navigation. |
| FR-16 | **Stop** in the page, the panel or the app stops the chat's turn and ends its pending browser calls. | Press Stop during `browser_wait`: the call returns stopped. |
| FR-17 | **Take over** holds the agent's calls on that tab and returns "the person has control of this tab" until **Hand back**; the agent is told when control returns. | Take over, then the agent clicks: it is told to wait. |
| FR-18 | **Stop all** (app and extension) ends every chat's work in that browser profile and releases the debugging API from every tab. | The debugging notices go. |
| FR-19 | When the extension starts and Evo is not running, the browser starts Evo in the tray and connects. | Quit Evo, restart the browser: "Connected to Evo" without a window. |
| FR-20 | Closing a tab, a group, the browser or the extension, or quitting Evo, ends the affected calls at once with a clear reason. | Close the tab during a wait: the call returns "tab closed". |
| FR-21 | Every tool result from a Webbridge tab marks page content as untrusted data, as in the built-in browser. | Review of the result wording. |
| FR-22 | The agent cannot use pages the browser forbids extensions to debug, nor pages of a **Block** site, and is told why. | Navigate to the browser's settings page: refused with a reason. |
| FR-23 | When the browser's policy turns off the debugging API, the extension and Settings say so and the agent is told the browser cannot be used. | Set the policy on a test machine: the message shows. |
| FR-24 | Webbridge activity is written to Evo's logs: connections, set-up, refused connections, and each act with its chat and site (never page content or typed text). | Logs show the connection and the acts. |
| FR-25 | The side panel has a header with the chat's title, **History** (recent chats, searchable by title), **New chat**, a **⋯** menu (this site's permission, **Stop all**, **Open in Evo**, **Settings**) and **Close**; a new or empty chat shows an empty state with the field focused. | US-8 scenarios 1 and 6; **Open in Evo** shows the chat in the app. |
| FR-26 | The panel's composer shows the chat's model and thinking effort and changes them, offering the same models (from the person's providers) and the efforts the chosen model supports as the app does; a change applies to the chat, not only to the panel. | Pick a model in the panel: the app's composer shows it; the next turn uses it. |
| FR-27 | The panel's composer shows the approval mode and switches it among the app's modes; the mode is the app's (every chat follows it, as when it is changed in the app) and the app shows the change. Site permissions keep their rules in every mode (FR-13). | US-8 scenario 3. |
| FR-28 | **+** in the panel attaches: a screenshot of the visible part of the current tab, files from the computer, images and files pasted or dropped into the panel, and another open tab (which joins the chat's group). Attachments show as removable chips, follow the app's size limits, and go with the next message. | Attach a screenshot and a PDF, send: the agent receives both. |
| FR-29 | A microphone button dictates into the panel's field; dictated text is editable and is only sent when the person sends it. Voice input is off until the person turns it on in **Settings → Browser**, where they are told which service turns speech into text. | Turned off: no microphone button. Turned on: speech becomes text in the field, nothing sent. |
| FR-30 | **Settings → Browser** sets the agent's default browser: the built-in one (the default) or a connected Chrome or Edge profile. A browser tool call that names no browser uses it; a default that is not connected falls back to the built-in browser. | Default Edge: `browser_navigate` with no target opens in Edge and later calls reuse that tab; named `builtin` stays built-in. |
| FR-31 | A chat sent from a profile's side panel works in that browser by default. A tab the person shared with the chat (Include this tab, `choose`) is read and acted in, but a navigation that names no tab opens a new tab instead of taking the shared one away. | Ask in the panel to open a page: it opens in a new tab of that browser; the shared tab keeps its page. |
| FR-32 | A chat made in the side panel is named after its first message, as the app names its own, and shows in the app's sidebar at once. | Send "Summarize this page" in a new panel chat: the panel and the app show that title. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | No network port is opened for Webbridge. | `netstat` shows no listener added by Evo for it. |
| NFR-2 | A simple call (`browser_read_page` on a small page) answers in under 300 ms on a local machine. | Time it on a Webbridge tab. |
| NFR-3 | Nothing runs and nothing is registered until the person sets Webbridge up; the app starts without it. | Start the app with nothing set up: no Webbridge process. |
| NFR-4 | Windows 10/11 with Chrome and Edge first; macOS and Linux (Chrome, Chromium, Edge) follow in the same code. | Manual check on Windows. |
| NFR-5 | The extension holds no secret, no transcript and no page content beyond what is in flight. | Review of its storage after a session. |
| NFR-6 | Large results (full-page screenshots, long page text) arrive whole. | A full-page screenshot of a long page works. |

## 5. Success criteria

- SC-1 An agent reads and fills a page on a site the person is signed in to,
  in the person's browser, with no step from the person beyond allowing the
  site.
- SC-2 The person can tell at a glance which tabs an agent is using, and can
  stop it or take over within one click.
- SC-3 Closing the browser, a tab, a group or pressing Stop ends the agent's
  access at once.
- SC-4 No web page and no other program run by another user can use the
  bridge.
- SC-5 A conversation started in the side panel continues in the app and back
  without losing anything.

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-07 — Q: Is the bridge wanted? → A: Yes; an extension bridge to
  control the person's own Chrome or Edge, designed and built.
- 2026-11-07 — Q: Native messaging or a token-protected local WebSocket? → A:
  Native messaging, no port (FR-3, NFR-1; [ADR 0012](../../architecture/decisions/0012-native-messaging-bridge-to-the-persons-browser.md)).
- 2026-11-07 — Q: Should a tab's grant outlive an app restart? → A: No; the
  groups end with the app or the browser.
- 2026-11-18 — Q: A bridge that only relays tabs the person shared one by one,
  or Evo working in the browser? → A: Evo working in the browser: a side panel
  chat, tab groups per chat the agent opens tabs in, permissions per site,
  an in-page indicator with stop and take over, waking Evo in the tray
  (sections 1–4).
- 2026-11-18 — Q: Should the side panel's composer match the app's? → A: Yes:
  history, new chat and a menu in the header; model and effort, approval
  mode, attachments with a page screenshot, and voice input in the composer
  (US-8, FR-25 – FR-29).
- 2026-11-18 — Q: What is the feature called? → A: Evo Webbridge (the
  extension's name in the browser stores and the browser's own notices);
  Webbridge for short in the app.
- 2026-11-18 — Gate A: the person accepted the proposed answers:
  - The debugging API is held on a tab from the agent's first call while the
    chat's turn runs, and released 60 s after the turn ends.
  - Agent tabs open in the background in the person's last focused window; a
    window of their own only if background screenshots prove unreliable.
  - The side panel offers the recent chats of every workspace; new chats start
    in the workspace set in **Settings → Browser** (default: last used).
  - Site permissions are one list for all browsers and profiles.
  - Publishing to the Chrome Web Store and Edge Add-ons is this feature's last
    phase; until then the extension loads unpacked with a fixed id.
  - Acts on the tab the person has focused wait while they used it in the last
    5 s.
  - Voice input uses the browser's built-in speech recognition, off by
    default, with a notice where it is turned on (FR-29).

## 7. History

- 2026-11-07 — Draft: share-a-tab bridge.
- 2026-11-18 — Rewritten as Webbridge: Evo works in the person's browser with
  a side panel, tab groups per chat and permissions per site. Requirement,
  scenario and criteria IDs restart; nothing cited the earlier draft's IDs.
- 2026-11-18 — Side panel composer: US-8, FR-25 – FR-29; FR-1 adds the active
  tab permission.
- 2026-11-18 — Renamed Evo Webbridge; the product name is written "Evo" in
  this feature's documents and UI text (identifiers stay lowercase).
- 2026-11-18 — Accepted (Gates A and B); tasks written.
- 2026-11-18 — FR-27: the approval mode is the app's, so the panel switches the app's mode.
- 2026-11-19 — FR-30 (default browser), FR-31 (a panel chat works in its browser; shared tabs are not navigated away), FR-32 (panel chats are named after their first message).
- 2026-11-18 — Checks run on Edge; Chrome runs the same extension and code and
  is not checked separately.
- 2026-11-18 — The toolbar button opens the side panel (no popup); the app's
  settings page is **Settings → Browser**, holding the built-in browser and Webbridge.
