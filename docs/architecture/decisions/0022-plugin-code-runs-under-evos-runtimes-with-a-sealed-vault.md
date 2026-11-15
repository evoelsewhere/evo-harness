# 0022. Plugin code runs under evo's runtimes; its secrets live in a keychain-sealed vault and reach it through a loopback host API

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-14 |
| ADD sections | [4 Process view](../README.md#4-process-view), [7 Data view](../README.md#7-data-view), [9 Cross-cutting concepts](../README.md#9-cross-cutting-concepts), [10 Deployment view](../README.md#10-deployment-view) |
| Related | [Plugin settings, secrets and runtime](../../features/plugin-settings/spec.md) · [0021](0021-plugin-views-run-in-native-webviews.md) |

## Context

- A plugin's views are contained (ADR 0021), but its connector servers and
  hooks run with the person's full rights as soon as it is on, and the
  secrets it asks for are kept in plain text in `plugins/installed.json`
  and passed in command arguments.
- Plugins are to ship backends in TypeScript/JavaScript and Python, and to
  sign in to online services (OAuth, device codes, API keys, web logins)
  whose tokens renew.
- Of the candidate runtimes only Deno enforces network and file
  permissions; Bun and Python do not. Operating-system sandboxes would
  cover every language but cost a different design per platform.
- Keychains differ: Windows keeps at most 2,560 bytes per entry and lets the
  user's processes read them; macOS asks per reading executable; Linux needs
  a Secret Service.
- OAuth for native apps is done in the system browser with PKCE and a
  loopback redirect (RFC 8252); evo already does this for connectors.

## Decision

evo launches every piece of plugin code itself, under permissions the
person approved, and keeps each plugin's secrets in a sealed vault that only
the shell opens; running code asks the shell for a secret when it needs it.

- **Runtimes:** TypeScript/JavaScript backends run in Deno with
  `--cached-only`, `--allow-net` limited to the approved origins and the
  host API, reads and writes limited to the plugin and its data, no
  subprocesses or FFI. Python runs through uv; its network and files are
  declared and consented to, and labelled "not enforced". Deno and uv are
  downloaded on first need with pinned checksums into the agent folder.
- **Gating:** connector servers, backends and hooks run only with the
  approved `runtime` / `hooks` permissions and no missing required setting.
  Plugin servers start on demand by default, stop when idle and are
  restarted when they crash.
- **Vault:** one random 32-byte key per plugin in the keychain (service
  `evo.plugins`); everything secret of that plugin — secret settings,
  tokens, cookies, client secrets — sealed with AES-256-GCM in
  `plugins/vault/<plugin>.vault`. No plain fallback.
- **Host API:** the shell serves `127.0.0.1:<random port>` with a bearer
  key per process start (and one per plugin for hooks, given to engines by
  the `set_plugin_host` command). A key reads only its plugin's accounts
  and settings; tokens and cookies need the `secrets` permission; tokens
  are renewed by the shell before they are handed out.
- **Sign-in:** OAuth in the system browser (PKCE, state, loopback), device
  codes, API keys, and web logins in an incognito window whose declared
  domains' cookies are kept. Views see account states only.

Spikes (Windows, 2026-11-14) confirmed the parts: the keychain key and
sealed vault; Deno refusing every host, path, variable and subprocess not
allowed; a web login in an incognito window capturing only the declared
domain's cookies; wake times of 0.25 s (Deno) and 2.1 s (Python). Details
in the [plan](../../features/plugin-settings/plan.md), section 1. macOS is
still to be checked.

## Consequences

- Plugin parts that ran freely now wait for a one-time review; plugins with
  secrets in their command lines must move them to `env`.
- A new library crate (`evo-vault`), a `keyring` dependency, a loopback
  HTTP server in the shell, a new engine command, and two runtimes fetched
  at run time join the deployment.
- The keychain protects secrets at rest (copies, backups, logs), not
  against other programs of the same user or the agent's shell commands;
  docs and the security notes must say so.
- Network enforcement is real only for Deno backends; every other
  network permission must carry the "not enforced" label until
  operating-system sandboxes are added (a later ADR).
- Contributors must not pass secrets in arguments, URLs, files or logs, and
  must route any new plugin capability through the host API or the bridge
  with a permission.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Bun for JS/TS backends | No permission system to enforce network or file rules. |
| One keychain entry per secret | Windows' entry size limit; a macOS prompt per entry. |
| Secrets in an evo-encrypted file with the key on disk | The key travels with the data. |
| Secrets and tokens in the process environment | Tokens renew; environments are inherited by children and leak into logs. |
| OS sandboxes for every plugin now | A different design per platform; deferred, with honest labels meanwhile. |
| OAuth in an embedded webview | Against native-app practice; blocked by several providers; the password would pass through evo. |
