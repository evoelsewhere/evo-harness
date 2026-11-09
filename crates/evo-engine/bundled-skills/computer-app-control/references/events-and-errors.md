# Events and errors

## Contents

- What the person sees and can do
- Approval prompts
- Events during a turn
- Errors and what to do
- Setup: Settings

Every failed action comes back as `Error (<action>): <message>`. Messages
are quoted below as they start.

## What the person sees and can do

While an app is attached, its window is off their screen and a card floats
over the conversation. It shows the app live, with a virtual cursor that
travels to each point just before the input lands, and a line saying what
you are doing. Its controls:

| Control | Effect on you |
|---|---|
| **Stop** | Revokes control for this chat: the action running at that moment ends where it got to, and the app goes back to the person. Until they press **Allow again**, `attach` is refused. |
| **Allow again** (on the card after a Stop) | Lifts the Stop. It does not attach anything: attach again only if the person wants you to continue. |
| **Show the app** | Brings the window back to their screen and to the front, and ends app control for the rest of the turn. They may be working in it now. |
| **Close** | Hands the app back. `attach` is refused for the rest of the turn. |
| Clicks and keys in the card | Go to the app, as theirs. They may be helping (a sign-in, a captcha): look again before your next step. |
| Minimize, drag, resize | Only change the card. |

The app also goes back by itself when your turn ends, when the chat closes,
and when evo quits.

## Approval prompts

Settings decide whether each call asks first (**Ask every time**, the
default, which asks in every approval mode of the chat) or follows the
chat's approval mode (**Allow without asking**). A prompt lists the call's
acting steps as they will run (`click e12`, `type "Dear team…" (240 chars)
into e5`, `key ctrl+s`). Plan calls the person can approve at a glance: one
purpose per call, no surprises inside it. Calls made only of `status`,
`wait` and `detach` never ask.

A refusal comes back as `The person declined: …`, and the same actions are
refused without asking for the rest of the run. Do not retry them or reach
the same effect another way: stop and ask the person how to proceed.

## Events during a turn

| Event | What you see | What to do |
|---|---|---|
| The person pressed Stop | The running action ends; the next says `The user stopped …` | Nothing more in the app this turn. Say where you got to and ask whether to continue. |
| The person closed the card or pressed Show the app | `No app is attached…`; `attach` says `The person closed the app's card during this turn…` | Do not attach again this turn. Finish without the app, or ask. |
| The app was blocked in Settings after you attached it | `… is blocked in Settings …` or `… is not in the allowed apps … so it was handed back.` | Leave it. Tell the person the change stopped the work. |
| App control was switched off | The tool is gone, or `Computer app control is off.` | Stop; tell the person. |
| The person is typing in the same app | `The user is typing or clicking in this app right now…` | Wait and retry, or use `invoke` by ref, which needs no held keys. |
| The app window was closed | `The attached window (…) was closed…` | Do not reattach on your own: the person may have closed it on purpose. Ask. |
| A modal dialog opened | Results name the dialog's window; screenshots show the dialog and say so | Work in the dialog, or close it with Escape or Cancel before going back. |
| A menu or drop-down opened | A click result notes it; the next snapshot lists it first | Pick the item by ref, or press Escape to close it. |
| The app is busy with what you sent | `Note: The app is still handling this…` | Take a snapshot to see the menu or dialog it opened. Do not repeat the action. |
| The app took the front when it started | `open_app` notes that it brought its window to the front | Tell the person; evo does not take the front back. |
| The turn ended | The app is handed back automatically | Attach again in the next turn if the task continues. |

## Errors and what to do

| Error | Cause | Next step |
|---|---|---|
| `Computer app control is off.` | Switched off in Settings | Ask the person to turn it on in Settings → Computer apps. |
| `The app did not finish this action within … s; it was stopped where it got to.` | The action ran out of time | Read back what already happened before sending anything again; split a long `type` text into parts. |
| `No app is attached.` | Nothing attached, detached, or the card was closed | `list_windows`, then `attach`, unless the person closed the card this turn. |
| `No controllable window matches.` | The id is stale, or the app is never listed (blocked or protected) | `list_windows` again; a window that is not listed cannot be attached. |
| `"…" is already controlled from another chat.` | Another chat has it | Pick another window or ask the person. |
| `Unknown ref …` / `… no longer exists…` / `… has closed…` | A newer snapshot retired the ref, the app replaced the control, or its dialog or menu closed | Take a snapshot or `find`, and use a ref from it. |
| `… is waiting on the dialog …` | A ref behind an open modal dialog | Deal with the dialog first. |
| `(x, y) is outside the … screenshot` | Coordinates from an older or different-sized screenshot | Take a screenshot and use its pixels. |
| `in_zoom needs a zoom first` | `in_zoom` without a zoom, or after a newer screenshot | `zoom` on the region again. |
| `That point is on the window's frame or title bar…` | The point is not content | Use a shortcut or `invoke` for window-level commands. |
| `… is not visible on screen; try invoke instead.` | The ref's element has no position (scrolled out, collapsed) | `invoke` it, or scroll it into view first. |
| `… has no invoke/toggle/select/expand action` / `… refused the action` | The element offers no accessibility action, or the app declined it | Click it by ref or coordinates, or use its keyboard shortcut. |
| `… does not accept a value…` / `… is read-only` | `set_value` on a control without a writable value | Click it and `type`, or leave it. |
| `Unknown key name …` | A key name the tool does not know | Use names such as `enter`, `pagedown`, `f2`, `ctrl+shift+s`. |
| `Refused <key>: …` | A system shortcut | Do not retry; there is no in-app equivalent to send. |
| `The app is not responding.` | The app is hung or busy | `wait`, then look again; tell the person if it stays hung. |
| `Windows blocked input to this app…` | The app runs as administrator | Tell the person; it cannot be driven. |
| `Windows would not let evo hold Ctrl/Shift/Alt…` | The shortcut could not be delivered with its modifiers | `find` the command and `invoke` it. |
| `The app's page channel closed` / `The app has no page to send input to.` | A web-based app evo started quit or has no page yet | `wait`, then look again; `list_windows` if it closed. |
| `… is not an installed or running app.` / `No installed or running app has the exe …` | `open_app` with a name the catalog does not list | `search_apps` and pass the exe it lists. |
| `Skipped N action(s) … because <action> failed.` | An earlier action in the batch failed | Look at the app, then send the remaining steps again. |

Apps that read raw input (many games), some Store app surfaces and
administrator apps ignore background mouse and keyboard input;
accessibility actions (`invoke`, `set_value`) are the way in when they
expose the control. When nothing works, the step needs the person.

## Setup: Settings

Settings → Computer apps holds the on switch, Approval (Ask every time or
Allow without asking), and each app's rule: Default, Allowed (once any app
is Allowed, only those can be controlled) or Blocked (wins over Allowed).
Computer app control is available on Windows.
