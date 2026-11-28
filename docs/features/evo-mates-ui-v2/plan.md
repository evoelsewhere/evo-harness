# Evo Mates UI v2 — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-28 |
| ADD sections | [Desktop UI](../../architecture/README.md) (front ends, stores) |
| ADRs | [0028 two front ends in one app process](../../architecture/decisions/0028-two-front-ends-in-one-app-process.md) (unchanged) |

## 1. Context and constraints

- The Evo Mates window is `apps/desktop/src/mates/mates-app.tsx`; its views
  live in `apps/desktop/src/features/mates/` (sidebar `mates-shell.tsx`,
  `roster.tsx`; mate conversation `conversation.tsx`, `thread.tsx`,
  `thread-rows.ts`, `composer.tsx`; channel `channel-view.tsx`,
  `channel-rows.ts`; profile `mate-panel.tsx`; home `today.tsx`,
  `today-sections.ts`; `gallery.tsx`, `files-page.tsx`).
- State comes from existing stores: `mates-store` (mates, channels, view,
  panel), `session-store` (chat rows: unread, preview, `work` = working /
  waiting), `engine-store` (the shown chat's transcript: turns with tool
  parts, busy, queued), `channel-store` (channel entries). Faces are
  `components/mate-face/mate-face.tsx`.
- Generated shadcn parts already in the app and used here: `command`
  (cmdk), `popover`, `dropdown-menu`, `context-menu`, `tooltip`, `kbd`,
  `dialog`, `tabs`.
- The shell already answers everything the spec needs: `mates_today`
  (replies and cost per mate), `mates_agenda`, `mates_files`,
  `engine_questions` (open questions of a chat: kind and title), the
  `work` list. No new command is needed.
- Verification uses the fake-IPC harness in `scripts/readme-shots/`
  (Playwright on Vite :1420), extended with sample data for every state.
- DESIGN.md: achromatic, shadow-as-border, 400/500/600, status colours at
  dot size only; the Evo Mates layer allows mate hues on faces, plates,
  bubbles and the mention chip, and now the profile hero band (spec 6.2).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | UI only; no decision moves into the UI. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | No crate change. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | No protocol change (NFR-4). |
| Models come from models.dev; no hardcoded model facts | ok | The Overview shows the model name the engine store already has. |
| Real engine in the app; settings and credentials reload live | ok | Unchanged; the harness's fake IPC is for screenshots only. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Approval cards are the same Harness panels inside a new frame; find marks text only (no HTML injection: matches are split text nodes). |
| Desktop UI structure and `DESIGN.md` | ok | Components read stores through selectors; `features/mates` uses `components/*` and the public `features/chat` index only; DESIGN.md's Evo Mates layer gains the hero band and the presence mark (dot size). |

## 3. Design

### 3.1 Overview

```text
MatesApp (mates-app.tsx)
├─ MatesSidebar  ── full (280) | rail (68)        ← mates-ui-store.sidebar + width rule  FR-1–FR-6
│   ├─ HomeRows (Today·badge, Files)
│   ├─ Section "Pinned" / "Mates" / "Channels"   (foldable, counts)
│   │    └─ MateRow / ChannelRow (presence, bold unread, count, ⋯)
│   └─ Footer
├─ Main
│   ├─ TodayPage (greeting, counters, team strip, Needs you inbox…)   FR-25, FR-26
│   ├─ MatePane: MateHeader · FindBar · MateConversation · MatePanel  FR-9–FR-18, FR-22–FR-24
│   ├─ ChannelPane: ChannelHeader · FindBar · ChannelView            FR-19–FR-21
│   ├─ Gallery (steps strip) · FilesPage (by day, grid/list)          FR-28, FR-30
├─ QuickSwitcher (Ctrl+K)      FR-7
├─ ShortcutsSheet (Ctrl+/)     FR-8
└─ useMatesKeys (one keyboard hook for the window)  FR-8
```

The refactor keeps every store and shell call and rebuilds the views on
three new shared pieces: a **presence** model (one function from a mate's
chat state to a mark and a word), a **window preferences** store, and a
**keyboard** hook. Pure logic goes in `.ts` files with `bun test` tests;
views stay thin.

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| desktop UI | `src/features/mates/presence.ts` (+test) | `presenceOf(mood, paused)` → `{mark, word, tone}`; `usePresence(mate)` hook (work, busy, queued, failed, paused); `channelPresence` (who works) | FR-4, FR-5, FR-9, FR-19 |
| desktop UI | `src/features/mates/presence-face.tsx` | `PresenceFace`: `MateFace` + corner mark (dot, ring pulse, hollow), `aria-label` with the word | FR-5, NFR-3, NFR-6 |
| desktop UI | `src/stores/mates-ui-store.ts` | Persisted (`localStorage` `evo.mates-ui`): `sidebarFolded`, `foldedSections`, `filesView`; actions | FR-1, FR-3, FR-30 |
| desktop UI | `src/features/mates/sidebar-layout.ts` (+test) | `sidebarForm(width, folded)` → `full` / `rail`; `panelDocks(width)`; conversation order and next-unread helpers | FR-1, FR-8, FR-31 |
| desktop UI | `src/features/mates/mates-shell.tsx` | Rewritten sidebar: full and rail forms, foldable sections, home rows with badges, fold button, footer | FR-1 – FR-3 |
| desktop UI | `src/features/mates/roster.tsx` | Rows and rail tiles with presence, bold unread and counts, plain previews, ⋯ button, Pause and Mute in the menu; channel rows | FR-2, FR-4 – FR-6 |
| desktop UI | `src/features/mates/plain-preview.ts` (+test) | Markdown → one plain line for previews | FR-4 |
| desktop UI | `src/features/mates/quick-switcher.tsx`, `switcher-items.ts` (+test) | cmdk dialog: recents, mates, channels, actions, message hits; ranking | FR-7, NFR-5 |
| desktop UI | `src/features/mates/use-mates-keys.ts`, `shortcuts-sheet.tsx` | One window keyboard hook (moves Ctrl+K/N/1–9/Esc out of the sidebar), the sheet | FR-8 |
| desktop UI | `src/features/mates/mate-header.tsx` | Mate header (live presence face, name, state, Find, model pill, profile toggle, ⋯, Resume) — replaces `MateBar` | FR-9 |
| desktop UI | `src/features/mates/find-bar.tsx`, `find.ts` (+test) | Find bar; `findMatches(texts, query)`; marks via a `highlight` prop on bubbles (text split, no HTML) | FR-15 |
| desktop UI | `src/features/mates/thread-rows.ts` (+tests) | New row kinds `day`, `unread`, `stopped`; `stepItems(turn)` (plain words with target, failed/denied); long-text flag | FR-11, FR-12, FR-14, FR-17, FR-18 |
| desktop UI | `src/features/mates/thread.tsx` | Sticky day separators, New line, jump-to-latest pill, hover time and refined bar, fold, quote jump with flash, steps chip, stopped line; no dots while a card waits | FR-10 – FR-18 |
| desktop UI | `src/features/mates/steps-chip.tsx` | Steps chip with in-place list | FR-17 |
| desktop UI | `src/features/mates/conversation.tsx` | Card frame "{name} needs you", paused bar | FR-10, FR-27 |
| desktop UI | `src/features/mates/channel-header.tsx` | Channel header: instructions line, members popover, rule chip menu, Stop, Find, ⋯ — replaces `ChannelBar` | FR-19 |
| desktop UI | `src/features/mates/channel-view.tsx`, `channel-rows.ts` (+tests) | Day separators, hand-off cards, route faces, `@` list with role and presence, find marks | FR-11, FR-15, FR-20, FR-21 |
| desktop UI | `src/features/mates/mate-panel.tsx` (+ `mate-overview.tsx`) | Hero with tinted band and quick actions; Overview tab; Profile tab saves on blur | FR-22 – FR-24, FR-31 |
| desktop UI | `src/features/mates/today.tsx`, `today-sections.ts` (+tests) | Greeting, counters, team strip, Needs you with kinds (oldest first) | FR-25, FR-26 |
| desktop UI | `src/lib/mates.ts` | `matesApi.questions(chat)` over the existing `engine_questions` | FR-26 |
| desktop UI | `src/features/mates/gallery.tsx` | Steps strip, Describe a mate first, card hover | FR-28 |
| desktop UI | `src/features/mates/empty-state.tsx` | One empty-state component used everywhere | FR-29 |
| desktop UI | `src/features/mates/files-page.tsx`, `files-page.ts` (+test) | Day groups, grid/list | FR-30 |
| desktop UI | `src/mates/mates-app.tsx` | Layout: sidebar form, panel dock/float with scrim, switcher, sheet, keys | FR-1, FR-7, FR-8, FR-31 |
| docs | `DESIGN.md` (Evo Mates layer), `apps/desktop/README.md` (UI map), `docs/features/evo-mates/ux.md` (pointer to v2) | Record the hero band, presence marks and the new layout | NFR-2 |
| tooling | `scripts/readme-shots/mates_v2_shots.py`, `mock-tauri.js` | Sample states (working, waiting, unread, paused, error), long conversation over days, files; screenshots at 1440 / 1000 / 720 px, light and dark; overflow scan | NFR-1, verification |

### 3.3 Interfaces

No new Tauri command or event. One new client call over an existing
command:

```ts
// lib/mates.ts
questions: (chat: string) => invoke<OpenQuestion[]>("engine_questions", { chat })
// OpenQuestion.kind: "permission" | "questions" | "choice"; title.
```

Today maps it: `permission` → "Approval", `questions` / `choice` →
"Question"; a title naming a secret or a sign-in (the shell's
`request_secret`, `connect_app` cards) → "Secret" / "Sign-in"; none →
"Waiting for you".

Presence:

```ts
type Presence = "idle" | "working" | "waiting" | "error" | "queued" | "paused";
presenceOf(mood: MateMood, paused: boolean): { presence: Presence; word: string | null }
```

### 3.4 Data and storage

- New persisted UI store `evo.mates-ui` in `localStorage` (the same place
  as `evo.ui`): `{ sidebarFolded: boolean, foldedSections: string[],
  filesView: "grid" | "list" }`. Per window profile, like other UI
  preferences (NFR-4).
- Nothing in the database, the engine or `~/.evo`.

### 3.5 Behaviour

- **Sidebar form** (FR-1): `rail` when `window.innerWidth < 760` or
  `sidebarFolded`; full otherwise. Resizes re-evaluate; the person's
  choice is kept. Ctrl+B toggles `sidebarFolded`.
- **Profile dock** (FR-31): docked from 1100 px, else floating with a scrim
  (click closes). Replaces v1's 920 px float rule (`panelFloats`).
- **Order for Alt+↑/↓** (FR-8): Today, Files, pinned mates, mates,
  channels, as the sidebar shows them (hidden excluded); Alt+Shift skips to
  entries that are unread or waiting, wrapping around.
- **New line** (FR-12): on opening a conversation whose row is unread, the
  thread records the index of the first mate bubble after the person's
  last message and scrolls it into view; the line stays until the person
  switches away. While not at the bottom, new bubbles add to a counter on
  the pill.
- **Day separators** (FR-11): `threadRows` inserts `day` rows when the
  local date changes; time lines after 15 minutes stay inside a day. CSS
  `position: sticky` on day rows within the scroller.
- **Find** (FR-15): the bar keeps `{query, index}`; rows get the list of
  matches; the shown match scrolls into view and has a stronger mark.
- **Quote jump** (FR-16): bubbles carry `data-key`; a reply's quote finds
  the bubble whose text starts with the quoted text (the reply stores the
  text, not the key) and flashes it with a CSS class for 1.5 s.
- **Steps chip** (FR-17): `stepItems(turn)` turns each tool part into
  `{word, target, status}` (`read` → "Read", `notes.md`; `web_search` →
  "Searched the web", the query; denied/failed → status). Unknown tools →
  "Used a tool". The chip text stays `stepsLine` (evo-mates FR-31).
- **Channel hand-off cards** (FR-20): `channelRows` already yields
  `handoff` rows with from/to and text; the view renders them as cards with
  a disclosure. Route rows gain the picked mates' ids.
- **Today** (FR-25, FR-26): counters from `todaySections` plus "working
  now" (mates whose chat or seat is working, A-1); Needs you sorted by the
  oldest open question's `at`; kinds from `matesApi.questions`.
- **Keyboard** (FR-8): `useMatesKeys` listens on the window; skips when a
  dialog or menu is open, except Esc; Ctrl+F only when a conversation or
  channel is shown.

### 3.6 Errors and edge cases

- `engine_questions` fails or returns none: "Waiting for you".
- A quote whose bubble is not loaded: a toast "Not in this part of the
  conversation" (spec edge case).
- Very narrow (< 640): the rail stays; the panel floats full width.
- No mates: the rail shows Today, Files, New; the gallery shows the steps
  strip.
- Reduced motion: the presence pulse and the hover lift use
  `motion-safe:` only.

### 3.7 Security and trust

No new input reaches a model or the shell. Find and previews treat
message text as text (no HTML). Approval and question cards are the same
components as before, so approvals stay exactly as strict.

### 3.8 Configuration

| Key | Type | Default | Where | Applies |
| --- | --- | --- | --- | --- |
| `sidebarFolded` | bool | false | `evo.mates-ui` | live |
| `foldedSections` | string[] | [] | `evo.mates-ui` | live |
| `filesView` | "grid" \| "list" | "grid" | `evo.mates-ui` | live |

No settings UI: the fold button, section headers and the Files toggle edit
them.

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Hide the sidebar entirely in narrow windows (a drawer) | The owner chose a rail of faces (spec 6.2); a rail keeps presence visible. |
| Keep a read position per conversation for the New line | Needs new data in the shell; the unread flag gives the same answer for the common case (A-2). |
| Side-panel threads in channels | Needs engine routing per thread (spec 2.2). |
| A new component library or theme | DESIGN.md and shadcn parts already cover it; one product with Evo Harness. |
| Virtualising the thread | NFR-2 of evo-mates is already met; not needed for this UI work. |

## 5. As-built notes

Empty until the status is `In progress`.
