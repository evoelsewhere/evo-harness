# Sidebar items: a More menu and Edit sidebar — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-11 |
| ADD sections | [Desktop UI](../../architecture/README.md) |
| ADRs | none |

## 1. Context and constraints

- `apps/desktop/src/features/shell/app-sidebar.tsx` renders New chat,
  Scheduler, Usage and Customize as four hand-written `SidebarMenuItem`s. Each
  one calls a `useUiStore` action (`openScheduler`, `openUsage`,
  `openCustomize`) and is active from a `useUiStore` flag (`schedulerOpen`,
  `usageOpen`, `customizeOpen`).
- App-wide UI choices are kept in the app database's settings, through
  `db.settings()` / `db.setSetting(key, value)` (`src/lib/db.ts`), for
  example `ui.lastChat` in `stores/session-store.ts`. They are not in
  `localStorage`.
- Primitives available: `components/ui/dropdown-menu.tsx`, `dialog.tsx`,
  `sidebar.tsx`. There is no checkbox component yet.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | n/a | UI only. |
| Layers point down; library crates stay free of engine/desktop concerns | n/a | No crate changes. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | No protocol change. |
| Models come from models.dev; no hardcoded model facts | n/a | |
| Real engine in the app; settings and credentials reload live | ok | The choice applies at once; no restart. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | n/a | Only the person's own choice is stored. |
| Desktop UI structure and `DESIGN.md` | ok | Components read a store through selectors. `features/shell` owns the sidebar. The new checkbox in `components/ui/` is generated shadcn code. Visual style is that of the existing rows and menus. |

## 3. Design

### 3.1 Overview

```text
sidebar-items.ts (list)  ──►  ui-store: sidebarShown (map id → bool, saved)
        │                               │
        ▼                               ▼
app-sidebar.tsx: shown rows · More ▸ menu (hidden items · Edit sidebar…)
                                         │
                                         ▼
                          edit-sidebar-dialog.tsx (checkbox per item)
```

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| desktop UI | `apps/desktop/src/features/shell/sidebar-items.ts` | The list: `{ id, label, icon, defaultShown, open(), isActive(ui) }` for Scheduler, Usage, Customize. Pure helper `isShown(item, saved)`. | FR-1, FR-5 |
| desktop UI | `apps/desktop/src/stores/ui-store.ts` | `sidebarShown: Record<string, boolean>`, `setSidebarShown(id, shown)`, loaded at start from setting `ui.sidebar.items` and saved on change. | FR-4, FR-5 |
| desktop UI | `apps/desktop/src/features/shell/app-sidebar.tsx` | Renders the shown items, then the More row with a `DropdownMenu` (hidden items, separator, Edit sidebar…). More is active when a hidden item's page is open. | FR-2, FR-3, FR-6 |
| desktop UI | `apps/desktop/src/features/shell/edit-sidebar-dialog.tsx` | `Dialog` with one checkbox row per item, and Done. | FR-4 |
| desktop UI | `apps/desktop/src/components/ui/checkbox.tsx` | shadcn checkbox (base-ui), generated. | FR-4, NFR-2 |
| docs | `apps/desktop/README.md` | UI map: More menu and Edit sidebar. | — |

### 3.3 Interfaces

```ts
// features/shell/sidebar-items.ts
export interface SidebarItem {
  id: string;                 // stable; used in the saved setting
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  defaultShown: boolean;
  open(): void;               // what the row and the menu entry do
  isActive(ui: UiState): boolean; // is this item's page open (a ui-store selector)
}
export const SIDEBAR_ITEMS: SidebarItem[];
export function isShown(item: SidebarItem, saved: Record<string, boolean>): boolean;
```

No Tauri, engine or protocol interface changes.

### 3.4 Data and storage

The app setting `ui.sidebar.items` (app database, `app_settings`) holds a JSON
object `{ "<item id>": true | false }` with only the choices the person made.
A missing id follows the item's `defaultShown`, and unknown ids are ignored.
No migration is needed, since settings are free-form key/value.

### 3.5 Behaviour

- Start: the ui-store reads `ui.sidebar.items` with the other settings.
  Until it loads, items follow their defaults (no flicker for default users).
- Toggling a checkbox updates `sidebarShown` at once (the sidebar re-renders)
  and writes the setting. A failed write is logged, and the change still
  holds for the session.
- More menu: hidden items in list order, a separator (only when there is at
  least one hidden item), then Edit sidebar…. Picking Edit sidebar… closes
  the menu and opens the dialog.

### 3.6 Errors and edge cases

- Malformed saved value (not an object, or non-boolean values): treated as
  no choices made.
- Every item hidden: only More is shown under New chat.
- A chat window (`chat-*`) has no sidebar, so it is not affected.

### 3.7 Security and trust

n/a — a local UI preference. No untrusted input.

### 3.8 Configuration

| Key | Type | Default | Where | Applies |
| --- | --- | --- | --- | --- |
| `ui.sidebar.items` | `{ [id]: boolean }` | `{}` (every item follows its default) | app database settings | live |

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| `localStorage` | Lost when WebView2's data is cleared, and other UI choices already live in the app database. |
| Store the list of shown ids | An item added later would need a migration to decide whether it shows. A map with per-item defaults handles that. |
| Toggle rows with check marks inside the menu (no dialog) | Harder to scan, and it closes on each pick. A dialog matches the requested design and leaves room for reordering later. |

## 5. As-built notes

- `components/ui/checkbox.tsx` was written by hand in the style of the other
  generated components (on `@base-ui/react/checkbox`, accent like `switch`),
  because `shadcn add` could not install in this environment.
- The sidebar loads `ui.sidebar.items` itself when it mounts
  (`AppSidebar` calls `loadSidebarItems`), instead of from the engine store's
  start-up sequence.
