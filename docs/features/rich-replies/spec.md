# Rich replies: charts, cards and actions in the agent's answers — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | engine · desktop UI |
| Created | 2026-11-18 |
| Updated | 2026-11-18 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Office v3](../office-v3-parity/spec.md) (FR-13, US-6: superseded by FR-35 when stage 2 ships) · [Plugin UI](../plugin-ui/spec.md) · [Agent tools for working with the person](../agent-tools/spec.md) · [Usage and cost history](../usage-and-cost/spec.md) · [UX polish](../ux-polish/spec.md) |

## 1. Summary

The agent answers in Markdown only. A reply full of numbers — a week of
spend, test results per package, file sizes, a before/after comparison —
reads as a wall of text that the person has to scan line by line.

This feature lets the agent put **UI blocks** in its reply: stat tiles, bar
lists, charts, sortable tables, steps, callouts and buttons, plus diagrams
and diffs drawn from ordinary fenced code. The agent writes a block as a
small JSON description inside its reply; Evo draws it with its own
components in its own visual style. The reply stays text, so it is stored,
copied, searched and reopened like any other reply.

It also replaces the two widget tools Evo has today (`visualize_read_me`
and `show_widget`). They make the model read long guidelines, then write a
whole HTML page as an escaped tool argument that loads scripts from public
CDNs, and they draw charts a second way. Stage 2 puts an `evo-html` block in
their place: plain HTML written in the reply, run in an isolated frame with
no network, through the same streaming, fallback and text path as the other
blocks.

The feature is a contract between what the model writes and what the app
draws, so this spec defines the block format and the tool the model sees.
It is delivered in four stages (section 2.3); this spec describes all of
them so that stage 1 is shaped for the rest, and only stage 1 is planned
now.

## 2. Scope

### 2.1 Goals

- A **UI block** format the agent can write in a reply: a fenced code block
  whose info string marks it as Evo UI and whose body is one JSON object
  naming a component and its data.
- A fixed **catalogue** of components that Evo draws in its own style, in
  light and dark, with the app formatting every number.
- Blocks that are **safe while streaming**: a block shows a placeholder
  until its fence closes, and a block that cannot be drawn falls back to
  readable text. A reply never breaks because of a block.
- Blocks that stay **text everywhere text is used**: copy, quote, export,
  previews, notifications and the context sent back to the model.
- **Actions** on blocks: a button or a clicked row can send a message in the
  chat or open a file or a link. Later stages add answers (forms, choices)
  and commands that run through the normal approvals.
- A **guide on demand**: the system prompt carries only a short index of
  the components; the agent reads the full description of the ones it needs
  through a tool, so a large catalogue does not cost tokens on every turn.
- **Versioned** blocks: a reply written today draws the same way in a later
  Evo, or falls back to text.
- **One way to draw each thing**: components for data, Mermaid for
  diagrams, `evo-html` only for what neither can show. The old widget tools
  go away; chats that used them still open.

### 2.2 Non-goals

- Free-form HTML in reply text before stage 2. In stage 1 the existing
  `show_widget` tool ([office v3 FR-13](../office-v3-parity/spec.md)) stays
  as it is and the guide tells the agent to prefer a block for data the
  catalogue can show (FR-11); stage 2 replaces it (FR-27, FR-30).
- Network access for model-written HTML, in any stage.
- Plugin-defined components. A plugin that needs its own view in the chat
  uses a plugin card ([Plugin UI](../plugin-ui/spec.md), FR-7).
- Editing a block after it is drawn, or dragging data between blocks.
- Rich blocks in messages the person writes. Only the agent's replies are
  drawn.
- Exporting a block as an image or data file in stage 1; stage 3 adds it
  (FR-29).
- A setting to turn rich replies off. Every session can use them; if a
  setting is added later, turning it off removes the index and the guide
  tool and draws existing blocks as their text.
- Making the agent fetch data it does not have. A block shows data the
  agent already has from a tool result or from the person.

### 2.3 Stages

| Stage | Adds | Planned |
| --- | --- | --- |
| 1 | The block format, the guide tool, streaming and fallback, text rendering, the stage-1 catalogue (`stats`, `bars`, `chart` line/bar/area, `table`, `steps`, `callout`, `actions`), diagrams and diffs from fenced code, the `send` and `open` actions. | Now |
| 2 | HTML views: an `evo-html` block for what components and Mermaid cannot show, isolated and with no network. It replaces `visualize_read_me` and `show_widget`; old chats keep their widgets; the pptx skill's preview moves to it. Needs an ADR. | Next |
| 3 | Layout blocks that nest components (`grid`, `stack`, `card`, `text`, `badge`, `divider`) and more components: pie and scatter charts, sparklines in stat tiles, `timeline`, `checklist`, `keyvalue`, `compare`, `files` (a tree), `tests` (results), `gallery`, `link` (a preview). "Copy as image" and "Copy as CSV". | Later |
| 4 | Input: `choose` and `form` blocks whose answers return to the agent, and a `run` action that runs a tool through the approval mode. | Later |

Each later stage starts by editing this spec (its requirements are listed
below and marked with their stage) and gets its own plan section and tasks.

## 3. User scenarios

### US-1 Numbers at a glance (priority P1, stage 1)

A person asks "how much did I spend on models this week, per day?". The
agent has the numbers from a tool result.

Acceptance scenarios:

1. **Given** the agent has seven daily totals, **when** it replies, **then**
   the reply shows stat tiles (total tokens, total cost, active days) and a
   bar list of the seven days, followed by a sentence of interpretation.
2. **Given** the block gives costs as raw numbers with a currency format,
   **then** every value in the block is formatted the same way (no "$1K"
   next to "$439.01").
3. **Given** the block names a source ("Usage, 10–16 Nov"), **then** it is
   shown under the block.

### US-2 Compare and sort (priority P1, stage 1)

A person asks which test packages are slowest. The agent replies with a
table of package, tests, failures and duration.

1. **Given** a `table` block, **when** the person clicks the Duration
   header, **then** rows sort by duration; clicking again reverses.
2. **Given** a row has an `open` action for a file, **when** the person
   clicks the row, **then** the file opens in Files at the given line.

### US-3 Follow-up from a block (priority P1, stage 1)

1. **Given** a bar list whose items have `send` actions ("Show details for
   11 Nov"), **when** the person clicks the 11 Nov bar, **then** that text
   is sent as the person's message in the chat, exactly as if typed.
2. **Given** an `actions` block with two buttons, **then** the buttons show
   their labels, and hovering a `send` button shows the text it sends.

### US-4 Streaming and fallback (priority P1, stage 1)

1. **Given** the reply is streaming and a block's fence is open, **then**
   the block shows a placeholder of its type, not raw JSON.
2. **Given** a block whose JSON is invalid, or names an unknown component
   or a newer version, **then** the reply shows the block's text rendering
   (or the raw code with a short "Couldn't draw this block" note) and the
   rest of the reply is unaffected.
3. **Given** a chat with blocks is closed and reopened, **then** the blocks
   are drawn again from the stored reply.

### US-5 Copy and context (priority P1, stage 1)

1. **Given** a reply with a `stats` block and a `table` block, **when** the
   person copies the message, **then** the clipboard holds readable text:
   one `label: value` line per tile and the table as tab-separated rows.
2. **Given** the same reply appears in a sidebar preview or a notification,
   **then** the block shows as its title (or its component name), not JSON.

### US-6 Diagrams and diffs (priority P2, stage 1)

1. **Given** a reply with a ` ```mermaid ` fence, **then** it is drawn as a
   diagram, with a toggle to see its source.
2. **Given** a reply with a ` ```diff ` fence, **then** added and removed
   lines are marked, with a copy button for the diff.

### US-7 Dashboards from parts (priority P2, stage 3)

1. **Given** a `grid` block with three `card`s, each holding a `stats` or a
   `chart`, **then** they are laid out in columns that wrap to one column in
   a narrow chat pane.

### US-8 Answer inside the reply (priority P2, stage 4)

1. **Given** a `choose` block with three options, **when** the person picks
   one, **then** the choice is sent to the agent as their answer and the
   block shows it as chosen.
2. **Given** a `run` action, **when** the person clicks it, **then** the
   tool call goes through the chat's approval mode exactly like a call the
   agent makes, and the button shows its result state.

### US-9 Something the catalogue cannot draw (priority P2, stage 2)

A person asks how a binary search narrows a range. No component fits, so
the agent writes an `evo-html` block with a small stepper.

1. **Given** the reply streams an `evo-html` block, **then** it shows a
   placeholder until the block closes, then runs, sized to its content.
2. **Given** the view is running, **then** it works with the network off,
   cannot reach the app, files or other chats, and follows the theme.
3. **Given** the view calls `evo.send("Show the worst case")`, **then**
   that text is sent as the person's message, as a `send` action is.
4. **Given** the person copies the message, **then** the view is copied as
   its title and "Interactive view", not its HTML.

### US-10 Pick a slide theme from a preview (priority P2, stage 2)

1. **Given** the pptx skill's visual preview step, **when** it shows themes
   and a slide grid, **then** they are an `evo-html` view, and clicking a
   theme sends the choice as the person's message, as it does today with
   `show_widget`.

### US-11 Old chats keep their widgets (priority P1, stage 2)

1. **Given** a chat that holds a `show_widget` call from before stage 2,
   **when** it is reopened, **then** the widget is drawn from its stored
   arguments under the new rules (FR-31).

### Edge cases

- A huge block (a 10,000-row table): the block is refused over its size
  limit (FR-14) and shown as its text rendering, truncated with a note.
- The agent puts a block in a reply to a question with no numbers: the
  guide asks it not to (FR-11); the block still draws if it is valid.
- A subagent writes a block in its report: the report is read by the parent
  agent, not by the person, so the block reaches the parent as text. The
  parent decides whether to show it in its own reply.
- A `send` button is clicked while the agent is still answering: the text
  is queued like a message typed during a turn.
- An `open` action names a file outside the chat's project, a missing
  file, or a link with a scheme other than `https`: nothing opens and a
  short note says why.
- The person switches theme while a chart is visible: it redraws in the new
  theme without reloading the reply.
- A chat in split view, or a narrow pane: blocks reflow; tables scroll
  inside their block, never the page sideways.
- Two blocks in one reply: each is drawn in place, between the reply's
  paragraphs.
- A model ignores the guide and writes a block in a slightly different
  shape (a missing optional field, an extra field): the block draws with
  what is valid; unknown fields are ignored.
- An `evo-html` view runs a script that never yields: the chat stays
  responsive, and the view is stopped with a note (FR-36).
- An `evo-html` view asks for a library Evo does not ship, or loads a
  script from a URL: it runs without it, and the view shows a short note
  naming what was not loaded.

## 4. Requirements

### 4.1 Functional

Stage 1 — format, guide and rendering

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | A UI block is a fenced code block with the info string `evo-ui` whose body is one JSON object with `v` (the format version, 1), `type` (a catalogue component), optional `title`, optional `source` (where the data comes from, shown under the block) and the component's own fields. Anything else in the fence is not a block. | A valid `stats` block is drawn; a `json` fence with the same body is shown as code. |
| FR-2 | While a block's fence is still open in a streaming reply, the block shows a placeholder sized for its type (once `type` has arrived) and no raw JSON. It is drawn when the fence closes. | US-4.1. |
| FR-3 | A block that is not valid JSON, names an unknown `type`, has a `v` newer than the app knows, or fails its component's required fields, shows its text rendering when one can be made, else its raw source as code with a "Couldn't draw this block" note. The rest of the reply draws normally. Unknown extra fields are ignored. | US-4.2; a block with an extra field draws. |
| FR-4 | Every component has a text rendering. Copying a message, quoting it into the composer, the one-line previews, notifications and any export use it in place of the block's JSON. | US-5. |
| FR-5 | The reply is stored as written. Reopening a chat draws its blocks again; no block state other than a table's sort is kept, and sort is not kept across reopen. | US-4.3. |
| FR-6 | The app formats every value from the block's `format`: `number`, `compact` (3.18B), `percent`, `currency` with an ISO code, `bytes`, `duration` (from seconds), `date`. One format applies to all values of a field, so a block never mixes "$1K" and "$439.01". The agent supplies raw numbers. Separators and dates follow the app's display locale; the currency always comes from the block's ISO code, never from the locale. | US-1.2; a `currency` `EUR` block shows euros whatever the locale. |
| FR-7 | Blocks follow the light and dark themes and DESIGN.md: achromatic, Geist, the one blue accent, shadow-as-border. In charts the first (or highlighted) series is the blue accent and the other series are greys of clearly different lightness, told apart also by dash and marker; at most 4 series are drawn this way and further ones in a light grey with the legend naming them. Status colours appear only as indicator dots (a `delta`'s direction, a step's status). DESIGN.md gains a short "Data visualisation" section with these rules. A theme switch redraws visible blocks. | Review in both themes; a 5-series chart reads in greyscale; edge case "theme". |
| FR-8 | Blocks fit the chat column: they reflow in a narrow pane and in split view; wide content (tables, charts with many points) scrolls inside its block. | Edge case "split view" at 360 px pane width. |
| FR-9 | ` ```mermaid ` fences are drawn as diagrams in the app's colours (following the background picked in Settings) with a toggle to their source and a full-screen view that fits, zooms and pans; ` ```diff ` fences mark added and removed lines. Both keep a copy button. A diagram that fails to parse shows its source with a note. | US-6. |
| FR-28 | Blocks are drawn wherever the desktop shows an agent's reply: the chat in every pane and split view, and Scheduler run views. The Webbridge side panel draws the stage-1 set when it shares the chat's reply components; until then it shows each block's text rendering (FR-4). | A Scheduler run with a `table` draws it; the side panel shows a block as drawing or as its text, never JSON. |

Stage 1 — the agent's side

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-10 | Every session's system prompt carries a short index of the UI blocks: what they are for, the fence and the list of components with one line each, and when not to use them. It costs no more than 300 tokens. | Count of the index's tokens. |
| FR-11 | A tool `ui_guide {components: string[]}` returns, for each named component, its fields, an example and its rules; unknown names are listed as unknown. The index and the guide tell the agent to: use a block only when it helps compare or scan (at least three related values, a sequence of steps, a diagram); take every number from a tool result or the person, never estimate; give raw numbers with a `format`; name the `source`; keep one or two sentences of interpretation outside the block; not repeat the block's data in prose. | Review of the index and guide; an eval (SC-4). |
| FR-12 | `ui_guide` is read-only, needs no approval and is available in every approval mode and to read-only roles. | A read-only session calls it without a prompt. |
| FR-13 | A subagent's report and mail between sessions carry blocks as their source text; the receiving agent sees JSON, not a drawing. | Edge case "subagent". |
| FR-14 | A block's body is at most 32 KB; a `table` has at most 500 rows and 12 columns; a `chart` at most 6 series and 1,000 points in all; `stats` 1–8 tiles; `bars` at most 50 items; `actions` 1–4 buttons. Over a limit, the block shows its text rendering, truncated, with a note. | Edge case "huge block". |

Stage 1 — components

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-15 | `stats`: 1–8 tiles, each with `label`, `value`, optional `format`, `delta` (a signed change, shown with its sign and a small status dot) and `hint`. Tiles wrap into rows. | US-1.1. |
| FR-16 | `bars`: a horizontal bar list of `items` (`label`, `value`, optional `action`) with one `format`, in the given order; bars are scaled to the largest value or to an optional `max`. | US-1.1, US-3.1. |
| FR-17 | `chart`: `kind` `line`, `bar` or `area`; `x` (labels) and `series` (`name`, `values`); a `format` for the y axis; optional `stacked` for bar and area. Hovering shows the values at that x. A "Show data" toggle shows the same data as a table. | A two-series line chart shows a tooltip and its table. |
| FR-18 | `table`: `columns` (`key`, `label`, optional `format`, `align`) and `rows` (objects by key, optional `action` per row). Clicking a header sorts by it; numbers sort as numbers. | US-2. |
| FR-19 | `steps`: an ordered list of `items` with `title`, optional `detail` and `status` (`done`, `current`, `todo`, `failed`, `skipped`). | A list with each status draws its marker. |
| FR-20 | `callout`: `tone` (`info`, `success`, `warning`, `error`), optional `title`, and `text` in inline Markdown (links, emphasis, code; no blocks). | Each tone draws; a heading inside `text` shows as plain text. |
| FR-21 | `actions`: 1–4 buttons, each with `label` and an `action`; the first may be marked `primary`. | US-3.2. |
| FR-22 | An `action` is one of `{"send": text}` — send `text` as the person's message in this chat, exactly as typed (queued during a turn); or `{"open": target}` — open a file path (optionally `:line`) inside the chat's project in Files, or an `https` link the way links in replies open. A `send` button shows its text on hover. Any other action, or a target outside these rules, is refused with a note and does nothing. | US-3; edge case "open". |

Stage 2 — HTML views (next)

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-27 | A fenced block with the info string `evo-html` holds an HTML document (markup, CSS, SVG, inline script) written as is, not inside JSON, up to 256 KB. It runs in a frame isolated from the app: an opaque origin; no network at all (no fetch, no remote scripts, images or fonts, no CDN); no `eval`; no access to the app's IPC, files, storage, cookies or other chats. Libraries Evo ships for it (FR-34) are put in by the app when the document names them. It gets the theme tokens, follows theme changes and sizes to its content up to 2,400 px. While its fence is open it shows a placeholder; it runs when the fence closes. Its title is its `<title>`; its text rendering is the title and "Interactive view". Stage 2's plan records the isolation in a new ADR. | US-9; a view that fetches a URL, loads a CDN script or reads the app's storage gets nothing; a view draws with the network off. |
| FR-30 | `visualize_read_me` and `show_widget` are no longer offered to the agent, and their guidelines and SVG layout checker are removed. `ui_guide` gains an `html` guide, written to DESIGN.md, that says when to use `evo-html` (only when no component or Mermaid fits) and how (the theme tokens, `evo.send`, the shipped libraries, no network). | A new session's tool list has neither tool; `ui_guide(["html"])` returns the guide. |
| FR-31 | Chats that hold `show_widget` calls from before stage 2 draw those widgets from the stored arguments, in the same isolated frame and under the same rules as `evo-html`; their `sendPrompt` maps to `evo.send`. A widget that cannot run under those rules (it needed a CDN script) shows its title, a note that it needed the internet, and its source on request. | US-11; an old chart widget that used a CDN shows the note, not an error. |
| FR-32 | `evo.send(text)` from a view sends `text` as the person's message, as a `send` action does (FR-22), only after a click or key press in the view. It is the view's only way to reach the chat. | US-9.3; a view that calls `evo.send` on load sends nothing. |
| FR-33 | The prompt index (FR-10) names `evo-html` for free-form views and tells the agent to use components and Mermaid first; it stays within 300 tokens. | The system prompt test. |
| FR-34 | Evo ships a fixed list of libraries a view may name, bundled with the app and served into the frame without the network. The list is `d3` (scales, shapes, selections, transitions; no fetch helpers); a library is added only by editing this requirement. Naming any other library, or a URL, loads nothing and the view shows a note (edge case). | A view naming `d3` uses it offline; a CDN `<script src>` is not loaded. |
| FR-35 | The pptx skill's visual preview (themes, slide grid) uses `evo-html` and `evo.send` instead of `show_widget`; with no view support it falls back to text approval as today. [Office v3](../office-v3-parity/spec.md) FR-13 and US-6 are superseded by this requirement and FR-27–FR-32. | US-10. |
| FR-36 | A view that keeps its thread busy (an endless loop) does not freeze the chat; Evo stops it after 5 s without a response and shows "This view stopped responding" with a Run again button. | Edge case "never yields"; the chat scrolls and takes input while such a view runs. |

Stage 3 — layout and more components (later)

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-23 | Layout blocks `grid` (columns that wrap), `stack` (vertical or horizontal), `card` (a titled surface), `text` (inline Markdown), `badge` and `divider`, with `children` holding other components. Nesting is at most 4 levels; the size limits of FR-14 apply to the whole block. | US-7. |
| FR-24 | More components: `chart` kinds `pie` and `scatter`; a `spark` series on a stat tile; `timeline`, `checklist`, `keyvalue`, `compare` (two or three columns of the same fields), `files` (a tree with sizes or changes), `tests` (suites with pass/fail/skip and durations), `gallery` (images from the project or `https`), `link` (a preview card). Each has a text rendering. | One example per component draws and copies as text. |
| FR-29 | A block's menu offers "Copy as image" for charts and dashboards (a PNG in the current theme) and "Copy as CSV" for tables and charts (the same data as "Show data"). | A copied chart pastes as an image; a copied table pastes into a spreadsheet as columns. |

Stage 4 — input (later)

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-25 | `choose` (single or multiple choice, with an optional free answer) and `form` (text, number, select, checkbox fields) send the person's answer to the agent as their message, written as a readable list like an `ask_user` answer; the block then shows the answer and cannot be sent again. | US-8.1. |
| FR-26 | A `run` action names a tool and its arguments. Clicking it makes the tool call go through the chat's approval mode, read-only roles and per-tool permissions exactly as if the agent called it; the approval says it came from a button in a reply. The button shows running, done or failed. | US-8.2; in Ask mode the click shows an approval. || FR-37 | A file the reply names is a link: a relative markdown link (`[notes](docs/notes.md)`), a path in backticks, or a file name with an extension in the text (`pricing-notes.md`, `src/app.ts:42`); never inside a code block or an existing link. In Evo Harness it opens in the Files tool (at the line when one is given); in Evo Mates it opens in the person's default app for that file. Paths are relative to the chat's folder (a mate's folder for a mate, the channel's folder in a channel). A file that does not exist says so instead of opening. | Given a reply "(source: pricing-notes.md)" in a chat whose folder holds it, when the person clicks the name, then the file opens. |
| FR-38 | The agent is told to write each file it mentions as a markdown link with its path relative to the working folder, so the link finds the file. | Given the system prompt, then it holds that rule. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Security: a block is untrusted content written by a model that may have read injected text. A block cannot run code outside an `evo-html` frame, reach the network, read files or act without the person's click; `open` and `send` do only what FR-22 allows and `run` goes through approvals. An `evo-html` view runs only in its isolated frame with no network (FR-27). | Review; fuzzed blocks never run script outside a frame or open anything without a click; a view cannot reach the network. |
| NFR-2 | Performance: a reply with ten stage-1 blocks draws in under 100 ms after its last fence closes on the developer machine; streaming stays smooth (no dropped frames while text arrives around an open block); chart code loads only when the first chart is drawn, so app start is not slower. | Measured. |
| NFR-3 | Accessibility: every block has an accessible name (its title or type); charts have their data table (FR-17); colour is never the only carrier of meaning; text meets 4.5:1 contrast; motion respects reduced-motion; table sort and buttons work by keyboard. | Review with a screen reader and keyboard only. |
| NFR-4 | Compatibility: a block of format version `v` draws the same in every later Evo, or falls back per FR-3. A component's fields are only added, never renamed or retyped, within a version. | A stored stage-1 reply draws unchanged after stage 2 ships. |
| NFR-5 | Provider-agnostic: blocks rely only on the model writing text, so every model and provider Evo supports can use them. | A block written by two different providers draws. |
| NFR-6 | Works on Windows and macOS. | Both. |

## 5. Success criteria

- SC-1 A reply that compares at least seven values (a week of costs) shows
  them as stat tiles and a bar list that the person reads without scrolling
  through prose; every value matches the tool result it came from.
- SC-2 No reply ever shows a broken drawing or a wall of raw JSON in place
  of a block that failed: the person always sees either the block or
  readable text.
- SC-3 Copying any reply with blocks gives text a person can paste into a
  plain chat or email and read.
- SC-4 On a fixed set of 20 questions whose answers compare numbers from a
  tool result, the agent uses a block in at least 16; on 20 questions with
  no data to compare, it uses one in at most 2.
- SC-5 The system prompt grows by no more than 300 tokens however many
  components the catalogue holds.
- SC-6 After stage 2, on the 20 data questions of SC-4, no reply draws with
  an HTML view what a component can show, and every HTML view works with
  the network off.

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-18 — Q: one fixed set of components, or room for many more kinds
  of UI? → A: many more, in stages: a catalogue first, then layout blocks,
  then input, then sandboxed HTML; this spec covers all four and stage 1 is
  built first (section 2.3).
- 2026-11-18 — Q: the model writes HTML, or describes components? → A:
  describes components as JSON in a fenced block; the app draws them
  (FR-1). HTML waits for stage 4.
- 2026-11-18 — Q: the fence's info string? → A: `evo-ui`, one fence for
  every component, with `type` inside (FR-1).
- 2026-11-18 — Q: chart colours under DESIGN.md's one-accent rule? → A:
  the first series in the blue accent, others in greys told apart by
  lightness, dash and marker; at most 4 such series; status colours only as
  dots; DESIGN.md gains a "Data visualisation" section (FR-7).
- 2026-11-18 — Q: does `send` send at once or fill the composer? → A:
  sends at once as the person's message, queued during a turn; the text
  shows on hover (FR-22).
- 2026-11-18 — Q: a setting to turn rich replies off? → A: none (section
  2.2).
- 2026-11-18 — Q: which clients draw blocks? → A: every desktop pane, split
  view and Scheduler run views; the Webbridge side panel when it shares the
  reply components, text renderings until then (FR-28).
- 2026-11-18 — Q: number locale? → A: the app's display locale for
  separators and dates; currency from the block's ISO code (FR-6).
- 2026-11-18 — Q: the guide tool's name? → A: `ui_guide`, read-only
  (FR-11, FR-12).
- 2026-11-18 — Q: stage 4's isolation? → A: the plugin views' isolation
  (ADR 0021) with the network closed, recorded in a new ADR in stage 4's
  plan (FR-27).
- 2026-11-18 — Q: save a block as an image or data file? → A: stage 2,
  "Copy as image" and "Copy as CSV" (FR-29). Now stage 3 after the stages
  were reordered (below).
- 2026-11-18 — Q: redesign `visualize_read_me` and `show_widget`, in this
  spec or apart? → A: in this spec. They are replaced by components,
  Mermaid and an `evo-html` block; HTML views move from stage 4 to stage 2,
  layout to stage 3, input to stage 4 (section 2.3, FR-27, FR-30–FR-36).
- 2026-11-18 — Q: stage 4's isolation (revisited) → A: supersedes the
  earlier answer. HTML views run in an opaque-origin frame with no network,
  as the widget frame does today minus its CDN allowlist and `eval`, not in
  a native webview: a view needs no bridge beyond `evo.send`, theme and
  size. Recorded in a new ADR with stage 2's plan (FR-27).
- 2026-11-18 — Q-A: which libraries does Evo ship for `evo-html` views? →
  A: `d3` only to start (no fetch helpers); charts belong to components; a
  library is added by editing FR-34.
- 2026-11-18 — Q-B: old `show_widget` widgets that loaded a CDN script? →
  A: they run under the new rules; those that needed the network show
  their title, a note and their source; no old CDN policy is kept for
  history (FR-31).

## 7. History

- 2026-11-18 — Draft.
- 2026-11-18 — Clarified: all nine open questions answered; FR-6, FR-7,
  FR-27 and section 2.2 updated; FR-28 and FR-29 added.
- 2026-11-18 — Gate A: spec accepted. Plan and tasks for stage 1 written.
  Section 2.2 now names the existing `show_widget` tool, found while
  planning.
- 2026-11-18 — Amended: the widget tools' redesign folded in. Stages
  reordered (HTML views 2, layout 3, input 4); FR-27 rewritten; FR-30 to
  FR-36, US-9 to US-11, SC-6 added; two questions open (Q-A, Q-B). Stage 1
  is unchanged. The amendment needs acceptance (gate A) before stage 2 is
  planned.
- 2026-11-18 — Q-A and Q-B answered (FR-34, FR-31). Gate A: amendment
  accepted.
- 2026-11-18 — Gate B: stage 1 plan accepted (status Accepted). Stage 1
  implementation started (status In progress).
- 2026-11-19 — Amended at the person's request after trying stage 1: FR-9
  diagrams take the app's colours (and the chosen background) and open
  full screen.
- 2026-11-28 — Owner asked that a file named in a reply be a link that opens it ("refer file why not a link click vào ra file"): FR-37, FR-38 added.
