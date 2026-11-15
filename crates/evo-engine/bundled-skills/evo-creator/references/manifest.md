# The manifest

evo reads `.evo-plugin/plugin.json` (or `plugin.json` at the root). Every
field is optional except that a plugin should have a `name`. A wrong entry
is left out and listed on the plugin's page with its reason; the rest of
the plugin still loads. `templates/plugin/.evo-plugin/plugin.json` uses
every field.

## Fields

| Field | What |
| --- | --- |
| `name` | lowercase, dashes; the prefix of everything it adds (`issue-desk:triage`) |
| `version`, `description`, `author` (text or `{name}`), `homepage`, `repository` (text or `{url}`), `license`, `keywords` | shown on the plugin's page |
| `skills` | extra skill folders (paths); `skills/` is read anyway. A root `SKILL.md` with no `skills/` makes the plugin one skill (`references/skills.md`) |
| `commands` | command files or folders; replaces the default `commands/` (`references/skills.md`) |
| `agents` | subagent files or folders; replaces the default `agents/` (`references/skills.md`) |
| `hooks` | paths to hook files or inline `{Event: [...]}`; `hooks/hooks.json` is read anyway (`references/hooks.md`) |
| `mcpServers` | paths to `.mcp.json`-style files or an inline map; `.mcp.json` is read anyway (`references/connectors.md`) |
| `settings` | what the person fills in (`references/accounts.md`) |
| `accounts` | services it signs in to (`references/accounts.md`) |
| `runtime` | a backend evo runs (`references/backend.md`) |
| `ui` | views (`references/views.md`) |
| `permissions` | what it asks for: ids, or `{id, reason, optional}` |

Paths are relative to the plugin folder and must stay inside it.

## Placeholders

In hooks, connector servers and a backend's `env`, evo replaces:

| Placeholder | With |
| --- | --- |
| `${PLUGIN_ROOT}` | the plugin's folder |
| `${PLUGIN_DATA}` | its data folder (kept across updates, removed on uninstall) |
| `${user_config.KEY}` | a plain setting's value (a backend's `env` also takes `${setting.KEY}`) |
| `${secret.KEY}` | a secret setting — only inside `env` or `headers`; anywhere else it is removed and reported |
| `${secret.account.ID}` | the access token of account `ID`'s default sign-in, for a connector server (env or headers); the server restarts after a renewal |

A placeholder whose setting is empty stays as written: treat a value that
starts with `${` as unset.

## Permissions

| Id | For |
| --- | --- |
| `chat:read`, `chat:send`, `tools:call`, `composer:write`, `storage` | what views may do (`references/views.md`) |
| `network:https://host[:port]` | an origin views may fetch and a Deno backend may reach |
| `files:read:<path>`, `files:write:<path>` (`workspace` for the open workspace) | folders a Deno backend may read or change |
| `runtime` | run its backend or connector servers |
| `hooks` | run its hooks |
| `secrets` | use secret settings and accounts |
| `startup` | start its backend when evo opens |

`runtime`, `hooks` and `secrets` are added by evo when the plugin has
those parts; list them anyway with a `reason`. Mark ones the plugin can do
without as `"optional": true`; a view asks for them when needed
(`evo.permissions.request`). An update that asks for more waits for the
person's consent.

Trust and what each permission lets a part do: `references/security.md`.
