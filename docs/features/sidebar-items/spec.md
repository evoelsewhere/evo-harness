# Sidebar items: a More menu and Edit sidebar — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop UI |
| Created | 2026-11-11 |
| Updated | 2026-11-11 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Memory and the Scheduler](../memory-and-scheduler/spec.md) · [Usage and cost history](../usage-and-cost/spec.md) · [Customize](../customize-skills-plugins-connectors/spec.md) |

## 1. Summary

The top of the sidebar lists the app's pages (Scheduler, Usage, Customize) as a
fixed list in the code. Each new page means another permanent row. This
feature turns the list into a set of items the person picks. The items they
show stay in the sidebar, and the rest go into a **More** menu. **Edit
sidebar…** opens a dialog with one checkbox per item. New pages can then be
added without crowding the sidebar.

## 2. Scope

### 2.1 Goals

- One list of sidebar items, each with a name, an icon, what it opens and
  whether it shows by default.
- A **More** row that opens a menu with the hidden items and **Edit
  sidebar…**.
- An **Edit sidebar** dialog: a checkbox per item. A change shows in the
  sidebar right away and is kept across restarts.

### 2.2 Non-goals

- Reordering items (drag and drop): items keep the order of the list.
- Hiding **New chat** or **Search chats**: they stay where they are.
- Workspaces, pinned chats and the footer (Settings, theme) are unchanged.
- New pages: this feature only makes room for them.

## 3. User scenarios

### US-1 Hide an item I don't use (priority P1)

A person who never opens Usage wants it out of the sidebar, but still within
reach.

Acceptance scenarios:

1. **Given** Usage is shown, **when** they open More › Edit sidebar…,
   untick Usage and close the dialog, **then** the Usage row is gone from the
   sidebar and Usage is listed in the More menu.
2. **Given** Usage is hidden, **when** they pick Usage in the More menu,
   **then** the Usage page opens, the same as from the sidebar row.
3. **Given** Usage is hidden, **when** evo restarts, **then** it is still
   hidden.

### US-2 Show an item again (priority P1)

1. **Given** Usage is hidden, **when** they tick it in Edit sidebar, **then**
   the row comes back in its usual place, and the More menu no longer lists it.

### US-3 A new page added in a later version (priority P2)

1. **Given** a later version adds an item that is hidden by default, **when**
   the person updates, **then** it shows in the More menu and their earlier
   choices are kept.

### Edge cases

- Every item hidden: the sidebar shows only More; the menu lists every item.
- No item hidden: the More menu shows only Edit sidebar….
- A saved choice names an item that no longer exists: it is ignored.
- The page of a hidden item is open: the More row is marked as the active
  row, so the person can see where they are.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | The sidebar's page rows come from one list of items (name, icon, action, shown by default). Adding an item to the list is the only change a new page needs to appear in the sidebar, the More menu and Edit sidebar. | Given a new entry in the list, when the app runs, then it shows in all three places with no other code change. |
| FR-2 | Shown items appear as sidebar rows in list order, below New chat, followed by a More row. | Given Scheduler and Customize shown and Usage hidden, then the rows read New chat, Scheduler, Customize, More. |
| FR-3 | The More row opens a menu listing the hidden items in list order, a separator, and Edit sidebar…. Picking an item does what its row does. | Given Usage hidden, when More › Usage is picked, then the Usage page opens. |
| FR-4 | Edit sidebar… opens a dialog with one checkbox per item (icon and name). Ticking or unticking takes effect right away. A Done button and the close button close the dialog. | Given the dialog open, when Usage is unticked, then the Usage row disappears before the dialog is closed. |
| FR-5 | Choices are kept across restarts. An item the person never changed follows its default. | Given Usage hidden and evo restarted, then Usage is hidden. Given an item added later, then it follows its own default. |
| FR-6 | A row shows as active while its page is open. When the open page belongs to a hidden item, the More row shows as active. | Given Usage hidden and the Usage page open, then More is highlighted. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Looks and behaves like the other sidebar rows and menus (DESIGN.md: Geist, achromatic, one blue accent), in light and dark. | Check in the running app, both themes. |
| NFR-2 | Works with the keyboard: More, the menu and the dialog can be reached and used without a mouse. | Tab to More, Enter opens the menu, arrows move, Enter picks; the checkboxes toggle with Space. |
| NFR-3 | With no saved choice, the sidebar shows New chat, Usage and More; Scheduler and Customize are in the More menu. | Fresh profile: rows read New chat, Usage, More; More lists Scheduler and Customize. |

## 5. Success criteria

- SC-1 A person can hide or show any page row in two clicks from the sidebar,
  with no restart.
- SC-2 A new page is added to the sidebar by one entry in one list.

## 6. Clarifications

### 6.1 Open

- none

### 6.2 Resolved

- 2026-11-11 — Q: Which items are shown by default? → A: Usage only;
  Scheduler and Customize start in More (FR-5, NFR-3).
- 2026-11-11 — Q: Does New chat stay fixed? → A: Yes, fixed like Search chats
  (section 2.2).
- 2026-11-11 — Q: Is More always shown? → A: Yes, it holds Edit sidebar…
  (FR-2).

## 7. History

- 2026-11-11 — Draft.
- 2026-11-11 — Clarified and accepted (spec and plan); in progress.
