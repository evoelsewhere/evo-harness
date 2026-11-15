# Plugin AI: plugins use the person's models directly — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | engine · protocol · desktop shell · desktop UI · crate `evo-plugins` · crate `evo-store` |
| Created | 2026-11-14 |
| Updated | 2026-11-15 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Plugin UI](../plugin-ui/spec.md) · [Plugin settings, secrets and runtime](../plugin-settings/spec.md) · [Usage and cost](../usage-and-cost/spec.md) · [Memory topics](../memory-topics-and-chat-recall/spec.md) (a model call outside any chat) |

## 1. Summary

Today a plugin that needs an answer from a model has to send a message into
the person's open chat and read the reply back: the exchange shows in the
chat, waits for the chat's turn, and needs the right to read every chat.
This feature lets a plugin — its views, its backend and its hooks — use the
models of the providers the person has connected, directly and in its own
UI, without touching any chat. The plugin says what it needs (fast or
capable, vision, structured output, a budget) and evo picks a fitting
connected model, falling back to another when one fails; it can run a
multi-step agent whose tools the plugin supplies on the spot, give it
evo's own tools under the usual approvals, attach context it is allowed
to see, and keep threads per subject. The person approves `ai` once per
plugin, sets which models and how much money each plugin may use, sees the
cost in Usage under the plugin's name, and can stop any run.

## 2. Scope

### 2.1 Goals

- Plugins get model answers without the chat: streamed into their own UI.
- Only the person's connected providers and their models, never a key the
  plugin brings for evo's own use, and never a key shown to the plugin.
- Model choice by need and budget, with the person's rules on top, and
  fallback when a provider fails.
- Multi-step agent runs with tools the plugin defines at call time (in its
  view or its backend) and, when allowed, evo's own tools.
- Context the plugin may attach under its permissions, fitted to the
  chosen model.
- Threads: a plugin's conversations kept per subject, resumable, visible
  to the person.
- Cost visible and capped per plugin; every run can be stopped.

### 2.2 Non-goals

- Models outside the person's connected providers, or a plugin calling a
  provider through evo with its own key. (A plugin may still call a
  service with its own key from its backend, as today — that is not evo's
  model and not counted here.)
- Image, audio or video generation, embeddings, fine-tuning: later
  features if needed.
- Changing how chats, subagents or nested sessions run.
- Calls with no plugin: the agent already has models in chats.

## 3. User scenarios

### US-1 An answer streamed into the plugin's card (priority P1)

A charting plugin shows an "AI insight" card under a chart: it sends the
chart's summary and the model's answer appears word by word in the card,
with Re-analyse and Copy, while the person keeps working in their chat.

Acceptance scenarios:

1. **Given** the plugin has `ai` approved, **when** its view asks for an
   insight, **then** the text streams into the view within NFR-1's time,
   and no chat shows the exchange.
2. **Given** a run is streaming, **when** the person (or the view) cancels
   it, **then** it stops at once and is billed only for what was used.
3. **Given** the person's open chat is in a long turn, **then** the
   plugin's run does not wait for it.

### US-2 The right model, chosen by need (priority P1)

The plugin asks for a "fast" model for short summaries and a "best" model
with reasoning for a weekly report, with vision when it sends a chart image.

Acceptance scenarios:

1. **Given** two connected providers, **when** the plugin asks for
   `fast` with vision, **then** evo uses a connected model that reads
   images and is among the cheapest and quickest that do.
2. **Given** the person allowed this plugin only provider A, **then** no
   run of it uses provider B, whatever it asks.
3. **Given** the chosen model fails (rate limit, outage, overloaded),
   **then** evo retries once with the next fitting model and the plugin
   sees which model answered.
4. **Given** no connected model fits the need, **then** the plugin gets a
   clear error that says what is missing ("no connected model reads
   images").

### US-3 An agent with the plugin's own tools (priority P1)

The plugin asks: "explain this stock's move" and gives the model three
tools — get prices, get news, get the chart's data. The model calls them as
many times as it needs, then answers.

Acceptance scenarios:

1. **Given** tools defined in the view's code, **when** the model calls
   one, **then** the view's function runs with the model's input and its
   result goes back to the model; the view sees each step (`tool_call`,
   `tool_result`) as it happens.
2. **Given** the plugin's backend tools are offered to the run, **then**
   the model may call them as it calls the view's.
3. **Given** a step limit, **when** the model has not finished by then,
   **then** the run ends with what it has and says so.
4. **Given** a tool throws, **then** the model receives the error as the
   tool's result and may go on.

### US-4 A person in control of money and models (priority P1)

Acceptance scenarios:

1. **Given** a plugin asks for `ai`, **then** the consent shows it with
   the plugin's reason, like its other permissions; without it no call
   runs.
2. **Given** a plugin's page, **then** the person sees its AI use (runs,
   tokens, cost today and this month), sets which providers or models it
   may use, a default model, and a spending cap per day and per month.
3. **Given** the person set a cap and it is reached, **then** further runs
   are refused with "<plugin> reached its AI budget for today", and a
   running one stops before its next model call. With no cap set (the
   default), the consent and the plugin's page say the plugin may spend
   money on the person's providers.
4. **Given** Usage, **then** the plugin's runs appear as their own source
   with the plugin's name, apart from chats.
5. **Given** a run is going, **then** the person can see it (the plugin's
   page, Runtime) and stop it.

### US-5 The backend and hooks use the same AI (priority P1)

Acceptance scenarios:

1. **Given** a plugin's backend, **when** it asks evo's host API for a
   completion or an agent run, **then** it gets the same answers, choice
   of model, limits and accounting as a view.
2. **Given** a hook of the plugin, **then** it can do the same within its
   time limit.

### US-6 Evo's own tools in a plugin's run (priority P2)

The plugin lets its agent search the web, read and edit the workspace's
files, or run a command — any tool the chat's agent has — to do its job.

Acceptance scenarios:

1. **Given** the plugin was allowed the evo tools it asks for, **when** its
   run calls one, **then** it runs under the person's approval mode as in
   a chat, in the folder the run names (the open workspace by default), and
   an approval names the plugin.
2. **Given** a tool the plugin was not allowed, **then** the model is not
   offered it.

### US-7 Context the plugin may attach (priority P2)

Acceptance scenarios:

1. **Given** `chat:read`, **when** the plugin attaches the open chat,
   **then** its recent messages go with the run (incognito chats never).
2. **Given** a `files:read` permission, **then** it may attach files from
   those places; images are sent to models that read them.
3. **Given** the plugin's own skills, **then** it may use one as the run's
   instructions.
4. **Given** more context than the chosen model takes, **then** evo cuts
   the oldest or largest parts first and tells the plugin what it cut.

### US-8 Threads per subject (priority P2)

The stock plugin keeps one conversation per ticker: "Re-analyse" continues
it; switching tickers switches threads; the person can read a thread later.

Acceptance scenarios:

1. **Given** a thread id chosen by the plugin, **when** it runs again in
   that thread, **then** the model sees the earlier turns (fitted to its
   context).
2. **Given** the plugin's page, **then** the person can list its threads,
   read one as a conversation, and delete it.
3. **Given** the plugin is uninstalled, **then** its threads are deleted.

### Edge cases

1. No provider connected, or none allowed for this plugin: the call fails
   with a message that says what to do; the plugin's page says the same.
2. The person removes a provider or model while a run uses it: the run
   ends with that reason.
3. The plugin is turned off, its `ai` permission taken back, or it is
   uninstalled during a run: the run stops at once.
4. The view that started a run closes: the run goes on. Its events and
   result are kept with the run (and in its thread, if it has one); the
   plugin's views read them when they open again, and its other open views
   may follow it.
5. The model asks for a view-defined tool after the view closed: the tool
   result says the view is gone; the model may go on without it.
6. A tool or the model loops: the step limit and the budget end it.
7. Offline: the call fails as offline; nothing is billed.
8. The model does not support tools, images or structured output: it is
   not chosen for a need that requires them; a plugin naming it gets a
   clear error.
9. Structured output does not match the schema: evo retries once, then
   returns the error with the raw text.
10. Many calls at once from one plugin: at most 3 run at a time per plugin
    (6 over all plugins); others wait up to 30 s, then are refused.
11. A tool result or attached context contains instructions: it is data;
    the run's system prompt says so and evo tools stay under approval.
12. The app quits during a run: the run ends; a thread keeps what was
    finished.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | A plugin with the `ai` permission can ask for a model's answer to messages (system prompt, text and images), as a whole or streamed, from its views, its backend and its hooks; without the permission every call is refused. | US-1, US-5; a plugin without `ai` gets a permission error. |
| FR-2 | The answer streams as events: text as it comes, reasoning when the plugin asks for it, tool calls and results, and the end with the model used, tokens, cost and why it ended. | US-1 scenario 1; the end event names the model. |
| FR-3 | Any run can be cancelled by the plugin or the person; it stops at once. | US-1 scenario 2; US-4 scenario 5. |
| FR-4 | Runs never appear in, wait for, or change any chat. | US-1 scenarios 1, 3. |
| FR-5 | Only models of providers the person has connected are used; the plugin never sees a provider key. | US-2; no call reaches a provider without the person's credentials in evo. |
| FR-6 | A plugin can list the models it may use, with what each can do (text, images, tools, structured output, reasoning), its context size and its price, from evo's model catalogue. | `models()` lists only connected, allowed models with their capabilities. |
| FR-7 | A plugin may ask by need instead of by name: a tier (fast, balanced, best), required abilities, a reasoning level and a budget per run; evo picks the fitting connected model by the catalogue's data. | US-2 scenario 1. |
| FR-8 | A plugin may name a model; the person's rules still apply, and a model they do not allow is refused. | US-2 scenario 2. |
| FR-9 | When the chosen model fails for a reason that may pass (rate limit, overloaded, unavailable), evo tries the next fitting model once; the end event says which model answered. When a provider refuses its credentials (an invalid or expired key), evo also tries the next model and leaves that provider out of plugin runs until the person's providers change. | US-2 scenario 3; a provider with a bad key does not stop runs. |
| FR-10 | A run may be an agent: the model may call tools for up to a step limit, then answer. Tools may be functions in the calling view, tools of the plugin's backend, or (FR-16) evo's tools. | US-3. |
| FR-11 | A view defines its tools at call time (name, description, input schema, a function); evo calls the function in that view and returns its result to the model. | US-3 scenario 1. |
| FR-12 | Structured output: the plugin may give a JSON schema; the answer is JSON matching it, checked by evo. | Edge case 9. |
| FR-13 | The person approves `ai` in the plugin's consent with its reason; taking it back stops its runs. | US-4 scenario 1; edge case 3. |
| FR-14 | Per plugin, the person sets the allowed providers or models, a default model, and spending caps per day and month (none by default); the plugin's page shows its runs, tokens and cost, and the consent for `ai` says the plugin may spend money on the person's providers. | US-4 scenario 2. |
| FR-15 | Every run is counted in Usage as the plugin's, with its tokens and cost; caps are enforced before each model call. | US-4 scenarios 3, 4. |
| FR-16 | A plugin may ask for any of evo's own tools in its runs (the tools a chat's agent has), each group through a permission the person approves (reading, editing files, commands, web, apps); they run under the approval mode as in a chat, in the run's folder, and an approval names the plugin. | US-6. |
| FR-17 | A run may attach context the plugin may see: the open chat's recent messages (`chat:read`, never incognito), files and images from approved places, one of its skills as instructions; evo fits it to the model's context and reports what it cut. | US-7. |
| FR-18 | Threads: a run may name a thread; the thread keeps the conversation (messages, tool steps, which model) per plugin; later runs in it continue it. | US-8 scenario 1. |
| FR-19 | The person can list, read and delete a plugin's threads from its page; uninstalling removes them. | US-8 scenarios 2, 3. |
| FR-20 | At most 3 runs go at once per plugin and 6 over all plugins; more wait up to 30 s, then are refused with a reason. | Edge case 10. |
| FR-21 | A running run is visible on the plugin's page (what, which model, for how long, cost so far) with Stop. | US-4 scenario 5. |
| FR-24 | A run outlives the view that started it: it goes on to its end, its events and result are kept with the run (and in its thread), and the plugin's views can follow a run or read its result by its id when they open. | Edge case 4. |
| FR-22 | Tool results and attached context are data to the model: evo marks them as such in the run, and nothing in them widens what the run may do. | Edge case 11. |
| FR-23 | The evo-creator skill documents the AI API (`window.evo`, the host API, permissions, patterns) so the agent can build plugins that use it. | The skill's reference names every call; its test covers them. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Latency: the first streamed text of a run starts within the provider's own first-token time plus 1 s when plugin AI has run in the last 5 minutes, and within plus 5 s otherwise. | Measured on Windows with two providers. |
| NFR-2 | Security: no provider key, token or other plugin's data reaches a plugin; prompts and answers are not written to logs; a plugin's runs cannot read another plugin's threads. | Review; a test plugin tries each. |
| NFR-3 | Cost: every model call's tokens and cost are counted, including failed and cancelled calls, the same way as chats'. | Usage totals match the providers' reported usage. |
| NFR-4 | Architecture: model calls and the agent loop run in the engine, as every other model call; the shell relays and enforces permissions and caps. | Plan's architecture check. |
| NFR-5 | Works on Windows and macOS with every provider evo supports. | Both platforms. |

## 5. Success criteria

- SC-1 A plugin card shows a streamed AI answer with no chat involved and
  no `chat:read` permission.
- SC-2 A plugin asking by need gets a fitting model on a machine with any
  one of evo's providers connected, without naming a model.
- SC-3 An agent run with three view-defined tools finishes a multi-step
  task and the view shows each step as it happens.
- SC-4 The person can see what each plugin spent on AI today and stop it,
  and a cap is never exceeded by more than one model call.

## 6. Clarifications

### 6.1 Open

None.

### 6.2 Resolved

- 2026-11-14 — Q: model calls only through the person's connected
  providers? → A: yes (the person's request); FR-5, non-goals.
- 2026-11-14 — Q: how dynamic? → A: all four layers — choice by need with
  fallback, agent runs with tools given at call time, context, threads
  (the person's request); FR-7 – FR-19. Phases: 1 calls and choice, 2
  agent runs, 3 threads, 4 context and evo tools.
- 2026-11-14 — Q: default spending caps? → A: none until the person sets
  one; the consent and the plugin's page say it may spend money; FR-14,
  US-4 scenario 3.
- 2026-11-14 — Q: where do threads live and show? → A: in evo's database,
  readable and deletable on the plugin's page only, never in the chat
  list; removed with the plugin; FR-19.
- 2026-11-14 — Q: which evo tools may a plugin's agent use? → A: all the
  chat agent's tools, each group a permission, under the approval mode;
  FR-16, US-6.
- 2026-11-14 — Q: does a run outlive its view? → A: always; its events and
  result are kept with the run and its thread; FR-24, edge cases 4, 5.
- 2026-11-14 — Taken as proposed (the person answered the four above and
  left these): one plugin-AI engine started on the first call and stopped
  after 5 idle minutes (NFR-1); a plugin's default model is its own
  setting, else tier `balanced`, else evo's default (FR-7); tiers from the
  catalogue, pinnable by the person (FR-7); 3 runs at once per plugin and 6
  over all, queued up to 30 s (FR-20); no prompt per model call, only evo
  tools ask, per the approval mode (FR-13, FR-16).

## 7. History

- 2026-11-14 — Draft, from the limit a plugin hit: its AI had to go through
  the open chat.
- 2026-11-14 — Clarified: no default caps, threads on the plugin's page,
  all evo tools under approval, runs outlive their view; the other five
  as proposed.
- 2026-11-14 — Accepted (Gate A).
- 2026-11-14 — Plan accepted (Gate B).
- 2026-11-14 — FR-9 widened: a provider that refuses its key is skipped (found with a real model: the cheapest connected provider had an invalid key and every `fast` run failed).
- 2026-11-15 — Built (phases 1–5). FR-16's `apps` group is not offered to plugin runs yet (tasks T23); the rest of FR-16 and every other requirement checked on Windows, NFR-5 (macOS) not yet.
- 2026-11-15 — FR-16's `apps` group built (T23): the browser and terminal tools, in a context of the run's own; tools that only mean something in a chat, and control of other apps, are not offered.
