# Floating cards for the agent's browser pages — Plan

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

- **Stage mode (recommended).** A page the agent is involved in gets a webview of
  the real size 1280×800 (so its viewport is the desktop size with no emulation, and
  a `browser_viewport` reset returns to it) and stays visible but parked off-screen (a hidden webview does not render, see the spike). While a card or the panel shows it, the shell runs
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
| shell | `src-tauri/src/browser.rs`, `browser/stage.rs` (new) | `browser_stage(tab, on, fps)`: park a visible 1280×800 webview off-screen (or take it off the stage) and start / change / stop the screencast; ack frames, cap to the wanted rate, emit `browser://frame` to the owner window; stop when the tab closes. | FR-9, FR-10 |
| shell | `src-tauri/src/browser/tabs.rs`, `browser.rs` | Emit `browser://driven` the first time a tool marks a page. | FR-1, FR-15 |
| shell | `src-tauri/src/agent_tools/mod.rs` | Emit `agent://activity { chat, kind, id, tool, phase }` around every browser and terminal tool call (the tab it resolved to). | FR-2 |
| shell | `src-tauri/src/agent_tools/browser/viewport.rs` | No change: the stage's real 1280×800 size is what a reset returns to; a size set by the tool is an override that wins. | FR-9 |
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

Recorded 2026-11-08.

- **Spike result: stage mode works.** Run on Windows (WebView2) with the animated
  test page (a full-canvas 60 fps animation, the worst case):
  - A webview made with `visible: false` does **not** render: its
    `requestAnimationFrame` stalled after the first frame and no screencast frame
    arrived (S1 failed for that placement).
  - A webview that is *visible* but parked off-screen (bounds at −5000, −5000) or
    1×1 renders normally: the page ran at ~60 fps and `Page.startScreencast`
    delivered ~50 frames/s (one frame per page frame at `everyNthFrame` 1, JPEG
    quality 60, about 16 KB per frame; ~12 fps at `everyNthFrame` 5). So the stage is
    "visible, off-screen", not "hidden".
  - S2: with `Emulation.setDeviceMetricsOverride` 1280×800 the page sees 1280×800
    whatever the webview's bounds are (400×300, then 640×400: still 1280×800), and
    `Input.dispatchMouseEvent` at 1280×800 coordinates hit the right element (button
    click, wheel scroll 400 px), `Input.dispatchKeyEvent` with `text` and
    `Input.insertText` typed into a focused field (the field has to be clicked first;
    no focus emulation was needed).
  - S3: three staged pages at ~12 fps each (a worst-case animation, plus the spike's
    own JavaScript polling) used about 95 % of one core against about 50 % for the
    same pages animating with no screencast; idle after: about 1 %. Real pages are
    mostly static, the production path acks frames in Rust, and only pages a card or
    the panel shows are streamed.
- **Consequence for FR-10.** A page parked with `visible: false` is throttled
  (animations and rAF-driven pages stall), which is what happens today to a page
  whose tab is not the shown one. Agent pages should therefore be parked
  off-screen and visible, not hidden, whether or not a card shows them; the stage
  mode is the same call without frames.
- Decision: stage mode, no native-webview fallback needed.

Phase 2 (shell), 2026-11-08:

- **Real size, not emulation.** The staged webview is 1280×800 itself, parked at
  (−10000, −10000), so no `Emulation` call is made and `browser_viewport` needed no
  change. Checked live: the page sees 1280×800 on the stage, a mobile override
  changes it and `clearDeviceMetricsOverride` (what a reset sends) returns it to
  1280×800.
- **`browser_stage` is opt-in.** Nothing stages a page by itself yet: the window asks
  (phase 4), because a staged page ignores `browser_bounds` / `browser_visible` (the
  panel's placement calls are no-ops for it) and would show nothing in the panel
  until the canvas exists. Checked live: before staging a hidden page's
  `requestAnimationFrame` advanced 0 per second, staged 60; with `fps` 10 about 9.8
  frames per second of 1280×800 (16 KB); `fps` 5 gave about 4.3; no `fps` stops the
  frames while the page keeps running; the panel's calls left the page where it was.
- **Frames bypass the event ring.** `Page.screencastFrame` events are acknowledged at
  once and sent to the owner window as `browser://frame { tab, seq, data, width, height }`
  only while the page streams; they are not stored in the ring (a frame is 16 KB and
  more).
- **Events.** `browser://driven { tab, chat }` once per page on the first tool use;
  `agent://activity { chat, kind, tool, requestId, phase, tab, args }` at the start
  and end of every browser and terminal tool call (`agent_tools/activity.rs`); `tab`
  is the browser tab as the tool resolves it (null before `browser_navigate` has
  opened one, filled in at the end), and a terminal call's own `tabId`.

Phase 3 (cards store and layer), 2026-11-08:

- **Hide is minimize.** The spec lists a minimize and a hide button, but both leave
  the tab as a chip, so the card has one: Minimize. Open in panel is the other.
- **An agent's shell no longer opens the side panel** (`terminal-store.ts`, it did
  before): the tab shows as a card, or as a chip when the setting is off or three
  cards are already shown. A page the agent opens still opens the Browser panel
  until phase 4, because `show_browser` (`agent_tools/browser/driver.rs`) asks for
  the panel to place a native page; once pages are staged in phase 4 that call
  returns at once for a staged page and the panel stays closed.
- **`stores/cards-sync.ts`** removes a card when its page or shell closes. It is a
  separate module (the card layer imports it) so `cards-store` and `agent-events`
  do not load the browser and terminal stores, which touch the DOM when they load.
- **A card of a tab in the panel is not drawn**, derived from the panel state rather
  than stored (`features/cards/use-cards.ts`), so showing the tab elsewhere in the
  panel needs no bookkeeping; "open in panel" minimizes the card and opens the tab.
- **Resize** keeps a page card 16:10 after the layer clamps its width.
- A shell card's bar shows the shell's name (as the panel's tab does), not the title
  the shell sets.
- Checked in the app (2026-11-08): an agent chat with the panel not showing the page
  or shell got a card each, cascading from the top right, with "Opening …" and
  "Running echo card-test" on their bars; minimize → chip → click → card; open in
  panel made the card go and the panel show that tab; a drag moved the card and a
  corner drag resized it at 16:10; a new chat showed none and the first chat's
  cards came back in place; with the setting off a tab became a chip.

Phase 4 (bodies), 2026-11-08:

- **Pages are staged by the shell, not asked for by the window.** A page a tool
  creates is parked on the stage at once (`create_for_chat`), and a page the person
  opened is parked the first time a tool uses it (`browser://driven` handler), on
  Windows. `show_browser` therefore returns at once for a staged page (and for a
  page about to be made), so an agent never opens the side panel for a page. A
  person's page the agent starts using switches from the native view to the
  picture in the panel at that moment (its viewport becomes 1280×800).
- **Frames only for what is shown.** `StageController` (in the card layer) asks the
  shell for ~10 fps for the shown cards' pages and for the page the panel shows
  when it is one the agent uses; everything else stays parked without frames.
  Checked: 19 frames in 2 s with a card shown, 0 with every card minimized, 19 in the
  panel after "open in panel".
- **The picture** is a `<canvas>` (`browser-canvas.tsx`) painting the newest frame,
  scaled to fit and letterboxed, in the card and in the panel. Mouse, wheel, typing
  and paste are sent as `Input.dispatch*` in page coordinates (`input.ts`, tested).
  Checked on the card's picture with real input events: a click on the button set the
  page's title, typing filled the field, a wheel turn scrolled the page by 430 px.
  The page's mouse cursor shape is not shown.
- **Terminal card**: the tab's own xterm element is moved into the card and fitted
  (`terminal-body.tsx`), view-only (`pointer-events: none`); the panel takes it back
  when the tab is opened there. Its output showed `echo p4-card` and its result.
- **Busy ring.** The shell's activity events set `busy` on a card while a tool works on
  its tab; the card gets a ring.
- **Off Windows** a staged page does not exist: the browser card says a live view needs
  Windows and the panel keeps its native page (NFR-3); not run here.
- **Cards cascade tightly** (24 px across, 32 px down): a second card covers most of the
  first until it is dragged or minimized.

Phase 5 and the virtual cursor (FR-17), 2026-11-08:

- **Virtual cursor.** Every DevTools mouse event an agent tool sends goes through
  `Page::cdp`, which now first tells the window (`browser://pointer { tab, chat, kind,
  x, y }`, kind move / down / up / wheel; `browser/stage.rs::pointer_payload`). The
  window keeps the newest pointer per page (`stage-stream.ts`) and the canvas draws
  `AgentCursor` over the picture: a small symmetric arrowhead (dark, light blue outline and faint glow, softly rounded corners) that glides (150 ms) to the mapped point
  (`toDisplayPoint`, the inverse of `toPagePoint`), a grey ring that spreads at each
  press, hidden four seconds after the last event. Reduced motion is handled by the
  app's global rule. Checked live: a click on the test page showed the arrow and
  ring over the button with "Clicking ref_1" on the bar.
- **Focus (FR-11).** With approvals on, a pending approval replaces the composer
  (existing behaviour), so the composer loses focus then; a card appearing does not
  take it (checked in Approve-for-me mode: the composer stayed focused).
- Checked in the running app in two chats: FR-5 (a 710×432 layer kept the card inside),
  FR-8 (a page marker and a typed value survived card → chip → card → panel),
  FR-12 (an open menu overlapping a card was on top), FR-13 (three cards and a chip),
  FR-15 (a page opened by the person got no card), FR-16 (closing the tab removed
  card and chip), NFR-1 (about 10 frames a second during a drag, the card followed the
  pointer exactly), NFR-2 (one webview per tab), NFR-4 (an approval showed in the chat
  and in no card).

Terminal cards dropped, 2026-11-08 (the person's request):

- A card is only ever a page's. `CardKind` is `"browser"`; the terminal body, the
  terminal branches of the store, the sync, the title and panel checks, the chip icon
  and the activity words are gone, and a saved card of a shell from an earlier build
  is dropped when the store loads. The shell still sends `agent://activity` for
  terminal tools (the window ignores it) and an agent's shell tab still does not open
  the side panel (`terminal-store.ts`): it is in the Terminal tool when opened.
- Checked in the app: `terminal_run` from an agent chat ran with the panel closed, no
  card and no chip, and the tab `t1` was running in the terminal store.
