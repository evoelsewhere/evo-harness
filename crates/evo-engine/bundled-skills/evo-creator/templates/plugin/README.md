# my-plugin

One paragraph: what it does for the person, and where they find it in evo.

## What it asks for

| Permission | Why |
| --- | --- |
| Connect to api.example.com | Read your items from Example |
| Keep its own data | Remember what you pinned |
| Run its own code | Its backend reads your items |
| Run commands while the agent works | Log when a turn ends |
| Use the keys and accounts you give it | Your Example sign-in and API key |

## Set it up

1. Customize → Plugins → **Add from folder** → this folder; allow it.
2. **Settings**: the service address, and your OAuth app's client ID and
   secret (create the app at example.com/developers with the callback URL
   `http://127.0.0.1/callback`).
3. **Accounts** → Example → **Sign in**.
4. Open **My page** in the sidebar, or ask the agent about your items.

## Parts

- `.evo-plugin/plugin.json` — the manifest
- `ui/` — the page, panel, card and composer view (one page here)
- `server/main.ts` — the backend (Deno)
- `hooks/done.js` — a hook when a turn ends
