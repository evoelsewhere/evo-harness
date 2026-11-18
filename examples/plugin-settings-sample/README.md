# Issue desk — a plugin with settings, a backend, accounts and AI

A sample Evo plugin that uses what a plugin's running code needs
([plugin settings spec](../../docs/features/plugin-settings/spec.md),
[ADR 0022](../../docs/architecture/decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md)):

| Part | Where | What it shows |
| --- | --- | --- |
| Settings | `settings` in `.evo-plugin/plugin.json` | a text field with a pattern, a number with limits, a choice, a secret (the GitHub app's client secret) kept in the plugin's sealed vault, grouped fields |
| A backend | `runtime.server`, `server/main.ts` | a TypeScript MCP server Evo runs with Deno, on demand, stopped after 10 idle minutes, allowed only `api.github.com` and `news.ycombinator.com` |
| An OAuth account | `accounts[github]` | sign-in in your browser with PKCE, your own OAuth app's client ID and secret, renewal by Evo |
| A web login | `accounts[hn]` | a login window of its own; only `news.ycombinator.com`'s cookies are kept |
| A page | `ui.sidebar[desk]`, `ui/desk.*` | the accounts' states with Sign in buttons (`evo.auth`) and your issues (`evo.backend.call`) |
| Plugin AI | `ai` permission, `ui/desk.js`, `issue_detail` in `server/main.ts` | an issue summarised by your own models ([plugin AI spec](../../docs/features/plugin-ai/spec.md)): streamed into the page, a thread per issue for follow-up questions, the backend's tool for the model to read the issue, a cost cap per run |
| Permissions | `permissions` | two network origins and `ai` with their reasons, and `startup` as an optional one; `runtime` and `secrets` are implied by the backend and the accounts |

## Try it

1. Create a GitHub OAuth app (github.com → Settings → Developer settings →
   OAuth Apps → New) with the callback URL `http://127.0.0.1/callback`
   (GitHub accepts any port on a loopback address).
2. In evo: Customize → Plugins → **Add from folder** → this folder. The
   consent dialog lists what it may do with its reasons; allow it.
3. The plugin's page → **Settings**: fill in the client ID and secret.
   The secret shows as set, never again in clear.
4. **Accounts** → GitHub → **Sign in**: your browser opens GitHub; approve,
   and Evo shows your GitHub login. Hacker News → **Log in** opens a login
   window; when you are in, it closes and only that site's cookies are kept.
5. Open **Issue desk** in the sidebar and press **Load**, or ask the agent:
   *"What GitHub issues are assigned to me?"* — it calls the plugin's
   `my_issues` tool. With no account signed in, the chat first asks
   *"Sign in to GitHub for issue-desk?"*.
6. **AI summary** on the page: type a public issue (`owner/name#number`)
   or press **Summarise** by one of yours. Your own models answer (no key
   of the plugin's); ask follow-up questions under it. The plugin's
   **Models** tab shows what it spent, its runs and each issue's thread;
   set which models it may use and a cap there.
7. **Runtime** shows the backend: downloaded Deno the first time, asleep
   when idle, its log. **Data** shows what the plugin keeps.

## How it is built

- The backend holds no secret. When it needs the GitHub token or the
  Hacker News cookies it asks Evo's host API with the key Evo gives it:

  ```ts
  const res = await fetch(`${Deno.env.get("EVO_PLUGIN_HOST")}/v1/accounts/github/token`, {
    headers: { Authorization: `Bearer ${Deno.env.get("EVO_PLUGIN_KEY")}` },
  });
  const { accessToken } = await res.json();
  ```

  Evo renews the token first when it is about to expire, so the backend
  never restarts for a renewal. When GitHub refuses the token, the backend
  says so (`POST /v1/accounts/github/signed-out`) and Evo asks you to sign
  in again.
- Its settings reach it as environment variables
  (`"PAGE_SIZE": "${user_config.pageSize}"`); a secret setting would be
  `${secret.KEY}`, and only in `env`.
- Deno runs it with `--allow-net` for the approved origins and Evo's host
  API only, reads in the plugin and its data folder, writes in its data
  folder, no subprocesses. A Python backend would run through uv with the
  same settings, but its network and files are not enforced.
- The page reaches Evo only through `window.evo`; it sees accounts' states
  and names, never a token or a cookie.
- The AI summary asks for what it needs, not a model:

  ```js
  const run = evo.ai.run({
    need: { tier: "fast", tools: true },
    thread: "owner/name#123",            // follow-ups continue the conversation
    backendTools: ["issue_detail"],      // the model reads the issue through the backend
    budget: { maxCostUsd: 0.05 },
    messages: [{ role: "user", content: "Summarise issue owner/name#123." }],
  });
  for await (const event of run) if (event.type === "text") answer.textContent += event.text;
  ```

  Evo picks the cheapest connected model that calls tools among those you
  allow, falls back once if it fails, and counts the cost against the
  plugin. `issue_detail` uses your GitHub sign-in when there is one and
  reads public issues without it.

### The host API

| Call | Answer |
| --- | --- |
| `GET /v1/accounts` | `[{id, service, state, label}]` |
| `GET /v1/accounts/{id}/token[?workspace=]` | `{accessToken, expiresAt}` — needs `secrets`; renewed first when due |
| `GET /v1/accounts/{id}/cookies?url=` | `{header}` — a web login's cookies for that URL, declared domains only |
| `POST /v1/accounts/{id}/signed-out` | 204 — the service refused it; Evo asks the person to sign in again |
| `GET /v1/settings[?workspace=]` | `{values}` — plain settings, no secrets |
| `POST /v1/log` `{level, message}` | 204 — a line in Settings → Logs, secrets hidden |

Errors are `{code, message}` with `code` one of `unauthorized`,
`no_permission`, `not_signed_in`, `offline`, `unknown_account`,
`rate_limited`, `bad_request`.

Hooks of a plugin get the same two variables, `EVO_PLUGIN_HOST` and
`EVO_PLUGIN_KEY`, once the person allowed its hooks.
