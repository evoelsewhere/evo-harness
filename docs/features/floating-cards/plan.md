# Floating cards for the agent's browser pages and terminals — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-08 |
| ADD sections | [4. Process view](../../architecture/README.md#4-process-view) |
| ADRs | none (a spike decides how a page is drawn; if it fails and the fallback needs a new dependency direction, an ADR is added) |

## 1. Context and constraints

- A browser page is a native child webview laid over a placeholder in the side
  panel (`features/browser/browser-panel.tsx` measures the placeholder and
  calls `browser_bounds`; `stores/browser-store.ts` shows one page per chat
  with `browser_visible`). A native webview always paints above the main
  webview's DOM, so a floating DOM card cannot sit over it, and the existing
  code hides the page while a menu or dialog lies over it.
- A terminal is an xterm instance in the DOM (`features/terminal/terminal-instance.ts`)
  fed by a pty in the shell; an instance's element can be moved between
  containers.
- Tabs already belong to a chat (session-panels), and the shell knows which
  pages a tool used (`browser/tabs.rs`, `DRIVEN_MS`) and emits `browser://created`
  for pages a tool opens.
- Windows has the DevTools protocol (`Page.startScreencast`,
  `Input.dispatch*`, `Emulation.setDeviceMetricsOverride`); other platforms
  do not (agent-browser FR-25), so a live browser card is Windows-only and
  elsewhere the chip alone shows.
- The agent's tools click and screenshot in the page's CSS pixels through
  DevTools; a browser-level zoom changes what those coordinates mean, so the
  viewport size must come from emulation, not from zoom.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | The shell adds a stage mode and two events (activity, driven); the webview draws cards. No agent decision moves. |
| Layers point down; library crates stay free of engine/desktop concerns | n/a | Desktop only. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | Shell ↔ webview events only. |
| Models come from models.dev; no hardcoded model facts | n/a | |
| Real engine in the app; settings reload live | ok | The card setting applies at once. |
| Untrusted input stays untrusted; permissions stay in the path | ok | A card forwards the person's own input; the agent's approvals stay in the chat (NFR-4); page text shown in a card is the page as the person sees it. |
| Desktop UI structure and `DESIGN.md` | ok | A new store holds card state, `features/cards/` holds components, no feature imports another's internals; Geist, achromatic, shadow-as-border. |

## 3. Design

### 3.1 Overview

```text
shell (Rust)                                   webview (React)
browser/tabs.rs  ── first tool use of a page ─▶ browser://driven {tab}   ┐
agent_tools/mod  ── a tool starts / ends ─────▶ agent://activity         ├▶ stores/cards-store.ts ─▶ features/cards/
browser.rs       ── browser_stage(tab, on) ◀── card / panel shows it     │      (per chat: placement, mode, order)
                 ── Page.screencastFrame ─────▶ browser://frame {tab,jpeg}┘   card-layer · card · chip-row
terminal/*       ── terminal://created (owner) ─▶ (existing event)            browser body (canvas) · terminal body (xterm)
```

### 3.2 How a page is drawn (decided by the spike, T1 – T3)

The agent's page must be 1280×800 whatever size it is shown at and a card must
sit under menus, which a native webview cannot do. The design therefore draws
the agent's pages **as a picture**:

- **Stage mode (recommended).** A page the agent is involved in is kept at an
  emulated 1280×800 viewport (`Emulation.setDeviceMetricsOverride`, scale 1) and
  its webview stays hidden. While a card or the panel shows it, the shell runs
  `Page.startScreencast` and emits the frames; the webview paints them on a
  `<canvas>` scaled to fit. The person's mouse, wheel and keys on the canvas are
  sent back as `Input.dispatchMouseEvent` / `dispatchKeyEvent` in 1280×800
  coordinates, the same coordinates the agent's tools use.
- **Fallback (if the spike fails).** If a hidden webview produces no frames or
  the latency is poor, keep the native webview: the card is a transparent frame
  with a native webview placed over its body, 16:10, scaled by emulation `scale`
  (or letterboxed), hidden while a menu or dialog covers it. Then FR-12 is met by
  hiding, as the panel does today, and FR-9 inside the panel by letterboxing.

The spike answers three questions on a real page (the local test page and a
video page): S1 frames arrive from a webview that is hidden (or kept
`visible` but off-screen) at ≥ 8 per second; S2 a click, wheel and typed text
sent from canvas coordinates land where the agent's tools would put them;
S3 CPU stays reasonable with three staged pages. The decision and numbers go
into section 6.

Pages the person opens are not staged: they stay native in the panel at their
real size (spec FR-9, clarification 3). A page is **agent-involved** from the
first tool use or when a tool creates it, for the rest of its life (sticky: the
shell emits `browser://driven` once per page).

### 3.3 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| shell | `src-tauri/src/browser.rs`, `browser/stage.rs` (new) | `browser_stage(tab, on)`: apply / clear the 1280×800 override and start / stop the screencast; ack frames, cap to the wanted rate, emit `browser://frame` to the owner window; stop when the tab closes. | FR-9, FR-10 |
| shell | `src-tauri/src/browser/tabs.rs`, `browser.rs` | Emit `browser://driven` the first time a tool marks a page. | FR-1, FR-15 |
| shell | `src-tauri/src/agent_tools/mod.rs` | Emit `agent://activity { chat, kind, id, tool, phase }` around every browser and terminal tool call (the tab it resolved to). | FR-2 |
| shell | `src-tauri/src/agent_tools/browser/viewport.rs` | A reset on a staged page returns to 1280×800, not to no override; a size set by the tool wins over the stage default. | FR-9 |
| desktop UI | `src/stores/cards-store.ts` (new) | Per chat: the cards (kind, id, placement x/y/w/h, mode `card` / `chip` / `panel` / `hidden`, order, activity line); the three-visible rule; positions kept for the session and across a reload; selectors for the showing chat. | FR-3, FR-4, FR-5, FR-13, FR-16 |
| desktop UI | `src/stores/ui-store.ts`, `features/settings/sections/` | The `agentCards` setting (default on) and its switch. | FR-14 |
| desktop UI | `src/stores/agent-events.ts` | Apply `browser://driven`, `agent://activity` and `terminal://created` (agent-owned) to the cards store. | FR-1, FR-2, FR-15 |
| desktop UI | `src/features/cards/card-layer.tsx`, `card.tsx`, `chip-row.tsx` (new) | The layer over the chat area; a card's bar, drag, resize, minimize, open-in-panel, hide; the chip row above the composer; no focus stealing; menus and dialogs above cards. | FR-2, FR-4, FR-5, FR-6, FR-11, FR-12 |
| desktop UI | `src/features/cards/browser-body.tsx` (new) | Canvas, frames, input forwarding, activity ring. | FR-7, FR-8, FR-9 |
| desktop UI | `src/features/cards/terminal-body.tsx` (new) | Moves the tab's xterm element into the card, fits it, view-only. | FR-7, FR-8 |
| desktop UI | `src/features/browser/browser-panel.tsx`, `stores/browser-store.ts` | A staged page shows in the panel as the same canvas, letterboxed; a tab in the panel has no card. | FR-6, FR-9 |
| desktop UI | `src/features/chat/chat-view.tsx` | Hosts the card layer and the chip row. | FR-1, FR-4 |

### 3.4 Interfaces

- Commands: `browser_stage(tab: String, on: bool, fps: Option<u32>)`.
- Events (shell → webview): `browser://frame { tab, seq, jpeg (base64), width, height }`,
  `browser://driven { tab, chat }`, `agent://activity { chat, kind: "browser" | "terminal", id, tool, phase: "start" | "end" }`.
- No engine protocol change.

### 3.5 Data

Card state is UI state: in memory, mirrored to `sessionStorage` per chat so a
window reload keeps positions (as `lib/tab-chats.ts` does for ownership). The
`agentCards` setting is an app setting beside the other UI settings.

### 3.6 Failure modes

| Failure | Behaviour |
| --- | --- |
| The webview or DevTools is unavailable (non-Windows) | The browser card is a chip only, with "Open in panel"; terminal cards work. |
| No frame for 2 s from a staged page | The card shows the last frame dimmed with "waiting for the page"; the screencast restarts once. |
| The tab closes while a card shows | Card and chip go (FR-16); the screencast stops. |
| A fourth card is wanted | It becomes a chip (FR-13). |
| The chat is not showing | Its stages keep running without frames (the screencast stops; the page keeps its 1280×800); frames resume when it is shown. |

## 4. Alternatives considered

| Option | Why not (or why) |
| --- | --- |
| Native webviews as cards over the chat | Cannot go under menus or dialogs; each card would need its own letterbox / emulation scale; kept as the fallback only. |
| Browser zoom to fit | Changes the coordinate meaning for the agent's DevTools clicks; emulation is exact. |
| An always-on-top OS window per tab | Non-goal for now; needs reparenting webviews between windows and is harder to keep per session. |
| `Page.captureScreenshot` polling instead of a screencast | Works on hidden pages but costs a full capture per frame; used only if the screencast fails the spike. |

## 5. Phases (to become tasks)

1. **Spike** — the three questions; decide stage mode or fallback.
2. **Shell** — stage mode and frames, driven and activity events, viewport reset.
3. **Cards store and layer** — state, setting, layer, cards, chips, events.
4. **Bodies** — browser canvas with input, terminal body, panel canvas.
5. **Verify and docs** — each requirement in the running app, README map.

## 6. As-built notes

n/a — empty until the status is `In progress`.
