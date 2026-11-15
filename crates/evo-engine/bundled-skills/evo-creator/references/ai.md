# The person's AI models

A plugin can use the models the person connected in evo, directly: no key
of its own, no chat. evo chooses the model, streams the answer, runs the
plugin's tools, keeps threads, and counts what it costs against the
plugin. The person approves `ai` once and controls the rest on the
plugin's **Models** tab (which providers and models, a default, a model per
tier, caps per day and month).

## Permissions

| Permission | For |
| --- | --- |
| `ai` | any call below; the consent says it may cost money |
| `ai.tools:read` | evo's `read`, `grep`, `find`, `ls` in a run |
| `ai.tools:edit` | evo's `edit`, `write` |
| `ai.tools:command` | evo's `bash` (and `powershell` on Windows) |
| `ai.tools:web` | evo's `web_search`, `web_fetch` |
| `ai.tools:apps` | evo's browser (`browser_*`, in background tabs of the run's own) and terminals (`terminal_*`) |
| `chat:read` | attaching the open chat (`context.openChat`) |
| `files:read:<folder>` / `files:read:workspace` | attaching files (`context.files`) |

Ask only for what the plugin uses, each with a `reason`.

## From a view: `window.evo.ai`

| Call | Returns |
| --- | --- |
| `evo.ai.models()` | the models it may use: `[{ provider, model, name, vision, tools, reasoning, context, maxOutput, cost?: { input, output }, tiers }]` (USD per million tokens) |
| `evo.ai.run(options)` | a run: an async iterator of events, with `.id` (a promise of its id), `.result` and `.cancel()` |
| `evo.ai.complete(options)` | the run's result, without the events |
| `evo.ai.follow(runId)` | a run again: its kept events, then live ones if it is still going |
| `evo.ai.runs()` | `{ going: [{ id, model, costUsd, seconds }], recent: [{ id, threadId, status, modelId, costUsd, startedAtMs, … }] }` |
| `evo.ai.threads()` | `[{ id, title, createdAt, updatedAt, messages, going }]` |
| `evo.ai.thread(id)` | `{ id, title, messages: [{ role, text, tools?: [{ name, input, result, isError }], model?, at }] }` |
| `evo.ai.deleteThread(id)` | — (not while a run continues it) |

### Options

```js
{
  messages: [{ role: "user" | "assistant", content: "text" | [{ type: "text", text }, { type: "image", data, mimeType }] }],
  system: "You write release notes.",                // the plugin's instructions
  model: { provider: "anthropic", model: "…" },      // only to name one the person allows
  need: { tier: "fast" | "balanced" | "best", vision: true, tools: true, reasoning: "off" | "low" | "medium" | "high" },
  budget: { maxCostUsd: 0.05, maxOutputTokens: 800 },
  schema: { type: "object", … },                     // the answer as JSON matching it: result.json
  thread: "AAPL",                                    // continue (or start) a thread
  tools: { name: { description, input: {…JSON Schema}, run: async (input) => value } },   // views only
  backendTools: true | ["quote"],                    // the plugin's own server tools
  evoTools: ["read", "edit", "command", "web", "apps"], // each needs its ai.tools:<group>
  folder: "C:/work/repo",                            // where evo's tools work; default the open chat's folder
  maxSteps: 8,                                       // model calls, 1 – 50
  context: { openChat: true | { lastMessages: 20 }, files: ["notes.md"], skill: "analyst" },
}
```

- **Don't name a model** unless the person picked it in the plugin's own
  settings; say what is needed (`need`) and let evo choose from what the
  person allows. Never hardcode model names or prices.
- **`messages`** end with the user's; earlier `assistant` turns are the
  plugin's own history. With `thread`, evo adds the thread's earlier turns
  itself — send only the new message.
- **`schema`**: evo gives the model a `respond` tool with that schema,
  checks the answer, asks once more if it does not match, then fails.
- **`tools`** run in the view that started the run: `run(input)` gets the
  input checked against `input` and returns any JSON value (or throws: the
  model gets the error). If the view closes, the model is told the tool is
  gone and the run goes on. Names: letters, digits, `-`, `_`; not
  `respond`.
- **`context`**: the open chat (never an incognito one), files inside the
  places the plugin may read, and one of its own skills as instructions.
  evo marks it all as data, fits it to the model (the thread's oldest
  turns first, then the largest pieces) and lists what it left out in the
  `started` event's `cut`.
- **`evoTools`** run under the person's approval mode, as a chat's agent
  does: reads never ask; edits and commands ask in the main window, in a
  dialog that names the plugin. With `apps`, the browser opens pages in
  background tabs that belong to the run and the terminals are the run's;
  both close when the run ends. A screenshot reaches the model as an
  image.

### Events and the result

```js
const run = evo.ai.run({ need: { tier: "fast" }, messages: [{ role: "user", content: question }] });
for await (const event of run) {
  if (event.type === "text") out.textContent += event.text;
}
const result = await run.result;   // throws EvoError("ai", …) when it stopped with an error
```

| `type` | Fields |
| --- | --- |
| `started` | `provider`, `model`, `droppedMessages?`, `cut?` |
| `text` | `text` (a piece; append them) |
| `reasoning` | `text`, when `need.reasoning` is set |
| `tool_call` | `callId`, `tool`, `input` |
| `tool_result` | `callId`, `tool`, `isError`, `preview` |
| `retry` | `from`, `to`, `reason` — the model failed before answering; evo tries the next once |
| `usage` | `usage: { input, output, costUsd }`, the run so far |
| `done` | `result: { text, json?, provider, model, usage, stopped, error? }` |

`stopped` is `end`, `steps` (reached `maxSteps`), `budget` (its budget or
the person's cap), `cancel` or `error`.

### When it is refused

A call throws an `EvoError` with a `code`:

| `code` | Means | Do |
| --- | --- | --- |
| `permission` / `no_permission` | `ai` (or a tool group, `chat:read`, a file place) not allowed | say what to allow on its Permissions tab |
| `budget` | the person's daily or monthly cap is reached | say so; point to the Models tab |
| `busy` | 3 runs of this plugin are going (6 over all), or its thread has one | wait for one to end, or `follow` it |
| `bad_request` | an option is wrong (the message says which) | fix the call |
| `no_chat` | `openChat` with no chat open | attach nothing, or ask the person to open one |
| `ai` | the run ended with an error: no connected model fits, the provider failed | show the message |

## From the backend: the host API

The same calls over HTTP with `EVO_PLUGIN_HOST` and
`Authorization: Bearer $EVO_PLUGIN_KEY`:

| Route | Answer |
| --- | --- |
| `GET /v1/ai/models` | the models list |
| `POST /v1/ai/run` with the options | NDJSON: `{"type":"run","runId"}`, then one event per line until `done` |
| `GET /v1/ai/runs/{id}` | `{ live }`, or `{ live: false, run, result, events }` |
| `POST /v1/ai/runs/{id}/cancel` | 204 |
| `GET /v1/ai/threads` | the threads |
| `GET /v1/ai/threads/{id}`, `DELETE /v1/ai/threads/{id}` | a thread / 204 |

A backend has no view to run functions in: it offers its own server tools
with `backendTools`, and `tools` is refused (400). Refusals are JSON
`{ code, message }` with 401, 402 (`budget`), 403, 404, 429 (`busy`) or 400.

```ts
const res = await fetch(`${host}/v1/ai/run`, {
  method: "POST",
  headers: { Authorization: `Bearer ${key}` },
  body: JSON.stringify({ need: { tier: "fast" }, backendTools: ["quote"], messages: [{ role: "user", content: q }] }),
});
const lines = res.body!.pipeThrough(new TextDecoderStream()).getReader();
// split on "\n"; JSON.parse each; stop at {"type":"done"}
```

## Patterns

- **A streamed card.** The agent shows a card with `data`; the card runs
  `evo.ai.run` with that data and appends `text` events. Keep the run's id
  in `evo.storage` so a card opened again can `follow` it instead of
  paying twice.
- **An agent with the view's tools.** Pass `tools` whose functions read or
  change what the view shows (pick a chart, filter a list); the model
  calls them and the view updates as it goes.
- **A thread per subject.** `thread: ticker` keeps one conversation per
  subject; the person can read and delete it on the Models tab.
- **Structured output.** `schema` for anything the view will lay out
  (scores, lists, fields); read `result.json`, never parse `result.text`.
- **Cheap first.** `need: { tier: "fast" }` for summaries and labels;
  `"best"` only when the person asked for depth. Set `budget.maxCostUsd`
  on anything that loops.

## Rules

- What the model says, and what tools and attached files contain, is
  untrusted: render it with `textContent`, never `innerHTML`.
- Show the run's state: waiting for a model, streaming, stopped (and why),
  refused (the `EvoError`'s message).
- A run goes on when its view closes; the person stops it on the Models
  tab, or the view calls `.cancel()`.
- Never ask the person for a provider key: the plugin uses evo's.
