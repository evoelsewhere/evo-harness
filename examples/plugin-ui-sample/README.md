# Sticky Wall — a plugin with views

A sample Evo plugin that uses all four places a plugin view can show
([plugin UI spec](../../docs/features/plugin-ui/spec.md)):

| Slot | View | What it does |
| --- | --- | --- |
| Sidebar page | `wall` (`ui/wall.html`) | A wall of sticky notes: add, colour, pin to the open chat, drag to reorder, delete |
| Side panel | `notes` (`ui/notes.html`, Ctrl/⌘+K) | Notes pinned to the open chat; the agent's reply as it streams; keep the last reply as a note |
| Chat card | `board` (`ui/board.html`) | A table or bars the agent shows with `plugin_show_card` |
| Composer action | `insert` (`ui/insert.html`) | Pick a note to put into the message box |

## Try it

1. Customize → Plugins → Marketplaces → add this folder (or **Add from folder**).
2. Approve its three permissions: read your chats, keep its own data, put
   text in the message box.
3. Open **Sticky Wall** in the sidebar, the note icon in the side panel's
   tool rail, or the note button next to the model picker.
4. Ask the agent for a card, for example: *"Show a sticky-wall board card
   with this week's tasks as a table."* It calls:

   ```json
   { "plugin": "sticky-wall", "card": "board",
     "data": { "title": "This week", "columns": ["Task", "Owner"], "rows": [["Ship", "Ana"], ["Test", "Bo"]] } }
   ```

Turn on **Developer mode** for the plugin (Customize → Plugins) and its open
views reload whenever you save a file in this folder.

## How it is built

- `.evo-plugin/plugin.json` declares the views under `ui` and the
  permissions they need under `permissions`. Each view is an HTML file in
  the plugin; icons are SVG or PNG.
- Each view runs in its own webview, served from the plugin folder at an
  origin of its own. It reaches Evo only through `window.evo`, which Evo
  injects before the page's scripts. Every call is checked against the
  permissions the person approved.
- Scripts are files (`<script src>`): the page's policy runs no inline
  script and loads nothing from other sites unless a `network:` permission
  names the site.
- The app's theme arrives as CSS variables (`--evo-background`,
  `--evo-foreground`, `--evo-border`, `--evo-brand`, `--evo-radius`,
  `--evo-font-sans`, …) and follows light and dark (`ui/style.css`).
- Chat text is untrusted: the views put it in the page with `textContent`,
  never as HTML (`el()` in `ui/notes-store.js`).

## `window.evo`

| Call | Permission | Returns |
| --- | --- | --- |
| `evo.chat.active()` | `chat:read` | the open chat's id, or `null` |
| `evo.chat.list({ archived? })` | `chat:read` | `[{ id, title, preview, pinned, archived, parent, turns, createdAt, updatedAt }]` |
| `evo.chat.messages(chat?, { limit? })` | `chat:read` | `[{ role, text, at, tools?, tool?, isError? }]` |
| `evo.chat.onEvent(fn, chat?)` | `chat:read` | an unsubscribe function; `fn({ type, … }, chat)` with `turn_start`, `text_delta`, `message`, `tool_start`, `tool_end`, `turn_end` |
| `evo.chat.send(text, chat?)` | `chat:send` | the chat's id |
| `evo.tools.call(name, args)` | `tools:call` | the tool's result; approvals name the plugin |
| `evo.composer.insert(text)` | `composer:write` | — |
| `evo.storage.get/set/delete/list` | `storage` | values kept in the plugin's data folder (10 MB) |
| `evo.ui.notify(body, { title?, level? })`, `evo.ui.open(view)` | — | — |
| `evo.theme.tokens()`, `evo.theme.onChange(fn)` | — | `{ tokens, dark }` |
| `evo.permissions.query(id)`, `evo.permissions.request(id)` | — | `granted`, `denied`, `prompt` / `prompted`: an optional permission, asked for once |
| `evo.backend.call(tool, args)` | — | a tool result of the plugin's own backend (started if it sleeps) |
| `evo.auth.status()` | — | the plugin's accounts: `{id, service, kind, state, signIns: [{label, state, default}]}`, never a token or cookie |
| `evo.auth.signIn(account)` | — | opens the sign-in; `signedIn` or `cancelled` when it ends |
| `evo.view.data()`, `evo.view.resize(height)` | — | a card's data; a card or composer view's height |

A refused call throws an `EvoError` with a `code` (`permission`,
`bad_args`, `no_chat`, `rate_limited`, …) and a message.
