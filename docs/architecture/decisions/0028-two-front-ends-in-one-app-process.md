# 0028. Evo Harness and Evo Mates are two front ends in one app process

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-25 |
| ADD sections | [4. Process view](../README.md#4-process-view), [5.2 Desktop app](../README.md#52-desktop-app), [10. Deployment view](../README.md#10-deployment-view) |
| Related | [Evo Mates spec](../../features/evo-mates/spec.md); keeps [0001](0001-desktop-app-is-the-only-front-end.md) (the desktop app stays the only front end); builds on [0018](0018-the-shell-hosts-engines-windows-attach.md) |

## Context

Evo Mates is a second app for AI teammates: a roster of mates, one lasting
conversation each, and channels of several mates. It needs everything Evo
Harness already has: the engine, providers and credentials, models.dev data,
connectors, plugins, skills, approvals, memory, the Scheduler, usage
records and the app database.

Facts that shape the choice:

- The shell hosts engines and windows only attach to them as viewers
  ([0018](0018-the-shell-hosts-engines-windows-attach.md)). Work goes on with
  any window closed while the app runs in the tray.
- The app is single-instance: a second launch hands its arguments to the
  running process.
- The desktop already builds more than one page into one app: the quick entry
  box is its own page and window next to the main one.
- `evo.db` is opened by the shell, and each chat's transcript is written
  only by that chat's engine
  ([0007](0007-one-transcript-file-per-chat.md)). Two app processes would
  both want to own both.

The owner decided on 2026-11-25 that the person installs once, gets two
apps with their own launcher entries, and switches between them in one
click (spec 6.2).

## Decision

Evo Harness and Evo Mates are two front ends of one desktop app process:
one installer, one shell, one engine supervisor, one tray icon, one
database. Each front end is its own page and its own window, opened by its
own launcher entry.

- **One shell, two pages.** The desktop app builds a third page beside the
  main page and the quick entry page. The Mates window loads it. Both pages
  use the same engine client, stores and components; feature code of one
  front end does not import the other's internals.
- **Launchers hand over to the running app.** The Evo Mates launcher starts
  the same app with an argument that names the front end. The
  single-instance handler opens or focuses that front end's window. With no
  app running, the app starts with only that window.
- **App identity per window where the platform allows.** On Windows each
  window has its own application user model ID, so the taskbar shows two
  apps. On macOS a small launcher bundle in Applications opens the running
  app on the Mates window; the Dock shows one Evo. On Linux each front end
  has its own desktop entry.
- **Mates are engine sessions.** A mate's conversation is an engine session
  the shell hosts, like any chat; channels and hand-offs use the session
  mesh. No second engine kind and no second transport.
- **Window lifecycle.** Closing either window detaches it; the app keeps
  running while any window is open or the tray icon is on.

## Consequences

- Everything set up once works in both apps at once: providers, keys,
  connectors, plugins, settings, approvals and usage.
- No new process, transport or store: the shell, the protocol and
  `evo.db` stay the single owners they are today.
- A crash of the app process takes down both front ends. Engines are
  already separate processes, so work in progress survives as it does
  today.
- The desktop build grows a page and its bundle; shared code must stay in
  shared modules so the two front ends do not drift.
- Packaging grows: a second Start menu entry and taskbar identity on
  Windows, a launcher bundle on macOS, a second desktop entry on Linux. The
  release pipeline signs and ships them together.
- The ADD's process view, desktop app section and deployment view are
  updated with the feature's plan, when this ADR is accepted.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Two separate apps, each with its own shell, sharing the crates | Two processes would both open `evo.db`, both supervise engines and both own a tray icon; making them share needs a background service and a new local transport, which is a larger and riskier change than the feature. |
| One window with a mode switch between Harness and Mates | No launcher entry or taskbar identity of its own, so Evo Mates does not feel like an app; both apps could not be seen at once. |
| A separate Mates app that talks to the running Harness over a local port | A new network listener and trust boundary, plus a protocol between two apps, for no gain over a second window. |
| A Mates page inside the Harness sidebar | Mixes two different products in one navigation; the person who wants only mates still sees the developer tools. |
