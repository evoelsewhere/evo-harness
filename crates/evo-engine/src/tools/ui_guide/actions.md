## actions

Buttons for the next steps the person is likely to take.

Fields:
- `items` (required): 1 to 4 buttons. Each button has:
  - `label` (required): a few words.
  - `action` (required): what a click does, see below.
  - `primary` (optional): `true` on the first button to mark the likely choice.

The action object, here and on `bars` items and `table` rows, has exactly one key:
- `{"send": text}`: sends `text` as the person's next message in this chat, exactly as written (queued if you are still answering). Write it in the person's words: "Show details for 11 Nov". The person sees the text on hover.
- `{"open": path}`: opens a file of this chat's project in Evo's Files. A path relative to the project folder, or absolute inside it; add `:line` to go to a line (`src/main.rs:42`).
- `{"open": url}`: opens an `https://` link.

Anything else is refused and does nothing: other keys, two keys at once, other link schemes (`http:`, `file:`, `javascript:`), and paths outside the project.

Example:

```evo-ui
{"v":1,"type":"actions",
 "items":[{"label":"Fix the failing test","action":{"send":"Fix the failing test in evo-store"},"primary":true},
          {"label":"Open the test","action":{"open":"crates/evo-store/tests/migrations.rs:120"}},
          {"label":"Read the docs","action":{"open":"https://example.com/docs/testing"}}]}
```

Rules:
- A button only sends a message or opens something. It never runs a tool or a command: to run something, `send` a message that asks you to.
- Offer only steps that follow from your reply. At most 4 buttons.
- Put the buttons at the end of the reply.
