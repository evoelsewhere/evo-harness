# 0013. App-side agent tools run in the Rust shell

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-08 |
| ADD sections | [4. Process view](../README.md#4-process-view), [8. Interfaces](../README.md#8-interfaces) |
| Related | [Agent browser spec](../../features/agent-browser/spec.md), [Agent terminal spec](../../features/agent-terminal/spec.md), [0003](0003-engine-runs-as-a-child-process-per-chat.md) |

## Context

The agent's browser and terminal tools were written in TypeScript and run in the
webview, offered to each engine as external tools. Two things make that a poor
home for them as the tool set grows:

- A tool call needs a live webview. The app can hide to the tray or close its
  window, and then a scheduled task or a background chat that calls a browser or
  terminal tool waits for a window that is not there.
- The tool logic (DevTools commands, waits, output parsing, gating) is native
  work in disguise: the webview only relays it to Rust commands, and it cannot be
  tested without the webview. The ADD already says the shell provides the native
  capabilities and the webview owns presentation only; the session tools already
  run in the shell (the mesh hub answers them).

## Decision

The tools for the built-in browser, the terminal and app control are implemented
in the Rust shell (`apps/desktop/src-tauri/src/agent_tools/`). The shell answers
an engine's `external_tool_request` for those tools itself, the way the mesh hub
answers the session tools. It owns their state (which chat a page or shell
belongs to, the output log of each shell) and the approval gate. The webview
keeps presentation: it shows tabs and pages from events, and renders and answers
the approval questions the shell raises.

## Consequences

- Tools work with the window hidden or closed, in scheduled runs and background
  chats.
- Tool logic is unit-testable with `cargo test`; the page helper script
  (`browser_page.js`) stays JavaScript because it runs in the page.
- The webview's browser and terminal tool code is deleted once the Rust tools
  replace it; the stores keep only what they show.
- The shell and the webview agree on a few events and commands (section 3.3 of the
  agent browser plan) for tabs, panels and approvals; they are additive.
- Tools that drive the UI (open a panel) are events the webview applies.
- The scheduler, `notify`, `send_file` and `show_file` tools stay in the webview
  for now (they act on UI state); moving them is a later decision.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Keep the tools in the webview | The window-dependency above, and logic that needs the webview to be tested. |
| Native engine tools with a browser-host protocol to the shell | The engine would need a new host protocol for webviews and ptys that live in the shell; a larger change with no gain over answering external tools in the shell. Possible later. |
