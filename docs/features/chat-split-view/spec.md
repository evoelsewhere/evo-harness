# Split view: several chats side by side in one window — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | desktop UI |
| Created | 2026-11-13 |
| Updated | 2026-11-13 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Engines run without a window](../shell-hosted-engines/spec.md) · [Nested sessions: roles and mail](../session-roles-and-mail/spec.md) |

## 1. Summary

A window shows one chat at a time. Watching several chats at once means
switching back and forth, or opening chats in windows of their own and
arranging them by hand. This matters more now that a chat starts nested
sessions and subagents that run beside it.

Split view lets the person drag a chat from the sidebar onto the chat
area. The area then splits into two panes side by side, like snapping
windows, and each pane shows a live chat. One pane has the focus: the side
panel, the shortcuts and the title bar follow it.

Panes open, close, resize and move with smooth, spring-like animation, so
the layout changes feel calm and easy to follow.

## 2. Scope

### 2.1 Goals

- Drag a chat row from the sidebar onto the chat area to show it in a
  second pane. Dropping it on the left or right half decides the side.
  Dropping it on an existing pane, when two are open, replaces that
  pane's chat.
- Two live panes side by side. Each streams its chat, has its own
  composer, and answers its own approvals and questions.
- One focused pane. A click or keyboard focus moves the focus. The side
  panel, the title bar, the chat highlighted in the sidebar, and commands
  such as the palette all follow the focused pane.
- Drag the divider between the panes to resize them.
- Drag a pane's header to the other side to swap the panes.
- Close a pane. The chat itself is not closed, and its work goes on.
- The same actions without a mouse: **Open in › Split view** in a chat's
  menu, and keyboard shortcuts to move the focus between the panes and to
  close a pane.
- Smooth motion for opening, closing, resizing and moving panes, and for
  the drop preview. The motion follows the system's "reduce motion"
  setting.
- The layout is kept per window and comes back after a restart.

### 2.2 Non-goals

- Separate OS windows per chat. These exist already through **Open in ›
  New window** and are unchanged.
- Split view inside a chat window (a window that shows one chat).
- The same chat in two panes at once.
- More than two panes, and stacked (top and bottom) panes.
- Tabs inside a pane.
- Splitting the Scheduler, Usage or Customize pages. These still take the
  whole chat area while open.

## 3. User scenarios

### US-1 Watch two chats side by side (priority P1)

A person runs a long task in one chat and wants to follow a nested session
it started, without switching back and forth.

Acceptance scenarios:

1. **Given** chat A is shown, **when** the person drags chat B from the
   sidebar to the right edge of the chat area and drops it, **then** the
   area splits into two panes: A on the left and B on the right. Both
   stream live.
2. **Given** two panes, **when** the person types in B's composer and
   sends, **then** B runs and A keeps running undisturbed.
3. **Given** two panes, **when** B asks for an approval, **then** the
   approval shows in B's pane and is answered there, whichever pane has the
   focus.

### US-2 See where a chat will land before dropping it (priority P1)

While dragging, the person sees where the chat will go.

Acceptance scenarios:

1. **Given** a chat is being dragged over the chat area, **when** the
   pointer nears an edge of a pane, **then** a translucent preview fills
   the half where the new pane will open. It moves smoothly as the pointer
   moves between zones.
2. **Given** a drag, **when** the pointer leaves the chat area or the
   person presses Esc, **then** the preview fades out and nothing changes.

### US-3 Arrange and close panes (priority P2)

Acceptance scenarios:

1. **Given** two panes, **when** the person drags the divider, **then**
   the divider and a veil over each pane follow the pointer, and both
   panes take their new widths when it is let go. Neither gets narrower
   than the minimum width.
2. **Given** two panes, **when** the person closes one, **then** the other
   grows to fill the space with a smooth animation and gets the focus.
3. **Given** two panes, **when** the person drags the left pane's header
   to the right half, **then** the panes swap sides.

### US-4 The layout comes back (priority P3)

**Given** a window with two panes, **when** the app restarts, **then**
the window shows the same panes, sizes and focus. A chat deleted
meanwhile leaves its pane out.

### Edge cases

- A chat that is already shown in a pane is dropped again: that pane gets
  the focus, and no second pane opens.
- A third chat is dropped while two panes are open: it replaces the chat
  of the pane it is dropped on.
- The window is too narrow for two panes: the layout is kept, but only the
  focused pane shows, with a compact switcher in its header to show the
  other. Both show again when the window widens (FR-13).
- A chat shown in a window of its own is dropped: its window closes and
  the chat shows in the pane (FR-11).
- A chat in a pane is archived or deleted: its pane closes.
- The focused pane's chat is opened from the sidebar with a plain click:
  it replaces the focused pane's chat, as clicking does today.
- A nested session started from one pane: it opens where it does today,
  and does not open a pane by itself. Its card offers Open beside (FR-14).
- The Scheduler, Usage or Customize page is opened: it takes the area.
  Closing it brings the panes back.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | The chat area of the main window holds one or two panes side by side, each showing one chat. | Given one pane, when a chat is dropped on the right half, then two panes show. |
| FR-2 | A chat row dragged from the sidebar over the chat area shows where it will land. With one pane: the left or right half, where the new pane opens. With two panes: the pane under the pointer, whose chat it replaces. | Given one pane and a drag over its right half, then the preview covers the right half. |
| FR-3 | Each pane is a full, live chat: thread, composer, approvals, questions, queued messages and the engine's status. | Given two working chats in panes, then both stream and both composers send to their own chat. |
| FR-4 | One pane has the focus, and it is marked. The side panel, the title bar, the sidebar highlight and the commands follow the focused pane's chat. A click, keyboard focus or a shortcut moves the focus. | Given focus on A, when B's thread is clicked, then the side panel shows B's tools. |
| FR-5 | The divider between the panes resizes them by drag and by keyboard. While it is dragged, the divider and a veil over each pane show the new widths at full frame rate. The chats take those widths when it is let go, as the veils fade. No pane gets narrower than its minimum width. | Given two panes, when the divider is dragged and let go, then both panes have the widths shown. |
| FR-6 | A pane can be closed, from its header or with a shortcut. Its chat keeps running in the shell. The remaining panes fill the space. | Given a running chat in a closed pane, then its run goes on and the sidebar shows it working. |
| FR-7 | A pane's header can be dragged to the other half to swap the two panes. | Given panes A·B, when A's header is dropped on the right half, then the order is B·A. |
| FR-8 | Without a mouse: **Open in › Split view** in a chat's menus (sidebar row and chat header) opens it in the other pane, plus shortcuts to move the focus between the panes and to close the focused pane. | Given a chat's context menu in the sidebar, when Split view is chosen, then it opens in a pane on the right. |
| FR-9 | Pane changes animate with a spring-like ease: a new pane grows in from its side, a closed pane shrinks out while the other fills in, a swap slides, and the drop preview glides between halves. Each takes no longer than about 400 ms. With "reduce motion" on, panes change at once, with at most a short fade. | Observe with reduce motion off and on. |
| FR-10 | The layout of each main window (panes, sizes, focus) is saved and restored at the next launch. Panes whose chat no longer exists are left out. | US-4. |
| FR-11 | A chat is shown in at most one place. Dropping a chat that a pane shows focuses that pane. Dropping a chat that has a window of its own closes that window and shows the chat in the pane. | Given B in its own window, when B is dropped on the right half, then B's window closes and B shows in the right pane. |
| FR-12 | A chat that is archived or deleted closes its pane. | Given B in a pane, when B is deleted, then its pane closes. |
| FR-13 | When the chat area is narrower than two minimum-width panes (about 360 px each), only the focused pane shows, with a compact switcher in its header to show the other. Both show again when the area widens. | Given two panes, when the window narrows below the threshold, then one pane and the switcher show. |
| FR-14 | A started nested session's card in its parent's chat offers **Open beside**, which shows the session in the other pane and keeps the parent in its own. The card can also be dragged onto the chat area like a sidebar row. | Given a parent alone, when Open beside is chosen on its session's card, then the parent shows on the left and the session on the right. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Smoothness: dragging, the drop preview, resizing and the pane animations hold 60 fps with 2 working chats streaming, on the developer's Windows machine. | Performance trace of a resize while two chats stream. |
| NFR-2 | Both panes' chats are attached to the window (each pane is a viewer of its chat's engine). A chat that leaves its pane is let go as switching chats does today. | The supervisor's viewers match the panes. |
| NFR-3 | One pane looks and behaves exactly as today: no header or divider shows until a second pane opens. | Visual comparison with one pane. |
| NFR-4 | Accessibility: the panes are labelled regions, the dividers are keyboard-operable separators, and the drop has a keyboard alternative (FR-8). | Keyboard-only walkthrough. |
| NFR-5 | Light and dark themes, following DESIGN.md (shadow-as-border, one blue accent for the focus mark). | Both themes. |
| NFR-6 | No new animation library: the motion is CSS (springs through `linear()` easing) and the browser's own animation API. | The dependency list is unchanged. |

## 5. Success criteria

- SC-1 A person can follow two running chats at once without switching,
  and act in either.
- SC-2 Opening a second pane by drag takes one gesture, and the person can
  tell where it will land before letting go.
- SC-3 Layout changes feel smooth: there are no jumps or flashes, and the
  thread scroll position of each pane is kept through resizes and moves.

## 6. Clarifications

### 6.1 Open

n/a — all answered.

### 6.2 Resolved

- 2026-11-13 — Q3: how does a drag start? → A: from a chat row in the
  sidebar (including nested sessions; drafts are not draggable) and from
  a pane's header. Drags from outside the app are ignored (FR-2, FR-7).
- 2026-11-13 — Q6: what does a pane header show? → A: only while two
  panes are open: the chat's title, its working dot, and buttons to close
  the pane and to open the chat in a window of its own. The focused pane
  gets a thin accent ring (FR-4, FR-6, NFR-3).
- 2026-11-13 — Q1: how many panes, and in what shapes? → A: two panes,
  side by side (FR-1, FR-2; non-goals).
- 2026-11-13 — Q2: a motion library or CSS only? → A: no preference
  given. The spec chooses CSS with spring curves through `linear()`
  easing, which is enough for two panes and adds no dependency (FR-9,
  NFR-6).
- 2026-11-13 — Q4: a window too narrow for its panes? → A: only the
  focused pane shows, with a switcher (FR-13).
- 2026-11-13 — Q5: a chat open in a window of its own? → A: dropping it
  moves it into the pane and closes its window (FR-11).

## 7. History

- 2026-11-13 — Draft; Q1, Q2, Q4 and Q5 answered.
- 2026-11-13 — Q3 and Q6 resolved with the proposed answers; accepted
  (Gate A).
- 2026-11-13 — FR-5: a dragged divider shows the new widths with veils
  and applies them on release. Laying both chats out again on every frame
  ran at about 36 fps, under NFR-1. The veils follow the pointer at full
  frame rate, the way iPad Split View resizes.
- 2026-11-13 — Implemented; every requirement checked in the app (tasks.md, section 2).
- 2026-11-13 — FR-14: Open beside and drag on a nested session's card. Open session replaced the focused pane, which is the parent's own.
