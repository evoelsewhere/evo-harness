# Connector servers

MCP servers the plugin brings as they are — a published server run with
`npx`/`uvx`, or a remote one over HTTP. They join evo's connector pool, and
their tools reach every chat (as `mcp__plugin_<plugin>_<server>__<tool>`).

## Where

`.mcp.json` at the plugin's root (read anyway), more files or an inline
map under `"mcpServers"`:

```json
{ "mcpServers": {
  "tracker": {
    "command": "npx", "args": ["-y", "tracker-mcp@1.4.2"],
    "env": { "TRACKER_URL": "${user_config.trackerUrl}", "TRACKER_TOKEN": "${secret.trackerToken}" }
  },
  "docs": {
    "url": "https://mcp.example.com/mcp",
    "headers": { "Authorization": "Bearer ${secret.account.docs}" }
  }
} }
```

| Field | What |
| --- | --- |
| `command`, `args`, `env`, `cwd` | a local server over stdio |
| `url`, `headers` | a remote server (streamable HTTP) |

Secrets only in `env` and `headers` (`${secret.KEY}`, or
`${secret.account.ID}` for an account's token, renewed by evo with a
restart of the server).

## How evo runs them

- They need `runtime` (added for you); with secrets, `secrets`.
- They start on demand and sleep when idle, like a backend; their tools are
  offered while asleep.
- evo cannot confine them: the consent says their network and files are
  "not enforced". They get the person's full environment (unlike a
  backend).
- The person sets each tool to Allow, Ask or Block in Customize →
  Connectors; Ask is the default.

## Connector server or backend?

| Choose | When |
| --- | --- |
| A connector server | a maintained MCP server already exists; pin its version (`tracker-mcp@1.4.2`, `uvx tracker-mcp==1.4.2`) |
| A backend (`references/backend.md`) | you write the code: Deno confines it, it asks evo for tokens instead of holding them, the plugin's views can call it |

## Tools the agent uses well

Whichever you write, each tool needs a name that says what it does
(`search_issues`), a description that says when to use it and what it
returns, a small input schema with described fields, and answers that are
short, structured and honest about failure.
