# 0015. The shell drives other desktop apps in the background only, never with the person's input

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-09 |
| ADD sections | [4. Process view](../README.md#4-process-view), [9. Cross-cutting concepts](../README.md#9-cross-cutting-concepts), [12. Risks and technical debt](../README.md#12-risks-and-technical-debt) |
| Related | [Computer app control spec](../../features/computer-app-control/spec.md), [0013](0013-app-side-agent-tools-run-in-the-rust-shell.md), [0014](0014-agent-tools-by-layer.md), [0010](0010-engine-enforces-approval-mode.md) |

## Context

The [computer app control](../../features/computer-app-control/spec.md)
feature lets an agent act inside another program on the person's machine. That
is a new trust boundary: until now the shell drove only its own webviews and
pseudo-terminals.

Windows has one pointer, one keyboard focus and one foreground window per
session. Any input that goes through the system's input stream (`SendInput`,
`SetCursorPos`, `SetForegroundWindow`) therefore takes them from the person.
macOS works the same way for its key window.

The other ways to give the agent its own screen each have a cost:

- a virtual monitor needs a signed display driver and an elevation;
- a hidden desktop is a known malware technique that security products flag;
- a second session needs patched or server editions;
- the OS's own agent session is in preview and closed to third parties.

v3 drove apps with posted messages and accessibility actions on windows moved
off screen. Its accuracy was limited by toolkits that ignore posted input and
by Chromium windows that stop painting off screen. The 2026-11-09 spikes show
that both problems are addressable in the background:

- a DevTools pipe into Chromium and Electron apps that evo starts works off
  screen;
- a one-pixel topmost corner keeps an already-open Chromium window painting.

The details are in
[research.md](../../features/computer-app-control/research.md).

## Decision

The shell drives other apps only through background channels, and never moves,
presses or focuses anything with the person's own input devices. The rules:

- **Allowed channels, in order:**
  1. a DevTools pipe into a web-based app that evo itself started;
  2. the OS accessibility API (UI Automation and MSAA on Windows, AX on macOS);
  3. messages posted to the app's own windows;
  4. per-process events on macOS.
- **Never used for the agent:** `SendInput`, `SetCursorPos`,
  `SetForegroundWindow` and activation, `BlockInput`, a virtual display
  driver, a hidden desktop, or the clipboard.
- **The window is moved, never hidden.** A controlled window moves out of the
  person's view. It is not minimized, made transparent or cloaked. A window
  that needs it keeps one pixel on screen, on top, so it keeps painting. Every
  moved window is journalled and put back.
- **One gate class for the tool.** The tool gets a gate class of its own. In
  "Ask every time" it asks for every acting call in every approval mode,
  because it acts outside evo's sandbox on the person's other programs.
- **Its own module in the shell.** The code lives in the shell's agent tools
  (ADR 0013) as a platform backend behind one trait. A later backend (an OS
  agent session) can be added without changing the tool.

## Consequences

- The person keeps the use of their machine while the agent works. Input
  isolation is a measured property (spec NFR-2), not a hope.
- Some tasks cannot be done in the background: hover-only UI, drag and drop
  between apps, key tips, raw-input apps, canvases without structure. The
  agent must say so (spec FR-19) instead of working around it with real
  input. This is listed as a known limit in ADD section 12.
- A DevTools channel into a third-party app is a powerful capability. It is
  only opened for apps evo starts, only as a pipe (no network port), and is
  closed with the app.
- Contributors must not add `SendInput`, cursor or foreground calls to the
  computer app control code. A unit test greps the module for them.
- Approval gating gains a class whose rule depends on a feature setting. The
  shell's gate and the engine's classification stay aligned (ADR 0010, 0014).

## Alternatives considered

| Option | Why not |
| --- | --- |
| Virtual monitor (IddCx display driver) | Needs a signed driver and an elevation, and still shares the one pointer and foreground; rejected by the owner (spec 6.2). |
| Brief "take-over" with real input while the person is idle | Takes the pointer and focus, however briefly; rejected by the owner (spec 6.2). |
| Transparent, click-through, topmost window | Counts as hiding the window; rejected by the owner (spec 6.2). |
| Hidden desktop (`CreateDesktop`) | Not composited, so only posted input works; it is flagged as a malware technique. |
| A second OS session (RDP loopback) or the OS agent session | Not available on client editions without patches, or not open to third parties yet; kept as a possible later backend. |
| Synthetic pen or touch pointer | Goes through the shared input path and targets screen points that an off-screen window does not have. |
