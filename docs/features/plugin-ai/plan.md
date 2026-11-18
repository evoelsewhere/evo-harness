# Plugin AI: plugins use the person's models directly — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-14 |
| ADD sections | [4. Process view](../../architecture/README.md#4-process-view) · [7. Data view](../../architecture/README.md#7-data-view) · [8. Interfaces](../../architecture/README.md#8-interfaces) · [9. Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts) |
| ADRs | [0023 Plugin AI runs in one long-lived engine process](../../architecture/decisions/0023-plugin-ai-runs-in-one-long-lived-engine-process.md) · [0016 Memory indexing in a short-lived engine](../../architecture/decisions/0016-memory-indexing-runs-in-a-short-lived-engine-process.md) · [0022 Plugin code under Evo's runtimes](../../architecture/decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md) |

## 1. Context and constraints

- **What exists.** Model calls happen only in engines: a chat's engine
  (its agent loop, subagents, side questions, `child_agents` for `cdylib`
  plugins) and the short-lived `--mode index` process of memory indexing.
  Each records its usage through `usage_log` into `evo.db`'s
  `usage_events`, whose `source` already allows `plugin` (unused by plugin
  views or backends) but has no column naming the plugin.
- **Building blocks in the engine:** `evo_harness::agent_harness::ChildHarness`
  with an in-memory session (as `side_questions.rs` and `child_agents.rs`
  use it) runs an agent loop with chosen tools, bounded turns and
  cancellation; `evo-runtime` resolves providers and credentials and reloads
  them live; `models_dev::DevModel` carries `reasoning`, `tool_call`,
  `vision`, `context_window`, `max_tokens`, `cost`, `deprecated` — enough to
  rank models without hardcoding any.
- **Building blocks in the shell:** the plugin bridge (`plugin_ui/bridge.rs`)
  already maps a view to its plugin, checks permissions and pushes events to
  a view (`chat.onEvent`); the host API (`plugin_host/`) authenticates
  backends and hooks by key; the MCP pool calls a plugin's backend tools
  (`call_server`); the supervisor and `agent_tools::ask_person` hold
  questions; the settings fan-out sends `reload_providers` to engines.
- **Constraints:** the engine owns agent logic (NFR-4) — the shell never
  calls a provider; models from models.dev only; a run may use any Evo tool,
  under the approval mode (spec 6.2); runs outlive views (FR-24); no default
  caps (FR-14); threads live in `evo.db` and show only on the plugin's page.
- **Schema:** this feature adds migration 0014. A debug build that applies
  it to the real `~/.evo/agent/evo.db` locks out an older installed release
  (it refuses a newer schema): development and e2e checks run on a scratch
  agent folder, and the release ships the migration with the app.
- **Spikes before the main work** (Phase 0):
  - S1: a `--mode plugin-ai` engine runs two `ChildHarness` runs at once
    with streamed text, one with an external tool round-trip over stdio;
    cancel stops one without the other.
  - S2: the host API streams NDJSON to a Deno `fetch` (a close-delimited
    HTTP/1.1 body) with first bytes under 50 ms after the engine's first
    delta.
- **Spike results** (Windows 11, 2026-11-14):
  - S1 (an `evo-harness` test with the `faux` provider, removed after):
    two runs, each an `AgentHarness` of its own over an in-memory session,
    ran at once in one process — building both took 6 ms; run B streamed
    9 text deltas while run A waited 313 ms on a "remote" tool answered
    after 300 ms over a channel (13 ms overhead), then A streamed its
    answer. Cancelling a run by dropping its future (as `side_questions`
    does) stopped its stream at once (no delta after 20 ms); the other run
    finished. Consequence: a run needs no parent harness — `ChildHarness`
    (which needs one) is not used; each run is `AgentHarness::create` with
    the run's model, tools and system prompt (ADR 0023's "in-memory
    `ChildHarness`" is this harness of its own). The JSONL plumbing over
    the engine's stdio is the same as chat engines' and was not part of
    the spike.
  - S2: a close-delimited HTTP/1.1 NDJSON body (no `Content-Length`, no
    chunking, `Connection: close`), one line every 100 ms: Deno `fetch`
    read each line 0–7 ms after it was sent, Node `fetch` 0–17 ms, Python
    `http.client` 0 ms. The host API streams this way; its HTTP code
    writes the head, then each event line as it comes, then closes.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | Model choice, the agent loop, tools, structured output and thread history are the engine's (`plugin_ai/` in `evo-engine`); the shell starts the process, routes calls, enforces permissions, concurrency and caps, and stores nothing of a run's content itself. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | `evo-store` gains tables and queries; `evo-plugins` gains permission ids; `evo-protocol` gains the plugin-AI wire types. Nothing in a library knows the shell. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | ok, new plane | A new engine mode with its own command set (`ai_*`), typed in `evo-protocol::plugin_ai`, used by the engine and the shell together. Chat engines' protocol is unchanged. |
| Models come from models.dev; no hardcoded model facts | ok | Tiers and capabilities are computed from `DevModel` fields at run time; the person may pin models. No model id appears in code. |
| Real engine in the app; settings and credentials reload live | ok | The plugin-AI engine is a real engine; it gets `reload_providers` like chat engines. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok, with a new trust boundary | Plugins (untrusted) reach models and Evo tools. `ai` and each tool group are permissions; Evo tools run in the engine's existing gate chain (approval mode, sandbox config, project trust); tool results and attached context are marked as data. ADR 0023. |
| Desktop UI structure and `DESIGN.md` | ok | A Models tab in the plugin page, a consent row, an approval dialog, Usage labels; stores and features as the UI map says. |

## 3. Design

### 3.1 Overview

```text
 view (window.evo.ai)                    backend / hook (host API /v1/ai/*)
   │ bridge: ai.models · ai.run ·          │ POST /v1/ai/run → NDJSON stream
   │ ai.cancel · ai.toolResult · ai.follow │ GET /v1/ai/models · POST /v1/ai/runs/{id}/cancel
   ▼                                       ▼
 shell plugin_ai/ ── permissions (ai, ai.tools:*), caps, 3/6 concurrency + 30 s queue,
   │                 run registry (run → plugin, origin, followers), approvals queue
   │ stdin/stdout JSONL (evo-protocol::plugin_ai)
   ▼
 evo-engine --mode plugin-ai   (one process; started on first call; stopped after 5 idle min)
   ├─ choose: allowed models × need × budget → ordered candidates (models.dev data)
   ├─ run: an AgentHarness per run (in-memory session seeded from its thread),
   │        tools = remote (view functions, backend MCP tools) + evo tools by group,
   │        structured output through a forced `respond` tool,
   │        fallback to the next candidate on retryable errors
   ├─ approvals: evo tools through the approval gate → ai_question to the shell
   └─ evo.db: usage_events (source plugin, plugin_id) · plugin_ai_runs · plugin_ai_threads/_messages
```

A run belongs to a plugin, not to a window: the shell keeps it in a
registry, forwards its events to every follower (the starting view, other
views of the plugin that follow it, a backend's open stream), and the
engine stores its result and its thread so a view that opens later can read
it (FR-24).

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| protocol | `crates/evo-protocol/src/plugin_ai.rs` (new) | Wire types: commands `ai_models`, `ai_run`, `ai_tool_result`, `ai_answer`, `ai_cancel`, `reload_providers`, `stop`; events `ai_event {runId, event}` (`started` with model, `text`, `reasoning`, `tool_call`, `tool_result`, `retry` with the next model, `usage`, `done` with reason, result, usage and cost, `error`), `ai_tool_call {runId, callId, tool, input}`, `ai_question {runId, id, title, detail, options}`. | FR-1 – FR-3, FR-10, FR-11, FR-16 |
| engine | `crates/evo-engine/src/plugin_ai/{mod,choose,run,tools,threads}.rs` (new), `modes.rs` | The mode's loop; `choose` ranks candidates (pure, tested); `run` builds an `AgentHarness` of its own (in-memory session) with the chosen model, system prompt (with the data marking of FR-22), messages, tools and step limit, streams events, retries on retryable errors with the next candidate, enforces the per-run budget between model calls; `tools` makes remote tools (await `ai_tool_result`, 60 s per call) and Evo tools by group with an `ExecutionEnv` in the run's folder and the saved sandbox config, approvals through `ai_question`; `threads` loads and appends a thread's messages. | FR-1 – FR-12, FR-16 – FR-18, FR-22, FR-24, NFR-1, NFR-4 |
| engine | `crates/evo-engine/src/usage_log.rs` | `Source::Plugin` rows carry `plugin_id`. | FR-15, NFR-3 |
| crate | `crates/evo-store/src/{plugin_ai.rs (new),usage.rs}`, `migrations/0014_plugin_ai.sql` | Tables for runs and threads; `usage_events.plugin_id`; queries for spend per plugin (today, month), runs, threads; delete a plugin's rows. | FR-14, FR-15, FR-18, FR-19, FR-24 |
| crate | `crates/evo-plugins/src/ui.rs` | Permissions `ai` ("Use your AI models (may cost money)") and `ai.tools:read`, `ai.tools:edit`, `ai.tools:command`, `ai.tools:web`, `ai.tools:apps`, with labels and "not optional unless declared". | FR-13, FR-16 |
| desktop shell | `src-tauri/src/plugin_ai/{mod,engine,runs,bridge,host}.rs` (new) | Start, stop (5 idle min) and restart the engine; send it `reload_providers` with the settings fan-out; the run registry and routing (events to followers; `ai_tool_call` to the view's function or `pool.call_server`); permission checks; caps from the person's settings and spend so far; 3 per plugin / 6 overall with a 30 s queue; stop runs of a plugin turned off, revoked or uninstalled; approvals to the main window (`plugin-ai://question`, OS notice when unfocused). | FR-1, FR-3, FR-4, FR-13 – FR-16, FR-20, FR-21, FR-24 |
| desktop shell | `src-tauri/src/plugin_ui/{bridge.rs,client.js}` | `evo.ai.models()`, `evo.ai.run(options)` returning an async iterator and `.cancel()`, `evo.ai.complete(options)`, `evo.ai.follow(runId)`, `evo.ai.runs()`, `evo.ai.thread(id)` helpers; view tool functions kept in the view and answered on `ai_tool_call`. | FR-1 – FR-3, FR-6 – FR-12, FR-18, FR-24 |
| desktop shell | `src-tauri/src/plugin_host/` | `GET /v1/ai/models`, `POST /v1/ai/run` (NDJSON stream), `GET /v1/ai/runs/{id}` (result), `POST /v1/ai/runs/{id}/cancel`; backend tools by name only. | FR-1, US-5 |
| desktop shell | `src-tauri/src/plugins.rs` | Plugin AI settings in `installed.json` (allowed providers/models, default model, tier pins, caps); describe adds them and spend; uninstall deletes its runs and threads. | FR-14, FR-19 |
| desktop UI | `src/features/customize/plugin-page/ai-tab.tsx` (new), `plugin-consent.tsx`, `src/features/plugin-ui/ai-question-dialog.tsx` (new), `src/features/usage/*` | Models tab: models allowed, default model, tier pins, caps, spend, running runs with Stop, threads (read as a conversation, delete); consent row with the money note; the approval dialog for plugin runs; Usage shows plugin rows by name. | FR-13, FR-14, FR-16, FR-19, FR-21 |
| skill | `crates/evo-engine/bundled-skills/evo-creator/references/ai.md` (new), `recipes.md`, SKILL.md | The AI API for plugin authors, with patterns (a streamed card, an agent with view tools, a thread per subject). | FR-23 |

### 3.3 Interfaces

```ts
// window.evo.ai (needs `ai`)
evo.ai.models(): Promise<Array<{ provider: string; model: string; name: string;
  vision: boolean; tools: boolean; reasoning: boolean; context: number;
  cost: { input: number; output: number } | null; tiers: ("fast"|"balanced"|"best")[] }>>

const run = evo.ai.run({
  messages: [{ role: "user", content: "…" | [{ type: "text", text }, { type: "image", data, mimeType }] }],
  system?: string,
  model?: { provider: string; model: string },            // else chosen by need
  need?: { tier?: "fast"|"balanced"|"best"; vision?: boolean; tools?: boolean; reasoning?: "off"|"low"|"medium"|"high" },
  budget?: { maxCostUsd?: number; maxOutputTokens?: number },
  schema?: object,                                          // JSON Schema of the answer
  tools?: Record<string, { description: string; input: object; run(input): Promise<unknown> }>,
  backendTools?: boolean | string[],                        // the plugin's backend MCP tools
  evoTools?: ("read"|"edit"|"command"|"web"|"apps")[],     // groups the person approved
  maxSteps?: number,                                        // default 8, at most 50
  folder?: string,                                          // evo tools' folder; default the open workspace
  context?: { openChat?: { lastMessages: number }; files?: string[]; skill?: string },
  thread?: string,                                          // continue / start this thread
});
for await (const event of run) { /* started · text · reasoning · tool_call · tool_result · retry · done · error */ }
run.id; run.cancel(); await run.result;                     // { text, json?, model, usage, costUsd, stopped: "end"|"steps"|"budget"|"cancel"|"error" }
evo.ai.complete(options) → Promise<result>                  // run() without the stream
evo.ai.follow(runId) → the same iterator, from what is kept // FR-24
evo.ai.runs() → [{ id, thread?, status, model, startedAt, costUsd }]
```

Host API (backend key; `ai` and tool permissions checked as for views):

```text
GET  /v1/ai/models                 → [{provider, model, …}]
POST /v1/ai/run  {…as above, tools by backend name only}  → NDJSON: one event per line
GET  /v1/ai/runs/{id}              → {status, result?}
POST /v1/ai/runs/{id}/cancel       → 204
errors: no_permission · no_model (with what is missing) · budget · busy · offline · bad_request
```

Engine (`evo-engine --mode plugin-ai`, JSONL on stdio; `evo-protocol::plugin_ai`):

```jsonc
→ {"type":"ai_run","runId":"r1","plugin":{"id":"p@m","name":"p","root":"…","skills":[…]},
   "candidates":[{"provider":"a","model":"m1"},{"provider":"b","model":"m2"}],   // allowed set; engine ranks with need
   "need":{…},"budget":{…},"system":"…","messages":[…],"schema":{…},
   "remoteTools":[{"name":"getPrice","description":"…","inputSchema":{…},"origin":"view"}],
   "evoTools":["read","web"],"maxSteps":8,"folder":"…","context":{…},"thread":"AAPL"}
← {"type":"ai_event","runId":"r1","event":{"type":"started","provider":"a","model":"m1"}}
← {"type":"ai_tool_call","runId":"r1","callId":"c1","tool":"getPrice","input":{…}}
→ {"type":"ai_tool_result","runId":"r1","callId":"c1","result":{…},"isError":false}
← {"type":"ai_question","runId":"r1","id":"q1","title":"p wants to run a command","detail":"…","options":["Allow once","Deny"]}
→ {"type":"ai_answer","id":"q1","index":0}
→ {"type":"ai_cancel","runId":"r1"}
← {"type":"ai_event","runId":"r1","event":{"type":"done","stopped":"end","result":{…},"usage":{…},"costUsd":0.0031}}
→ {"type":"ai_models","allowed":{…}}  ← {"type":"response","data":[…]}
```

Settings (per plugin, in `installed.json`):
`ai: { providers?: [id], models?: [{provider, model}], defaultModel?, tiers?: {fast?, balanced?, best?}, capDayUsd?, capMonthUsd? }`.

### 3.4 Data and storage

| Where | What | Notes |
| --- | --- | --- |
| `evo.db` `usage_events.plugin_id` (migration 0014, `ALTER TABLE … ADD COLUMN`) | the plugin of a `source = plugin` row | Spend per plugin per day and month is a query on it. |
| `evo.db` `plugin_ai_runs` (0014) | `id, plugin_id, thread_id?, status, provider, model, started_at, ended_at, stopped, cost_usd, result_json, events_json` (the last 2,000 events, for `follow`) | Kept 7 days, then pruned; removed with the plugin. |
| `evo.db` `plugin_ai_threads`, `plugin_ai_messages` (0014) | a thread per `(plugin_id, thread_id)` with a title; its messages in order (role, content JSON, tool steps, model, at) | Written by the engine; read by the shell for the Models tab; removed with the plugin. |
| `installed.json` | the plugin's `ai` settings | No secret. |
| memory (shell) | the run registry, followers, the queue | A run's state survives a view closing; not an app restart (edge 12). |

Prompts and answers are stored only in threads and in a run's kept result
and events; never in logs (NFR-2).

### 3.5 Behaviour

- **Start:** the first `ai.*` call starts the engine (`--mode plugin-ai`,
  with the sandbox config and approval mode as chat engines get them); it
  stays while calls come and stops after 5 idle minutes without runs.
- **A run:** the shell checks `ai`, every requested `ai.tools:*`, the caps
  (spend today and this month from `usage_events`), the concurrency (3 per
  plugin, 6 overall; a 30 s queue); computes the allowed candidate set from
  the person's rules and the connected providers; registers the run (its
  plugin, origin, followers) and sends `ai_run`. The engine ranks the
  candidates by need (`choose`), starts the first, and streams.
- **Choosing (`choose`, pure):** filter by required abilities (vision,
  tools when tools or a schema are asked, reasoning level supported),
  context ≥ the request's size, not deprecated; tier `fast`: cheapest
  output price first; `best`: reasoning models first, then largest context,
  then highest price (as a proxy for capability) within each provider;
  `balanced`: the median of the price order; a tier pinned by the person
  wins; a named model must be in the set. Ties keep the person's provider
  order.
- **Fallback:** on rate limit, overload, a 5xx or a timeout before any
  text, the run moves to the next candidate once and emits `retry`.
- **Budget:** before each model call, the engine checks the run's cost so
  far against `maxCostUsd`; the shell checks the plugin's caps when the run
  starts and on each `usage` event, and cancels when one is passed (at most
  one call over, SC-4).
- **Structured output:** a forced `respond` tool with the schema as its
  input; the engine validates and retries once with the error, then
  returns the error and the raw text (edge 9).
- **Tools:** remote view tools go to the follower view that defined them
  (the origin view); if it is gone the result says so (edge 5). Backend tools
  go through `pool.call_server` (waking the backend). Evo tools run in the
  engine through the same gate chain as a chat's, with approvals as
  `ai_question`: the shell shows them in the main window naming the plugin,
  sends an OS notice when no window is focused, and answers `-1` (deny) if
  the run is cancelled.
- **Context:** `openChat` reads the window's open chat's last N messages
  through the shell (needs `chat:read`; never incognito) and passes them as
  a quoted block; `files` are read by the engine within approved
  `files:read:*` places; `skill` loads the plugin's skill body as the
  system prompt's start. The engine fits everything to the model's context
  (the thread's oldest turns first, then the largest attachments) and
  reports what it cut in `started`.
- **Threads:** `thread` loads its messages into the run's session; at the
  end the new turn (user message, tool steps, answer) is appended.
- **Outliving a view:** the registry keeps the run; when the origin view
  closes, its followers drop it, the run goes on; `follow(runId)` replays
  the kept events and continues live; `runs()` lists recent runs.
- **Stops:** the plugin turned off, `ai` revoked, uninstalled, a provider
  removed mid-run, the person's Stop: the shell sends `ai_cancel`.

### 3.6 Errors and edge cases

| Case | What happens |
| --- | --- |
| No connected provider, or none allowed | `no_model` with "connect a provider" or "allow a model for <plugin>"; the Models tab says the same (edge 1). |
| No model fits the need | `no_model` naming the missing ability (US-2.4). |
| Provider removed during a run | The engine's next call fails; the run ends with that reason (edge 2). |
| Engine exits | Runs end with `error: the AI engine stopped`; the next call starts it again. |
| View tool after the view closed | Tool result `the view is closed`; the model may go on (edge 5). |
| Loops | `maxSteps` and the caps end it (edge 6). |
| Offline | `offline`; nothing billed (edge 7). |
| Too many runs | Queued up to 30 s, then `busy` (edge 10). |
| App quits | Runs end; threads keep finished turns (edge 12). |

### 3.7 Security and trust

- **New boundary:** untrusted plugin code can spend the person's money and,
  with `ai.tools:*`, act through Evo's tools. Gates: `ai` and each tool
  group are consent permissions with reasons; the consent states the money;
  caps when set; the approval mode for Evo tools (its "auto" mode runs them
  without asking — the consent for `ai.tools:command` says so).
- **No keys:** provider credentials stay in the engine; models and prices
  are all the plugin learns.
- **Isolation between plugins:** runs, threads and follows are checked
  against the calling plugin (bridge's view → plugin map, host API key).
- **Prompt injection:** tool results and attached context go into the run
  as quoted data with a system instruction that they are not instructions;
  nothing in them changes the run's tools or permissions.
- **Logs:** run ids, plugins, models, token counts and costs only.
- ADR 0023 records the process and the boundary.

### 3.8 Configuration

| Setting | Where | Default |
| --- | --- | --- |
| A plugin's allowed providers/models, default model, tier pins, caps | its Models tab (`installed.json`) | all connected providers; no default; no pins; no caps |
| Engine idle stop | constant | 5 minutes |
| Concurrency | constants | 3 per plugin, 6 overall, 30 s queue |
| `maxSteps` | per run | 8 (at most 50) |
| Kept runs | constant | 7 days |

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Run plugin AI in the open chat's engine (a side question or a child run there) | Ties a plugin to a chat and its engine's life; a chat in a long turn or with no engine blocks it; usage and approvals land in that chat (against FR-4). |
| A short-lived engine per run (as indexing) | Seconds of start-up per run (against NFR-1) and no shared state for queues, follows and approvals. |
| The shell calls providers itself | Breaks "the engine owns agent logic", duplicates provider code and credentials handling. |
| Store a plugin's id in `usage_events.chat_id` instead of a new column | Mixes two meanings in one column and confuses every chat query; a nullable column is a cheap, additive migration. |
| Threads in the plugin's data folder | The person chose Evo's database (spec 6.2); a folder cannot be queried for the Models tab and is the plugin's to change. |
| One engine per plugin | More processes for little gain; one engine with per-run isolation and shell-side limits is enough. |

## 5. As-built notes

Phase 1 (calls and model choice), 2026-11-14:

- **The run loop calls the provider directly** (`stream_simple`, as
  `--mode index` does), not through an `AgentHarness`: a run without tools
  is one model call, and the plugin's history and images map straight to
  the provider's context. Phase 2 decides between this loop with tools and
  a harness per run (spike S1 showed both work).
- **The shell speaks the plugin-AI JSON without linking `evo-protocol`**
  (which pulls `evo-ai` and `evo-agent` into the shell build), as it does
  with chat engines; the shapes in `evo-protocol::plugin_ai` are the
  contract and its tests pin them.
- **`RunEnv`** carries what a run reaches outside itself (provider
  resolution, run storage, usage), so tests use `faux` providers and never
  open the real `evo.db`.
- **Providers that refuse their key** (401, "invalid API key") are left out
  of plugin runs until `reload_providers` (spec FR-9 widened): with real
  providers the cheapest connected one had an invalid key and every `fast`
  run failed.
- **Spend** is read from `usage_events.plugin_id` (local day and month);
  Usage gains `byPlugin` and each response's `pluginId`.
- **The host API's AI routes** are async and outside the 15 s connection
  deadline (a run streams as long as it runs); other routes keep it.
- **The plugin page's tab is "Models"** (the person's word for what it holds:
  which models, default, tiers, budget, spend, runs).
- **Incident:** a running `tauri dev` rebuilt itself on the shell edits and
  applied migration 0014 to the real `~/.evo/agent/evo.db`; the new tables
  were empty and it was rolled back to schema 13 with the person's OK. E2E
  checks run on a scratch agent folder.

Phase 2 (agent runs with the plugin's tools), 2026-11-14:

- **The loop stays a direct provider loop** (Phase 1's), now with steps:
  each step is one `stream_simple` call; tool calls in a step run in order;
  the budget is checked before every step; a failure after anything was
  streamed or a tool ran is not retried on another model. A harness per
  run is not needed until Evo's tools (Phase 4) want its gate chain.
- **Tools live in `plugin_ai/run.rs`** (no `tools.rs` yet): the engine
  checks the model's input against the tool's schema before the plugin sees
  it, and answers an unknown tool or bad input to the model itself.
- **`respond`**: a run with a `schema` gets an extra `respond` tool whose
  parameters are the schema; its arguments are checked
  (`evo_ai::schema::validate_tool_arguments`), and a wrong shape gets one
  retry. Plain JSON text that matches is accepted too. `respond` is a
  reserved tool name.
- **Step limit:** 8 by default, `maxSteps` 1 – 50; reaching it ends with
  `stopped: "steps"` and the last text.
- **The shell routes tool calls in `plugin_ai/mod.rs`** (no `runs.rs`):
  each run keeps its origin view and a route per tool (view, or a server's
  pool key and tool name). View tools come only from views (`tools`); a
  backend offers its own servers' tools with `backendTools` (true or names),
  and the host API refuses inline `tools` (a backend has no function to
  answer with). `backendTools` reads the pool's list for servers keyed
  `plugin:<name>:`; a view tool wins a name clash. At most 32 tools.
- **A tool's answer only goes to a running engine** (it never starts one),
  which also keeps the shell's engine start out of a recursive async cycle.
- **`client.js`** takes `tools` as `{name: {description, input, run}}` or a
  list; functions stay in the view, the shell gets the definitions, and a
  call that arrives before `ai.run` answers waits for the run's id. A
  function that throws answers the model with an error.

Phase 3 (threads and runs that outlive views), 2026-11-14:

- **Threads live in `plugin_ai/run.rs`** (no `threads.rs`): a run with
  `thread` loads its last 400 messages, fits them beside the new messages
  (three quarters of the model's window, less output, system prompt and
  tools; oldest turns first and whole, so what is kept starts at a user
  message) and says how many it left out in `started.droppedMessages`. At
  the end the turn is appended if the model answered at least once.
- **A kept turn** is the provider messages as JSON (`plugin_ai_messages`),
  without reasoning, with images as a note (`[an image was attached]`) so
  the database does not grow with them, and with an answer added to any
  tool call left open (the run stopped, or `respond`), so the thread
  replays cleanly with any provider.
- **One run per thread at a time:** the shell refuses a second run in a
  thread with one going (`busy`), and a thread with a run going cannot be
  deleted.
- **Reading threads:** the shell turns kept messages into a conversation
  (`thread_view`: words, the tools each answer called with their input and
  what came back, the model). Plugins read and delete their own threads
  (`evo.ai.threads()`, `thread(id)`, `deleteThread(id)`; host API
  `GET /v1/ai/threads`, `GET` / `DELETE /v1/ai/threads/{id}`); the person
  on the Models tab (`ai-threads.tsx`).
- **T16 came with Phase 1 and 2:** `follow`, `runs` and the followers were
  built in Phase 1; Phase 2's running-app check closed the origin view
  mid-run and followed the run from the reopened view.
- **Found by the thread test:** a run's final answer was not added to its
  history before the run returned, so it would have been missing from the
  thread; fixed.

Phase 4 (context and Evo's tools), 2026-11-15:

- **Context** (`plugin_ai/context.rs` in the engine and in the shell): the
  shell checks each part — `chat:read` for `openChat` (the window's open
  chat, never incognito; 20 messages by default, at most 100),
  `files:read:<folder>` / `files:read:workspace` for files (at most 10,
  resolved and kept inside a place; relative paths from the open
  workspace), the plugin's own skill folders for `skill` — and sends the
  engine the chat's lines, the files' paths and the skill's path. The
  engine reads them (text up to 200 KB, images up to 5 MB, the skill
  without its front matter, up to 50 KB), marks each as
  `<attached …>` data before the last user words, puts the skill before
  the plugin's `system`, and fits: the thread's oldest turns first, then
  the largest pieces; images only for models that read them. What was left
  out or shortened is in `started.cut`. A thread keeps
  `[context was attached]` instead of the context.
- **Evo's tools** (`plugin_ai/tools.rs`): groups `read` (read, grep, find,
  ls), `edit` (edit, write), `command` (bash, and powershell on Windows),
  `web` (web_search, web_fetch when web is on), built for the run's folder
  (its `folder`, else the window's open chat's folder, else home). They run
  in the engine, not through an `AgentHarness`: the run loop validates the
  input, gates the call and executes it. Evo's names win a clash with the
  plugin's tools.
- **Approvals:** the shell sends the person's approval mode and sandbox
  config with each run (the plugin-AI engine is long-lived, and the
  settings may change while it runs); the engine applies the sandbox
  config before building the tools and gates each call with
  `approval::classify` and the chat's rule (reads never ask; `ask` asks for
  edits and commands, `auto-edits` for commands, `auto` never), and a
  command that would stop Evo always asks (`self_kill`; the shell now
  starts the engine with `EVO_HOST_PID` / `EVO_HOST_EXE`). A question is
  `ai_question` → the main window's dialog
  (`features/plugin-ui/ai-question-dialog.tsx`) titled "<plugin>: …" with
  the folder, and an OS notice when no window is in front; options Allow
  once, Allow for this run, Deny (Allow once and Deny for a command that
  would stop Evo). Closing the dialog denies; a run that ends closes its
  questions; open questions are listed again when the window reloads.
- **Not yet: the `apps` group.** App tools (browser, terminal and others)
  run in the shell's `agent_tools`, which work per chat; plugin runs have
  none. `evoTools: ["apps"]` is refused for now (T23).

Phase 5 (documentation and verify), 2026-11-15:

- **evo-creator** gains `references/ai.md` (every `evo.ai` call, options,
  events, refusals, the host API's `/v1/ai/*`, permissions, patterns), a
  recipe and a row in SKILL.md; its test reads the calls from `client.js`
  and the routes from `plugin_ai/host.rs`, so a new call without docs
  fails it.
- **The sample** (`examples/plugin-settings-sample`, issue-desk) summarises
  an issue on its page: `need: { tier: "fast", tools: true }`, a thread
  per issue for follow-ups, the backend's new `issue_detail` tool for the
  model (with the person's GitHub sign-in, or public issues without), a
  cost cap per run. Models add Markdown even when told not to; the page
  shows text only, so it drops bold markers.
- **Stopped calls are counted** (NFR-3): a provider's cut stream reports
  little output and no cost, so the run counts at least the text that
  streamed (about 4 bytes a token), priced at the model's models.dev rates
  when the provider gave none (`run::counted`).
- **Left:** the `apps` group (T23), NFR-1 against a second provider, and
  NFR-5 (macOS). The spec stays In progress until they pass.

T23 (the `apps` group), 2026-11-15 — design:

- **Which tools:** the browser (`browser_*`) and terminal (`terminal_*`)
  tools of the shell's `agent_tools`. Tools that only mean something in a
  chat (cards, panels, previews, sending or showing files, scheduled tasks,
  reading sessions or usage) are not offered, nor `computer_app` (control
  of other apps; ADR 0015 keeps it to a chat's agent the person watches).
- **Where they run:** in the shell, as for a chat's agent, with a context
  of their own instead of a chat: the owner key `plugin-ai:<run id>` (its
  browser tabs open in the background, its terminals belong to it), the
  run's folder, the plugin's name on every approval. The shell sends their
  definitions with the run (origin `app`); the engine relays the model's
  calls (`ai_tool_call`, origin `app`) and waits up to 15 minutes for one,
  since the person may be asked.
- **Approvals:** the shell's own gate (`agent_tools/gate.rs`, per class and
  site, by the approval mode) asks through the plugin-AI dialog, with
  "Allow for this run" for its "Allow for this chat".
- **Results:** text and images (a screenshot reaches the model as an
  image).
- **The end of a run** closes its tabs and kills its terminals.

T23 as built, 2026-11-15:

- `agent_tools::ToolCtx` gains `run: Option<PluginRun>` (the run's id and
  folder): `ask` goes to `plugin_ai::ask_in_run` (the plugin-AI dialog,
  answered in the shell, -1 when the call or the run ends or the run is
  already over) and `folder` is the run's. `call_for_plugin_run` runs a
  `browser_*` / `terminal_*` tool with the owner key `plugin-ai:<run id>`;
  `end_plugin_run` closes its tabs and kills its terminals when the run's
  `done` passes the shell.
- The engine waits up to 15 minutes for an app tool (`APP_TOOL_WAIT`) and
  turns `{content: [text | image]}` answers into the model's content; a
  thread keeps `[an image was attached]` instead of a screenshot.
- The consent for `ai.tools:apps` now reads "Let its AI use Evo's browser
  and terminals".
- Found in the running app: loading Evo's own UI (the dev server on
  `localhost:1420`) in a run's tab reset the window's plugin views, as a
  reloaded window does; a release build does not serve its UI there.
