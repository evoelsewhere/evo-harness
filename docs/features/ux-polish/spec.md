# UX polish: clearer errors, states and text — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop UI |
| Created | 2026-11-13 |
| Updated | 2026-11-13 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Split view](../chat-split-view/spec.md) · [Memory and the Scheduler](../memory-and-scheduler/spec.md) |

## 1. Summary

An audit of the running app on 2026-11-13 found places where it says less
than it knows or gets in the way:

- Provider errors appear as raw JSON.
- A summary card covers a split pane.
- "No model" shows while the engine starts.
- A step the person stopped looks like a failure.
- Scheduled runs share one title.
- Secondary text is too faint to read (3.1:1).
- Shortcuts have no list.
- The new-chat suggestions ignore where the chat works.
- Previews show markdown marks.
- Popovers move differently from split view.
- The composer loses focus after Send.

This feature fixes each of them. The audit and its scores are in section 6.

## 2. Scope

### 2.1 Goals

Each item in section 4.1, one requirement each.

### 2.2 Non-goals

- A redesign of any screen or a new visual language (DESIGN.md stands).
- Translating the UI.
- Changing what the engine reports. Errors are explained from the text it
  already sends.

## 3. User scenarios

### US-1 Understand what went wrong (priority P1)

A person's Anthropic key has expired. Their prompt fails. They want to
know why and where to fix it, without reading JSON.

1. **Given** a 401 from a provider, **when** the turn fails, **then** it
   says "Anthropic rejected the API key". It has a button that opens
   Settings › Providers, and the raw message is folded under Details.

### US-2 Read the app comfortably (priority P2)

Secondary text (times, model names, hints) is readable on every surface in
both themes.

### US-3 Find past scheduled runs (priority P2)

Runs of one scheduled task are told apart in the sidebar and in pane
headers.

### Edge cases

- An error the app does not recognise still shows a readable first line,
  with the rest under Details.
- A scheduled run's chat that the person renamed keeps their title.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | A failed turn explains its error in one sentence when it is recognised: an invalid or missing key (401/403), a rate limit (429), an overloaded or unavailable provider (5xx/529), a network failure, or context too long. A key error offers **Open Providers**, and the other recognised errors offer **Retry**. The raw text is under **Details**. Anything else shows its first line and Details. | A 401 shows the sentence, Open Providers and Details. |
| FR-2 | The summary card never covers a pane. With two panes it is not pinned and opens only from the toolbar, as in a narrow chat. | With two panes, the card does not show until it is asked for. |
| FR-3 | While a chat's engine starts, the model picker shows "Loading models…" instead of "No model". | A chat just opened does not say No model. |
| FR-4 | A tool or app step ended by Stop shows as stopped, in neutral colours, not as a failure. | A stopped app step reads "Stopped", not "App step failed". |
| FR-5 | A scheduled run's chat shows its run date after the task name ("Morning briefing · Nov 12") in the sidebar, the pane header and the window title, unless the person renamed it. | Two runs of one task read differently. |
| FR-6 | Secondary text (`muted-foreground`) reaches at least 4.5:1 on the background, card and sidebar surfaces, in light and dark. | Measured contrast ≥ 4.5:1. |
| FR-7 | Ctrl/⌘+/ opens a list of keyboard shortcuts, grouped by area, from one registry. The command palette offers it too. | Ctrl+/ shows the list. |
| FR-8 | The new-chat suggestions fit where the chat works. In a repository with changes, the first suggestion reviews them ("Review the 35 changed files"). Outside a project, the suggestions are general tasks, not code tasks. | "This computer" shows no "Explain this project". |
| FR-9 | Previews of chat text (Scheduler past runs, sidebar tooltips) show plain text, without markdown marks. | A past run reads "Needs your attention…", not "**Needs…". |
| FR-10 | Truncated titles show in full on hover: sidebar rows, pane headers, Scheduler past runs. | Hovering a cut title shows all of it. |
| FR-11 | Popovers, menus, hover cards and dialogs open with the same spring curve as split view, within their current durations. Reduced motion is respected. | Opening a menu uses the spring easing. |
| FR-12 | After Send, the focus goes back to the composer. While a turn runs, the "Working for 12s" line names the current step ("· running a command"). | Send leaves the caret in the composer, and a running tool is named. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | DESIGN.md holds: achromatic, one blue accent, weights 400/500/600, shadow-as-border. | Review. |
| NFR-2 | No new dependency. | `package.json` unchanged. |

## 5. Success criteria

- SC-1 The re-audit scores every category at 4/5 or higher (section 6).
- SC-2 No raw JSON error, no faint unreadable text, and no overlapping
  card in the audited screens.

## 6. Clarifications

### 6.1 Open

n/a — the person asked for every proposed item ("làm hết luôn",
2026-11-13).

### 6.2 Resolved

- 2026-11-13 — The audit: screens captured from the running dev app
  (chat, new chat, Settings, Customize, Scheduler, Usage, command palette,
  side panel, split view), scored 1–5:

  | Category | Before | Findings |
  | --- | --- | --- |
  | Clarity of errors | 2 | Raw JSON for provider errors (FR-1); a stopped step shown as failed (FR-4) |
  | Feedback and loading states | 3 | "No model" while starting (FR-3); the working line does not say what runs (FR-12) |
  | Layout and overlap | 3 | The summary card covers the right pane (FR-2) |
  | Readability and accessibility | 2 | Secondary text 3.1:1 (FR-6); cut titles without a tooltip (FR-10) |
  | Wayfinding | 3 | Identical scheduled-run titles (FR-5); markdown in previews (FR-9); no shortcut list (FR-7) |
  | Relevance | 3 | Project suggestions outside a project (FR-8) |
  | Motion and polish | 4 | Popovers use another easing than split view (FR-11); focus lost after Send (FR-12) |

- 2026-11-13 — Re-audit after the changes, same screens:

  | Category | Before | After |
  | --- | --- | --- |
  | Clarity of errors | 2 | 4 |
  | Feedback and loading states | 3 | 4 |
  | Layout and overlap | 3 | 5 |
  | Readability and accessibility | 2 | 4 |
  | Wayfinding | 3 | 4 |
  | Relevance | 3 | 4 |
  | Motion and polish | 4 | 5 |

  Each is at 4 or higher (SC-1). Feedback stays at 4 until FR-12 is seen
  in the app.

## 7. History

- 2026-11-13 — Draft from the audit; every item accepted by the person
  ("làm hết luôn"), so both gates are passed together.
- 2026-11-13 — FR-5 also dates runs saved before runs were flagged, when they are still named after their task.
