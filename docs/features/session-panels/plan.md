# Side panel tabs, browser pages and terminals per chat — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-07 |
| ADD sections | [4. Process view](../../architecture/README.md#4-process-view) |
| ADRs | none |

## 1. Context and constraints

- `ui-store` held one `openTabs` / `activePanel` for the window; `browser-store`
  and `terminal-store` held one list of tabs. Pages are native child webviews
  and shells are ptys in the Rust shell; both outlive a window reload.
- The chat showing is `engine-store.activeId`. Chat windows are separate
  webviews with their own stores.
- Existing code reads `openTabs`, `activePanel`, and the browser's `tabs` /
  `activeId`; keeping those names meaning "the showing chat's" keeps the call
  sites unchanged.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | View state only; no engine or shell change. |
| Layers point down; library crates stay free of engine/desktop concerns | n/a | Desktop UI only. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | No protocol change. |
| Models come from models.dev; no hardcoded model facts | n/a | |
| Real engine in the app; settings and credentials reload live | ok | |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | An agent can no longer name another chat's tab. |
| Desktop UI structure and `DESIGN.md` | ok | Stores keep the data, components read selectors; `components/ui/` untouched. |

## 3. Design

### 3.1 Overview

```text
engine-store.activeId ──▶ chat-scope.enterChat(chat)
                              ├─ ui-store.enterChat      swap openTabs / activePanel
                              ├─ terminal-store.setChat  tabs of `chat`, its active tab
                              └─ browser-store.setChat   tabs of `chat`, show its active page
```

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| desktop UI | `apps/desktop/src/stores/chat-scope.ts` (new) | Follows the showing chat; releases a chat that disappeared from the session list. | FR-1 – FR-4, FR-6 |
| desktop UI | `apps/desktop/src/stores/ui-store.ts` | `panelChat`, `panelsByChat`, `enterChat`, `forgetChat`, `openPanel(panel, chat)`. | FR-1, FR-4, FR-5 |
| desktop UI | `apps/desktop/src/stores/browser-store.ts` | Tabs carry `chat`; `all` is the truth, `tabs` / `activeId` mirror the showing chat's; `agentTab(id, chat)`. | FR-2, FR-4, FR-7, FR-8 |
| desktop UI | `apps/desktop/src/stores/terminal-store.ts` | Tabs carry `chat`; `tabs` holds every chat's, `chatTabs()` filters; `activeByChat`. | FR-3, FR-4, FR-7, FR-8 |
| desktop UI | `apps/desktop/src/lib/tab-chats.ts` (new) | Session-storage map from page label / pty id to chat, for a reload. | FR-7 |
| desktop UI | `apps/desktop/src/features/terminal/`, `features/browser/` | Panels and agent tools list and act on the calling chat's tabs. | FR-5 |

### 3.3 Interfaces

No wire change. `runUiTool(name, args, chat)`; `useTerminalStore.create({ chat })`;
`useUiStore.openPanel(panel, chat)`.

### 3.4 Data and storage

- `panelChat` and `panelsByChat` are saved with the UI state (local storage); a
  state saved before has no `panelChat`, so its tabs go to the first chat shown.
- Page and shell ownership is in session storage, which lasts as long as the
  native pages and ptys (the app's run).

### 3.5 Behaviour

- Switching: the old chat's tabs are stored under its id and the new chat's are
  loaded (none for a chat that never opened any). The native page of the old
  chat is hidden, the new chat's visible page is shown.
- An agent in a chat that is not showing: `openPanel(panel, chat)` edits that
  chat's saved tabs; its page is created hidden.

### 3.6 Errors and edge cases

| Case | Result |
| --- | --- |
| Agent names another chat's tab id | "No browser tab …" / "No terminal tab …" |
| Draft discarded with a terminal | Its shell and pages end (`releaseChat`) |
| Reload with unknown ownership | The tab goes to the first chat shown |
| Agent action that needs a visible page (screenshot, click) in a chat that is not showing | Works on the hidden page as far as the page allows; no panel is taken over |

### 3.7 Security and trust

An agent can no longer read or type into a terminal or page of another chat.

### 3.8 Configuration

None.

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| One side panel per chat component, unmounted on switch | Pages and shells must keep running; the stores hold them. |
| Ownership kept in the Rust shell | Needs a protocol change in the pty and page commands; session storage covers a reload. |

## 5. As-built notes

n/a — empty until implemented and verified.
