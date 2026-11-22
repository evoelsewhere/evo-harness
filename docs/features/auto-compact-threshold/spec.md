# Auto-compact threshold: the person chooses when a chat compacts — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | harness, engine, desktop shell, desktop UI |
| Created | 2026-11-23 |
| Updated | 2026-11-23 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Engines run without a window](../shell-hosted-engines/spec.md) (settings fan-out) · [Code mode](../code-mode/spec.md) (the same live-setting path) |

## 1. Summary

A chat compacts itself when its context passes a threshold. Today that
threshold is "the model's window minus 16,384 tokens", so it is about 87 % of
a 128k window and 95 % of a 328k one, and the person cannot change it except
by editing `settings.json` by hand. Some people want a chat to compact early
(at 60 %) to keep it fast and cheap; others want it as late as possible.

This feature adds one setting: auto-compact on or off, and the share of the
model's window at which it compacts, 30 % to 95 %, chosen with a slider. The
slider is in Settings and in the context panel (the box that shows "91 % used"),
and a change reaches every chat that is open, from its next run, with no
restart.

## 2. Scope

### 2.1 Goals

- One global setting: auto-compact on or off, and a trigger percent (30–95,
  default 90) of the model's context window.
- A slider with the value beside it, and a switch, in Settings and in the
  context panel.
- A change applies to every running chat from its next run and to every chat
  started later, without restarting engines.
- The context panel's "Autocompact buffer" and "until auto-compact" follow the
  setting.

### 2.2 Non-goals

- A threshold per chat or per model.
- Changing how a compaction summarises (`keepRecentTokens`, the summary
  prompt, the model that writes it).
- Changing the **Compact session** button: it compacts now, whatever the
  threshold.
- Interrupting a run in progress to apply a new threshold.

## 3. User scenarios

### US-1 Compact earlier (priority P1)

A person drags the slider in the context panel to 60 %. The "Autocompact
buffer" segment grows, "until auto-compact" shrinks to the tokens left before
60 %, and the chat compacts when it passes 60 % of its window.

1. **Given** a chat at 45 % of a 200k window and a trigger of 90 %, **when** the
   slider is moved to 60 %, **then** the panel says "30k until auto-compact"
   (200k × 60 % − 90k).
2. **Given** the trigger is 60 % and a run starts at 62 %, **then** the chat
   compacts before it answers.

### US-2 Turn auto-compact off (priority P2)

1. **Given** auto-compact is on, **when** the person turns the switch off,
   **then** the slider dims, the panel says "Auto-compact is off", and no chat
   compacts by itself until it is turned on.

### US-3 Change it in Settings (priority P2)

1. **Given** Settings is open, **when** the person moves the slider there,
   **then** the context panel of any open chat shows the new value.

### Edge cases

- A window so small that the percent leaves no room to cut (the threshold
  would be under twice `keepRecentTokens`): the threshold is raised to that
  floor, so a compaction always frees something.
- A chat in a run when the value changes keeps its old threshold until the
  run settles.
- An engine that is still starting takes the value once it is set up.
- `settings.json` has `compaction.reserveTokens` and no percent, and the app
  has never saved the setting: that threshold stays (window − reserve).
- The model's window is unknown (0): nothing compacts, as today.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | The app saves `compaction = { enabled, triggerPercent }` (default on, 90; percent clamped to 30–95). | Settings: move the slider, restart the app: the value stays. |
| FR-2 | A chat compacts when its context tokens exceed `window × triggerPercent / 100`, but never below `2 × keepRecentTokens` and never above the window. Off means never by itself. | Unit tests of the threshold and `should_compact`. |
| FR-3 | Engines follow the setting: a running engine from its next run, a starting one once set up, a new one from its start. No restart. | Supervisor tests; running app: change the value with two chats open. |
| FR-4 | Settings → General has the switch and the slider with the percent beside it. The slider saves when released (keys: on each press). | Running app. |
| FR-5 | The context panel has the same switch and slider. "Autocompact buffer" is `window − threshold`; "until auto-compact" is `threshold − tokens` (never below 0). | Breakdown test; running app. |
| FR-6 | `settings.json` `compaction.{enabled, triggerPercent}` is read as the default for a person who never set the app setting; `reserveTokens` without `triggerPercent` keeps the old threshold. The app setting wins. | Settings tests. |
| FR-7 | The **Compact session** button and manual compaction ignore the threshold and the switch. | Existing behaviour unchanged. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | The slider is usable by keyboard (arrows, Home, End) and labelled for screen readers. | Keyboard walkthrough. |
| NFR-2 | A burst of changes sends one command per settled value to each engine, not one per pixel. | Slider commits on release. |
| NFR-3 | Light and dark themes, following DESIGN.md. | Both themes. |

## 5. Success criteria

- SC-1 A person can make a chat compact at 60 % in one gesture, from the panel
  they already read the usage in.
- SC-2 The panel's numbers match when the chat actually compacts.

## 6. Clarifications

### 6.1 Open

n/a — all answered.

### 6.2 Resolved

- 2026-11-23 — Q1: threshold by percent or tokens, and the default? → A: a
  percent of the window, 30–95 %, default 90 % (about the old behaviour).
- 2026-11-23 — Q2: scope and place? → A: global; in Settings and in the
  context panel.
- 2026-11-23 — Q3: when does a change apply? → A: to open chats from their
  next run, through a new engine command (no restart).
- 2026-11-23 — Q4: an on/off switch? → A: yes, beside the slider.

### 6.3 Assumptions

- A run in progress keeps the threshold it started with; "next turn" means
  the next run. The in-run compaction check reads the settings at run start.
- The 2 × `keepRecentTokens` floor (40k with the default 20k) keeps a
  compaction useful; the slider does not show it.
- The percent is stored as a whole number.

## 7. History

- 2026-11-23 — Draft from the framing (Q1–Q4 answered); accepted to build.
- 2026-11-23 — Built (T1–T9); every test passes, the running app was not checked (the person's own app was open).
