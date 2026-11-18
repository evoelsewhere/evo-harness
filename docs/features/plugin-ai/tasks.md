# Plugin AI: plugins use the person's models directly — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-14 |

## 1. Tasks

Tests come before the code they check. `[P]` marks tasks of one phase with
disjoint files that may run in parallel.

### Phase 0 — Spikes

Done when: both spikes have results in the plan (section 1), and the plan
is changed where they disagree with it.

- [x] T1 S1: a `--mode plugin-ai` engine running two `ChildHarness` runs at once with streamed text and one external tool round-trip; cancel one — scratch branch of `crates/evo-engine` — check: results in plan.md §1
- [x] T2 [P] S2: NDJSON streaming from the host API to Deno `fetch`, first-byte delay — scratch — check: results in plan.md §1

### Phase 1 — Calls and model choice

Done when: a view and a backend stream an answer from a model chosen by
need among allowed connected models, with fallback, permissions, caps,
concurrency, Usage by plugin and Stop; no chat is touched.

- [x] T3 [FR-6, FR-7, FR-8] `choose`: ranking by tier, abilities, context, deprecated, pins and allowed set; tests first — `crates/evo-engine/src/plugin_ai/choose.rs` — check: `cargo test -p evo-engine plugin_ai::choose`
- [x] T4 [FR-1, FR-2, FR-3] Wire types — `crates/evo-protocol/src/plugin_ai.rs` — check: serde round-trip tests
- [x] T5 [FR-15, FR-14] Migration 0014 (`usage_events.plugin_id`, `plugin_ai_runs`, `plugin_ai_threads`, `plugin_ai_messages`), queries for spend per plugin, prune, delete per plugin; tests first — `crates/evo-store/src/{plugin_ai.rs,usage.rs}`, `migrations/0014_plugin_ai.sql` — check: `cargo test -p evo-store`
- [x] T6 [FR-1 – FR-5, FR-9, FR-15, NFR-3, NFR-4] The mode: loop, `ai_run` without tools (text, images, reasoning), streaming events, cancel, fallback, per-run budget, usage rows with `plugin_id`, `reload_providers` — `crates/evo-engine/src/plugin_ai/{mod,run}.rs`, `modes.rs`, `usage_log.rs` — check: engine tests with the `faux` provider (two candidates, the first failing)
- [x] T7 [FR-13, FR-16] Permissions `ai`, `ai.tools:*` with labels — `crates/evo-plugins/src/ui.rs` — check: `cargo test -p evo-plugins`
- [x] T8 [FR-1, FR-3, FR-4, FR-13, FR-14, FR-20, FR-21] Shell: engine start/idle stop/restart, run registry and followers, permission and cap checks, 3/6 concurrency with a 30 s queue, stop on off/revoke/uninstall/provider removal, fan-out of `reload_providers` — `src-tauri/src/plugin_ai/{mod,engine,runs}.rs` — check: shell tests with a fake engine
- [x] T9 [FR-1 – FR-3, FR-6] Bridge `evo.ai.models`, `run` (async iterator, cancel), `complete` — `plugin_ui/bridge.rs`, `client.js`, `src-tauri/src/plugin_ai/bridge.rs` — check: tests; running app
- [x] T10 [P] [US-5] Host API `GET /v1/ai/models`, `POST /v1/ai/run` (NDJSON), `GET /v1/ai/runs/{id}`, cancel — `plugin_host/`, `src-tauri/src/plugin_ai/host.rs` — check: tests over loopback; running app with a Deno backend
- [x] T11 [FR-13, FR-14, FR-21] Models tab (allowed providers/models, default, tier pins, caps, spend today and this month, running runs with Stop); consent row with the money note — `plugin-page/ai-tab.tsx`, `plugin-consent.tsx`, `lib/plugins.ts` — check: running app
- [x] T12 [P] [FR-15] Usage shows plugin rows by plugin name — `src/features/usage/*`, `src-tauri/src/agent_tools/app/usage.rs` — check: running app

### Phase 2 — Agent runs with the plugin's tools

Done when: a run calls view-defined and backend tools for several steps,
returns structured output when asked, and stops at its step limit.

- [x] T13 [FR-10, FR-11, FR-12, FR-22] Remote tools (`ai_tool_call` / `ai_tool_result`, 60 s per call), step limit, forced `respond` tool for schemas with one retry, data marking of tool results — `plugin_ai/{run,tools}.rs` — check: engine tests with `faux` tool calls
- [x] T14 [FR-11] View tool functions in `client.js` answered on `ai_tool_call`; backend tools through `pool.call_server`; "the view is closed" — `plugin_ui/client.js`, `src-tauri/src/plugin_ai/runs.rs` — check: tests; running app

### Phase 3 — Threads and runs that outlive views

Done when: a thread continues across runs, a run started by a view that
closed finishes and can be followed or read later, and the person reads
and deletes threads on the Models tab.

- [x] T15 [FR-18, FR-24] Thread load and append in the engine; kept events and results in `plugin_ai_runs`; prune after 7 days — `plugin_ai/threads.rs`, `evo-store` — check: engine tests
- [x] T16 [FR-24] `evo.ai.follow(runId)`, `evo.ai.runs()`; followers in the registry; the origin view closing does not stop a run — `client.js`, `src-tauri/src/plugin_ai/runs.rs` — check: running app
- [x] T17 [FR-19] Threads in the Models tab: list, read as a conversation, delete; removal with the plugin — `ai-tab.tsx`, `plugins.rs` — check: running app

### Phase 4 — Context and Evo's tools

Done when: a run attaches the open chat, files and a skill within its
permissions, fitted to the model, and uses Evo tools under the approval
mode with approvals that name the plugin.

- [x] T18 [FR-17] Context: open chat's last messages (`chat:read`, never incognito), files within `files:read:*`, a skill as instructions; fitting and the report of what was cut — `plugin_ai/run.rs`, `src-tauri/src/plugin_ai/runs.rs` — check: engine tests; running app
- [x] T19 [FR-16] Evo tools by group in the engine with the run's folder, sandbox config and approval gate; `ai_question` → the main window's dialog and an OS notice — `plugin_ai/tools.rs`, `src-tauri/src/plugin_ai/mod.rs`, `features/plugin-ui/ai-question-dialog.tsx` — check: running app (read, edit and a command under "ask")

### Phase 5 — Documentation and verify

Done when: every requirement is checked and the docs describe what was built.

- [x] T23 [FR-16] The `apps` group: Evo's browser and terminal tools for plugin runs, relayed from the engine to the shell's `agent_tools` with a context of their own instead of a chat (plan, T23) — `plugin_ai/run.rs`, `src-tauri/src/plugin_ai/mod.rs`, `agent_tools/` — check: running app (added 2026-11-15: Phase 4 refuses `apps` for now)
- [x] T20 [FR-23] evo-creator: `references/ai.md`, recipes, SKILL.md table; the skill test covers the names — `crates/evo-engine/bundled-skills/evo-creator/` — check: `cargo test -p evo-engine office::`
- [x] T21 A sample: the issue-desk plugin gains an "AI summary" card using a thread and a backend tool — `examples/plugin-settings-sample/` — check: running app
- [x] T22 ADD (process, data, interfaces, cross-cutting), ADR 0023 Accepted, UI map — `docs/architecture/`, `apps/desktop/README.md` — check: review

## 2. Verification

| Requirement | Check | Result |
| --- | --- | --- |
| FR-1 – FR-5 | Engine and shell tests; running app: a view streams an answer, cancel, no chat changed, no key in the view | passed on Windows (2026-11-14): `plugin_ai::run` tests (stream, usage as the plugin's, kept result and events, cancel stops at once, budget before a call) and shell `plugin_ai` tests; real engine over stdio with Claude Sonnet 5.5 (ready 0.66 s, first text 1.81 s, usage row `source=plugin, plugin_id`, a cancelled run kept as `cancelled`); running app: a view's `evo.ai.run` streamed 5 deltas into its page, `follow` after the end replayed the run, `ai.models` refused without `ai`; the view never sees a key (it only calls the bridge) |
| FR-6 – FR-9 | `choose` tests; running app with two providers: by tier, with vision, a disallowed provider never used, fallback after a forced failure | passed on Windows (2026-11-14): `choose` tests (tiers by price and ability, abilities filter with the missing one named, a named model must be allowed and fit, the person's pins and default first, tier marks) and `run` tests (429 falls back once, 400 does not, a 401 provider is skipped until providers reload); 48 connected models listed with tiers; found with real providers: the cheapest provider's key was invalid, so FR-9 now skips a provider that refuses its key |
| FR-10 – FR-12 | Engine tests; running app: a three-tool view agent, backend tools, a schema answer | passed on Windows (2026-11-14): `plugin_ai::run` tests (a tool call and its answer, bad input back to the model and never to the plugin, `respond` checked against the schema with one retry and plain JSON text accepted, the step limit, cancel while a tool runs) and shell tests (view tools only from views, backend tools only from the plugin's own servers, a view tool wins a name clash, only the run's plugin answers its tools); real engine with Claude Sonnet 5.5: two prices in one step, a schema answer through `respond`, `maxSteps: 2` stopped with `steps`; running app: a view's `pick_chart` function drew its chart and a throwing function came back to the model as an error, a backend's `/v1/ai/run` with `backendTools` called its own `quote` tool twice, inline `tools` from a backend got 400, and a view closed mid-run left the run going with "the view that offered this tool is closed" as the tool's answer |
| FR-13 – FR-15 | Running app: consent with the money note, caps refuse and stop, Usage by plugin | passed on Windows (2026-11-14): the consent row read "Use your AI models — this may cost money on your providers" with the plugin's reason; a daily cap under what was spent refused the next run (`budget`); the Models tab showed spend, recent runs with threads and the person's default model; Usage showed "Plugins' AI" with the plugin's name and its responses by name. A cap reached in the middle of a run: shell test only |
| FR-16 | Running app: read, edit and command tools under "ask", approvals naming the plugin | passed on Windows (2026-11-15): engine tests (groups give their tools only; the approval mode decides as in a chat; "Allow for this run" stops the asking and an edit grant is not a command grant; a command that would stop Evo always asks; reads never ask; a denied edit leaves the file alone; only allowed groups are offered and Evo's names win a clash; a missing folder ends the run before any model call; an app tool's images reach the model) and shell tests (each group needs its `ai.tools:<group>` permission; a run gets the browser and terminal tools only; a shell question is answered in the shell, -1 when its call or run ends); running app with Claude Sonnet 5.5 under "Ask every time": `read` ran without asking, "ai-insight: Write file?" named the plugin and the folder, Allow once wrote `summary.md`, a denied `mkdir` was not run and the model said so; with `apps`: "ai-insight: Let the agent work on https://example.com?" and "Run a command in Windows PowerShell?" asked in the plugin-AI dialog, the terminal printed `hello-from-plugin`, a screenshot reached the model as an image (it described the page), a page on this machine was read without asking (`High tide at 6:42`), and the run's tab and terminal were gone when it ended. The OS notice when no window is in front was not seen |
| FR-17 | Engine tests; running app: open chat attached, incognito refused, files outside approved places refused | passed on Windows (2026-11-15): engine tests (files, images and the chat marked as data, front matter dropped from the skill, unreadable, binary and too-large files reported in `started.cut`; the largest pieces go first and images need a model that reads them; pieces go before the last user words; a thread keeps "[context was attached]" instead of them) and shell tests (each part needs its permission; files only inside approved places, `..` cannot leave one, relative paths from the workspace; a skill is one of the plugin's own); running app with Claude Sonnet 5.5: one run with the open chat, `notes.md` and the plugin's `analyst` skill answered "ANALYST: … Thursday at 5pm … price target of 210", a file outside the approved place got `no_permission`, an unknown skill `bad_request`. Incognito chats: never listed as open (`active_chat`, plugin-ui's check) |
| FR-18, FR-19, FR-24 | Running app: a thread continued, read and deleted; a run outliving its view and followed later | passed on Windows (2026-11-14): store test (turns appended, listed, read newest-last, per plugin, deleted, removed with the plugin); engine tests (a thread continues with its tool steps, another plugin's thread of the same name is its own, a failed run adds nothing, the oldest turns go first and whole, a kept turn has no reasoning, images or open tool calls); shell tests (a thread reads as a conversation with its tool steps, a thread with a run going cannot be deleted); running app with Claude Sonnet 5.5: a second run in thread `AAPL` remembered the code word and the chart a view tool showed in the first, thread `MSFT` did not, a second run in a busy thread got `busy`, `evo.ai.threads()` / `thread()` and the host API's `/v1/ai/threads` read them, the Models tab read the thread as a conversation with its `pick_chart` step and deleted `MSFT`, and uninstalling removed every thread, message and run. A run outliving its view and followed after reopening: Phase 2's check (2026-11-14) |
| FR-20, FR-21 | Shell tests; running app: the fourth run queues; Stop on the Models tab | passed on Windows (2026-11-15): shell tests (per-plugin and overall counts, followers, a finished run forgotten); running app: four runs at once from the issue-desk page — three started at once, the fourth was accepted 4.2 s later, right after one ended; Stop on the plugin's Models tab ended a running run with `cancel` |
| FR-22 | Engine test: instructions in a tool result change no tool or permission | passed (2026-11-14): `instructions_in_a_tool_result_add_no_tools` (a page telling the model it may call `delete_all`: the call never reaches the plugin; the system prompt marks given text as data). Evo tools' permissions: Phase 4 |
| FR-23 | Skill test | passed (2026-11-15): `office::tests::evo_creator_names_every_ai_call` (references/ai.md names every `evo.ai` call in client.js, every `/v1/ai/*` route of the host API and every AI permission) and `bundled_evo_creator_matches_what_evo_reads` |
| NFR-1 | Measured, warm and cold | partly (2026-11-14): engine ready 0.66 s from start; first text 1.8 s warm and 2.4 s cold (provider time included) with Claude Sonnet 5.5 — inside "provider first token + 1 s / + 5 s"; not measured against a second provider |
| NFR-2 | Review; a test plugin tries another plugin's runs and threads, and logs are checked for prompt text | passed on Windows (2026-11-15): shell tests (follows, tool answers, threads and deleting them checked against the calling plugin); running app: a second plugin got `not_found` following issue-desk's run, reading and deleting its thread, and listed no threads or runs; no prompt or answer text in the app's output, the agent's log folder or `app_logs`; a view never sees a provider key (only the bridge) |
| NFR-3 | Usage totals against providers' reported usage, including cancelled and failed calls | passed on Windows (2026-11-15): the plugin's `usage_events` rows and its runs' costs summed to the same $0.027094 over 12 calls; found: a stopped call was counted as the provider's aborted stream reported it (3 output tokens, $0) — fixed (`counted`: at least the streamed text, priced at the model's rates when the provider gave no cost; engine test), after which a stopped run counted 155 output tokens and $0.0017. Failed calls count what the provider reported (engine tests) |
| NFR-4 | Plan review: no provider call outside the engine | passed (2026-11-14): the shell starts the engine and relays JSON; every provider call is in `evo-engine/src/plugin_ai/run.rs` |
| NFR-5 | Windows and macOS | not run |
