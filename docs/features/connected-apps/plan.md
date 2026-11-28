# Connected apps — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-28 |
| ADD sections | [4 Process view](../../architecture/README.md#4-process-view), [5.1 Crate layers](../../architecture/README.md#51-crate-layers), [9 Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts) |
| ADRs | [0022 Plugin code runs under Evo's runtimes with a sealed vault](../../architecture/decisions/0022-plugin-code-runs-under-evos-runtimes-with-a-sealed-vault.md), [0028 Two front ends in one app process](../../architecture/decisions/0028-two-front-ends-in-one-app-process.md) |

## 1. Context and constraints

What exists (facts, 2026-11-28):

- **Connector pool** (`crates/evo-mcp`): `ServerConfig` with HTTP/SSE
  `url` and `headers` (`config.rs:28-141`); custom headers are sent as-is
  (`client.rs:116-127`). The pool already takes servers supplied by code,
  not the person's file: `PluginServer` and `Pool::set_plugin_servers`
  (`pool.rs:51-58`, :1092-1216), with saved permissions and cached tools
  under `pluginServers` in `mcp.json`. Statuses `off, connecting,
  connected, needs-auth, failed, sleeping, crashed` (:60-85). Tool names
  `mcp__<label>__<tool>` cut to 64 (:196-211). Default permission Allow
  for `readOnlyHint`, else Ask (:217-236).
- **Shell provider** `agent_tools::mcp::Connectors` claims `mcp__*`,
  `mcp_search_tools`, `mcp_call_tool` (`agent_tools/mcp.rs:497-538`); tool
  search past 40 tools / 60k chars (:39-46, :237-371); per-chat switches in
  `connectors.chatOff` (:54-160); asks Allow once / Always allow / Deny
  (:541-577); the mates' gate `mate_gate` (:673-738) and `mates/grants.rs`
  `plan()` (:45-78: read only when `readOnlyHint`, else sending).
- **Engine limits** for external tools: 128 tools, 256 KiB of definitions;
  every call is checked against the tool's `inputSchema` before it reaches
  the shell (`crates/evo-tools/src/tools/external.rs:327-412`). Inner
  arguments behind `mcp_call_tool` are not checked (`mcp.rs:272-284`).
- **Mates Apps** (`crates/evo-store/src/mates.rs:45-46`): `apps:
  Option<Vec<Value>>`, `None` = all; **not enforced** today (the Apps tab
  uses the per-chat switches; `evo-mates/tasks.md` T41 open). Connect card
  `connect_app` (`agent_tools/mates.rs:443-676`) with `standing()` Ready /
  SignIn / TurnOn / OffForMate / Missing and a 15-minute wait.
- **Secrets**: `evo-vault` (`KeyStore`, `Keychain` service `evo.plugins`,
  sealed `Vault` files; shell-only, from `spawn_blocking`). Provider keys
  are in `auth.json`; MCP OAuth tokens in `mcp-auth.json`.
- **UI**: Customize sections (`features/customize/customize-page.tsx:61-69`),
  `SIDEBAR_ITEMS` (`features/shell/sidebar-items.ts:26-51`), mate panel
  Apps tab (`features/mates/mate-panel.tsx:498-544`), settings registry
  (`settings-registry.ts:92-97`); browser via `tauri_plugin_opener`.
- **File upload precedent**: `agent_tools/browser/upload.rs:47-124` (10
  files, canonical paths, chat folder and temp only).
- `reqwest` 0.13 in the shell; `jsonschema` 0.60 in the workspace.

**Composio** (from OpenMausBot's client, read 2026-11-28, and Composio's
v3.1 API): base `https://backend.composio.dev/api/v3.1`, header
`x-api-key: <project key>` (keys start `ak_`).

| Call | Use |
| --- | --- |
| `POST /tool_router/session` `{user_id, manage_connections, multi_account:{enable, max_accounts_per_toolkit, require_explicit_selection}, auth_configs?}` → `{session_id, mcp:{type, url}, config}` | one session per installation |
| `GET`/`PATCH /tool_router/session/{id}` | read, raise the account cap, set `auth_configs` |
| `GET /tool_router/session/{id}/toolkits?…` | catalogue with connection state |
| `POST /tool_router/session/{id}/link` `{toolkit, alias?}` → redirect URL | sign-in link |
| `GET /connected_accounts?user_ids=…&toolkit_slugs=…` | accounts, status, alias |
| `DELETE /connected_accounts/{id}?revoke_on_delete=true` | disconnect and revoke |
| `GET`/`PATCH /auth_configs…` | the person's own auth configs, "enabled for Tool Router" |
| `POST /files/upload/request` → presigned HTTPS URL, `s3key` | file arguments |

The session's MCP server (`mcp.url`, HTTPS on `*.composio.dev`, same
`x-api-key` header) offers meta-tools: `COMPOSIO_SEARCH_TOOLS`,
`COMPOSIO_GET_TOOL_SCHEMAS`, `COMPOSIO_MULTI_EXECUTE_TOOL`,
`COMPOSIO_MANAGE_CONNECTIONS`, `COMPOSIO_WAIT_FOR_CONNECTIONS`,
`COMPOSIO_REMOTE_WORKBENCH`, `COMPOSIO_REMOTE_BASH_TOOL`,
`COMPOSIO_PROXY_EXECUTE`. Only `MULTI_EXECUTE` names app tools in its
arguments (`tools: [{tool_slug, arguments, …}]`); exact shapes are taken
from the live `tools/list` in T1 and pinned in fixtures.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | Connectors already live in the shell's pool and provider; this adds one code-supplied server and a gate there, as plugin servers do. No agent decision moves to the shell beyond the existing permission gate. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | `evo-mcp` gains a generic "app server" slot (like plugin servers); Composio's HTTP client is in the shell. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | Tools reach the engine through the existing external-tools path; no protocol change. |
| Models come from models.dev; no hardcoded model facts | n/a | No model involved. |
| Real engine in the app; settings and credentials reload live | ok | Saving or removing the key reconfigures the pool and re-sends tools; no restart. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Results stay tool output; every `MULTI_EXECUTE` is gated (grants, account, class, schema, files); sandbox meta-tools Block by default. |
| Desktop UI structure and `DESIGN.md` | ok | One feature folder `features/connected-apps/` used by Customize, the sidebar, Settings and the mate panel. |

## 3. Design

### 3.1 Overview

```text
UI features/connected-apps ──invoke──► shell connected_apps/ ──HTTPS x-api-key──► Composio REST
                                            │ key ◄─ evo-vault (keychain)
                                            │ session mcp.url + header (runtime only)
                                            ▼
                              evo-mcp Pool::set_app_server("connected_apps")
                                            │ tools mcp__connected_apps__COMPOSIO_*
                                            ▼
engine ──call──► shell Connectors::run ──► connected_apps::gate (grants, account, class,
                                               schema, files, sandbox, connect card)
                                            └─► pool.call ──► Composio MCP
```

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| crate | `crates/evo-mcp/src/pool.rs` | `AppServer` slot: `set_app_server(Option<AppServer>)`, key `app:<name>`, label `<name>`, permissions and cached tools saved under `appServers` (never the config or its headers) | FR-7, NFR-1 |
| shell | `src-tauri/src/connected_apps/composio.rs` | REST client: check key, session create/get/patch, toolkits, link, accounts, delete, auth configs, file upload request; 15–30 s timeouts; errors as readable text | FR-1, FR-4 – FR-6, FR-14 – FR-18 |
| shell | `src-tauri/src/connected_apps/key.rs` | key in the keychain via `evo-vault` (`KeyStore`, account `connected-apps`); `connectedApps.userId`, `connectedApps.sessionId` in `app_settings` | FR-1, FR-18, NFR-1 |
| shell | `src-tauri/src/connected_apps/accounts.rs` | label rules (unique, required once active, reuse of a dead label, five usable), pending links (10 min), status mapping | FR-3 – FR-6 |
| shell | `src-tauri/src/connected_apps/catalog.rs` | catalogue cache (a day, in the app cache dir), search by name and category | FR-3, NFR-3 |
| shell | `src-tauri/src/connected_apps/gate.rs` | `MULTI_EXECUTE` parse (unknown shape → refuse); per tool: service, account (FR-8), mate grant (FR-10), class read/send (FR-13), schema check against cached schemas (FR-9), files (FR-14); sandbox and proxy meta-tools; `MANAGE_CONNECTIONS` → Connect card | FR-8 – FR-11, FR-13, FR-14 |
| shell | `src-tauri/src/connected_apps/files.rs` | allowed roots (chat or mate folder, pastes dir), no hidden components, not Evo's data except the mate's own folder minus `memory/` and vaults, 10 files, upload after the last check | FR-14 |
| shell | `src-tauri/src/connected_apps/errors.rs` | scope-insufficient and no-managed-auth notes added to results | FR-15, FR-16 |
| shell | `src-tauri/src/connected_apps/mod.rs` | Tauri commands (3.3), start-up (session, pool), `apps://changed` | FR-1 – FR-6, FR-17 |
| shell | `src-tauri/src/agent_tools/mcp.rs`, `mates/grants.rs`, `agent_tools/mates.rs` | call the gate for the app server; class from the gate; `connect_app` standing for connected services and the link flow | FR-11, FR-13 |
| shell | `src-tauri/src/mates/mod.rs` | `apps` entries for connected services enforced (tools offered, search results filtered, calls refused) | FR-10 |
| UI | `apps/desktop/src/features/connected-apps/` | key card, catalogue, Connected tab, Connect dialog (label), Add account, Reconnect, Disconnect, pending link | FR-1 – FR-6, FR-17 |
| UI | `features/customize/customize-page.tsx`, `features/shell/sidebar-items.ts`, `features/settings/…`, `features/mates/mate-panel.tsx` | placements (FR-3); mate grants per service, tools and accounts | FR-3, FR-10 |
| UI | `components/ai-elements/tool-labels.ts` | "Called <service> · <tool> (<account>)" for `MULTI_EXECUTE` | FR-19 |
| UI | `features/chat/composer-*` | the `+` menu lists Connected apps as one connector switch | FR-12 |

### 3.3 Interfaces

Tauri commands:

```ts
apps_state(): { key: "none"|"saved"|"invalid", reason?: string, status: "ok"|"unavailable"|"off", accounts: Account[] }
apps_set_key(key: string): void          // checks first; error text on refusal
apps_remove_key(): void
apps_catalog(query?: string, category?: string): Toolkit[]   // {slug, name, logo, categories, managedAuth}
apps_connect(toolkit: string, label?: string): { pendingId, url }   // opens the browser too
apps_reopen(pendingId: string): void
apps_disconnect(accountId: string): void
apps_reconnect(accountId: string): { pendingId, url }
type Account = { id, toolkit, label, status: "active"|"needs-sign-in"|"expired"|"failed"|"pending", connectedAt }
```

Mate `apps` entry for a connected service (stored in the existing JSON):

```json
{"service": "composio:gmail", "tools": "all" | ["GMAIL_FETCH_EMAILS"] | "none", "accounts": ["work"]}
```

Gate result to the engine for a refused call: a tool error naming the
reason (`account required: work, personal`; `not allowed for this mate`;
`arguments do not match GMAIL_SEND_EMAIL: missing recipient_email — this is
about the call's shape, not access`).

### 3.4 Data and storage

- Keychain: account `connected-apps` → the project key.
- `app_settings`: `connectedApps.userId` (`evo_<uuid>`),
  `connectedApps.sessionId`, `connectedApps.chatOff` follows the existing
  `connectors.chatOff` (the server's label is the key).
- `mcp.json` `appServers.connected_apps`: tool permissions and cached tools
  only.
- App cache: `connected-apps/catalog.json` (a day), `schemas/<slug>.json`
  (from `GET_TOOL_SCHEMAS` and search results; refreshed on a miss).
- No migration.

### 3.5 Behaviour

- **Start**: no key → no server, no request (NFR-4). Key → get the saved
  session (or create one with `multi_account {enable, max 10,
  require_explicit_selection: true}`, `manage_connections.enable: true`);
  `set_app_server(mcp.url, x-api-key)`; tools re-sent to chats.
- **Connect**: label rules → `link` → open the browser → poll accounts
  every 2 s up to 10 minutes → `apps://changed`. Reopen reuses the URL
  while it lives.
- **Call**: engine → `Connectors::run` → `gate`:
  1. meta-tool policy: `SEARCH_TOOLS`, `GET_TOOL_SCHEMAS`,
     `WAIT_FOR_CONNECTIONS` pass (search results filtered by the mate's
     grants); `MANAGE_CONNECTIONS` becomes Evo's Connect card for the named
     toolkit; `REMOTE_WORKBENCH`, `REMOTE_BASH_TOOL`, `PROXY_EXECUTE` Block
     by default (Customize may allow them; never for a mate on Only these);
  2. `MULTI_EXECUTE`: parse every tool; grants and account first, then
     schema, then files; any refusal refuses the whole call;
  3. class: read when Composio marks the tool read-only, or its slug's
     action is a read verb (`GET`, `LIST`, `FETCH`, `SEARCH`, `FIND`,
     `READ`); anything else is sending → existing ask / `mate_gate` with the
     service, account, arguments and files on the card;
  4. files uploaded after the last check; `pool.call`.
- **Key removed / changed**: server removed, accounts and cache cleared;
  a new key builds a new user/session (FR-17, FR-18).

### 3.6 Errors and edge cases

- Composio down: server status `failed`, page shows "unavailable", retry
  after 30 s, 1, 2, 5 and 10 minutes, then every 30.
- Expired account: status from `connected_accounts`; the gate turns a call
  to it into the Reconnect card (mates) or a tool error naming Reconnect.
- Toolkit without managed sign-in: `link` error → FR-16 path (enable the
  person's newest enabled auth config for Tool Router, patch the session's
  `auth_configs`, retry once; else the steps).
- Scope refusal (`ACCESS_TOKEN_SCOPE_INSUFFICIENT`, 403): note appended
  (FR-15).
- Over 128 tools: not possible; the server offers the eight meta-tools.

### 3.7 Security and trust

- The key never leaves the keychain except as the header of requests to
  `backend.composio.dev` and the session's MCP URL; the MCP URL must be
  HTTPS on `composio.dev` or a subdomain, else it is refused.
- Results are tool output (untrusted); the agent never sees a sign-in URL;
  only the person's click opens it.
- Sandbox meta-tools can reach any connected app past a mate's grants, so
  they are blocked for mates with limited apps.
- Files: canonical paths, allowed roots only, listed on the card before
  upload, upload only to HTTPS presigned URLs on Composio's storage, no
  redirects.

### 3.8 Configuration

| Key | Type | Default | Where |
| --- | --- | --- | --- |
| keychain `connected-apps` | secret | none | Connected apps key card |
| `connectedApps.userId` / `.sessionId` | string | created | internal |
| `appServers.connected_apps.toolPermissions` | map | meta-tool policy above | Customize → Connectors |

Changes apply at once: the pool and the chats' tools are refreshed.

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Each app's tools listed directly (Composio's tools API, a Rust provider) | Hundreds of tools exceed the engine's 128-tool and 256 KiB limits; the session's search and execute meta-tools are what Composio built for agents, and they reuse the pool's permissions and switches. |
| A new shell `Provider` with its own prefix | Would duplicate permissions, search, chat switches and mate gating that `Connectors` already has. |
| Key in `auth.json` like provider keys | The spec asks for the keychain (FR-1, NFR-1); `evo-vault` already wraps it. |
| One Composio session per mate | Connections belong to the Composio user, not the session; one session with explicit account selection plus Evo's gate keeps grants in one place. |

## 5. As-built notes

- 2026-11-28 — `Pool::set_app_server` connects a new or changed app
  server before it returns (as `upsert` does), so the shell spawns it when
  it must not wait. Defaults (`AppServer::defaults`) < saved
  `appServers.<name>.toolPermissions`. A person's connector cannot take
  the app server's name; `remove` and `set_enabled` refuse it ("managed by
  Connected apps"). The snapshot marks it with `app` and hides its header
  values as it does a plugin server's.
- 2026-11-28 — The gate is pure checks (`gate::check`) plus one async
  step, `gate::prepare`, over a `Backend` trait (ask, upload), so the
  order checks → card → uploads → call is tested without the app. A
  `MULTI_EXECUTE` item names its account in `connected_account_id`
  (labels and ids both accepted from the agent; the call goes out with the
  id). With one account the caller may use, it is used even when the
  service has others the caller may not use. Schemas and Composio's
  `readOnlyHint` / `destructiveHint` tags come from the tools Evo has seen
  (`input_parameters`); a tool Evo has not seen is refused with "find it
  with COMPOSIO_SEARCH_TOOLS first". A path is "local" when it is absolute,
  starts with `/`, `\` or `~`, or has a drive letter; anything else in a
  file argument is sent unchanged.
- 2026-11-28 — `connected_apps::Service` (key, session, accounts) is
  independent of Tauri: a `KeyStore` (the keychain) and a `Settings`
  trait (`app_settings`). A key's mark (`connectedApps.keyMark`, a SHA-256
  prefix) tells a new project's key from the old one; a new key clears
  the user and session ids. The key is checked with `GET
  /connected_accounts?limit=1`. Tests use `connected_apps/fake.rs`, an
  HTTP server on 127.0.0.1 that answers from `testdata/`.
- 2026-11-28 — `Pool::start` keeps the app server (it is the app's, not
  the file's), so the order of `mcp::setup` and `connected_apps::app::setup`
  does not matter. The key's keychain account is `connected-apps` in the
  release build (`com.evoelsewhere.evo`) and `connected-apps@<identifier>`
  in any other build (debug, e2e), so testing never reads or replaces the
  person's key.
- 2026-11-28 — Connect (`connect.rs`): for a service without managed
  sign-in (from the catalogue's `composio_managed_auth_schemes`), the
  person's newest enabled auth config is set on the session
  (`PATCH /tool_router/session/{id}` `{auth_configs: {toolkit: id}}`)
  *before* the link; a managed service whose link is refused for its auth
  config takes the same path once. Reconnect removes the old account
  first, then links under its label (FR-5). The app polls every 2 s for
  ten minutes, then emits `apps://connected` and `apps://changed` and
  re-sends chats' tools. One more command, `apps_retry`, tries Composio
  again at once.
- 2026-11-28 — `agent_tools/mcp.rs` hands every call of the
  `app:connected_apps` server to `connected_apps::run::call` (one branch in
  `run_tool`) and filters the chat's tools with `run::offered` (one line
  in `chat_tools`). Class and approval (`run::decide`): a reading tool
  follows its permission — default Allow, saved per app tool slug under
  `appServers.connected_apps.toolPermissions` — and may be always allowed
  (Harness: Allow for the slug; a mate: a grant
  `mcp__connected_apps__<SLUG>`); a sending tool asks Allow once / Deny in
  Harness chats every time, and for a mate per Sending on your behalf
  (`grants::plan`). The pool's own permission on `MULTI_EXECUTE` stays
  Ask (its default) but is not what asks: the gate is.
- 2026-11-28 — The project switch (FR-12) is `"connectedApps": false` in
  the project's `.evo/settings.json` (nearest one up to the repository
  root); switching a capability off needs no project trust. The chat
  switch is the existing per-chat switch on the server key
  `app:connected_apps`.
- 2026-11-28 — Mates: a mate from a gallery role starts with `apps: []`
  (no connected service); a team file's `connectors` become
  `{service: "composio:<name>", tools: "none"}` entries, and team import
  no longer switches the Connected apps connector off for the chat (its
  services are governed by `apps`). `connect_app` and
  `COMPOSIO_MANAGE_CONNECTIONS` use one card (`run::connect_card`):
  Connect / Reconnect starts the sign-in, Open Apps opens the mate's Apps
  tab, then it waits up to 15 minutes. Schemas the gate needs are learned
  from search and schema results and fetched with
  `COMPOSIO_GET_TOOL_SCHEMAS` on a miss (memory only; no disk cache yet).
