# Recipes

Common plugins and the smallest set of parts for each. Start from the
closest one; each names the references to read.

## An AI answer in a card or page

Explain a chart, summarise an issue, label a list — with the person's own
models, no key of the plugin's.

```json
{ "name": "issue-ai",
  "ui": { "cards": [{ "id": "summary", "title": "AI summary", "entry": "ui/summary.html" }] },
  "permissions": [{ "id": "ai", "reason": "Summarise the issue you open" }] }
```

The view streams `evo.ai.run({ need: { tier: "fast" }, thread: issueId, … })`
into the card; give it `tools` (functions in the view) or `backendTools`
(its server's tools) to let the model look things up. Read `ai.md`.

## A pack of skills and commands

For a team's way of working: review checklists, release steps, house style.

```text
team-kit/
  .evo-plugin/plugin.json      { "name": "team-kit", "version": "1.0.0", "description": "…" }
  skills/review/SKILL.md
  skills/release/SKILL.md
  commands/standup.md
  agents/reviewer.md
```

No permission, no consent. Read `skills.md`.

## A guard on the agent's commands

Refuse or ask before risky commands; add context to prompts.

```json
{ "name": "guard",
  "hooks": { "PreToolUse": [{ "matcher": "bash|powershell", "hooks": [{ "type": "command", "command": "node", "args": ["${PLUGIN_ROOT}/hooks/guard.js"], "timeout": 10 }] }] },
  "permissions": [{ "id": "hooks", "reason": "Check the agent's commands before they run" }] }
```

Read `hooks.md`.

## Tools for a service with an API key

The agent searches a tracker; the person pastes a key once.

```json
{ "name": "tracker",
  "settings": { "apiKey": { "kind": "secret", "title": "API key", "description": "Tracker → Settings → API" } },
  "accounts": [{ "id": "tracker", "service": "Tracker", "kind": "token", "setting": "apiKey", "test": "whoami" }],
  "runtime": { "server": { "language": "typescript", "entry": "server/main.ts" } },
  "permissions": [{ "id": "network:https://api.tracker.example", "reason": "Search your tracker" }] }
```

The backend asks `GET /v1/accounts/tracker/token` and offers `search`,
`get_issue`, and `whoami` (the Test button). Read `backend.md`,
`accounts.md`.

## Tools for a service with OAuth

As above with `"kind": "oauth"` (or `"device"` for services without a
browser redirect). The plugin's own `clientId` when the service allows
public clients with PKCE; otherwise `clientIdSetting` and
`clientSecretSetting` for the person's own app, and say in the README how to
create it. Read `accounts.md`.

## A site with no API: a web login

The backend reads pages of a portal behind the person's login.

```json
"accounts": [{ "id": "portal", "service": "Portal", "kind": "web",
  "web": { "loginUrl": "https://portal.example.com/login", "domains": ["portal.example.com"], "done": { "cookie": "SESSION" } } }],
"permissions": [{ "id": "network:https://portal.example.com", "reason": "Read your tickets" }]
```

The backend asks `GET /v1/accounts/portal/cookies?url=…`, sends the
`Cookie` header, and calls `signed-out` when the portal shows its login
page again. Read `accounts.md`, `security.md`.

## An existing MCP server, packaged

```json
{ "name": "tracker-mcp",
  "settings": { "token": { "kind": "secret", "title": "Token" } },
  "mcpServers": { "tracker": { "command": "npx", "args": ["-y", "tracker-mcp@1.4.2"], "env": { "TRACKER_TOKEN": "${secret.token}" } } } }
```

Read `connectors.md`.

## A panel that follows the open chat

Notes, links or a checklist for the chat the person is in.

```json
"ui": { "panels": [{ "id": "notes", "title": "Chat notes", "entry": "ui/notes.html", "icon": "ui/icon.svg", "shortcut": "K" }] },
"permissions": [{ "id": "chat:read", "reason": "Pin notes to the open chat" }, { "id": "storage", "reason": "Keep your notes" }]
```

`evo.chat.active()`, `evo.chat.onEvent(fn)` without a chat follows the
window's open chat; `evo.storage` keeps the notes. Read `views.md`,
`components.md`.

## A card the agent fills

A chart, a table or a form in the transcript.

```json
"ui": { "cards": [{ "id": "chart", "title": "Chart", "entry": "ui/chart.html", "icon": "ui/icon.svg" }] }
```

The agent calls `plugin_show_card` with `{ "plugin": "<name>", "card":
"chart", "data": {…} }`; the card reads `evo.view.data()` and resizes. Put
the expected `data` shape in a skill of the plugin, so the agent knows it.
Draw charts with SVG you build (no CDN). Read `views.md`, `components.md`.

## A button in the composer

Insert a template, a link or a snippet into the person's message.

```json
"ui": { "composer": [{ "id": "insert", "title": "Insert a snippet", "entry": "ui/insert.html", "icon": "ui/icon.svg" }] },
"permissions": [{ "id": "composer:write", "reason": "Put the snippet in your message" }]
```

`evo.composer.insert(text)`, then the popover closes when the person
clicks away. Read `views.md`.

## A page with a backend and accounts

Everything together: `templates/plugin/` is this recipe — settings, an
OAuth account, a web login, a Deno backend, a page, a hook.
