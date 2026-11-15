# Trying and fixing a plugin

## Where evo shows what happened

| Place | Shows |
| --- | --- |
| Customize → Plugins → the plugin → Overview | problems in the manifest (each wrong entry with its reason), its parts, Developer mode |
| … → Permissions | what it asked for, what the person allowed, what waits |
| … → Runtime | "Not running yet" with what it waits for; the backend's download and preparation; each server's state (running, asleep, crashed) with Start / Stop / Restart; the last log lines |
| … → Accounts | each sign-in and its state |
| Settings → Logs, source `plugin:<name>` | the backend's stderr and `/v1/log` lines, secrets hidden |
| Customize → Hooks | every hook in effect and its recent runs (ok, blocked, error, timeout) |
| Customize → Connectors | the plugin's servers, their tools and each tool's Allow / Ask / Block |

Ask the person to read you what they see there; do not guess.

## Developer mode

On the plugin's Overview tab. Its open views reload when a file of the
plugin changes. A manifest change needs **Add from folder** again (or
Update for a marketplace plugin); approvals, settings and sign-ins are
kept, new permissions are asked for.

## Common problems

| What the person sees | Why | Fix |
| --- | --- | --- |
| `ui.<slot>.<id>: the entry … does not exist` | a typo, or the file is not in the plugin | fix the path (relative to the plugin's root) |
| `settings.<key>: the default is not in the expected form` | `pattern` must match the whole value | widen the pattern or change the default |
| `… the secret … is not passed in command lines or addresses` | `${secret.KEY}` in `args` or a URL | move it into `env` or `headers` |
| `runtime.server: a Python backend needs a uv.lock` | no lock file | run `uv lock` in the plugin and ship `uv.lock` |
| Runtime: "Its backend and connector servers waits for your approval: run its own code" | `runtime` not allowed yet | the person allows it on the Permissions tab |
| Runtime: "Its code waits for settings to fill in: …" | a `required` setting is empty | the person fills it in |
| Runtime: "could not be prepared" with a `deno cache` error | an import is wrong or not pinned | pin exact versions; check the URL |
| The backend's log: `Requires net access to "host:443"` | Deno refused an origin not approved | add `network:https://host` with a reason; the person allows it |
| The backend's log: `Requires read access` / `write access` | outside the plugin and its data folder | write to the data folder (the working directory), or ask for `files:…` |
| A tool answers `not_signed_in` | no sign-in for that account | the person signs in on the Accounts tab; the chat asks by itself for a required account |
| The host API answers 401 `unauthorized` | the key changed (plugin turned off and on, or the app restarted while the process was kept) | read `EVO_PLUGIN_KEY` from the environment each start; never store it |
| A view's call throws `EvoError` `permission` | the call needs a permission the plugin does not have | add it to `permissions` with a reason |
| A view shows nothing; the console says a script was refused | inline `<script>` or `onclick=` | move code to a `.js` file and add handlers in it |
| Sign-in: "needs your app's client ID: fill it in Settings" | `clientIdSetting` is empty | the person creates the OAuth app and fills in its id (and secret) |
| A hook never runs | `hooks` not allowed, wrong `matcher`, or the event never happens | check Customize → Hooks |

## Trying a backend by hand

A backend is an MCP server on stdio. To see it start outside evo:

```sh
deno run --allow-net --allow-env --allow-read server/main.ts
```

then type nothing: it should wait without printing (stdout is only for MCP
messages). Without `EVO_PLUGIN_HOST` it cannot reach evo; handle that with
a clear error instead of crashing. Inside evo, `evo.backend.call("tool",
{…})` from a view, or asking the agent to call the tool, tries it for real.
