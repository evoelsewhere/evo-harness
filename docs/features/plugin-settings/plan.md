# Plugin settings, secrets and runtime — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-14 |
| ADD sections | [4 Process view](../../architecture/README.md#4-process-view), [7 Data view](../../architecture/README.md#7-data-view), [9 Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts), [10 Deployment view](../../architecture/README.md#10-deployment-view) |
| ADRs | [0021 Plugin views run in native webviews](../../architecture/decisions/0021-plugin-views-run-in-native-webviews.md) · [0022 Plugin code runs under Evo's runtimes, secrets in a keychain-sealed vault](../../architecture/decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md) |

## 1. Context and constraints

- **Plugins today** (`crates/evo-plugins`):
  - `manifest.rs` reads `userConfig` (`string`/`number`/`boolean`/
    `directory`/`file`, `sensitive`) and fills `${user_config.KEY}`,
    `${PLUGIN_ROOT}`, `${PLUGIN_DATA}` into **every** string of
    `mcpServers` and hooks — args and URLs included.
  - `store.rs` keeps `config` (with sensitive values, in plain text),
    `approvedPermissions`, `developerMode`, `pending` in
    `plugins/installed.json`. `Store::enabled()` returns whole plugins.
  - `ui.rs` parses view permissions (strings only).
- **Where plugin parts run:**
  - connector servers: the shell's MCP pool (`crates/evo-mcp/src/pool.rs`,
    `set_plugin_servers`), connected at app start and forced on
    (`disabled = false`); tools are listed only while a server is
    `Connected` (`chat_tools`); `call` reconnects a closed connection but
    nothing watches for crashes; stderr lines go to a log callback
    (`client.rs`).
  - hooks: the engine (`crates/evo-engine/src/plugins.rs` →
    `evo-hooks`, commands spawned in `run.rs`) with the engine's own
    environment.
  - `bin/` on PATH for the agent's shell tools.
- **Credentials today:** providers in `auth.json`, connector OAuth in
  `mcp-auth.json` (`evo-mcp/src/auth.rs`: PKCE, loopback
  `127.0.0.1:<port>/callback`, refresh) — plain files. No keychain
  anywhere. `oauth2` and `ring` are already in both lockfiles (through
  `rmcp` and `rustls`); `keyring` is not.
- **Runtimes:** the Office runtime is downloaded on first use into
  `<agent dir>/runtimes` with checksums (`crates/evo-office/src/install.rs`)
  — the pattern for Deno and uv.
- **Webviews:** Tauri 2.12 `WebviewWindowBuilder` has `incognito(true)`
  (a fresh, non-persistent profile) and `Webview::cookies_for_url(url)`
  (HttpOnly cookies included). Plugin views and the bridge are as built in
  [Plugin UI](../plugin-ui/plan.md).
- **Questions in chats:** the shell's own tools ask through
  `agent_tools::ask_person` → `supervisor::questions` (kept per chat,
  notified when no window shows it, never timed out).
- **External facts that shape the design:**
  - Deno enforces `--allow-net=<host:port,…>`, `--allow-read/write=<paths>`,
    `--allow-env=<names>`, and `--deny-run`/`--deny-ffi`; with
    `--cached-only` it fetches nothing at run time. Bun and Python have no
    such permissions.
  - Windows Credential Manager keeps at most 2,560 bytes per entry; macOS
    asks the person per executable that reads an item; Linux needs a
    Secret Service.
  - OAuth for native apps (RFC 8252): system browser, PKCE, loopback
    redirect; device flow (RFC 8628) for no-redirect cases.
- **Spikes before the main work** (Phase 0):
  - S1: `keyring` from the shell on Windows and macOS (prompts, limits),
    and the sealed vault round trip.
  - S2: a Deno backend as an MCP server over stdio, with `--cached-only`,
    `--allow-net` including the loopback host API, and a refused fetch.
  - S3: web login in an incognito window: SSO redirects, `cookies_for_url`
    for declared domains (HttpOnly too), done detection, nothing left on
    disk.
  - S4: the MCP pool serving cached tool definitions for a sleeping
    server and waking it on the first call.
- **Spike results** (Windows 11, 2026-11-14):
  - S1: `keyring` 3.6 with `windows-native` holds the 44-character key;
    three vault writes and a read take about 50 ms; a 4,096-byte value
    written straight to the keychain is refused (limit 2,560), which
    confirms the vault. The Linux secret-service stack with vendored D-Bus
    adds about 770 lines to `Cargo.lock`; T8 decides between it and the
    pure-Rust variant. macOS not run.
  - S2: Deno 2.9.8 (43 MB download, 98 MB unpacked). `deno cache
    --node-modules-dir=none --allow-import=jsr.io,registry.npmjs.org,deno.land`
    caches an npm MCP server (21 MB) in 4 s. Run with the plan's flags it
    answers MCP over stdio; the allowed host and the host API's port work;
    another host, another loopback port, `localhost` for `127.0.0.1`,
    reads outside the plugin, writes to the plugin folder, an undeclared
    environment variable and a subprocess are all refused with
    `NotCapable`. Behind TLS interception (this network) Deno needs
    `DENO_TLS_CA_STORE=system,mozilla`; Evo sets it, and passes
    `HTTPS_PROXY`/`NO_PROXY` through, for runs and the cache step. The
    Windows checksum files are `Get-FileHash` text, so Evo keeps its own
    checksum table.
  - S3: an incognito login window followed a portal → SSO (another site)
    → portal flow with a second factor; on the landing page the done
    cookie appeared and `cookies_for_url` returned the declared domain's
    cookies, the HttpOnly session among them, without the SSO site's
    cookie; the main window had none; a new incognito window started
    empty; the app's profile on disk held no session value. Cookies are
    read off the main thread (`spawn_blocking`), as Tauri's docs require on
    Windows. The window's `plugin-login-*` label is under the invoke gate.
  - S4: a sleeping Deno backend answers its first tool call 225 – 280 ms
    after start. A Python backend (the `mcp` package through uv or its
    venv) takes about 2.1 s (6.1 s on the very first run, compiling
    bytecode): over NFR-2's 1.5 s, so NFR-2 is split by language (spec
    History). Python plugins must ship `uv.lock`; Evo runs `uv sync
    --frozen --compile-bytecode` with uv-managed Python only — an unpinned
    `mcp` resolved to 2.x and broke the 1.x API in the spike.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | Plugin processes, sign-in and secrets are native features of the shell, like the MCP pool and connector OAuth. The engine only runs plugin hooks, as today. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | New library crate `evo-vault` (keychain + sealed file), used by the shell only. `evo-plugins` stays pure parsing and storage; `evo-mcp` gains server lifecycles (a connector concern). |
| Protocol changes update `evo-protocol`, the engine and the desktop together | ok | One engine command `set_plugin_host` (the host API address and per-plugin hook keys for plugin hooks), added in the protocol, engine and shell together. Older engines ignore it; plugin hooks then get no host access. |
| Models come from models.dev; no hardcoded model facts | n/a | — |
| Real engine in the app; settings and credentials reload live | ok | A settings, secret or account change restarts that plugin's processes and reloads hooks (`resources_changed`); no app restart. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok, with a new trust boundary | Plugin backends reach Evo only through a per-process key on the loopback host API; tokens never reach views; tool calls from backends' MCP tools keep connector approvals. ADR 0022. |
| Desktop UI structure and `DESIGN.md` | ok | A plugin page under Customize; stores call `lib/plugins.ts`; shadcn components. |
| New store, process, trust boundary or toolchain → ADD + ADR | ADR 0022 | New persistent store (vault), new processes (Deno/uv backends, login windows), new toolchain shipped at run time (Deno, uv). ADD sections 4, 7, 9, 10 change with the feature. |

## 3. Design

### 3.1 Overview

```text
                      plugin manifest: settings · accounts · runtime · permissions
                                         │ (evo-plugins: parse, check, derive permissions)
                                         ▼
 Customize → plugin page ──Tauri──▶ shell ── plugins.rs (settings, accounts, runtime, data commands)
   (tabs, forms, consent)              │
                                       ├─ evo-vault: one keychain entry per plugin (32-byte key)
                                       │            + plugins/vault/<plugin>.vault (AES-256-GCM):
                                       │              secret settings, tokens, cookies, client secrets
                                       ├─ plugin_auth: OAuth (system browser, PKCE, loopback),
                                       │   device code, tokens, web login window (incognito),
                                       │   renewal (single flight), revoke, sign-in questions in chats
                                       ├─ plugin_host: 127.0.0.1:<port>, per-process bearer keys
                                       │   GET token / cookies · POST signed-out · POST log
                                       ├─ plugin_runtime: Deno / uv download + checksums,
                                       │   install-time dependency cache, launch specs
                                       └─ evo-mcp pool: plugin servers with lifecycles
                                           (on demand · with app · off), cached tools while asleep,
                                           idle stop, watchdog, logs
                                                     │ stdio (MCP)
                                    deno run --cached-only --allow-net=<allowed>,127.0.0.1:<host>
                                         --allow-read=<root,data> --allow-write=<data> … main.ts
                                    uv run --frozen --no-sync main.py   (network "not enforced")
 engine ── plugin hooks (approved only), env: EVO_PLUGIN_HOST + hook key (from set_plugin_host)
 views ── bridge: backend.call · auth.status · auth.signIn (no secrets)
```

A plugin's code runs only as Evo launches it, with what the person
approved. Secrets live in one sealed vault per plugin whose key is in the
keychain; only the shell opens it. Running code asks the shell for a token
or cookies with a key that names its plugin, at the moment it needs them,
so renewals never restart it.

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| crate (new) | `crates/evo-vault/src/{lib.rs,keychain.rs,seal.rs}` | `Vault::open(dir, plugin)`: a 32-byte key per plugin in the keychain (service `evo.plugins`, account = plugin id), created on first write; `<dir>/<safe id>.vault` sealed with AES-256-GCM (`ring`), written atomically. `get/set/remove(key)`, `clear()`. Errors name the platform cause (no Secret Service). | FR-5, FR-19, FR-30, NFR-1, edge 1, 13 |
| crate | `crates/evo-plugins/src/settings.rs` (new) | Parse `settings` (and legacy `userConfig`): kinds `text`, `multiline`, `number`, `boolean`, `select`, `folder`, `file`, `secret`; `title`, `description`, `default`, `required`, `group`, `min`, `max`, `pattern`, `options`, `scope` (`personal` · `workspace`). `validate(values)`; `missing_required(values)`. | FR-2, FR-3, FR-4 |
| crate | `crates/evo-plugins/src/accounts.rs` (new) | Parse `accounts`: `id`, `service`, `kind` (`oauth` · `device` · `token` · `web`), `reason`, `optional`, OAuth endpoints or `issuer`, `clientId` or `clientIdSetting`/`clientSecretSetting`, `scopes`, `revokeUrl`, `redirectPort`; web: `loginUrl`, `domains`, `done` (URL prefix or cookie name). | FR-20 – FR-24 |
| crate | `crates/evo-plugins/src/runtime.rs` (new) | Parse `runtime.server` (`language`, `entry`, `startup`, `idleStopMinutes`, `env`) and build a launch spec from the plugin, its approvals and the runtimes folder: Deno flags from `network:`/`files:` approvals, uv command for Python. Pure; unit-tested. | FR-10, FR-15, FR-16 |
| crate | `crates/evo-plugins/src/ui.rs`, `manifest.rs` | Permissions may be objects `{id, reason, optional}`; new ids `runtime`, `hooks`, `secrets`, `startup`, `files:read:<p>`, `files:write:<p>` (`<p>` = a path or `workspace`). Implied permissions: hooks → `hooks`; `mcpServers` or `runtime` → `runtime`; secret settings or accounts → `secrets`. Substitution: secret settings fill only `env` values (`${secret.KEY}`); a secret placeholder in args or URLs is left out and reported. | FR-6, FR-8, FR-9, FR-17, NFR-4 |
| crate | `crates/evo-plugins/src/store.rs` | `Installed` gains `workspaceConfig`, `startMode`, `idleStopMinutes`, `optionalAnswers`, `accounts` (metadata only), `reviewed`. `Store::enabled()` returns plugins with **parts gated**: hooks only with `hooks`, servers only with `runtime` and no missing required setting. | FR-3, FR-4, FR-8, FR-11, FR-17, FR-29 |
| crate | `crates/evo-mcp/src/pool.rs` | Plugin servers get a lifecycle (`Always` · `OnDemand{idle}` · `Off`), the status `Sleeping` with cached tools (saved under `pluginServers[plugin][server].tools`), wake on `call` (connect, then call), an idle reaper, a watchdog (restart on exit, 5 per minute, then `Crashed`), per-server log callback, `env` provided at connect time (never saved). | FR-11 – FR-13, SC-4 |
| protocol + engine | `crates/evo-protocol`, `crates/evo-engine/src/plugins.rs`, `evo-hooks/src/run.rs` | Command `set_plugin_host {url, keys: {pluginId: key}}`; plugin hooks run with `EVO_PLUGIN_HOST`, `EVO_PLUGIN_KEY` and the plugin's non-secret settings; only approved hooks are registered. | FR-6, FR-8, FR-26 |
| shell (new) | `src-tauri/src/plugin_host/` | Loopback HTTP server (hyper, already in the lockfile) on a random port. Keys: one per process start (backends, servers) and one per plugin for hooks; key → plugin. `GET /v1/accounts/{id}/token`, `GET /v1/accounts/{id}/cookies?url=`, `POST /v1/accounts/{id}/signed-out`, `GET /v1/settings`, `POST /v1/log`. Requires `secrets` for tokens and cookies; rate-limited; never logs secrets. | FR-6, FR-23, FR-26, NFR-1 |
| shell (new) | `src-tauri/src/plugin_auth/{mod,oauth,device,web,questions}.rs` | Accounts state machine; OAuth with `oauth2` (PKCE, state, loopback, fixed port when declared, issuer discovery); device flow; renewal ahead of expiry with one flight per account; revoke; web login window (`incognito`, read-only address bar, no IPC, `cookies_for_url` for declared domains, done by URL or cookie, 5-minute limit); sign-in questions in chats through `ask_person`. | FR-7, FR-20 – FR-31 |
| shell (new) | `src-tauri/src/plugin_runtime/{mod,install,launch}.rs` | Deno and uv: version and checksums per platform, download on first need, progress events; install-time dependency step (`deno cache` with network to the package registries only; `uv sync` into the plugin's data folder); launch spec → MCP pool entry. | FR-10, FR-15, FR-16, FR-18 |
| shell | `src-tauri/src/plugins.rs` | Commands for settings (personal and workspace, secrets through the vault), runtime (status, start, stop, restart, mode, idle), accounts (list, sign in, cancel, sign out, default, test), data (size, open, clear); one-time migration of `sensitive` values to the vault; uninstall clears vault and keychain. `changed()` passes gated servers with lifecycles to the pool. | FR-1 – FR-6, FR-11 – FR-13, FR-17, FR-19, FR-29, FR-30 |
| shell | `src-tauri/src/plugin_ui/bridge.rs`, `client.js` | Bridge calls `backend.call(tool, args)` (own plugin's server, waking it), `auth.status()`, `auth.signIn(account)`. | FR-14, FR-27 |
| shell | `src-tauri/src/agent_tools/mcp.rs` | A plugin tool whose account is not signed in asks "Sign in to <service> for <plugin>?" before the call. | FR-28 |
| desktop UI | `src/features/customize/plugin-page/{index,overview,settings-form,permissions,runtime,data,accounts}.tsx` | The plugin page with five tabs; forms drawn from the schema; account rows with Sign in / Sign out / default; device-code dialog; Runtime state, buttons and log tail; Data. `plugins-section.tsx` rows open it. | FR-1 – FR-4, FR-13, FR-19 – FR-23, FR-29 |
| desktop UI | `src/features/customize/plugin-consent.tsx` | Consent lists the new permissions with the plugin's reasons; "not enforced" marks; optional permissions asked later; the review-once banner. | FR-8, FR-9, FR-15, FR-17 |
| desktop UI | `src/lib/plugins.ts` | Types and calls for the above; events `plugins://runtime` and `plugins://accounts`. | all UI |

### 3.3 Interfaces

```jsonc
// plugin.json (additions)
"settings": {
  "workspace": { "kind": "text", "title": "Workspace", "required": true, "scope": "workspace" },
  "region":    { "kind": "select", "options": ["eu", "us"], "default": "eu", "group": "Connection" },
  "apiToken":  { "kind": "secret", "title": "API token", "description": "Settings → API in the service" }
},
"accounts": [
  { "id": "github", "service": "GitHub", "kind": "oauth", "reason": "Read your issues",
    "oauth": { "authorizeUrl": "https://github.com/login/oauth/authorize",
               "tokenUrl": "https://github.com/login/oauth/access_token",
               "deviceUrl": "https://github.com/login/device/code",
               "userinfoUrl": "https://api.github.com/user",
               "clientId": "Iv1.abc", "scopes": ["repo:read"], "revokeUrl": null } },
  { "id": "portal", "service": "Company portal", "kind": "web", "reason": "Read your tickets",
    "web": { "loginUrl": "https://portal.example.com/login",
             "domains": ["portal.example.com"], "done": { "cookie": "SESSION" } } }
],
"runtime": { "server": { "language": "typescript", "entry": "server/main.ts",
                         "startup": "onDemand", "idleStopMinutes": 10,
                         "env": { "REGION": "${user_config.region}", "API_TOKEN": "${secret.apiToken}" } } },
"permissions": ["runtime", "secrets",
  { "id": "network:https://api.github.com", "reason": "Read your issues" },
  { "id": "startup", "optional": true, "reason": "Keep issues in sync" }]
```

Host API (loopback, `Authorization: Bearer <key>`; backend env
`EVO_PLUGIN_HOST`, `EVO_PLUGIN_KEY`):

```text
GET  /v1/accounts                      → [{id, state, label}]
GET  /v1/accounts/{id}/token           → {accessToken, expiresAt}      (secrets; renews first when due)
GET  /v1/accounts/{id}/cookies?url=…   → {header: "a=1; b=2"}          (secrets; declared domains only)
POST /v1/accounts/{id}/signed-out      → 204                            (marks "needs sign-in again")
GET  /v1/settings?workspace=…          → {values}                       (no secrets)
POST /v1/log  {level, message}         → 204
errors: {code: "no_permission" | "not_signed_in" | "offline" | "unknown_account", message}
```

Tauri commands (main window): `plugin_settings_get(id, workspace?)`,
`plugin_settings_set(id, values, workspace?)`, `plugin_secret_set(id, key,
value, workspace?)`, `plugin_secret_clear(id, key, workspace?)`,
`plugin_runtime_state(id)`, `plugin_runtime_start|stop|restart(id)`,
`plugin_runtime_mode(id, mode, idleMinutes)`, `plugin_accounts(id)`,
`plugin_account_sign_in(id, account) → {flow: "browser" | "device" | "window", device?: {code, url}}`,
`plugin_account_cancel(id, account)`, `plugin_account_sign_out(id,
account, accountId)`, `plugin_account_default(id, account, accountId,
workspace?)`, `plugin_account_test(id, account)`, `plugin_data(id)`,
`plugin_data_clear(id)`, `plugin_optional_answer(id, permission, allow)`.
Events: `plugins://runtime {id, state, error?}`, `plugins://accounts {id,
account, state}`, `plugins://runtime-download {runtime, progress}`.

Bridge (views): `backend.call(tool, args)`, `auth.status()`,
`auth.signIn(account)`.

Engine command: `{"type": "set_plugin_host", "url": "http://127.0.0.1:<port>", "keys": {"<plugin id>": "<key>"}}`.

### 3.4 Data and storage

| Where | What | Notes |
| --- | --- | --- |
| keychain, service `evo.plugins`, account `<plugin id>` | 32 random bytes (base64) | One entry per plugin: under every platform's size limit, one macOS prompt per plugin at most. Only the shell reads it. |
| `<agent dir>/plugins/vault/<safe id>.vault` | AES-256-GCM sealed JSON: `{settings: {key: value}, workspace: {ws: {key: value}}, accounts: {account: [{accountId, label, accessToken, refreshToken, expiresAt, scopes, cookies}]}, oauthClient: {…}}` | Nonce per write; written atomically; unreadable without the key. Deleted with the plugin. |
| `plugins/installed.json` | adds `workspaceConfig` (non-secret values by workspace id), `startMode`, `idleStopMinutes`, `optionalAnswers`, `accounts` (ids, labels, default per workspace, state), `reviewed` | No secret ever; older files read the new fields as empty. |
| connectors file `pluginServers[plugin][server]` | adds `tools` (last tool list), `toolsAt` | Lets a sleeping server's tools be offered. |
| `<agent dir>/runtimes/{deno-<v>,uv-<v>}/`, `runtimes/deno-cache/`, `plugins/data/<id>/venv/` | runtimes, Deno's module cache, a Python plugin's environment | Shared runtimes; per-plugin Python env. |
| `evo.db` logs | source `plugin:<name>` | Stderr of plugin processes and `/v1/log`, with secrets redacted. |
| memory (shell) | host keys, sign-in flows in progress, renewal locks, server lifecycles | Keys change every app start. |

Migration (FR-17): at the first shell start of this build, each plugin's
`sensitive` values move from `config` to its vault and leave
`installed.json`; plugins with hooks or servers get `reviewed: false` and
their implied permissions show as waiting.

### 3.5 Behaviour

- **Gating:** a plugin part runs only when (1) the plugin is on, (2) its
  permission is approved, (3) no required setting is missing, (4) its
  runtime is installed. Otherwise the page says which. Hooks are
  re-registered and servers re-sent to the pool on every change.
- **Lifecycle (pool):** `Off` → nothing. `Always` → connect at start.
  `OnDemand` → if tools are cached: `Sleeping`, tools offered; else one
  discovery run, then sleep after the idle time. A call, a view's
  `backend.call`, or opening the plugin's view wakes it (`Starting` →
  `Running`). No use for the idle time → stop. Exit while running →
  restart after 1 s, 2 s, 4 s…; the 5th exit within a minute → `Crashed`
  with the last stderr lines.
- **Launch (Deno):** `deno run --cached-only --no-prompt --node-modules-dir=none
  --allow-net=<approved hosts:443>,127.0.0.1:<host port>
  --allow-read=<root>,<data>[,<approved>] --allow-write=<data>[,<approved>]
  --allow-env=EVO_PLUGIN_HOST,EVO_PLUGIN_KEY,<declared env names>
  --deny-run --deny-ffi <entry>`, `DENO_DIR` = the shared cache,
  `DENO_TLS_CA_STORE=system,mozilla`, the person's `HTTPS_PROXY` and
  `NO_PROXY`, cwd = the plugin's data folder. `workspace` paths resolve per process; a plugin
  with workspace-scoped values or `files:*:workspace` runs one process per
  workspace.
- **Install step:** after install or update (and after approval),
  `deno cache --node-modules-dir=none --allow-import=<registries> <entry>`
  runs with network only to the package registries, or `uv sync --frozen
  --compile-bytecode` (uv-managed Python, `uv.lock` required) into
  `data/venv`; failures show on the Runtime tab.
- **Sign-in (OAuth):** `plugin_account_sign_in` → loopback listener (random
  or declared port) → system browser with PKCE and state → callback →
  token exchange → vault → `plugins://accounts`. 5-minute limit; cancel;
  one flow per account, other callers wait on it (edge 8).
- **Device flow:** show code and URL, poll at the service's interval until
  done, expired or cancelled.
- **Web login:** incognito window `plugin-login-<n>`, address bar drawn by
  the shell, no IPC; on each page load, check the done rule; when met,
  `cookies_for_url` for each declared domain → vault; close.
- **Renewal:** `/token` renews when under 2 minutes remain; one flight per
  account; `invalid_grant` → "needs sign-in again" + notification;
  network error → keep, `offline` error to the caller, retry on the next
  ask.
- **Sign-in in chats:** before a plugin tool call whose server declares
  accounts that are not signed in (and not optional),
  `ask_person("Sign in to <service> for <plugin>?", ["Sign in", "Cancel"])`;
  Sign in starts the flow and the call waits for it.
- **Hooks:** the shell sends `set_plugin_host` to each engine after
  `ready` and on change; the engine adds `EVO_PLUGIN_HOST`/`EVO_PLUGIN_KEY`
  only to that plugin's hook commands.

### 3.6 Errors and edge cases

| Case | What happens |
| --- | --- |
| No Secret Service on Linux; keychain locked | Secret fields and sign-in show the reason; the plugin stays "Needs setup"; nothing written in plain form (edge 1). |
| Keychain entry deleted outside Evo | The vault cannot open: accounts and secrets show as not set; "Needs setup" (edge 2). |
| Runtime download fails or offline | Runtime tab: "Waiting for Deno (offline)" with Retry (edge 4). |
| `deno cache`/`uv sync` fails | Runtime tab shows the output; the server does not start. |
| A refused network call from Deno | Deno's error reaches the plugin; its stderr goes to the plugin log with the host (US-2). |
| Permission taken back while running | The process stops at once; a waiting tool call ends with "<plugin> may no longer run its code" (edge 5). |
| A secret placeholder in args or a URL | Left out; the plugin page lists the problem (FR-6). |
| Token renewal races | One flight per account; others await it. |
| Login window navigates to an undeclared domain | Allowed (SSO); its cookies are not kept (edge 12). |
| App quits during sign-in | Nothing kept; the account stays as it was. |

### 3.7 Security and trust

- **What the keychain protects:** secrets at rest — backups, synced
  folders, logs, a copied agent folder. It does not protect against other
  programs of the same user on Windows (Credential Manager entries are
  readable by the user's processes) or against the agent's own shell
  commands, which run as the person. The vault and host keys are added to
  the sandbox's default denied patterns, as a guardrail.
- **Host API:** binds to 127.0.0.1 only; a key per process start, never
  written to disk; a key reads only its own plugin's accounts and settings;
  tokens and cookies need `secrets`; responses carry `Cache-Control:
  no-store`; rate limit as the bridge's.
- **Views** never get secrets (FR-27); `backend.call` reaches only the
  view's own plugin's server.
- **OAuth:** system browser, PKCE, state, exact loopback redirect; tokens
  never in URLs Evo logs; renewal tokens never leave the shell.
- **Web login:** incognito profile per sign-in, no IPC, the real address
  shown, only declared domains' cookies kept.
- **Enforcement labels:** network and file permissions are enforced for
  Deno backends only; everything else says "not enforced".
- **Approvals:** plugin MCP tools keep connector approvals (Allow / Ask /
  Block) and approval modes; a backend cannot call Evo's tools.

### 3.8 Configuration

| Setting | Where | Default | Applies |
| --- | --- | --- | --- |
| Start mode per plugin | `installed.json` `startMode` | `onDemand` (`withApp` needs `startup`) | at once |
| Idle stop per plugin | `installed.json` `idleStopMinutes` | 10 | at once |
| Deno and uv versions | built-in table with checksums | pinned per release | next download |
| Plugin settings | `installed.json` / vault | manifest defaults | the plugin's processes restart |

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Bun for JS/TS | No permission system: network and files cannot be enforced (decided 2026-11-14). |
| One keychain entry per secret | Windows' 2,560-byte limit (cookies, JWTs), and a macOS prompt per entry. |
| An evo-encrypted file with a key on disk | The key sits next to the data; no better than plain text against copies. |
| Tokens in the environment at start | Tokens renew; the process would need restarting; environments leak to children. Kept only for old-style servers. |
| OAuth inside an Evo webview | Against native-app practice (RFC 8252); several providers block embedded sign-in; the person's password would pass through Evo. |
| Web login in the app's own browser profile | Leaks the person's other sessions to the plugin and the other way round. |
| Ask tokens over MCP | MCP has no client-to-server credential request; a host API is simpler and serves hooks too. |
| An HTTP proxy for network rules | Bypassable; Deno enforces for real where we can, and the rest is labelled. |

## 5. As-built notes

Phase 1 (manifest, vault and storage), 2026-11-14:

- **No `reviewed` flag.** The review of plugins installed before (FR-17)
  falls out of implied permissions: a plugin with hooks or servers now
  asks for `hooks` / `runtime`, its approvals lack them, so those parts
  wait (`gate`) while its skills keep working, and the page shows them as
  not approved.
- **Gating** lives in `evo_plugins::store::gate`, applied by
  `Store::enabled()` (used by the shell's MCP pool and the engine's hooks);
  `Plugin.waiting` lists what waits (`setup`, `hooks`, `runtime`) and for
  what. Code that uses secrets also waits for `secrets`.
- **Secrets in manifests:** a secret setting becomes `${secret.KEY}` inside
  `env` **and `headers`** (HTTP servers send tokens in headers); anywhere
  else it is left out and reported. A plain value an older build saved for
  a secret is never used.
- **`installed.json`** gains `workspaceConfig`, `secretsSet` (names only),
  `startMode`, `idleStopMinutes`, `optionalAnswers` and `accounts`
  (`AccountRecord`: no tokens). `Installed` derives `Default`; a reinstall
  carries the person's choices (`carry`).
- **Settings** use `kind` (`settings`) or `type` (`userConfig`);
  `sensitive` text is a secret; patterns are checked with `regex` as a
  whole match.
- **Vault:** file names are a readable part plus a hash of the plugin id —
  a test caught `a@x` and `a-x` sharing one file. Names inside:
  `setting:<key>[@<workspace>]`, `account:<record>`. Linux uses `keyring`'s
  pure-Rust Secret Service (`async-secret-service`, `crypto-rust`,
  `tokio`); vault calls must run on blocking threads.

Phase 2 (settings, secrets and consent in the app), 2026-11-14:

- **Shell:** `src-tauri/src/plugin_settings.rs` holds the settings, secret,
  data and optional-answer commands, the one-time move of `sensitive`
  values (run before the pool starts), filling `${secret.…}` into plugin
  servers' `env`/`headers` (in memory, in `plugins.rs`' `changed`), and
  the vault cleanup on uninstall. `plugin_settings_save` answers
  `{problems: [{key, message}]}` and saves nothing when a value does not
  fit.
- **Found in the running app:** the connector pool's state for the windows
  (`mcp_state`) carried plugin servers' `env` with filled secrets.
  `evo-mcp`'s snapshot now hides plugin servers' `env` and `headers`
  values and those values in their log lines (`hide_plugin_values`).
- **Permission rows** carry `reason`, `optional`, `implied` and
  `notEnforcedFor` (the plugin's hooks, connector servers, Python backend
  or `bin/` commands), shown in the consent dialog and the Permissions tab.
- **Optional permissions while in use:** bridge calls
  `permissions.query(id)` and `permissions.request(id)`; a request emits
  `plugin-ui://permission-request` to the main window, whose dialog
  (`features/plugin-ui/permission-request-dialog.tsx`) records the answer
  (`optionalAnswers`). A declined permission is not asked again.
- **The page** replaces the old single-column detail
  (`features/customize/plugin-page/`); it opens on Settings when the plugin
  needs setup. The Runtime tab shows what runs and what waits; its
  controls arrive with Phase 3.

Phase 3 (runtimes and lifecycles), 2026-11-14:

- **Pool** (`evo-mcp`): `set_plugin_servers(Vec<PluginServer>)` with a
  `Lifecycle` (`Always` · `OnDemand{idle}` · `Off`); statuses `sleeping`
  and `crashed`; tool lists saved under `pluginServers[plugin][server].tools`
  (kept while Off, so turning it back on sleeps at once); `wake`, `sleep`,
  `call_server`, `plugin_keys`; `tick` every 15 s puts idle servers to sleep
  and restarts ended ones (5 in a minute → `crashed`); a log sink sends
  plugin servers' lines, secrets hidden, to Settings → Logs as
  `plugin:<name>`.
- **Small environment:** a backend's config carries `minimalEnv`: its
  process gets only what a process needs (`PATH`, home, temp, system
  folders, locale, proxies) plus its own variables, and no `${VAR}`
  expansion — the person's other variables (keys of other tools) stay out
  even for Python. Connector servers declared the old way keep the full
  environment, as before.
- **Runtimes** (`src-tauri/src/plugin_runtime/`): one archive in memory
  (at most 300 MB), checked, the binary written under a temporary name and
  renamed. uv runs with `UV_PYTHON_PREFERENCE=only-managed` and its own
  Python folder. A failed preparation waits for Start (Try again) or a
  change of the plugin's files.
- **Backends join the pool as `backend`** once ready; until then the shell
  prepares them and reads the plugins again when done. A view opening wakes
  its plugin's servers.
- **Found in the running app:** switching a server from Off back to on
  demand left it `off`; fixed (it sleeps, or runs once to learn its tools).

Phase 4 (host API and hooks), 2026-11-14:

- **Server** (`src-tauri/src/plugin_host/`): a small HTTP/1.1 reader on a
  tokio `TcpListener` instead of hyper (one request per connection,
  headers ≤ 16 KiB, body ≤ 64 KiB, no chunked bodies, 15 s per
  connection), bound to `127.0.0.1:0` before the pool starts. Requests with
  an `Origin` header are refused. 200 calls per key per 10 s.
- **Keys:** one per plugin for its backend and one for its hooks
  (`evo_vault::random_key`, 32 bytes), made on first need and kept while
  the app runs, not one per process start: the key sits in the backend's
  pool config, and a new key would restart the backend at every change.
  `changed()` drops keys of plugins turned off or removed and of parts that
  no longer run; a request also checks that the plugin is on and its part
  (`runtime` or `hooks`) still approved.
- **Endpoints as planned.** `token` serves token accounts from their secret
  setting (a workspace's own first) and OAuth/device accounts from the
  vault entry `account:<record>`; renewal comes with Phase 5. `cookies`
  serves `account:<record>.cookies` matching the URL's host and path, for
  declared domains only. `signed-out` marks the record `needsSignIn`,
  logs it and emits `plugins://accounts`. `/v1/log` hides every vault
  string of 4+ characters.
- **Engine:** the command list lives in `evo-engine/src/rpc/protocol.rs`
  (`evo-protocol` holds no engine commands). `set_plugin_host {url, keys}`
  is kept in memory; `hooks::run` gives `EVO_PLUGIN_HOST` and
  `EVO_PLUGIN_KEY` only to the hook commands whose source is that plugin
  (`Hooks::env`, never serialised). Hooks of plugins get no settings in
  their environment; they ask `/v1/settings`.
- **Shell:** the supervisor sends `set_plugin_host` when an engine is set
  up and before each `reload_resources`, so a newly installed plugin's
  hooks have a key; an older engine's refusal is logged at info.
- **Gating:** unchanged from Phase 2 — `Store::enabled()` drops a plugin's
  hooks without `hooks`; the engine re-reads them on `reload_resources`.

Phase 5 (sign-in and sessions), 2026-11-14:

- **OAuth without the `oauth2` crate:** `plugin_auth/oauth.rs` uses the
  shell's `reqwest` (form bodies, `Accept: application/json`, form answers
  read too). PKCE S256, a 32-byte state, a loopback listener on
  `127.0.0.1` (the declared `redirectPort`, else a free one) that answers
  and ignores any request without the state. Discovery tries OpenID, then
  OAuth server metadata; declared endpoints win.
- **`oauth.userinfoUrl`** (new, optional) names the account where no ID
  token does (GitHub's `/user`); else the issuer's `userinfo_endpoint`;
  else the service's name.
- **Sign-in needs `secrets` approved.** One flight per account
  (`plugin_auth::start` returns `{flow, device?}`; `sign_in_and_wait` for
  chats and views), ended as cancelled after 5 minutes, by Cancel, or by
  closing the login window. A finished sign-in writes the vault first, then
  the record; signing in again as the same name replaces its session.
- **Device code:** the shell emits `plugins://device-code`; a dialog in the
  main window (`features/plugin-ui/device-code-dialog.tsx`) shows it
  wherever the sign-in started and closes on `plugins://accounts`.
- **Web login:** the spike's window, with the page's real address in the
  window title (Evo draws no separate address bar); done by address prefix
  or cookie; the declared domains' cookies (`name`, `value`, `domain`,
  `path`) are kept. The spike's commands are gone.
- **Renewal** happens where a token is asked for (`/token`, blocking on the
  host's thread) and every minute for sign-ins a connector server declared
  the old way uses: such a server writes `${secret.account.<id>}` in `env`
  or `headers` and gets the default sign-in's access token; a renewal
  re-sends the servers (their env changed, so they restart).
- **Chats:** a plugin tool whose plugin has a required account not signed
  in asks "Sign in to <service> for <plugin>?" through `ToolCtx::ask` (the
  chat's question, announced when no window shows it) after the tool's own
  approval; a key account is not asked for — the call ends naming Settings.
- **Accounts tab** between Settings and Permissions: each sign-in's name,
  state, missing access, default and the workspaces using it, Sign out;
  Sign in / Log in (again, or with another account); a key account's Test.
- **Uninstall** asks the services to revoke every sign-in first.

Phase 6 (verify), 2026-11-14:

- **Sample plugin** `examples/plugin-settings-sample/` (`issue-desk`):
  settings with a pattern, limits, a choice and a secret; a Deno backend
  with three tools that asks the host API for the GitHub token or the
  Hacker News cookies and reports refusals; a GitHub OAuth account with the
  person's own OAuth app (`clientIdSetting`, `clientSecretSetting`,
  `userinfoUrl`); a Hacker News web login; a sidebar page using `evo.auth`
  and `evo.backend.call`. In the running app it installed with no problem,
  its consent listed the two origins with reasons and the implied
  permissions, wrong values were refused, Deno came down and the backend
  ran with its three tools, and its login window opened on the real site
  with its address in the title. Signing in to GitHub needs a real OAuth
  app, so its sign-in ran only up to "fill in your client ID"; the flows
  themselves ran against the local test service in Phase 5.
- **Settings form:** secret settings show in their own Secrets block, not
  inside their `group`.
- **evo-creator** (FR-32): a bundled skill in
  `crates/evo-engine/bundled-skills/evo-creator/` — SKILL.md (the steps:
  ask, lay out, write each part, install from the folder, check, with a
  table of which reference to read for what), `references/` — `recipes.md`
  (common plugins and their parts), `manifest.md`, `skills.md` (skills,
  commands, subagents), `hooks.md`, `connectors.md`, `backend.md` (and the
  host API), `accounts.md` (settings and accounts), `views.md`
  (`window.evo`), `view-style.md` (DESIGN.md mapped to the `--evo-*`
  variables), `components.md` (UI pieces in that style), `security.md`,
  `debugging.md` (where Evo shows problems, the usual ones and fixes),
  `publishing.md` (marketplaces, versions), and `design.md`, a verbatim copy
  of the repository's DESIGN.md — and `templates/`: `plugin/` is a complete plugin
  Evo installs as it is (a manifest using every field, a themed view, a Deno
  backend, a hook, a README), `python-backend/` replaces its `server/`. A
  test in `office.rs` extracts `templates/plugin/` and loads it: it fails
  when the template stops loading cleanly, when `design.md` differs from
  DESIGN.md (copy DESIGN.md again after changing it), or when SKILL.md or a
  reference names a reference or template path that is not bundled.
- **ADD** updated: system context (runtime releases, services), process
  view (plugin backends, host API, login windows), data view (vault and
  keychain, runtimes, `installed.json`), interfaces (host API, manifest),
  cross-cutting (plugin code and secrets), deployment (runtimes downloaded
  with pinned checksums), risks, glossary.
