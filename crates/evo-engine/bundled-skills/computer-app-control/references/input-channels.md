# Input channels

## Contents

- Which channel a window uses
- Typing
- Clicks and the pointer
- Keys
- Setting a value
- Reading the window
- What the tool does when input shows no effect
- Refusals and safety nets
- A window kept off the screen

## Which channel a window uses

Every result line ends with how its input was delivered (`via …`). There
are three channels, chosen per window:

| Channel | Which windows | How input arrives |
|---|---|---|
| **Page** (`via page channel`) | A web-based app evo started itself with `open_app` (Edge, Chrome, Electron apps such as VS Code or Slack) | Straight into the page as real mouse and key events, through a private pipe to the app. Works with the window off screen and never in front. |
| **Web content** | A web-based app the person had already open (Edge, Teams, other WebView2 and Electron apps), and web panes inside native apps | Through the page's accessibility interface, and messages to the browser's window |
| **Native** | Every other Windows app | Through the app's accessibility interface (UI Automation) for buttons, fields and values, and messages to the focused control for keys and the pointer |

The page channel is the most reliable for web-based apps. When a web-based
app you need is already open and input there keeps missing, ask the person
whether you may restart it so evo starts it (`close_app`, then `open_app`):
never restart it on your own, unsaved work could be lost.

## Typing

| | Native | Web content | Page channel |
|---|---|---|---|
| Where the text goes | The control that has the app's keyboard focus, at its caret | The field given as `type`'s `ref`, else the field last clicked or invoked by ref, else the page's focused element; the tool presses Ctrl+End first, so the text goes **after everything already in the field** | The page's focused element, at its caret: click the field (or give `ref`) first |
| `\n` | A real Enter press | Shift+Enter, so a chat message is not sent | Shift+Enter, so a chat message is not sent |
| `\t` | A real Tab press: moves to the next field or cell | A tab character; focus does not move | A tab character |
| Pace | Waits after every Enter or Tab until the app has taken it in, and follows the focus to the next cell or field | A few milliseconds per character | One insertion per line |
| One `type` can fill | A whole form or table | One field | One field |
| Check | A field that reports its text is read before and after: a miss shows `not confirmed yet` and is never retyped | Read back automatically | The `changed:` line |

So only a native window fills several fields or cells from one `type`.
Elsewhere, fill one field per step: click or invoke it by ref, type (or
`set_value`), read it back, move on. Text is composed first, so Vietnamese
or any accented text arrives as the characters a keyboard would type.

## Clicks and the pointer

- **Page channel:** every click is a real mouse click in the page, at the
  ref's centre or the point, and the page's focus follows it.
- **A single left click by ref** elsewhere is performed through
  accessibility first: the element's invoke, toggle, select or expand
  action, or its default action. The result then says `(via UI Automation:
  …)`. No mouse click happened, so the caret and keyboard focus may not
  move.
- **A single left click by coordinates** is a posted mouse click in a native
  window, except on a button, tab, link, check box or menu item, which is
  pressed through its own action. In a document or text field the caret is
  then put at the point (`caret placed`), and a shift+click extends the
  selection to it. In web content the element at that point is tried
  through accessibility first.
- **Right, double and triple clicks** are posted mouse clicks, which web
  content without the page channel often ignores; a right click that opened
  no menu is followed by the request for a context menu Windows itself
  sends.
- **Hover** only moves the virtual pointer over the point; an app may not
  react, so a hover-only menu may not open. Look for the same command in a
  menu or by name.
- **Scroll** acts on the scrollable area under the ref or point, or at the
  window's centre when none is given.
- **Drag** presses at the start, moves in steps and releases at the drop
  point. It moves things inside the window; a file drop between apps follows
  the person's real cursor and cannot be done.

## Keys

- **Page channel:** real key events in the page, shortcuts included
  (`ctrl+n`, `ctrl+shift+p`); the page's focused element gets them.
- **Native:** posted to the focused control. `key` returns once the app has
  taken the key in, so the next action finds the focus where the key put
  it, including a dialog it opened. Modifiers are held for the app only;
  this can fail while the person is typing in the same app.
- **Web content:** sent to the browser's window, which routes them to the
  page's focused element. That is not necessarily the field last clicked by
  ref; click or invoke the field before keys that edit it.
- Some apps ignore background keys (WPF apps, some toolkits): the result's
  note says so. `find` the command and `invoke` it instead.

## Setting a value

| | Native | Web content |
|---|---|---|
| `set_value` | Writes through accessibility (the Value pattern); the app sees no key events | Focuses the field, selects all and types, so the page sees real input; line breaks become Shift+Enter |
| `direct: true` | Same as above | Writes through accessibility, for a field typing did not reach; rich editors may ignore it |

On a slider or spinner the number is clamped to the control's range. A
control that takes no value, or is read-only, is refused. Read the field
back: a written value is not always what the app keeps.

## Reading the window

- `snapshot` lists open menus and popups first. Each line is
  `- Role "name" value="…" [selected] [checked] [ref=e12] @x,y WxH`, with
  the box in screenshot pixels.
- `screenshot` with `marks: true` boxes and numbers the visible controls:
  the number is the ref (17 is `e17`). It hands out fresh refs, like a
  snapshot, and lists the marked controls under the picture.
- `zoom` with `region: [x, y, width, height]` (screenshot pixels, at most
  640×400) returns that region enlarged ×2. Aim inside it with a pointer
  action carrying `in_zoom: true` and the zoom picture's pixels; the next
  screenshot ends that.
- Elements scrolled out of the window are skipped and get no ref. Scroll,
  then look again.
- Values are cut at 200 characters, names at 120. A long text cannot be read
  back from a snapshot in full: read it in the app (scroll, its own find or
  word count) or from screenshots.

## What the tool does when input shows no effect

- Every acting action is read back: the result's `changed:` line lists what
  the window shows differently afterwards, or says `no visible change in the
  window`.
- A plain left click by ref with no visible change is tried once more on the
  other channel (an accessibility press becomes a mouse click, a mouse click
  the element's own action); the result says `No visible change; tried …`.
- Typing, keys and values are never sent twice: input could land twice.
- An action that still shows no change ends with `Nothing in the window
  changed. … Do not report it as done.` Try another way from the table in
  SKILL.md, or ask the person.

## Refusals and safety nets

- Refused: points on the window frame or title bar, refs into a window that
  is waiting on a modal dialog, refs into a dialog or menu that has closed,
  and typing into password fields.
- Blocked: Windows-key shortcuts and Ctrl+Alt+Delete. Closing keys are
  **not** blocked; the ground rules forbid them.
- Never controllable: evo itself, the Windows shell and security processes,
  apps running as administrator, and apps blocked in Settings.

## A window kept off the screen

The attached window is moved out of the person's view. A browser or another
Chromium window keeps one pixel on screen, in a corner, so it keeps
painting; other windows go fully off screen. It keeps running and keeps its
taskbar button. A window the person brings back is moved away again at your
next action, unless they pressed **Show the app**. Dialogs, menus and panes
the app opens stay off screen with it and are part of what you see and act
on.

A web page without the page channel may still lag in repainting. When a
step's result says the picture may be late, or a screenshot disagrees with
what you just did, trust `snapshot` and `find`.
