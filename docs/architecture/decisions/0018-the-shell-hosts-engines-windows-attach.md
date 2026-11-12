# 0018. The shell hosts engines; windows attach to them as viewers

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-11 |
| ADD sections | [4. Process view](../README.md#4-process-view), [6. Runtime view](../README.md#6-runtime-view), [12. Risks and technical debt](../README.md#12-risks-and-technical-debt) |
| Related | [Shell-hosted engines](../../features/shell-hosted-engines/spec.md); amends [0003](0003-engine-runs-as-a-child-process-per-chat.md); takes the decision [0014](0014-agent-tools-by-layer.md) deferred; follows [0016](0016-memory-indexing-runs-in-a-short-lived-engine-process.md) |

## Context

[0003](0003-engine-runs-as-a-child-process-per-chat.md) runs one engine
process per open chat, started and relayed by the shell. It does not say who
decides when an engine runs. In practice the webview decides:

- `engine_start` is a Tauri command that only a window calls.
- On `ready`, the window sets up the engine's external tools and model.
- The window answers the hub's host requests (`mesh://host`), runs the
  Scheduler's timers and first prompts, decides wakes, records finished turns
  (`chat_record_turn`) and passes settings on to the engines it holds.

So "background" work is the main window, hidden. When the window is
destroyed or reloaded, mail, notes, wakes and scheduled runs stop. Turns
nobody watched are not recorded, which also starves memory indexing, since
that selects chats by last activity. Approvals of a chat that no window shows
block with no notice.

[0014](0014-agent-tools-by-layer.md) left the Scheduler's runner in the
webview, "starting a chat's engine is still the window's job, and moving that
is its own decision (a later ADR)". [0016](0016-memory-indexing-runs-in-a-short-lived-engine-process.md)
already put the index timer in the shell so it works with the window hidden.

## Decision

The shell owns engine lifecycles. An engine runs while its chat has work or a
window shows it. Windows attach to and detach from running engines and never
own them.

- **One supervisor in the shell** starts engines and sets them up: folder,
  flags, model, session and app tools, connector tools. It restarts an engine
  for pending work, at most 3 times in 2 minutes. It stops engines that have
  no work and no viewer, beyond an app-wide idle limit. "Work" means a running
  turn, running subagents, pending mail or notes, a prompt waiting to be sent,
  or an open question.
- **The hub's `Effect::Host`** calls the supervisor directly. No window is
  involved in delivering mail, notes or a new session's brief.
- **The shell decides wakes** with today's guard rails. Windows report drafts
  and focus.
- **The shell records turns** (count, cost including subagents, tokens,
  preview, model, unread, last activity) and **notifications** for chats no
  window shows.
- **The Scheduler's runner moves to the shell.** It is the only writer of run
  records. A run's first prompt is kept as hub mail, so it survives a reload.
- **The shell keeps open questions** (engine dialogs and app-tool questions)
  per chat, and serves them to a window that attaches.
- **Settings reach every engine from the shell.** Settings the shell needs
  move from window storage to the app database.
- **Windows keep presentation and the person's intent:** rendering, prompts,
  steering, answers, cancelling and dialogs.

## Consequences

- Work survives closing or reloading windows. Nested sessions, mail, wakes
  and scheduled runs run with the app in the tray, and each decision has one
  owner instead of one per window.
- Hosting logic moves to Rust: wakes, turn records, tool setup, the Scheduler
  runner and the restart limiter. The TypeScript versions are deleted
  responsibility by responsibility, never both active at once.
- Engines can run that no window shows. The sidebar and the tray must show
  them, and quitting must say that work is running.
- The ADD's process view changes from "per open chat, stopped when the chat
  closes" to "per chat with work or a viewer". Section 12 records the risk of
  engines working unseen.
- Mesh FR-21 – FR-24, subagents FR-19 and Scheduler FR-12 – FR-16 change from
  "a window" to "the shell". Their specs are updated with the feature.
- The engine protocol is unchanged. The shell already reads every engine's
  stdout, and windows receive the same `engine://message` events.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Keep a hidden main webview alive as the host (today) | Throttled timers, lost in-memory state on reload, and every decision duplicated per window; with the tray off there is no host at all. |
| A separate background daemon process owning engines | A second long-lived process, its own IPC and lifecycle; the shell already is that process while evo runs. Running with evo quit is a non-goal. |
| Let engines host each other (a parent starts its children) | Engines would need database, window and tool-setup knowledge, which breaks the engine/shell split (0003, 0013). |
