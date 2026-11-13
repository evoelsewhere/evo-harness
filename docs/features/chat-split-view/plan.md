# Split view: several chats side by side in one window — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Status | In progress |
| Updated | 2026-11-13 |

## 1. Context and constraints

- **One chat per window today.** `stores/engine-store.ts` keeps one
  `activeId`. The active chat's runtime is mirrored into top-level fields
  (`status`, `transcript`, `state`, …; `mirror()`), and about 30 components
  read those fields or `activeId`. Other chats keep their `runtimes[chat]`
  while the window holds them: `activate()` attaches the new chat and
  does not detach the previous one.
- **Engines belong to the shell** (ADR 0018). A window is a viewer
  (`engine_attach` / `engine_detach`). The supervisor never trims an engine
  that has a viewer (`supervisor.rs`, `trim`). A pane's chat is therefore
  safe as long as the window keeps it attached (NFR-2).
- **Layout.** `app/app.tsx` has a `SplitLayout` (chat card plus side panel,
  for windows of at least 900 px) and an `OverlayLayout` (narrow windows).
  The chat card is the workspace toolbar over `features/chat/chat-view.tsx`.
- **Chat windows.** `chat_window_open`, `chat_window_focus` and
  `chat_window_close` live in `src-tauri/src/window.rs`. `selectSession`
  redirects to a chat's own window (`focusChatWindow`).
- **Menus and keys.** `features/shell/chat-menu.tsx` builds a chat's menu
  (with **New window**) for the sidebar and the header.
  `app/use-app-bootstrap.ts` holds the global shortcuts.
- **Motion.** `index.css` has motion tokens and `prefers-reduced-motion`
  rules. There is no animation or drag library. WebView2 (Chromium) and
  WebKit support the Web Animations API and `linear()` easing, which can
  draw a spring curve (NFR-6).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | UI only; no agent decisions. |
| Layers point down; library crates stay free of engine/desktop concerns | n/a | No crate changes. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | No protocol change. |
| Models come from models.dev; no hardcoded model facts | n/a | |
| Real engine in the app; settings and credentials reload live | ok | Each pane views a real engine through the existing attach. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Approvals are answered in the pane of the chat that asked, through the same store actions. |
| Desktop UI structure and `DESIGN.md` | ok, one note | Components keep reading through store selectors, and only `engine-store` talks to the client. DESIGN.md rules out `transform` and `opacity` changes on interactive controls (buttons, links). Pane motion animates containers, not controls, and the spec asks for it (FR-9). It uses the motion tokens' durations. |

## 3. Design

### 3.1 Overview

```text
 chat card (workspace toolbar follows the focused pane)
 ┌───────────────────────────────┬─┬───────────────────────────────┐
 │ PaneHeader  A  ● ×  ⧉          │┃│ PaneHeader  B  ● ×  ⧉          │
 │ <PaneChat chat=A>              │┃│ <PaneChat chat=B>   (focused)  │
 │   ChatView (reads A)           │┃│   ChatView (reads B = active)  │
 └───────────────────────────────┴─┴───────────────────────────────┘
          split-store: { panes: [A, B], focus: 1, ratio: .5 }
          engine-store: activeId = B   (always the focused pane's chat)
```

**The focused pane's chat is `activeId`.** Everything that follows the
active chat today follows the focused pane unchanged: the side panel, the
title bar, the sidebar highlight, commands, shortcuts and store actions
(FR-4).

The other pane renders its own chat. Its components read through
`usePaneEngine`, a selector hook that sees the store as if that pane's
chat were active. A `pointerdown` or `focusin` inside a pane makes it the
focused pane at once, before the click acts (`focusPane`). So the
existing actions, which act on `activeId`, always act on the pane the
person is using (FR-3).

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| desktop UI | `src/stores/split-store.ts`, `split-rules.ts` (new) | `{ panes: [string, string] \| null, focus: 0 \| 1, ratio }`, persisted per window. Actions: `open(chat, side)`, `replace(index, chat)`, `swap()`, `close(index)`, `setRatio`, `focusIndex`. Follows `activeId`: a chat picked elsewhere replaces the focused pane, or moves the focus when the other pane shows it. Closes a pane whose chat is archived or deleted. | FR-1, FR-4, FR-6, FR-7, FR-10 – FR-12 |
| desktop UI | `src/stores/engine-store.ts`, `pane-view.ts` (new) | `focusPane(chat)`: switches `activeId` synchronously to a chat whose runtime is held, so a click acts on the pane it lands in. `showChat(chat)`: attaches, or launches attached, without activating; this is the non-focused pane's half of `activate`. `usePaneEngine(selector)` and `PaneChatContext`. When the active chat closes while two panes are open, the other pane is focused instead of starting a new chat. | FR-3, FR-4, FR-12, NFR-2 |
| desktop UI | `src/features/split/` (new) | `split-area.tsx`: the panes, the divider (pointer and keyboard), the narrow-window switcher, and pane motion. `pane-header.tsx`: title, working dot, close, open in window, drag handle, focus ring. `drop-overlay.tsx`: the gliding preview and the drag ghost. `drop-geometry.ts`: payloads, drop targets and preview boxes (pure). `drag.ts`: the pointer drag controller. `motion.ts`: the spring `linear()` curve, `animatePane` and the reduced-motion check. | FR-1, FR-2, FR-5 – FR-7, FR-9, FR-13, NFR-1, NFR-3 – NFR-5 |
| desktop UI | `src/app/app.tsx` | The chat card's body renders `SplitArea` in place of `ChatView`. | FR-1, NFR-3 |
| desktop UI | `src/features/chat/*`, `src/features/cards/*`, `src/components/ai-elements/session-proposal.tsx`, `agent-run.tsx` | Render-time reads move from `useEngineStore` to `usePaneEngine`, and `s.activeId` to the pane's chat. Actions stay as they are. | FR-3 |
| desktop UI | `src/features/shell/sidebar-chats.tsx` | Chat rows are `draggable` and carry the chat-drag payload. | FR-2, Q3 |
| desktop UI | `src/features/shell/chat-menu.tsx` | **Split view** next to **New window**. | FR-8 |
| desktop UI | `src/components/ai-elements/session-proposal.tsx` | A started session's card: **Open beside** (`openBeside`), and the card drags like a sidebar row. | FR-14 |
| desktop UI | `src/app/use-app-bootstrap.ts` | Ctrl/⌘+1 and Ctrl/⌘+2 focus the left or right pane; Ctrl/⌘+\ closes the focused pane. | FR-8 |
| desktop UI | `apps/desktop/README.md` | UI map. | — |

No shell (`src-tauri`) change. FR-11 uses the existing
`chat_window_close`.

### 3.3 Interfaces

```ts
// split-store.ts
type Side = "left" | "right";
interface SplitState {
  panes: [string, string] | null; // null: one pane, the active chat
  focus: 0 | 1;
  ratio: number;                  // left pane's share, kept within the minimum widths
  open(chat: string, side: Side): Promise<void>;   // FR-1, FR-11
  replace(index: 0 | 1, chat: string): Promise<void>;
  swap(): void;                                     // FR-7
  close(index: 0 | 1): Promise<void>;               // FR-6
  focusIndex(index: 0 | 1): void;                   // FR-4, FR-8
  setRatio(ratio: number): void;                    // FR-5
}

// engine-store.ts
export const PaneChatContext: React.Context<string | null>;
export function usePaneEngine<T>(selector: (s: EngineStore) => T): T;
focusPane(chat: string): void;          // synchronous when the runtime is held
showChat(chat: string): Promise<void>;  // attach or launch, keep activeId

// drag payloads (dataTransfer)
"application/x-evo-chat": chat id   // a sidebar row
"application/x-evo-pane": "0" | "1" // a pane header
```

`usePaneEngine` returns `selector(s)` when the pane's chat is `activeId`
(and outside any pane). Otherwise it returns `selector(view(s, chat))`,
where `view` is the store with `activeId` set to the chat and
`mirror(runtimes[chat])` spread over it. `view` is memoised per
(store snapshot, chat) in a `WeakMap`, so one store update spreads once per
pane, not once per selector. `mirror` returns a shared empty `state`
object, so selectors stay stable.

### 3.4 Data and storage

- `split-store` is persisted in the window's local storage (key
  `evo-split`, main window only): `{ panes, focus, ratio }`. At
  bootstrap, once the active chat is restored, a saved pair whose chats
  still exist and are not archived is reopened: `showChat` for the other
  pane's chat. Otherwise the saved pair is dropped (FR-10).
- No database or shell state.

### 3.5 Behaviour

- **Open** (drop with one pane, or **Split view**): the chat's own window
  is closed if it has one (FR-11), then `showChat(chat)` runs, the panes
  become `[current, chat]` or `[chat, current]`, and the new pane is
  focused (`focusPane`).
- **Replace** (drop on a pane while two are open): the pane's old chat
  stays attached until the window lets it go, as when switching chats
  today.
- **Follow `activeId`**: a plain sidebar click calls `selectSession(X)`,
  and the subscription updates the panes. If the other pane shows X, the
  focus moves there; otherwise X replaces the focused pane.
- **Close**: the other pane's chat becomes the only one and is focused.
  The closed chat stays attached until the window lets it go, so its work
  goes on (FR-6).
- **Swap**: the panes swap and the ratio mirrors.
- **Divider**: pointer capture. While it is dragged, the divider line and
  a veil over each pane are moved by direct style writes, with no React
  render and no layout of the chats. On release, the ratio is set once
  and the veils fade over 180 ms. Arrow keys move it by 2 % (10 % with
  Shift), Home/End jump to the limits, and a double-click centres it.
  Each pane is at least 360 px (FR-5, NFR-1).
- **Narrow (FR-13)**: when the area is under 2 × 360 px plus the divider,
  only the focused pane renders. Its header shows a two-chat switcher. The
  other pane is not rendered but stays attached.
- **Motion (FR-9)**, with the Web Animations API (no React re-render per
  frame):
  - The spring curve is `linear(0, 0.2 6%, 0.6 14%, 0.95 26%, 1.05 36%,
    1.02 48%, 1)` (an underdamped spring, about 1.05 overshoot), over
    360–400 ms.
  - Open: the new pane slides 24 px in from its side and fades in (0 → 1).
    The staying pane is laid out at its final width at once and revealed
    from its old width with `clip-path: inset()`. Text reflows once, not on
    every frame (NFR-1).
  - Close: the leaving pane is kept for the length of the animation and
    shrinks out (scale .97, fade). The remaining pane is revealed to its
    full width as above.
  - Swap: FLIP. Each pane is measured before and after, and translated back
    from its old x to its new one.
  - Drop preview: the preview box's `inset` and `opacity` transition with
    the same curve, so it glides between halves.
  - With reduced motion on: no animation, except a 120 ms fade on the
    drop preview.

### 3.6 Errors and edge cases

- `showChat` fails (the engine cannot start): the pane shows the chat's
  usual engine error banner. The layout stays.
- A drop of a chat that is a draft (no row yet): not offered. Drafts are
  not draggable.
- An incognito chat can be dragged and works like any chat.
- Scheduler, Usage or Customize opens: the page replaces the chat card as
  today. The split state is untouched and shows again when the page
  closes.
- A chat window (`chatWindowId`) has no split: the split store is inert
  there and the menu item is hidden.
- The engine store's `closeChat` on the focused pane's chat with two
  panes: the pane closes and the other is focused; no new chat starts
  (FR-12).

### 3.7 Security and trust

n/a — no new input crosses a trust boundary. Drag payloads are chat ids
read only by this window. A payload from outside the app is ignored, as
only our own MIME types are accepted.

### 3.8 Configuration

n/a — no settings. The layout is window state, stored like the side
panel's (local storage).

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Make every store action take a chat id and remove `activeId` | It touches about 100 call sites, and the active-chat model is right for everything that follows focus. Focus-on-pointerdown gives the same correctness with far less change. |
| Two engine-store instances, one per pane | Both would talk to the same engine client and the same events, and they would fight over the same chat-scoped stores (panels, cards). |
| The `motion` library | Two side-by-side panes need little layout animation. WAAPI with a `linear()` spring gives the iOS-like feel with no dependency. The user had no preference (spec Q2). |
| Animating `width`/`flex-basis` | The thread would reflow on every frame, and that drops frames with long chats (NFR-1). A clip reveal reflows once. |
| `react-resizable-panels` for the divider | One divider between two panes is a few dozen lines, and a dependency is not worth it. |

## 5. As-built notes

- **Pointer drag, not HTML drag and drop.** The window keeps native file
  drops on for the composer, and on Windows that keeps the webview's own
  `dragover`/`drop` from firing. `drag.ts` follows `pointermove` from a
  press on a sidebar row or a pane header. It starts after 6 px, cancels
  on Esc, and swallows the click that ends a drag. Only drags started in
  the app are known, so the MIME types in 3.3 do not exist.
- **Pure parts split out.** The layout rules are in `split-rules.ts`, the
  drop geometry in `drop-geometry.ts` and the pane view in `pane-view.ts`.
  The tests import them without Tauri. A swap target names the place it
  goes to (`{ kind: "swap", into }`).
- **Stable pane slots.** Panes are keyed by slot (`a`/`b`), not by chat,
  and one pane renders the same tree as two (`PaneChatContext` set to
  `null`). A chat view stays mounted, with its scroll and its composer
  draft, when the area splits, a pane swaps sides, or a pane's chat
  changes. A first version wrapped the pane only when split: the chat
  view remounted, and the focus jumped to the other pane.
- **Composers.** With two panes, only the focused pane's composer takes
  drafts put into "the composer" from elsewhere and native file drops.
  The "chat is loading" skeleton shows only in the focused pane.
- **Divider (FR-5 changed).** Laying both chats out on every frame of a
  drag ran at about 36 fps. While dragged, the divider now moves veils
  with direct style writes, and the ratio is applied once on release
  (57 fps). The spec was updated first.
- **Motion.** The spring is a 13-stop `linear()` curve over 380 ms. Enter
  slides 40 px with a fade. Leave scales to .97 and fades over 200 ms
  before the store closes the pane. FLIP covers a moved pane (translate)
  and a grown pane (clip reveal). Ctrl+\ closes a pane without the leave
  animation.
- **Restore.** `startSplit()` runs at bootstrap, before the last chat is
  selected, so the saved pair follows that selection. `restoreSplit()`
  then attaches the other pane's chat.
- **Keyboard focus.** A focus event inside the other pane focuses it, as
  a pointer press does. No chat view focuses itself on mount, so a split
  does not move the focus by itself.
