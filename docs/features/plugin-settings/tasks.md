# Plugin settings, secrets and runtime — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-14 |

## 1. Phases

### Phase 0 — Spikes

Done when: the plan's four unknowns are answered and recorded in the plan
and ADR 0022.

- [x] T1 [FR-5, NFR-1] S1: a per-plugin key in the keychain (Windows; macOS noted) and an AES-256-GCM sealed vault round trip; entry size and removal — `crates/evo-vault/` — check: an ignored test run against the real keychain
- [x] T2 [FR-10, FR-15, FR-16] S2: a Deno backend as an MCP server over stdio with `--cached-only`, `--allow-net` (one allowed host + loopback), `--allow-read/write`, `--deny-run`; an allowed fetch works, a refused one fails, a read outside fails — spike files in the scratchpad — check: a script drives it
- [x] T3 [FR-24] S3: a web login in an incognito Tauri window: redirect across two sites, `cookies_for_url` for declared domains (HttpOnly too), done detection, nothing kept on disk — spike code in `src-tauri/src/plugin_auth/` — check: the dev app against a local test site
- [x] T4 [FR-11, NFR-2] S4: wake time of a sleeping Deno backend (start → first tool answer), and the same through uv/Python — check: measured

### Phase 1 — Manifest, vault and storage

Done when: settings, accounts, runtime and the new permissions are read and
checked, plugin parts are gated, and secrets have a vault.

- [x] T5 [FR-2, FR-3, FR-8, FR-9, FR-20, NFR-4] Tests: settings kinds and checks, accounts, runtime, permission objects, implied permissions, secret placeholders — `crates/evo-plugins/src/*/tests.rs` — check: fail before T6
- [x] T6 [FR-2, FR-3, FR-6, FR-8 – FR-10, FR-20] Parse and check `settings`, `accounts`, `runtime`, permission objects; substitution of secrets into `env` only — `crates/evo-plugins/src/{settings,accounts,runtime,ui,manifest}.rs` — check: T5 passes
- [x] T7 [FR-3, FR-4, FR-11, FR-17, FR-29] `installed.json` fields; gated `Store::enabled()`; `reviewed` — `crates/evo-plugins/src/store.rs` — check: tests; old files still read
- [x] T8 [FR-5, FR-19, FR-30, NFR-1] `evo-vault`: keychain key, sealed file, get/set/remove/clear, errors — `crates/evo-vault/` — check: tests (an in-memory keychain) and the ignored real-keychain test

### Phase 2 — Settings, secrets and consent in the app

Done when: a plugin's page edits its settings and secrets, and consent
covers its running code.

- [x] T9 [FR-2 – FR-6, FR-17] Shell commands for settings and secrets; the one-time move of `sensitive` values into the vault — `src-tauri/src/plugins.rs` — check: shell tests; running app
- [x] T10 [FR-1 – FR-4, FR-19] The plugin page: Overview, Settings (forms from the schema, workspace values), Data — `src/features/customize/plugin-page/` — check: running app
- [x] T11 [FR-8, FR-9, FR-15, FR-17] Consent with the new permissions, reasons and "not enforced" marks; optional permissions; the review banner — `src/features/customize/plugin-consent.tsx`, `src-tauri/src/plugins.rs` — check: running app

### Phase 3 — Runtimes and lifecycles

Done when: a Deno and a uv backend install, start on demand, sleep, wake
and recover, under their permissions.

- [x] T12 [FR-18] Deno and uv download with checksums, progress, install-time dependency step — `src-tauri/src/plugin_runtime/` — check: tests for checksums and paths; running app
- [x] T13 [FR-11 – FR-13, SC-4] Pool lifecycles: `Sleeping` with cached tools, wake on call, idle stop, watchdog, logs — `crates/evo-mcp/src/pool.rs` — check: `cargo test -p evo-mcp` with a test server
- [x] T14 [FR-10, FR-13, FR-15, FR-16] Launch specs into the pool; the Runtime tab — `src-tauri/src/plugin_runtime/launch.rs`, `plugin-page/runtime.tsx` — check: running app
- [x] T15 [FR-14] Bridge `backend.call` — `src-tauri/src/plugin_ui/bridge.rs`, `client.js` — check: tests; running app

### Phase 4 — Host API and hooks

Done when: backends and hooks get secrets from the host API, and hooks run
only when approved.

- [x] T16 [FR-6, FR-26, NFR-1] The loopback host API: keys, endpoints, permission checks, redaction — `src-tauri/src/plugin_host/` — check: tests
- [x] T17 [FR-6, FR-8, FR-26] `set_plugin_host` in the protocol and engine; hook environment; gated plugin hooks — `crates/evo-protocol`, `crates/evo-engine/src/plugins.rs`, `crates/evo-hooks` — check: engine tests

### Phase 5 — Sign-in and sessions

Done when: every sign-in kind works and sessions renew, expire and end as
the spec says.

- [x] T18 [FR-20, FR-21, FR-25, FR-30, FR-31] Accounts state, OAuth (PKCE, loopback, discovery), renewal, revoke, scope changes — `src-tauri/src/plugin_auth/{mod,oauth}.rs` — check: tests against a local OAuth test server
- [x] T19 [FR-22] Device-code sign-in — `plugin_auth/device.rs` — check: tests against the test server
- [x] T20 [FR-23, FR-24] API-key Test; web login window — `plugin_auth/web.rs` — check: running app with a local test site
- [x] T21 [FR-28] Sign-in questions in chats before a plugin tool call — `plugin_auth/questions.rs`, `agent_tools/mcp.rs` — check: running app
- [x] T22 [FR-27] Bridge `auth.status`, `auth.signIn` — `plugin_ui/bridge.rs`, `client.js` — check: tests
- [x] T23 [FR-20 – FR-24, FR-29, FR-30] Accounts in the plugin page; device-code dialog — `plugin-page/accounts.tsx` — check: running app

### Phase 6 — Verify

- [x] T24 A sample plugin with settings, a Deno backend, an OAuth account and a web login; a check of every requirement — `examples/plugin-settings-sample/` — check: section 2
- [x] T25 ADD (process, data, cross-cutting, deployment), ADR 0022 Accepted, UI map — `docs/architecture/`, `apps/desktop/README.md` — check: review

### Phase 7 — The evo-creator skill

Done when: the agent can make a plugin from the skill alone, and the skill
cannot drift from what Evo reads.

- [x] T26 [FR-32] Bundled skill `evo-creator`: SKILL.md, references (manifest, views and `window.evo`, backend and host API, accounts, DESIGN.md and how it maps to `--evo-*`), templates; tests that its example manifest loads with no problem and its DESIGN.md is the repository's — `crates/evo-engine/bundled-skills/evo-creator/`, `crates/evo-engine/src/office.rs` — check: `cargo test -p evo-engine`; the running app lists the skill

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 – FR-4 | Tests (settings); running app | passed on Windows (2026-11-14): `cargo test -p evo-plugins` (kinds, checks, older `userConfig`, workspace over personal, missing required); running app: the plugin page with five tabs opened on Settings for a plugin that needs setup; a wrong Space and Retries were refused with their reasons and not saved; right values saved and "Needs setup" went away. Workspace values by test and the form's scope picker (not run with a second workspace) |
| FR-5 – FR-7 | Vault tests; a scan of Evo's files for a known secret; process list | passed on Windows (2026-11-14): Phase 4 — a Deno backend and a plugin hook each got the account token through the host API with their own key, the backend's command line held no key, the windows' connector state showed it `(hidden)`, and a `/v1/log` line echoing the token was stored as `(hidden)`; vault tests and the real Windows keychain; running app: a plain `sensitive` token moved to the vault at start (gone from `installed.json`, absent from the vault file, one keychain entry), the connector server got it in its environment, the windows' connector state showed `(hidden)`, and no file or log of Evo held it; secrets only in `env`/`headers` (test); FR-7 with Phase 5 (sign-in, see FR-20 – FR-31); the sample plugin's client secret, set in the running app, was in no file of the agent folder |
| FR-8 – FR-9 | Tests (gating); running app (consent, take back, optional) | passed on Windows (2026-11-14): tests; running app: the consent dialog listed the network permission with the plugin's reason and "Not enforced for its hooks and connector servers", and hooks, runtime and secrets as needed by its parts; Allow ran its server; a view's `evo.permissions.request('startup')` showed the dialog once, Not now was kept (`denied` without asking again), a non-optional request was refused; Phase 4 — a plugin's `UserPromptSubmit` hook ran with its host key, did not run after `hooks` was taken back (the engine re-read its hooks), and ran again once allowed (`hooks_run_only_once_approved…`, `each_plugins_hooks_get_only_its_own_key`, `only_a_plugins_own_hooks_get_its_environment`) |
| FR-10 – FR-16 | Pool and launch tests; running app with the sample backend | passed on Windows (2026-11-14): `cargo test -p evo-mcp` (sleeping servers offer their last tools, a call starts them, Off never starts, 5 crashes stop it, backends get no `${VAR}` expansion) and `evo-plugins` launch tests; running app: a TypeScript backend came up with 5 tools, slept after its idle minute (no process), woke on `evo.backend.call` in 245 ms, restarted after its process was killed, followed Stop / Start / Off / On demand; it got its secret from the vault, reached the allowed host, and Deno refused another host, a file outside it and a subprocess; a Python backend came up through uv with uv's own Python. Connector servers declared the old way share the lifecycle (code; not run with one) |
| FR-17 | Running app with a plugin installed by the previous build | passed on Windows (2026-11-14): a plugin as an earlier build left it (on, plain token, hooks and a server) stayed on, its hooks and server waited with a Review banner, and its token moved to the vault |
| FR-18 – FR-19 | Running app on a clean agent folder | passed on Windows (2026-11-14): FR-18 — Deno 2.9.8 (about 22 s) and uv 0.11.23 (about 10 s) downloaded once into the agent folder and checked against their pinned SHA-256 (`plugin_runtime::install` tests: pins for five targets, a wrong checksum and a missing binary refused); dependencies prepared in 5 s (Deno) and 45 s (uv with its Python); FR-19 — the Data tab's sizes, Clear data, and uninstall removing the vault and the keychain entry |
| FR-20 – FR-31 | Local OAuth and login test servers; running app | passed on Windows (2026-11-14), FR-23's Test and FR-29's per-workspace choice by code only: `plugin_auth` tests against a local OAuth server (PKCE S256 checked by the server, a forged or state-less return refused, the service's error and cancel end it, renewal keeps the refresh token, `invalid_grant` refuses and a failing service keeps the session, device code waits through `authorization_pending` and `slow_down`, discovery with declared endpoints winning, scopes an update adds shown as missing) and `plugin_host`/`plugin_settings` tests; running app with a local test service: OAuth through the default browser and the loopback return named the account from its ID token; a backend asking after the token was due got a renewed one (one refresh); the device-code dialog showed the code, closed when the service confirmed, named the account from `userinfoUrl`; a view's `evo.auth.signIn('portal')` opened the incognito login window, the form's cookies of the declared domain came back to the backend as `SESSION=sess-abc; THEME=dark`, another domain was refused; the backend's `signed-out` marked it "Log in again" and stopped its cookies; Sign out revoked the refresh token at the service; a chat's agent calling the plugin's tool got "Sign in to Test ID for p5-accounts?", Sign in finished in the browser and the call went on; uninstalling revoked the remaining sign-in and removed the vault and the keychain entry |
| FR-32 | `cargo test -p evo-engine`; the engine lists the skill | passed (2026-11-14): `bundled_evo_creator_matches_what_evo_reads` (front matter; every `references/` and `templates/` path SKILL.md names is bundled; `references/design.md` equals DESIGN.md; `templates/plugin/`, extracted as bundled, loads with no problem, all four view slots, 8 settings, 3 accounts, a backend, a hook, a connector server, and `runtime`, `hooks`, `secrets` implied; no second SKILL.md) and `the_bundled_skills_load_as_skills`; a fresh engine extracted it and `list_customizations` listed `evo-creator`. Not yet run: an agent making a plugin with it end to end |
| NFR-1 | Review; a test plugin tries each leak and another plugin's account | partly (2026-11-14): host API — no key, a made-up key, a dropped key, a browser `Origin`, an unknown account and another domain's cookies refused; one key never reads another plugin (keys name one plugin); secrets absent from `mcp.json`, `installed.json`, the logs and the process list in the running app; sign-ins: no token or cookie in any file of the agent folder (the vault is sealed), views get states and names only (`public_account`), the login window kept its cookies to itself. A malicious-plugin review (a second plugin trying the first one's accounts) by the host API's per-plugin keys and tests, not by a running second plugin |
| NFR-2 | Measured | passed on Windows for TypeScript (2026-11-14): a sleeping Deno backend answered `backend.call` 245 ms after it was woken; Python per spike S4 (about 2.1 s, within its 3 s); a sleeping backend runs no process |
| NFR-3 | Windows and macOS | Windows passed (2026-11-14, every row above); macOS not run; Linux's "no secret service" reason by code (`evo-vault`), not run |
| NFR-4 | Tests and running app with skills-only plugins | partly (2026-11-14): a skills-only plugin implies no permission (test) |
