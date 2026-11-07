# Agent browser and app control: closing the tool gaps — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop shell (Rust), desktop UI |
| Created | 2026-11-07 |
| Updated | 2026-11-07 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [ADR 0013](../../architecture/decisions/0013-app-side-agent-tools-run-in-the-rust-shell.md), [Browser bridge](../browser-bridge/spec.md), [Session panels](../session-panels/spec.md), [Agent terminal](../agent-terminal/spec.md), [Approval modes](../approval-modes/spec.md), [Agent tools](../agent-tools/spec.md) |

## 1. Summary

The agent can drive the built-in browser (13 `browser_*` tools) and the app's
terminal (5 `terminal_*` tools), but it cannot do much that a person does in a
browser: hover, drag, right-click, wait for something, take a full-page or
element screenshot, answer a dialog, upload a file, switch to a phone viewport,
read cookies or a response body, or batch several steps. It also cannot steer
Evo itself the way a person does (open a panel, look at usage, search old
chats). This feature closes those gaps in waves, on the same terms as today:
the person can watch and take over, the page is untrusted, and the approval
mode applies.

The gaps below come from comparing Evo's tool set with two reference designs: a
desktop agent's built-in browser pane (coordinate and ref input, accessibility
tree, batch steps, tabs, viewport presets, console and network, dev-server
preview) and a driver for a person's own browser (cookies, storage, dialogs,
emulation, request mocking, file upload, performance, crawling, recording).

## 2. Scope

### 2.1 Goals

- An agent can finish the common browser tasks of a web developer and a tester
  without asking the person to click: pointer actions, waits, screenshots of a
  page, an element or a region, dialogs, file inputs, frames, several steps in
  one call.
- An agent can set up the page's environment for a test: viewport and device,
  colour scheme, locale, cookies and storage, request interception.
- An agent can see what the page did: console, requests with headers and
  bodies, downloads, performance numbers.
- The tools say what they cannot do on this platform instead of failing late.
- The agent can steer the parts of Evo a person steers by hand, within what the
  approval mode and safety rules allow.

### 2.2 Non-goals

- Controlling the whole desktop (other apps, the operating system's mouse and
  keyboard): a separate, later decision (section 6.2).
- Driving the person's own Chrome or Edge: the [browser bridge](../browser-bridge/spec.md) feature. This one only makes the tools ready to run against it (FR-25).
- Letting an agent change security settings, the approval mode, the sandbox or
  credentials.
- Crawling many pages in a loop as a built-in tool: the agent can loop itself.

## 3. User scenarios

### US-1 Test a page like a person would (priority P1)

A developer asks the agent to check a form on their dev server on a phone-sized
screen, with the file field and the confirm dialog.

Acceptance scenarios:

1. **Given** a page with a file input, **when** the agent attaches a file from
   the working folder, **then** the page sees the file and the agent reports
   its name.
2. **Given** a page that calls `confirm()`, **when** the agent clicks the
   button, **then** it is told a dialog is open with its text and answers it;
   the click does not hang.
3. **Given** the agent sets a 390×844 viewport, **when** it takes a screenshot,
   **then** the picture is that size and the page lays out as on a phone.

### US-2 Do something that needs the pointer (priority P1)

Acceptance scenarios:

1. **Given** a menu that opens on hover, **when** the agent hovers over it,
   **then** the menu opens and the agent can click an item.
2. **Given** a slider or a drag-and-drop list, **when** the agent drags from one
   point to another, **then** the page's own handlers run.

### US-3 Understand a failure (priority P2)

Acceptance scenarios:

1. **Given** a request that returned 500, **when** the agent asks for it,
   **then** it gets the status, headers and the start of the body.
2. **Given** a slow page, **when** the agent waits for a selector or for the
   network to go quiet, **then** it continues as soon as that happens, or says
   it timed out.

### US-4 Steer the app (priority P3)

Acceptance scenarios:

1. **Given** the agent wants the person to look at the preview, **when** it
   opens the Browser tool for its chat, **then** the panel shows it (for the
   chat showing) or marks it for when the chat shows.
2. **Given** the person asks "what did I spend this week", **when** the agent
   reads usage, **then** it answers from the same numbers as the Usage page.

### Edge cases

- A platform without DevTools access (macOS, Linux webviews): pointer, wait and
  screenshot tools use page scripts where they can; a tool that cannot work
  returns "not available on this platform" with what to use instead.
- A page that never settles, a dialog that opens during a wait, a tab that
  closes mid-call: the call returns and says so.
- A page in a chat that is not showing is hidden: tools that need pixels say
  so (see [session panels](../session-panels/spec.md)).

## 4. Requirements

### 4.1 Functional

Wave 1 — input and observation (every platform where the webview allows).

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | `browser_click` takes `button` (left, right, middle) and `clickCount` (1–3); a new `browser_hover` moves the pointer to a ref or point; a new `browser_drag` drags between two refs or points. | Hovering opens a hover menu; a drag moves a range slider. |
| FR-2 | A new `browser_wait` waits for a ref or text to appear or disappear, for the network to be quiet for N ms, or for a time, up to `timeoutMs` (max 60 s), and says which happened. | Waiting for a text that appears after 2 s returns at about 2 s; a missing one times out and says so. |
| FR-3 | `browser_screenshot` takes `fullPage`, `ref` (one element) and `region` (x, y, width, height), plus `scale`; a zoom on a region returns more detail. | A full-page shot of a long page is taller than the viewport; an element shot is its box. |
| FR-4 | Dialogs (`alert`, `confirm`, `prompt`, `beforeunload`) no longer block a tool call: the call returns noting the open dialog, and `browser_dialog` accepts or dismisses it (with text for a prompt). | A click that opens `confirm()` returns; `browser_dialog {accept:true}` runs the page's handler. |
| FR-5 | A new `browser_upload` sets files on a file input (by ref); paths must be allowed by the sandbox and the approval mode. | An input with `multiple` receives two files; a path outside the allowed folders is refused. |
| FR-6 | `browser_read_page`, `browser_find`, `browser_click`, `browser_type` and `browser_form_input` reach elements inside iframes (same-origin by script, any origin where DevTools is available) through a `frame` argument or frame-qualified refs. | A button inside an iframe is found and clicked. |
| FR-7 | A new `browser_batch` runs up to 20 steps (any browser tool but itself) in order and returns each result, stopping at the first failure. | Navigate, type, click and read in one call return four results. |
| FR-8 | `browser_read_page` takes `format` (`outline`, `text`, `markdown`): `text` returns the readable text, `markdown` keeps headings, lists and tables. | A table on a page comes back as a Markdown table. |
| FR-9 | `browser_tabs` can open a blank tab (`create`) and says for each tab whether it is loading, its URL and title. | `create` opens a tab; the list shows it. |
| FR-10 | A new `browser_status` reports which tools work on this platform (DevTools access, frame reach, dialogs, upload, emulation) and the page's readiness. | On a platform without DevTools, the list shows what falls back. |

Wave 2 — environment and diagnostics.

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-11 | A new `browser_viewport` sets size, device pixel ratio and touch (presets `mobile`, `tablet`, `desktop`), colour scheme (`light`, `dark`), locale, time zone and user agent for a tab, and resets them. | A mobile preset makes `(max-width: 480px)` match. |
| FR-12 | A new `browser_storage` reads, sets and clears cookies, `localStorage` and `sessionStorage` of the tab's origin; setting or clearing needs approval; values of cookies marked sensitive are shown masked unless asked for. | Reading cookies lists names and attributes; clearing asks first. |
| FR-13 | `browser_network` takes `filter` (URL text, status, type), `requestId` (headers and the first 20,000 characters of the response body), and records from navigation start, not from when the tool was first called. | A failed API call shows its response body. |
| FR-14 | A new `browser_mock` blocks, delays or answers requests that match a pattern, until cleared; it needs approval. | A blocked image URL does not load; clearing restores it. |
| FR-15 | `browser_console` takes `level` and `since` and can clear. | Errors only since the last clear. |
| FR-16 | Downloads started by a page go to a folder allowed by the sandbox, and `browser_downloads` lists them (name, size, state); starting one from an untrusted origin asks. | A click on a download link saves the file and lists it. |
| FR-17 | A new `browser_performance` returns load timings, paint metrics and layout shift for the tab, and `browser_pdf` saves the page as a PDF in an allowed folder. | A report has first contentful paint and total transfer size. |

Wave 3 — Evo itself.

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-18 | A new `app_panel` opens, focuses or closes a side panel tool for the agent's chat (browser, terminal, files, memory) and reports the panel state. | The agent opens Files for its chat; it shows when the chat does. |
| FR-19 | A new `usage_get` returns the Usage page's numbers (tokens and cost by model and day) for a period. | "This week" matches the Usage page. |
| FR-20 | New `session_search` and `session_read` find and read past chats' messages (own workspace, bounded size, treated as untrusted data). | Searching a phrase from an old chat finds it with a snippet. |
| FR-21 | A new `preview_start` starts a project's dev server from a saved launch configuration in a terminal tab, waits for its ready line and opens it in the browser. | A configured `dev` server starts and shows in the Browser tool. |

Safety and structure.

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-22 | Browser tools are classed as **observe** (screenshot, read, find, console, network, tabs list, status, wait), **act** (click, hover, drag, type, key, form input, scroll, dialog, batch of those) and **change** (upload, storage writes, mock, downloads, viewport reset of the person's tab, eval). Observe never asks; act asks in "Ask every time" for origins that are not local; change asks in "Ask every time" and "Approve edits". | In "Ask every time", a click on a public site asks once per origin per chat; `browser_upload` asks in "Approve edits". |
| FR-23 | Page content, console text, response bodies and storage values are untrusted data and are marked so in tool results. | Review of the result wording. |
| FR-24 | Every new tool is registered in the shell's tool registry (`agent_tools`), is answered by the shell with the window hidden, renders in the transcript (what it did, with an excerpt), and works on the calling chat's pages only. | A transcript row for each new tool; another chat's tab id is "no tab". |
| FR-25 | The tools run against a **page driver** with two back ends: DevTools (the built-in browser on Windows, and any [bridge](../browser-bridge/spec.md) tab) and page scripts (the built-in browser elsewhere). A tool takes `target` (`builtin` by default, or a bridge browser) and tab ids say which they belong to. | The same `browser_click` call clicks in a built-in tab and in a shared Chrome tab. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | A tool that needs DevTools access says so on macOS and Linux instead of failing; no tool hangs the UI. | Run each tool on a platform without DevTools: a clear message, UI responsive. |
| NFR-2 | A tool result is at most 20,000 characters (screenshots excepted, scaled to at most 1568 px on the long side) and says when it was cut. | A long page read is cut with a note. |
| NFR-3 | Waiting tools end when the turn is stopped, as the terminal's do. | Stop a turn during `browser_wait`: it returns at once. |

## 5. Success criteria

- SC-1 An agent can fill, submit and verify a form with a file field, a dialog
  and a phone viewport without a person touching the page.
- SC-2 An agent can explain a failed request from its status, headers and body.
- SC-3 No tool lets an agent reach another chat's page, a credential or a
  security setting.

## 6. Clarifications

### 6.1 Open

- [NEEDS CLARIFICATION: macOS and Linux have no DevTools access in the webview.
  Proposed: wave 1 degrades through page scripts where it can and says "not
  available" otherwise (FR-10); a native input layer is later work.]

### 6.2 Resolved

- 2026-11-07 — Q: Which waves? → A: All three (browser input and observation,
  environment and diagnostics, Evo itself), in that order, each its own commits.
- 2026-11-07 — Q: Should browser tools ask in the approval modes? → A: Yes, by
  class (FR-22).
- 2026-11-07 — Q: Drive the person's own Chrome or Edge? → A: Yes, through an
  extension bridge, designed and built with this work; it is its own feature
  ([browser bridge](../browser-bridge/spec.md)) and the tools here are written
  to run against it (FR-25).
- 2026-11-07 — Q: Control of the whole desktop (other windows, OS mouse and
  keyboard)? → A: Not now; revisit after the browser and the bridge.

## 7. History

- 2026-11-07 — Draft: gap analysis against two reference agent browsers and
  Evo's current tools.
- 2026-11-07 — Accepted with the clarified answers; the bridge back end moved to its own feature, built later.
- 2026-11-08 — Decision: the tools run in the Rust shell, not the webview ([ADR 0013](../../architecture/decisions/0013-app-side-agent-tools-run-in-the-rust-shell.md)); plan and tasks rewritten for it.
- 2026-11-07 — Clarified: all three waves, approvals by class, the bridge as its own
  feature with FR-25 here.
