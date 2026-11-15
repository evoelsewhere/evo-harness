# A backend and the host API

A backend is the plugin's own code: an MCP server over stdio that evo
runs, whose tools the agent can call (`plugin_<plugin>_backend`) and the
plugin's views can call (`evo.backend.call`). Use one for any logic that
talks to a service; keep views thin.

## Declaring it

```json
"runtime": { "server": {
  "language": "typescript",
  "entry": "server/main.ts",
  "startup": "onDemand",
  "idleStopMinutes": 10,
  "env": { "REGION": "${user_config.region}", "API_TOKEN": "${secret.apiToken}" }
} }
```

| Field | Values |
| --- | --- |
| `language` | `typescript`, `javascript` (run by Deno), `python` (run by uv) |
| `entry` | a file inside the plugin |
| `startup` | `onDemand` (default: started when a tool is called or a view opens, stopped after `idleStopMinutes`, 1–1440, default 10), `withApp` (needs the person's `startup` permission), `off`. The person can change it on the Runtime tab |
| `env` | upper-case names (not `EVO_…`) to text; placeholders as in `references/manifest.md`; a variable whose secret is missing is left out |

It needs the `runtime` permission (added for you), and `secrets` when its
env uses a secret.

## How evo runs it

- **TypeScript/JavaScript:** `deno run --cached-only --no-prompt
  --node-modules-dir=none` with `--allow-net` for the approved
  `network:` origins and the host API only, `--allow-read` for the plugin
  and its data folder (and approved `files:read:` paths),
  `--allow-write` for its data folder (and approved `files:write:` paths),
  `--allow-env` for its own variables, and no subprocess or FFI. Imports are
  fetched once at install (`deno cache`) from jsr.io, npm and deno.land:
  pin every version (`npm:zod@3.25.76`, `jsr:@std/path@1.0.8`).
  Working folder: its data folder.
- **Python:** `uv sync --frozen` into `<data>/venv` with uv's own Python,
  then that venv's `python <entry>`. Ship `pyproject.toml` and `uv.lock`
  (`uv lock`); without `uv.lock` it does not run. Network and files are not
  confined: the consent says so. Prefer TypeScript when it fits.
- Its environment is small: system basics, proxies, its `env`,
  `EVO_PLUGIN_HOST`, `EVO_PLUGIN_KEY`. Nothing else of the person's.
- stderr goes to the Runtime tab and Settings → Logs, secrets hidden. Never
  write to stdout except MCP messages.
- If it ends, evo starts it again; 5 crashes in a minute stop it.
- While it sleeps, evo still offers its last tool list, so the agent can
  call it; the call starts it (about 0.3 s for Deno, 2 s for Python).

## Writing the server

TypeScript (`templates/plugin/server/main.ts`):

```ts
import { McpServer } from "npm:@modelcontextprotocol/sdk@1.17.0/server/mcp.js";
import { StdioServerTransport } from "npm:@modelcontextprotocol/sdk@1.17.0/server/stdio.js";
import { z } from "npm:zod@3.25.76";

const server = new McpServer({ name: "my-plugin", version: "1.0.0" });
server.tool("search", "Search the tracker for issues.", { query: z.string() }, async ({ query }) => {
  return { content: [{ type: "text", text: JSON.stringify(await search(query)) }] };
});
await server.connect(new StdioServerTransport());
```

Python (`templates/python-backend/`): the `mcp` package's `FastMCP`, pinned
in `pyproject.toml`, locked in `uv.lock`.

Give each tool a description that says when the agent should use it; return
short JSON or text; return `isError: true` with a plain message when it
fails ("not signed in to GitHub; sign in on the plugin's Accounts tab").

## The host API

evo listens on `127.0.0.1`. The backend and the plugin's hooks get
`EVO_PLUGIN_HOST` (`http://127.0.0.1:<port>`) and `EVO_PLUGIN_KEY`, a key
that names this plugin. Send it as `Authorization: Bearer <key>`.

| Call | Answer |
| --- | --- |
| `GET /v1/accounts` | `[{id, service, state, label}]` |
| `GET /v1/accounts/{id}/token[?workspace=]` | `{accessToken, expiresAt}` — needs `secrets`; evo renews it first when it is about to expire |
| `GET /v1/accounts/{id}/cookies?url=` | `{header}` for a web login, only for its declared domains |
| `POST /v1/accounts/{id}/signed-out` | 204 — the service refused the token or cookies; evo asks the person to sign in again |
| `GET /v1/settings[?workspace=]` | `{values}`: plain settings, no secrets |
| `POST /v1/log` `{level, message}` | 204 — a line in Settings → Logs (`debug`, `info`, `warn`, `error`), secrets hidden |

Errors: `{code, message}` with `unauthorized` (401), `no_permission` (403),
`unknown_account` (404), `not_signed_in` (409), `rate_limited` (429),
`offline` (503), `bad_request` (400). 200 calls per 10 s.

Ask for the token each time you need it (it is cheap); never store it.
When a service answers 401 for it, call `signed-out`.
