# Workspace rail and "Needs you" — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-23 |
| ADD sections | [Desktop UI](../../architecture/README.md) |
| ADRs | none |

## 1. Context and constraints

- `features/shell/app-sidebar.tsx` lays out the sidebar: title bar, search,
  New chat, page rows, Pinned (all workspaces), one collapsible
  `WorkspaceGroup` per workspace, "This computer", Archived, footer.
- `features/shell/sidebar-chats.tsx` holds `ChatItem` (a row with its
  running / waiting / unread state), `NestedChats`, `ChatList`,
  `WorkspaceGroup` and `ArchivedGroup`.
- A chat's state comes from two stores: `engine-store` runtimes
  (`transcript.busy`, the last turn's `permission`, `busyElsewhere`) for
  chats with an engine in this window, and `session-store.work` for chats the
  shell runs without a window (shell-hosted engines NFR-5). `unread` is on the
  chat row.
- Scheduled runs are chats with `scheduled` set, titled after their task;
  `useDisplayTitle` adds the run date (ux-polish FR-5).
- `engine-store.newChat()` without an argument uses the open chat's
  workspace.
- Settings are saved with `db.setSetting` / read with `db.settings()`.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | Display only. |
| Layers point down; library crates stay free of engine/desktop concerns | n/a | No crate change. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | No protocol change (NFR-1). |
| Models come from models.dev; no hardcoded model facts | n/a | |
| Real engine in the app; settings and credentials reload live | ok | No mocks; state comes from the real stores. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Approvals are still answered in the chat; the sidebar only links to it. |
| Desktop UI structure and `DESIGN.md` | ok | Components read store selectors; colour only in status dots (NFR-3). |

## 3. Design

### 3.1 Overview

```text
┌ title bar ─────────────────────────────┐
├──────┬─────────────────────────────────┤
│ rail │ search · New chat · page rows   │
│  EH• │ Needs you (all workspaces)      │
│  EW  │ ── workspace header (name + …)  │
│  ▭   │ Pinned · Today · Yesterday ·    │
│  +   │ Previous 7 days · Older (fold)  │
│      │ Archived                        │
│      │ footer                          │
└──────┴─────────────────────────────────┘
```

`sidebar-model.ts` holds the pure rules (initials, time groups, run
folding, attention order, rail roll-up). `use-chat-states.ts` turns the two
stores into one `Record<chatId, "waiting" | "running">`. The components
render from these.

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| desktop UI | `apps/desktop/src/features/shell/sidebar-model.ts` (+ `.test.ts`) | `initials`, `timeGroup`, `groupByTime`, `foldRuns`, `needsYou`, `rollUp` | FR-2, FR-7, FR-9, FR-10, NFR-2 |
| desktop UI | `apps/desktop/src/features/shell/use-chat-states.ts` | One selector over `engine-store` and `session-store.work` | FR-2, FR-7 |
| desktop UI | `apps/desktop/src/features/shell/workspace-rail.tsx` | The rail, its icons, dots, tooltips and menu | FR-1, FR-2, FR-3 |
| desktop UI | `apps/desktop/src/features/shell/sidebar-chats.tsx` | `ChatItem` gets an optional workspace tag; `NeedsYou`, `WorkspaceChats` (header, Pinned, time groups, Older fold, run folds) replace `WorkspaceGroup` | FR-3, FR-7–FR-11 |
| desktop UI | `apps/desktop/src/features/shell/app-sidebar.tsx` | Layout: rail beside the column; follow the open chat | FR-1, FR-4 |
| desktop UI | `apps/desktop/src/stores/session-store.ts` | `shownWorkspace`, `showWorkspace()`, saved as `sidebar.workspace`; drop `collapsed` / `toggleCollapsed` | FR-4, FR-5 |
| desktop UI | `apps/desktop/src/stores/engine-store.ts` | `newChat()` default: the shown workspace | FR-6 |
| desktop UI | `apps/desktop/src/stores/workspace-icon-store.ts` | Chosen icons (setting `sidebar.workspaceIcons`), project logos found per workspace (blob URLs, this run only) | FR-13, FR-14 |
| desktop UI | `apps/desktop/src/lib/project-logo.ts` (+ `.test.ts`) | Candidate logo paths, image type by extension | FR-13 |
| desktop UI | `apps/desktop/src/features/shell/workspace-icon.tsx` | `WorkspaceIcon` (chosen / logo / initials), the icon set, the Change icon dialog | FR-12 – FR-15 |
| desktop UI | `apps/desktop/src/stores/appearance-store.ts`, `features/settings/sections/appearance-settings.tsx` | `sidebarLayout` (`rail` / `list`, in `evo.appearance`) and its control | FR-17 |
| desktop UI | `apps/desktop/src/features/shell/sidebar-chats.tsx` `WorkspaceGroup`, `stores/session-store.ts` `collapsed` | The List layout's collapsible group per workspace, folded state saved as before (`workspace_set_collapsed`, `sidebar.localCollapsed`) | FR-17 |
| docs | `apps/desktop/README.md` | UI map row for the sidebar | — |

### 3.3 Interfaces

```ts
// session-store
shownWorkspace: string;            // a workspace id or LOCAL_GROUP
showWorkspace(id: string): void;   // sets and saves `sidebar.workspace`

// sidebar-model
type ChatState = "waiting" | "running";
type TimeGroup = "today" | "yesterday" | "week" | "older";
function initials(name: string): string;
function timeGroup(at: number, now: number): TimeGroup;
function foldRuns<T extends RunLike>(chats: T[], isRun: (c: T) => boolean): { chat: T; runs: T[] }[];
function needsYou<T extends ChatLike>(chats: T[], states: Record<string, ChatState>): T[];
function rollUp<T extends ChatLike>(chats: T[], states: Record<string, ChatState>): { waiting: number; running: number; unread: number };
```

### 3.4 Data and storage

One app setting, `sidebar.workspace` (string: workspace id or `local`), in
the existing settings table. No migration. `workspaces.collapsed` and
`sidebar.localCollapsed` stay, unused (spec A-4).

### 3.5 Behaviour

- On load the shown workspace is the saved one if it still exists, else the
  last open chat's workspace, else the first workspace, else `local`.
- `app-sidebar` watches `engine-store.activeId`; when it names a chat whose
  workspace differs from the shown one, it calls `showWorkspace` (FR-4).
- Removing the shown workspace falls back as on load.
- The Older fold and the run folds keep their open state in component state;
  they open by themselves when they hold the active chat.

### 3.6 Errors and edge cases

A failed settings write is logged (`report`) and the in-memory choice stays.
A chat whose workspace id no longer exists falls under `local`.

### 3.7 Security and trust

n/a — display only; no new input, command or permission.

Workspace icons (FR-13, FR-14): `sidebar.workspaceIcons`, a JSON object
`{ [workspaceId]: { kind: "icon", name } | { kind: "emoji", text } | { kind: "logo" } | { kind: "initials" } }`;
no entry means automatic (logo, else initials). The logo is found with the
shell's existing `fs_stat` and `fs_read_bytes` commands (no new command),
read once per workspace per run and kept as a blob URL.

### 3.8 Configuration

`sidebar.workspace` — string, default unset → rules in 3.5, saved on each
change, read at load.

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Keep stacked workspaces, only add Needs you | Does not shorten the list with many workspaces; owner chose the rail. |
| A count badge on rail icons | DESIGN.md allows colour only at dot size; counts go in the tooltip. |
| A workspace dropdown instead of a rail | Hides the other workspaces' state behind a click. |

## 5. As-built notes

- `ConfirmDialog` moved from `sidebar-chats.tsx` to `features/shell/confirm-dialog.tsx`, so the rail and the chat rows share it without a circular import.
- `SIDEBAR_WIDTH` in `stores/ui-store.ts` grew (spec A-6).
- Needs you and the workspace header sit outside the scrolling area rather than being sticky: with Mica or vibrancy the sidebar is translucent, so a sticky header would show the rows passing under it. Needs you scrolls on its own past 40 % of the height.
- The logo lookup lists each candidate folder once with `fs_list_dir` rather than calling `fs_stat` per file, so a workspace costs about ten IPC calls.
- Checked in Vite with the fake Tauri IPC of `scripts/readme-shots/mock-tauri.js`; that mock lacks commands added since (`engine_ensure`, `plugins_state`, `plugin_ai_questions`, `engine_questions`), so the check added the first three in its own sample-data script, not in the repo.
