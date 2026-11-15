# Settings and accounts

## Settings

What the person fills in on the plugin's Settings tab. Keys use letters,
digits, `-` and `_` (camelCase reads best).

```json
"settings": {
  "baseUrl":  { "kind": "text", "title": "Server address", "required": true, "pattern": "https://[^ ]+", "group": "Connection" },
  "region":   { "kind": "select", "title": "Region", "options": ["eu", "us"], "default": "eu", "group": "Connection" },
  "project":  { "kind": "text", "title": "Project", "scope": "workspace", "description": "Each workspace may use its own." },
  "pageSize": { "kind": "number", "title": "Items per page", "default": 20, "min": 1, "max": 100 },
  "verbose":  { "kind": "boolean", "title": "Log more", "default": false },
  "apiToken": { "kind": "secret", "title": "API token", "description": "Settings → API in the service." }
}
```

| Field | What |
| --- | --- |
| `kind` | `text`, `multiline`, `number`, `boolean`, `select` (with `options`), `folder`, `file`, `secret` |
| `title`, `description` | what the person reads; say where to find a value |
| `default`, `required` | a required setting not filled holds the plugin's code back and shows "Needs setup" |
| `min`, `max` (numbers), `pattern` (text, a regular expression the whole value must match) | checked on save; a wrong value is refused with its reason |
| `group` | fields under one heading |
| `scope` | `personal` (default) or `workspace`: a workspace may set its own value |

A `secret` is kept in the plugin's vault (sealed with a key in the
operating system's keychain), never shown again, and reaches code only as
`${secret.KEY}` in `env`/`headers` or through the host API's token of a
`token` account. Any secret needs the `secrets` permission.

Code reads plain values from `env` placeholders or `GET /v1/settings`.

## Accounts

Services the plugin signs in to, listed on its Accounts tab with their
state; the person can hold several sign-ins of one service, one default and
one per workspace.

```json
"accounts": [
  { "id": "github", "service": "GitHub", "kind": "oauth", "reason": "List your issues",
    "oauth": { "authorizeUrl": "https://github.com/login/oauth/authorize",
               "tokenUrl": "https://github.com/login/oauth/access_token",
               "userinfoUrl": "https://api.github.com/user",
               "clientIdSetting": "githubClientId", "clientSecretSetting": "githubClientSecret",
               "scopes": ["repo"] } },
  { "id": "idp", "service": "Company ID", "kind": "oauth",
    "oauth": { "issuer": "https://login.example.com", "clientId": "evo-plugin-123", "scopes": ["openid", "email", "read"] } },
  { "id": "tv", "service": "Media", "kind": "device",
    "oauth": { "deviceUrl": "https://media.example.com/device/code", "tokenUrl": "https://media.example.com/token", "clientId": "abc" } },
  { "id": "tracker", "service": "Tracker", "kind": "token", "setting": "apiToken", "test": "check_key" },
  { "id": "portal", "service": "Portal", "kind": "web", "optional": true,
    "web": { "loginUrl": "https://portal.example.com/login", "domains": ["portal.example.com"], "done": { "cookie": "SESSION" } } }
]
```

| Kind | How the person signs in | What code gets |
| --- | --- | --- |
| `oauth` | their default browser; PKCE; back to evo on a one-time `http://127.0.0.1:<port>/callback` | `GET …/token` → access token, renewed by evo |
| `device` | evo shows a code and an address to open | the same |
| `token` | an API key in a `secret` setting (named by `setting`); `test` names a backend tool evo runs on "Test" | the token through `GET …/token`, or `${secret.KEY}` in env |
| `web` | a login window of its own storage, real address in its title, single sign-on allowed; done when `done.cookie` appears or the address starts with `done.url` | `GET …/cookies?url=` → a `Cookie` header for the declared `domains` only |

OAuth fields: `issuer` (endpoints discovered) or `authorizeUrl` +
`tokenUrl`; `deviceUrl` for `device`; `revokeUrl` (sign-out revokes);
`userinfoUrl` (the account's name, when the service sends no ID token);
`clientId`, or `clientIdSetting` and `clientSecretSetting` naming settings
the person fills with their own app's (for services that need a secret per
app, like GitHub); `scopes`; `redirectPort` when the service needs one
fixed port. Addresses are `https` (or `http` on `127.0.0.1`).

- Write the `reason` as what the person gets ("List your issues").
- `optional: true` when the plugin works without it; otherwise, when the
  agent calls one of its tools with the account not signed in, the chat
  asks the person to sign in first.
- An update that asks for more `scopes` marks the sign-in "Sign in again to
  grant …"; the old access keeps working until then.
- Views see only states and names (`evo.auth.status`); they may start a
  sign-in (`evo.auth.signIn`).
- Accounts need the `secrets` permission.
