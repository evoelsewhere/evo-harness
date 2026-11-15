# Views

A view is an HTML page in the plugin, shown by evo in one of four places.
Each runs in a webview of its own at an origin of its own, and reaches evo
only through `window.evo`, which evo puts in the page before its scripts.

## Declaring them

```json
"ui": {
  "sidebar":  [{ "id": "desk",  "title": "Issue desk", "entry": "ui/desk.html",  "icon": "ui/icon.svg", "defaultShown": true }],
  "panels":   [{ "id": "notes", "title": "Notes",      "entry": "ui/notes.html", "icon": "ui/icon.svg", "shortcut": "K" }],
  "cards":    [{ "id": "chart", "title": "Chart",      "entry": "ui/chart.html", "icon": "ui/icon.svg" }],
  "composer": [{ "id": "pick",  "title": "Insert",     "entry": "ui/pick.html",  "icon": "ui/icon.svg" }]
}
```

| Slot | Where | Size |
| --- | --- | --- |
| `sidebar` | a page in the chat area, from a sidebar item (`defaultShown` puts it there at install) | the chat area: wide |
| `panels` | the side panel, after the built-in tools; `shortcut` is one letter or digit (Ctrl/⌘ + it) | narrow: the side panel's width |
| `cards` | in a chat's transcript, when the agent calls `plugin_show_card` with `data` | up to 576 px wide; height from `evo.view.resize` |
| `composer` | a button by the model picker that opens the view in a popover | 380 × 300 px, height from `evo.view.resize` |

`id` is unique per slot; `entry` and `icon` (SVG or PNG) are files in the
plugin. The agent can open a page or panel with `plugin_open`.

## What a page may load

The page's policy: scripts only from the plugin's own files (no inline
`<script>`, no `onclick=` attributes, no `eval`), styles from its files or
inline, images and fonts from its files or `data:`, no frames, no forms
posted elsewhere. `fetch` reaches only origins it has `network:`
permission for. So: `<script src="page.js" defer>`, and handlers set in JS.

## `window.evo`

| Call | Permission | Returns |
| --- | --- | --- |
| `evo.plugin` | — | `{ id, name }` |
| `evo.chat.active()` | `chat:read` | the open chat's id, or `null` |
| `evo.chat.list({ archived? })` | `chat:read` | `[{ id, title, preview, pinned, archived, parent, turns, createdAt, updatedAt }]` |
| `evo.chat.messages(chat?, { limit? })` | `chat:read` | `[{ role, text, at, tools?, tool?, isError? }]` |
| `evo.chat.onEvent(fn, chat?)` | `chat:read` | an unsubscribe function; `fn({ type, … }, chat)` with `turn_start`, `text_delta`, `message`, `tool_start`, `tool_end`, `turn_end` |
| `evo.chat.send(text, chat?)` | `chat:send` | the chat's id |
| `evo.tools.call(name, args)` | `tools:call` | the tool's result; the person approves as for the agent, and the approval names the plugin |
| `evo.composer.insert(text)` | `composer:write` | — |
| `evo.storage.get/set/delete/list` | `storage` | JSON values in the plugin's data folder (10 MB) |
| `evo.ui.notify(body, { title?, level? })` | — | a notification in evo |
| `evo.ui.open(view)` | — | opens one of its pages or panels |
| `evo.theme.tokens()`, `evo.theme.onChange(fn)` | — | `{ tokens, dark }`; the tokens are also CSS variables |
| `evo.permissions.query(id)`, `evo.permissions.request(id)` | — | `granted`, `denied`, `prompt` / after asking once, the answer |
| `evo.backend.call(tool, args)` | — | a result of the plugin's own backend tool (started if asleep): `{ content: [{type, text}], isError? }` |
| `evo.auth.status()` | — | `[{ id, service, kind, state, signIns: [{ label, state, default }] }]`, never a token |
| `evo.auth.signIn(account)` | — | opens the sign-in; `"signedIn"` or `"cancelled"` when it ends |
| `evo.view.data()` | — | a card's `data` |
| `evo.view.resize(height)` | — | a card's or composer view's height in px |

A refused call throws an `EvoError` with `code` (`permission`, `bad_args`,
`no_chat`, `rate_limited`, `backend`, `sign_in`, …) and `message`. At most
100 calls a second. Incognito chats are invisible to plugins.

## Writing a view well

- Text from chats, tools, services and storage is untrusted: set it with
  `textContent` or build nodes; never `innerHTML` with it.
- Show the state of every call: loading, empty, refused (show the
  `EvoError`'s message), done.
- Keep it light: no framework is needed; if you use one, ship it as files
  in the plugin (no CDN — the policy blocks it).
- Style it with `references/view-style.md` so it looks like the rest of
  evo, in light and dark.
- Turn on Developer mode while you work: open views reload on save.
