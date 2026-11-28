# Rich replies: charts, cards and actions in the agent's answers — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-18 |

Stage 1 only (spec section 2.3). Desktop unit tests run with `bun test`
from `apps/desktop`; components have no DOM test harness, so they are
checked in the running app (WebView2 CDP, see `AGENTS.md`).

## 1. Phases

### Phase 1 — The format, in one pure module

Done when: `lib/ui-blocks/` parses, validates, formats and writes every
stage-1 component as text, with tests for each fixture.

- [x] T1 [P] [FR-1] [FR-3] [FR-14] [NFR-4] Write fixtures (a valid block per component; invalid JSON, unknown type, `v: 2`, missing field, over-limit list, one bad item, extra field) and the failing tests of `parseBlock` and `sniffType` — `apps/desktop/src/lib/ui-blocks/fixtures.ts`, `lib/ui-blocks/parse.test.ts` — check: `bun test src/lib/ui-blocks/parse.test.ts` fails for the missing module
- [x] T2 [P] [FR-1] [FR-3] [FR-14] [NFR-4] Types and parser: 32 KB check before parsing, `v`, required fields, limits, dropped bad items with a count, unknown fields ignored — `lib/ui-blocks/types.ts`, `lib/ui-blocks/parse.ts` — check: T1 passes
- [x] T3 [P] [FR-6] Failing tests of `formatValue`: each format, currency from the ISO code whatever the locale, one formatter per field (no "$1K" beside "$439.01") — `lib/ui-blocks/format.test.ts` — check: fails for the missing module
- [x] T4 [P] [FR-6] `formatValue` with `Intl` — `lib/ui-blocks/format.ts` — check: T3 passes
- [x] T5 [FR-4] [FR-14] Failing tests of `blockText` for every fixture and of `replaceBlocks` in `"text"` and `"title"` modes, including a truncated unclosed fence — `lib/ui-blocks/text.test.ts` — check: fails for the missing module
- [x] T6 [FR-4] [FR-14] `blockText` and `replaceBlocks` (plan 3.3, "Text rendering") — `lib/ui-blocks/text.ts` — check: T5 passes

### Phase 2 — The agent's side

Done when: every session lists `ui_guide`, the index is in the system
prompt within 300 tokens, and turn previews show block titles.

- [x] T7 [P] [FR-11] [FR-12] Failing tests: `ui_guide` returns the named guides in order, once each, lists unknown names, lists the known ones when none is known; it is registered for every session, `Read`, read-only safe, not reserved — `crates/evo-engine/src/tools/ui_guide.rs` (tests module), `crates/evo-engine/src/tools/info/tests.rs` — check: `cargo test -p evo-engine ui_guide` fails
- [x] T8 [P] [FR-11] [FR-12] [FR-15]–[FR-22] Write the guides (fields, one example, rules per component; `formats`; the `action` object) and the tool; register it; add its row — `crates/evo-engine/src/tools/ui_guide.rs`, `tools/ui_guide/*.md`, `tools/mod.rs`, `tools/info.rs`, transcript row in `apps/desktop/src/components/ai-elements/tool-kinds.ts` and `tool.tsx` — check: T7 passes; each guide's example is drawn by the desktop parser (`bun test src/lib/ui-blocks/guides.test.ts`)
- [x] T9 [FR-10] [FR-11] Failing test, then the index: `builtin_tool_snippet("ui_guide")`, `builtin_tool_guidelines` feeding `index.md` into the prompt guidelines; the test checks the index is present with `ui_guide` active, absent without it, and at most 300 tokens by the engine's estimate — `crates/evo-engine/src/session.rs`, `tools/ui_guide/index.md` — check: `cargo test -p evo-engine system_prompt`
- [x] T10 [P] [FR-4] Failing tests, then `ui_blocks::preview_text` (complete fence → `[title]`/`[type]`, unclosed tail dropped or `[type]`) and its call before the preview is cut — `crates/evo-engine/src/ui_blocks.rs`, `crates/evo-engine/src/lib.rs`, `crates/evo-engine/src/modes.rs` — check: `cargo test -p evo-engine ui_blocks` and the existing turn-preview tests pass
- [x] T11 [FR-13] Test that a subagent's report with a block reaches the parent as its source text — `crates/evo-engine/src/tools/agent` tests — check: `cargo test -p evo-engine agent_report_keeps_blocks`

### Phase 3 — Drawing the blocks

Done when: every stage-1 component, Mermaid and diff draw in a reply in
light and dark, with placeholder and fallback, and actions work.

- [x] T12 [FR-7] Chart tokens `--block-viz-1`…`--block-viz-4`, `--block-viz-rest` in light and dark; DESIGN.md "Data visualisation" section (accent first series, greys by lightness plus dash and marker, at most 4 coloured series, status colours only as dots) — `apps/desktop/src/index.css`, `DESIGN.md` — check: review; tokens resolve in both themes (CDP `getComputedStyle`)
- [x] T13 [FR-1] [FR-2] [FR-3] [FR-8] [NFR-3] `blocks` prop on `Markdown`; `UiBlock` dispatcher with placeholder (via `useIsCodeFenceIncomplete` and `sniffType`), fallback, title, source and accessible name; memoised parse — `components/markdown.tsx`, `components/reply-blocks/ui-block.tsx` — check: in the app, a streamed block shows its placeholder, then draws; an invalid fixture shows its fallback; a `json` fence stays code
- [x] T14 [P] [FR-15] [FR-16] `stats` and `bars` — `components/reply-blocks/simple-blocks.tsx`, `frame.tsx` — check: US-1 in the app, both themes
- [x] T15 [P] [FR-18] `table`: header sort (numbers as numbers, toggle), keyboard, inner scroll — `components/reply-blocks/table.tsx` — check: US-2.1 in the app
- [x] T16 [P] [FR-19] [FR-20] [FR-21] `steps`, `callout` (inline Markdown only), `actions` — `components/reply-blocks/simple-blocks.tsx`, `inline-text.tsx` — check: each status and tone draws; a heading in `text` shows as plain text
- [x] T17 [P] [FR-7] [FR-17] [NFR-2] `chart` as lazy SVG: line/bar/area, stacked, rounded ticks, tooltip, legend, "Show data" — `components/reply-blocks/chart.tsx` — check: a 2-series line chart shows its tooltip and table; a 5-series chart reads in greyscale; the chart chunk loads only with the first chart (network tab)
- [x] T18 [P] [FR-9] Mermaid (add `mermaid` with `bun add`, lazy, strict, themed, source toggle, error fallback) and diff gutter marks — `components/reply-blocks/mermaid-block.tsx`, `components/markdown.tsx`, `apps/desktop/package.json`, `bun.lock` — check: US-6 in the app
- [x] T19 [FR-22] [NFR-1] Move `sendToChat` into the store as `sendOrSteer`; failing tests, then `runBlockAction` (path inside the chat's folder, `..` and absolute paths outside refused, `https` only, unknown actions refused); wire `bars`, `table` rows and `actions` to it; `send` text on hover — `stores/engine-store.ts`, `components/widget-frame.tsx`, `components/reply-blocks/block-action.ts`, `lib/ui-blocks/open-target.ts`, `open-target.test.ts` — check: `bun test src/lib/ui-blocks/open-target.test.ts`; US-3 in the app; a widget's `sendPrompt` still works
- [ ] T20 [FR-28] [FR-13] Turn `blocks` on for replies in the chat and the side chat, not in `agent-run.tsx` — `features/chat/chat-turn.tsx`, `features/side-chat/side-chat-panel.tsx` — check: a block draws in a chat, in split view and in a Scheduler run's chat; a subagent run shows JSON

### Phase 4 — Text everywhere

Done when: copy, quote, previews and the Webbridge side panel never show a
block's JSON.

- [x] T21 [P] [FR-4] Failing tests, then `messagePlainText` and `plainText` use `replaceBlocks` — `features/chat/message-text.ts`, `message-text.test.ts`, `lib/plain-text.ts`, `plain-text.test.ts` — check: `bun test src/features/chat/message-text.test.ts src/lib/plain-text.test.ts`; US-5 in the app
- [x] T22 [P] [FR-28] Webbridge: DOM-only port of `blockText` for stage 1; `markdown.js` shows an `evo-ui` fence as that text, else its title — `extensions/evo-webbridge/sidepanel/ui-blocks.js`, `sidepanel/markdown.js` — check: `bun test src/lib/ui-blocks/webbridge-port.test.ts` (the port's text equals the desktop's for every fixture; unreadable blocks show their name)

### Phase 5 — Verify

Done when: every row of the verification table has a result.

- [x] T23 [SC-4] Write the 40-question eval (20 with numbers to compare from a tool result, 20 without) and run it on two providers; record the counts — `docs/features/rich-replies/assets/eval.md` — check: at least 16/20 and at most 2/20
- [ ] T24 [NFR-2] [NFR-3] Measure a 10-block reply's draw time and streaming smoothness (CDP performance trace); keyboard-only and screen-reader pass — check: under 100 ms, no long frames while streaming; findings fixed or recorded
- [x] T25 UI map entry for reply blocks; spec status and catalogue row updated — `apps/desktop/README.md`, `docs/features/README.md`, `spec.md` — check: review

### Phase 6 — Files named in replies

Done when: a file named in a reply opens on click in both apps.

- [x] T26 [FR-37] Failing tests, then `linkFiles`: relative links, backtick paths and bare file names become file links, never inside code or links — `components/file-refs.ts`, `file-refs.test.ts` — check: `bun test file-refs`
- [x] T27 [FR-37] Replies in Harness chats, the side chat, subagent runs, a mate's conversation and channels pass the open handler (Files tool in Harness, default app in Mates, relative to the right folder; a missing file says so) — `components/markdown.tsx`, `features/chat/chat-turn.tsx`, `features/mates/thread.tsx`, `features/mates/channel-view.tsx`, `features/side-chat/side-chat-panel.tsx`, `components/ai-elements/agent-run.tsx` — check: typecheck; manual
- [x] T28 [FR-38] System prompt rule for file links — `crates/evo-engine/src/session.rs` — check: engine test `the_prompt_asks_for_file_links`

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | `parse.test.ts`; manual: `evo-ui` draws, `json` fence stays code | passed (2026-11-19): `parse.test.ts`; UI in Chromium with the fake IPC on Vite (`evo-ui` drawn, `json` fence stays code) |
| FR-2 | Manual: placeholder while streaming (T13) | passed (2026-11-19): streamed deltas through the fake IPC — "Chart, loading" placeholder, no JSON, chart drawn on close |
| FR-3 | `parse.test.ts`; manual: fallback for each invalid fixture | passed (2026-11-19): `parse.test.ts`; UI shows "Couldn't draw this block" with the source, and a newer block as text with its note |
| FR-4 | `text.test.ts`, `message-text.test.ts`, `plain-text.test.ts`; `cargo test -p evo-engine ui_blocks` | passed (2026-11-19): desktop tests; `cargo test -p evo-engine ui_blocks` (6) |
| FR-5 | Manual: reopen a chat with blocks | passed (2026-11-19): a stored reply loaded with the chat draws its blocks (fake IPC) |
| FR-6 | `format.test.ts` | passed (2026-11-18) |
| FR-7 | Manual in both themes; 5-series chart in greyscale; DESIGN.md review | passed (2026-11-19): screenshots light and dark — accent first series, greys with dash and marker (3 series checked, not 5) |
| FR-8 | Manual at 360 px pane width and in split view | partly (2026-11-19): 560 px window, no sideways page scroll, table scrolls inside; split view not run |
| FR-9 | Manual: US-6, Mermaid parse error | partly (2026-11-19): diagram drawn in both themes, source toggle, diff marks 1 added / 1 removed; parse-error fallback not run |
| FR-10 | `cargo test -p evo-engine system_prompt` (index present, ≤ 300 tokens) | passed (2026-11-19): 238 tokens by the engine's estimate |
| FR-11 | `cargo test -p evo-engine ui_guide`; review of index and guides | passed (2026-11-19): 10 engine tests; `guides.test.ts` (every example drawn); guides reviewed |
| FR-12 | `cargo test -p evo-engine ui_guide` (class, read-only safe); manual in a read-only session | test passed (2026-11-19); manual in a read-only session not run |
| FR-13 | `cargo test -p evo-engine agent_report_keeps_blocks` | passed (2026-11-19) |
| FR-14 | `parse.test.ts`, `text.test.ts` (limits, truncation note) | passed (2026-11-18) |
| FR-15 | Manual: US-1 | passed (2026-11-19): tiles with delta dots and source |
| FR-16 | Manual: US-1, US-3.1 | passed (2026-11-19): bars scaled to the largest; a click sent "Show details for 11 Nov" (`engine_send`) |
| FR-17 | Manual: tooltip and "Show data" | passed (2026-11-19): tooltip with a gap as —, "Show data" lists 7 rows, stacked bars |
| FR-18 | Manual: US-2 | passed (2026-11-19): Duration sorts ascending, then descending |
| FR-19 | Manual: each status | passed (2026-11-19) |
| FR-20 | Manual: each tone, heading as plain text | partly (2026-11-19): warning tone with bold, code and a link; other tones not looked at |
| FR-21 | Manual: US-3.2 | passed (2026-11-19): primary and outline buttons, hint on hover |
| FR-22 | `block-action.test.ts`; manual: US-3, edge case "open" | passed (2026-11-19): `open-target.test.ts`; `../outside.log` refused with a toast; send hint on hover |
| FR-28 | Manual: chat, split view, Scheduler run, Webbridge side panel | partly (2026-11-19): chat; Webbridge text by `webbridge-port.test.ts`; split view and a Scheduler run not run |
| NFR-1 | Review; `block-action.test.ts`; fuzzed fixtures never run script or open anything | passed (2026-11-19): review (no HTML from blocks; Mermaid strict); `open-target.test.ts`; invalid fixtures shown as code |
| NFR-2 | T24 measurement | partly (2026-11-19): opening a chat whose reply has 10 blocks draws in 290–362 ms (Vite dev mode, fake IPC), faster than the same reply as code fences (407–435 ms); chart and `mermaid` load only when used. Block drawing alone and streaming frames not measured |
| NFR-3 | T24 keyboard and screen-reader pass | partly (2026-11-19): keyboard — header sort, row and bar actions by Enter; every block has an accessible name, charts a label and a data table. Screen reader not run |
| NFR-4 | `parse.test.ts` (unknown fields, `v` handling) | passed (2026-11-18) |
| NFR-5 | T23 run on two providers | passed (2026-11-19): copilot and xiaomi both write blocks the desktop draws (16/16 and 23/23 on the final run) |
| NFR-6 | Manual on Windows and macOS | not run |
| SC-4 | T23 eval | passed (2026-11-19): copilot/claude-sonnet-5.5 16/20 and 0/20; xiaomi/mimo-v2.6-pro 20/20 and 1/20; no made-up numbers. See `assets/eval.md` |
| SC-5 | `cargo test -p evo-engine system_prompt` | passed (2026-11-19) |
| FR-37 | `bun test file-refs`; manual click in both apps | partly (2026-11-28): `file-refs.test.ts` 8 pass, `markdown-file-links.test.ts` 2 pass (the reply renders the name as a button); the click in the running apps not run |
| FR-38 | `the_prompt_asks_for_file_links` | passed (2026-11-28) |

FR-23 to FR-27, FR-29 to FR-36, SC-6 and US-9 to US-11 belong to stages
2–4; their rows are added when those stages are planned.
