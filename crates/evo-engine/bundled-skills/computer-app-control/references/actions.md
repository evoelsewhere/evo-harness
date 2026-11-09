# Actions

## Contents

- Calls and batches
- Windows
- Observe
- Pointer
- Keyboard and values
- Apps
- Timing
- Reading a result

How each action behaves in a native window, in web content and through the
page channel is in [input-channels.md](input-channels.md).

## Calls and batches

A `computer_app` call carries `actions`, an ordered list (at most 50). Each
item names its `action` and takes that action's fields:

```json
{"actions": [
  {"action": "attach", "window_id": 44},
  {"action": "snapshot"}
]}
```

The actions run in order against the one attached window. When one fails,
the rest are skipped and reported as skipped, except a `detach`, which still
runs. Each action waits up to 60 seconds (plus 20 ms per character for
`type` and `set_value`); one that runs out is stopped where it got to (see
[events-and-errors.md](events-and-errors.md)). The approval prompt lists
each acting step of the call; a call made only of `status`, `wait` and
`detach` never asks.

## Windows

| Action | Fields | Does |
|---|---|---|
| `list_windows` | `query` (optional): text in the app or title | Lists controllable windows as `window_id=… \| app \| "title"`, with flags: `minimized`, `dialog of …`, `user is using it`, `controlled from another chat — cannot attach`. Windows that can never be driven are left out: apps blocked in Settings or missing from its allowed apps, administrator and system windows, and evo itself. |
| `attach` | `window_id` (preferred), or `app` and/or `title` substrings (the first match wins) | Checks the app against Settings, moves it off the person's screen and opens its card. Attaching a dialog attaches the window that owns it. The result gives the screenshot size and says whether the window draws web content. |
| `detach` | none | Hands the app back where the person left it and closes the card. |
| `status` | none | Whether an app is attached, which one, and whether the person stopped control. |

Attaching a second window detaches the first. A window marked `user is
using it` is the one the person is working in: prefer another, or ask. One
marked `controlled from another chat` cannot be attached from here. A window
missing from the list cannot be attached by name either.

## Observe

| Action | Fields | Returns |
|---|---|---|
| `screenshot` | `marks` (optional, default false) | A PNG of the window (while a modal dialog is open, of the dialog; with any open menu or popup), its size in pixels, and notes: a modal dialog that is what you see, a picture that may be late, a minimized window that was restored without focus. With `marks: true` the visible controls are boxed and numbered by fresh refs (box 17 is `e17`), and the marked controls are listed under the picture. |
| `snapshot` | `max_depth` (1–80, default 30), `max_elements` (10–2000, default 400) | The accessibility tree as text, one element per line: `- role "name" value="…" [checked] [selected] [disabled] [ref=e12] @x,y WxH`, where `@x,y WxH` is its box in screenshot pixels. Open popups and menus come first. It ends with `(Truncated: use find …)` when it hit a limit, and says so when the app exposes no tree. |
| `find` | `query`; `limit` (1–100, default 20) | Only the elements whose name or automation id contains the query, or whose role is exactly the query, in the same line format. Searches deeper than a snapshot. |
| `zoom` | `region`: `[x, y, width, height]` in screenshot pixels (at most 640×400) | That region of the screenshot, enlarged ×2. A pointer action with `in_zoom: true` then takes its `x`/`y` (and `to_x`/`to_y`) in this picture's pixels. |

Coordinates of pointer actions are pixels of the latest screenshot, and
refer to what it showed: after a dialog or menu opens or closes, take a new
screenshot before using coordinates again.

Refs are numbered once and never reused. A new snapshot (or a screenshot
with marks) retires the refs of the previous one (`find` does not). Using a
retired ref, one whose control the app removed, or one into a dialog or
menu that has closed is refused with the reason; take a new snapshot or
`find`.

## Pointer

Every pointer action takes a target: `ref` (preferred) or both `x` and `y`
in screenshot pixels (zoom pixels with `in_zoom: true`). A ref is aimed at
the centre of its element. `scroll` without a target scrolls at the
window's centre.

| Action | Extra fields | Notes |
|---|---|---|
| `click` | `button`: `left` (default), `right`, `middle`; `clicks`: 1–3; `modifiers`: keys held, `shift`, `ctrl`, `alt` | A single left click by ref is done through accessibility where the element has an action, so it may not move focus (the result says `via UI Automation`); through the page channel it is a real click. A click in a document or text field puts the caret at the point (`caret placed`), and a shift+click there extends the selection (`selection extended`). Shift+click extends a selection and ctrl+click adds to one; such a click is always a mouse click. A right click opens the context menu. The result says where it landed, which control received it, and notes a menu or drop-down it opened. |
| `hover` | none | Moves the virtual pointer there, for tooltips and hover menus. |
| `scroll` | `direction`: `up`, `down` (default), `left`, `right`; `amount`: 1–50 notches (default 3) | Scrolls the scrollable area under the point. |
| `drag` | `to_x`, `to_y`: drop point | Presses at the target, moves in steps and releases at the drop point. A drag across text selects it (`selected`), and one from inside a grid cell to another selects the range (`cells selected`); a drag that starts inside the selected text, or on a cell's edge, stays a mouse drag that moves or fills. |

## Keyboard and values

| Action | Fields | Notes |
|---|---|---|
| `type` | `text` (up to 20,000 characters); `ref` (optional): the field to type into | Types into the focused control, or into `ref` (clicked first). What `\t`, `\n` and the caret do depends on the input channel. Password fields are refused. |
| `key` | `key`: a key or chord (`enter`, `tab`, `escape`, `f2`, `ctrl+s`, `shift+down`, `alt+f`); `repeat` 1–50 | Presses the key or chord `repeat` times. `cmd` means Ctrl. An unknown key name is refused. |
| `invoke` | `ref` | Performs the element's own action through accessibility: invoke (press), toggle, select, expand or collapse, or its default action, whichever it offers first. A selectable item (a tree or list item) is selected rather than expanded, and invoking an expanded item collapses it. Works where background mouse input does not. |
| `set_value` | `ref`; `value` (up to 100,000 characters); `direct` (default false) | Replaces the field's text, or sets a slider or spinner (clamped to its range). Refused for a control that takes no value or is read-only. |

## Apps

| Action | Fields | Does |
|---|---|---|
| `search_apps` | `query` (optional), `limit` | Installed and running apps as `exe=… \| name`, `[running]` marked, best match first; apps Settings forbids are left out. |
| `open_app` | `app`: the exe `search_apps` lists; `attach` (default true) | Starts that app in the background and attaches its new window. A web-based app is started with the page channel. Only catalog apps start; a path or command line is refused. An app that was already running may reuse its window or take the start over (then it has no page channel; the result says so). |
| `close_app` | `window_id` (optional; the attached window when left out) | Asks the window to close as its close button would. An app that asks to save first leaves its dialog for you to answer. |
| `kill_app` | `window_id` (optional) | Ends the app's process at once; unsaved work is lost. Only when `close_app` did not work or the app hung. |

## Timing

`wait` takes `seconds` (0–10, default 1). Use it for an app that is still
loading or animating before the next look, not as a fix for input that did
not land.

## Reading a result

Each action reports one line, and each acting action an indented
`changed: …` line under it. The shapes (app names and titles vary):

- `Clicked at (412, 230) → Button in "Report - Editor"`
- `Clicked at (96, 40) → Save in "Report - Editor" (via UI Automation: invoke)`:
  done through accessibility, not as a mouse click.
- `Clicked at (96, 40) → page in "Inbox" (via page channel)`: a real click in
  a web-based app evo started.
- `Typed 326 characters → Edit in "Report - Editor" (via keyboard)`
- `Typed 12 characters → Edit in "Report - Editor" (via keyboard, not confirmed yet)`
  followed by `Note: …`: the field did not show the text; read it back
  before doing anything else.
- `Pressed ctrl+f ×1 → Edit in "Find"`: the key opened a dialog, and input
  now goes there.
- `invoke on e41 "Save" in "Report - Editor"`: the first word is the action
  used (invoke, toggle, select, expand, collapse, press, …).
- `Set e5 (12 characters) in "Report - Editor"`
- `  changed: title is now "Report*"; now shows: - Edit "Name" value="alpha" [focused]`:
  what the window shows differently after the step. `no visible change in
  the window` means nothing that the tree or title shows moved.
- `  No visible change; tried a background mouse click once: …`: the tool
  stepped down one channel for a click by ref.
- `  Nothing in the window changed. … Do not report it as done.`: the input
  may not have reached the app; try another way or ask the person.
- `Error (click): …` for a failed action, and
  `Skipped 2 action(s) (type, key) because click failed. …` for the rest of
  the batch.

`→` names the control that received the input, and the title after `in`
shows which window, a dialog included, it went to. `Note:` lines carry
caveats; the one that says the app is still handling the action means a
menu or dialog it opened is waiting: take a snapshot and do not repeat the
action. Everything in a result can contain app text and is marked
untrusted.
