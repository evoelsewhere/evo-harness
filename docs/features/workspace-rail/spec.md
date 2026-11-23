# Workspace rail and "Needs you" — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop UI |
| Created | 2026-11-23 |
| Updated | 2026-11-23 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Shell-hosted engines](../shell-hosted-engines/spec.md) (NFR-5: working chats without a window) · [Subagents and nested sessions](../subagents-and-nested-sessions/spec.md) · [UX polish](../ux-polish/spec.md) (FR-5: scheduled run titles) · [Sidebar items](../sidebar-items/spec.md) |

## 1. Summary

The sidebar lists every workspace one under another, each with all of its
chats by date. With several workspaces and several chats running at once it
becomes a long, mixed list where the chat that waits for an approval is hard
to find. This feature puts the workspaces on a narrow rail at the sidebar's
left edge (one workspace's chats shown at a time, the others' state as a dot
on their icon), adds a **Needs you** section across all workspaces for chats
that wait, have an unread reply or are running, groups a workspace's chats
by time, and folds the runs of one scheduled task into a single row.

## 2. Scope

### 2.1 Goals

- See at a glance, across every workspace, which chats wait for the person,
  have an unread reply or are running, and open one in one click.
- Keep one workspace's chats on screen at a time, and still see the state of
  the other workspaces.
- Shorten a workspace's list: time groups, older chats folded, scheduled runs
  folded.

### 2.2 Non-goals

- No change to how chats run, are stored or are reported by the engine or
  the shell: the sidebar only arranges state it already has.
- No board (kanban) view of chats and no new search: the command palette
  (Mod+P) stays the way to find any chat.
- No automatic archiving of old chats.
- Workspaces are not reordered by hand; the rail keeps the existing
  alphabetical order with "This computer" last.

## 3. User scenarios

### US-1 Find what needs me across workspaces (priority P1)

A person running chats in three workspaces wants to see which of them wait
for an approval or have a new reply, without opening each workspace.

Acceptance scenarios:

1. **Given** a chat in workspace B waits for an approval while workspace A
   is shown, **when** the person looks at the sidebar, **then** the chat is
   listed under **Needs you** with B's short name, and B's rail icon carries
   the amber dot.
2. **Given** that row, **when** the person clicks it, **then** the chat opens
   and the rail switches to workspace B.
3. **Given** the person answers the approval and the chat finishes and is
   read, **then** it leaves **Needs you**.

### US-2 Work in one workspace at a time (priority P1)

A person wants the list to show only the workspace they work in.

Acceptance scenarios:

1. **Given** several workspaces, **when** the person clicks a rail icon,
   **then** the list below shows that workspace's chats only, with its name
   and a New chat button on top.
2. **Given** the person restarts Evo, **then** the rail shows the workspace
   shown when Evo closed.
3. **Given** the person clicks the sidebar's New chat after picking a
   workspace on the rail, **then** the new chat is made in that workspace.

### US-3 A shorter list (priority P2)

Acceptance scenarios:

1. **Given** chats from today, yesterday, this week and earlier, **then**
   they are listed under Today, Yesterday, Previous 7 days and Older, and
   Older is folded to one row with its count.
2. **Given** seven runs of one scheduled task in a workspace, **then** they
   show as one row (the latest run) with "7 runs", which unfolds to the
   earlier runs.

### Edge cases

- The shown workspace is removed: the rail shows the first workspace left,
  or "This computer".
- The open chat is in Older or is an earlier scheduled run: it stays visible
  (the fold opens) so the selected row is never hidden.
- A nested session waits for an approval: it shows in **Needs you** (its
  parent's workspace tag), as well as under its parent.
- A draft (no prompt yet) never shows in **Needs you**.
- Only "This computer" exists: the rail still shows it, with Add workspace.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | The sidebar must show a rail at its left edge with one icon per workspace (its initials; tooltip with name and folder), then "This computer", then Add workspace. The shown workspace's icon is marked selected. | Given two workspaces, then the rail shows their initials, "This computer" and Add workspace, and the shown one is selected. |
| FR-2 | A rail icon must show, as a dot, the most urgent state of its workspace's chats (nested sessions included): amber when one waits for the person, a spinner when one runs, blue when one has an unread reply; nothing otherwise. Its tooltip names the counts. | Given a waiting chat in workspace B, then B's icon has the amber dot and its tooltip says "1 waiting". |
| FR-3 | The chat list must show only the shown workspace's chats, under a "Chats" header (the name is on the rail) with a New chat button and a menu (Show in folder, Change icon…, Remove workspace…). Needs you and the header stay in place; only the chats and the archive scroll. Right-clicking a rail icon opens the same menu. | Given workspace A shown, then no chat of B is in the list below the header. |
| FR-4 | The shown workspace must follow the open chat: opening a chat from another workspace (from Needs you, the palette, a notification) shows that workspace. Picking a rail icon shows that workspace without changing the open chat. | Given A shown, when a chat of B is opened, then the rail selects B. |
| FR-5 | The shown workspace must be remembered across restarts. | Given B shown when Evo closes, when it starts, then B is shown. |
| FR-6 | The sidebar's New chat (button and Mod+N) must make the chat in the shown workspace. | Given B picked on the rail, when New chat, then the draft is in B. |
| FR-7 | A **Needs you** section above the workspace list must list, across all workspaces, the top-level chats that wait for the person, have an unread reply or are running, and the nested sessions that wait for the person; waiting first, then unread, then running, newest first within each. Each row shows its workspace's initials. The section is hidden when empty and shows at most 6 rows, then "N more". Chats stay in their workspace list too. | Given a waiting chat, an unread chat and a running chat in different workspaces, then Needs you lists them in that order with their tags. |
| FR-8 | A workspace's pinned chats must be listed first, under Pinned, inside that workspace. | Given a pinned chat in A, when A is shown, then it is under Pinned at the top; when B is shown it is not listed. |
| FR-9 | The other chats must be grouped under Today, Yesterday, Previous 7 days and Older by their last activity; Older is folded to "Older · N" unless opened or it holds the open chat. Empty groups are not shown. | See US-3 scenario 1. |
| FR-10 | Runs of one scheduled task in a workspace (same task title) must show as one row — the latest run — with an "N runs" toggle that lists the earlier runs. The row sits in the time group of its latest run. A renamed run is a chat of its own. | See US-3 scenario 2. |
| FR-11 | Nested sessions keep showing under their parent (as today); the Archived group stays at the end of the list. | Given a chat with nested sessions, then they show under it as before. |
| FR-12 | Each rail icon must show the workspace's name under it, cut with "…" when it does not fit. | Given "evo-harness", then its rail icon reads "evo-harness" below it. |
| FR-13 | A workspace's icon must be, in order: the icon the person chose for it; else the project's logo found in its folder; else its initials. "This computer" keeps the computer icon. | Given a folder with `public/favicon.svg` and no chosen icon, then the rail shows that image. |
| FR-14 | The workspace menu (rail right-click and the list header's "…") must have **Change icon…**: a dialog to pick one of a set of icons, an emoji, the project's logo (when one is found) or the initials, and to go back to automatic. The choice is remembered across restarts and applies at once. | Given the person picks the rocket icon for B, then B's rail icon is a rocket, also after a restart. |
| FR-16 | Settings → Appearance must have **Workspace names** (on by default): off, the rail shows icons only and is narrower. The choice is kept on this machine. | Given it is switched off, then no names show under the rail icons and the rail narrows, also after a restart. |
| FR-17 | Settings → Appearance must have **Workspaces in sidebar**: **Rail** (FR-1 – FR-12, the default) or **List**. List shows the earlier layout: no rail; Needs you on top; one shared Pinned section; then every workspace one under another, each collapsible (its collapsed state remembered), with its icon, name, New chat and menu, and its chats newest first with scheduled runs folded (FR-10); then This computer and Archived; Settings and the theme switch in the footer. **Workspace names** applies to Rail only. The choice is kept on this machine and applies at once. | Given List is picked, then the rail is gone and every workspace is listed with its chats; given Rail again, then the rail is back. |
| FR-15 | Rows in **Needs you** must show the workspace's icon (small) instead of its initials. | Given a waiting chat of a workspace with a logo, then its Needs you row shows the logo. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | No engine, protocol, shell or database schema change: the sidebar derives everything from the chat list and the working state it has. | The diff touches `apps/desktop/src` and docs only. |
| NFR-2 | The grouping rules are pure functions with unit tests. | `bun test src/features/shell/sidebar-model.test.ts` passes. |
| NFR-3 | Follows DESIGN.md (status colour only in dots), except that the shown workspace's rail icon takes a light tint of the accent picked in Settings → Appearance; the other icons stay plain. Rail icons and rows are keyboard reachable with accessible names. | Manual check in the running app, light and dark. |

## 5. Success criteria

- SC-1 With chats running in three workspaces, a waiting chat is reachable in
  one click from the sidebar whichever workspace is shown.
- SC-2 A workspace with 7 scheduled runs and 30 older chats shows no more
  than about 10 rows before Older is opened.

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-23 — Q: When a chat from another workspace is opened from Needs
  you, does the rail switch to that workspace? → A: Yes, the rail follows
  the open chat (FR-4).
- 2026-11-23 — Q: Where do pinned chats show when one workspace is shown at
  a time? → A: Inside their own workspace, at the top (FR-8).
- 2026-11-23 — Q: Does a chat in Needs you also stay in its workspace list?
  → A: Yes, both; the workspace list stays stable (FR-7).
- 2026-11-23 — Q: Time groups and folded scheduled runs in the same change?
  → A: Yes (FR-9, FR-10).

### 6.3 Assumptions

- A-1 The section is named "Needs you" and also lists running chats, as in
  the approved mockup; running rows show the spinner, so they read apart
  from waiting ones.
- A-2 Nested sessions appear in Needs you only while they wait for the
  person; their running and unread state already rolls up under the parent.
- A-3 Initials: the first letters of the first two words of the workspace
  name (split on spaces, `-`, `_`, `.` and camel case), else its first two
  letters; "This computer" shows a computer icon.
- A-4 The per-workspace collapsed state is used by the List layout only
  (FR-17); the Rail layout ignores it.
- A-5 "Previous 7 days" counts calendar days in the person's time zone.
- A-6 The rail takes 72 px with names under the icons, so the sidebar's
  default width grows from 272 to 328 px (minimum 272, maximum 480); a width
  the person set is kept.
- A-7 The project logo is looked for at a fixed list of usual places (the
  folder's root, `public/`, `static/`, `assets/`, `.github/`, `docs/`,
  `src-tauri/icons/`, and the same under each folder of `apps/` and
  `packages/`), PNG, SVG, ICO, JPG or WebP files up to 1 MB; it is found
  again at each start, not saved.
- A-8 The icon set is about 30 general-purpose outline icons; an emoji is
  any text up to two characters wide.

## 7. History

- 2026-11-23 — Draft from the approved mockup and the four answers above;
  accepted to build in the same session.
- 2026-11-23 — Owner asked for the settings and theme buttons on the rail's
  foot, and for rail icons in the Appearance accent (NFR-3); then for the
  tint on the shown icon only, the others plain.
- 2026-11-23 — Owner found the initials hard to tell apart and chose names
  under the icons plus an icon per workspace: chosen, project logo or
  initials (FR-12 – FR-15).
- 2026-11-23 — Polish: every rail icon in a 36 px tile (no fill unless shown
  or hovered), 11 px names ("Local" for
  This computer), the list header reads "Chats".
- 2026-11-23 — Owner asked for a setting to hide the rail's names (FR-16).
- 2026-11-23 — Owner asked for a choice between the rail and the earlier
  stacked list (FR-17); answers: Needs you stays in both, chats stay flat
  in List (runs still folded), Pinned is one shared section in List.
