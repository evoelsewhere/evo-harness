# UX polish: clearer errors, states and text — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Status | In progress |
| Updated | 2026-11-13 |

## 1. Context and constraints

- A turn's error is a string on the turn (`features/chat/chat-turn.tsx`,
  `turn.error`), as the provider or engine wrote it
  (`http error 401: {…}`).
- Tool and app steps are drawn by `components/ai-elements/tool.tsx`. Each
  family has labels for each phase, and an error phase reads "… failed"
  whatever the cause.
- The summary card (`features/chat/summary-card.tsx`) measures the chat
  card to decide whether it is pinned. With split view, the chat card
  holds two panes.
- Colours are tokens in `index.css`. `--muted-foreground` is #8f8f8f in
  both themes' light surfaces, which measures 3.1:1 on #fafafa.
- Scheduled runs' chats are flagged `scheduled` and titled with the task
  name (`src-tauri/src/agent_tools/scheduler/run.rs`).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | UI only. |
| Layers point down; library crates stay free of engine/desktop concerns | n/a | |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | No protocol change. |
| Models come from models.dev; no hardcoded model facts | ok | Provider names come from the catalog, never a list in the UI. |
| Real engine in the app; settings and credentials reload live | ok | |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Error text and previews are shown as plain text. |
| Desktop UI structure and `DESIGN.md` | ok | Token changes keep the achromatic palette. |

## 3. Design

### 3.1 Overview

These are small, separate changes, one per requirement. Each has a pure
helper with tests where there is logic: error explanation, plain-text
previews, scheduled titles and suggestions.

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| desktop UI | `src/features/chat/explain-error.ts` (new, tested) | `explainError(text, provider?)` → `{ kind, title, action }` | FR-1 |
| desktop UI | `src/features/chat/turn-error.tsx` (new), `chat-turn.tsx` | The failed-turn block: sentence, action button, Details | FR-1 |
| desktop UI | `src/features/chat/summary-card.tsx` | Not pinned while split | FR-2 |
| desktop UI | `src/components/ai-elements/model-selector.tsx` | "Loading models…" while the engine starts | FR-3 |
| desktop UI | `src/components/ai-elements/tool.tsx` | A step aborted by Stop reads "Stopped", neutral | FR-4 |
| desktop UI | `src/lib/chat-title.ts` (new, tested); sidebar, pane header, window title | `displayTitle(chat)` adds the run date to scheduled runs | FR-5 |
| desktop UI | `src/index.css` | Darker `--muted-foreground` (light and dark) | FR-6 |
| desktop UI | `src/features/shell/shortcuts.ts`, `shortcuts-dialog.tsx` (new); `use-app-bootstrap.ts`; command palette | The registry, the dialog and Ctrl/⌘+/ | FR-7 |
| desktop UI | `src/features/chat/chat-empty.tsx`, `suggestions.ts` (new, tested) | Suggestions by place: no project, repository, changes | FR-8 |
| desktop UI | `src/lib/plain-text.ts` (new, tested); Scheduler past runs; sidebar tooltip | Markdown to plain preview | FR-9 |
| desktop UI | sidebar, pane header, Scheduler | `title` on cut text | FR-10 |
| desktop UI | `src/index.css` | `--ease-spring` on popover, menu, hover card and dialog content | FR-11 |
| desktop UI | `chat-composer.tsx`, `chat-turn.tsx` | Focus after Send; the running step on the working line | FR-12 |

### 3.3 Interfaces

```ts
type ErrorKind = "auth" | "rate" | "unavailable" | "network" | "context" | "other";
explainError(text: string, provider?: string): { kind: ErrorKind; title: string; detail: string };
displayTitle(chat: { title: string; scheduled?: boolean; createdAt: number; titleEdited?: boolean }): string;
plainText(markdown: string): string;
suggestionsFor(place: { workspace: boolean; repo: boolean; changed: number }): Suggestion[];
```

### 3.4 Data and storage

n/a — no new data.

### 3.5 Behaviour

- **Retry** sends the turn's prompt again (`sendPrompt`). **Open
  Providers** opens Settings › Providers.
- The working line names the latest running tool part by its family
  label ("running a command", "using the browser", …).

### 3.6 Errors and edge cases

An unrecognised error shows its first line (up to 160 characters), with
Details holding the full text.

### 3.7 Security and trust

Error text and previews stay plain text; nothing from them is rendered as
HTML or run.

### 3.8 Configuration

n/a — no settings.

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Rename scheduled-run chats in the database | It would change saved titles and the shell's naming. A display rule leaves data alone and respects renames. |
| A separate muted token for small text | One darker token fixes every use at once and keeps the palette achromatic. |

## 5. As-built notes

- **FR-3** also covers a chat that loads while its engine has not
  reported a model yet. The ready-but-restoring case showed "No model" in
  the first check.
- **FR-5** dates any chat named after a scheduled task: runs saved before
  the `scheduled` flag existed were still identical. `useDisplayTitle`
  reads the task list once, and before that only flagged runs get a date.
  It lives in `src/hooks/` so that the sidebar and split view can share it.
- **FR-7**: the dialog's open state is `ui-store.shortcutsOpen`, so the
  command palette (`features/sessions`) does not reach into
  `features/shell`.
- **FR-11** sets tw-animate's `--tw-ease` on overlay content, with no
  change to the generated `components/ui` files.
