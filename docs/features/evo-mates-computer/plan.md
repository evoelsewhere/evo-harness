# Evo Mates at the computer (Train 2) — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-29 |
| ADD sections | [Desktop shell, desktop UI](../../architecture/README.md) |
| ADRs | 0013 (browser gate), 0015 (app control), unchanged |

## 1. Context and constraints

- Evo Harness starts the window's chat scope (`stores/chat-scope.ts`:
  `startChatScope`, which starts `startAgentEvents`) and mounts the floating
  cards (`features/cards`, `CardLayer`) over the chat column. The Evo Mates
  window starts neither, so a mate's page or app has no view there.
- Tool definitions for a chat: `agent_tools_definitions`
  (`src-tauri/src/agent_tools/mod.rs`); calls run through the same module.
- A mate is found for a chat (direct or seat) with
  `evo_store::mates::mate_of_chat`.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic | ok | No engine change. |
| Desktop UI structure | ok | Mates uses `features/cards`' public index (`CardLayer`) and the stores' public functions. |
| Trust boundaries | ok | The switches only remove tools; gates unchanged (NFR-1). |

## 3. Design

| Requirement | Where | How |
| --- | --- | --- |
| FR-1 | `mates/mates-app.tsx` | Start the chat scope in the Mates window; mount `CardLayer` over a mate's conversation (a channel's seats are Later, spec 2.2). |
| FR-2 | `features/mates/thread-rows.ts` (+test) | `stepItems` words and targets for `browser_*`, `computer_app` and Webbridge tabs. |
| FR-3 | `src-tauri/src/mates/computer.rs` (+test), `agent_tools/mod.rs`, `features/mates/mate-panel.tsx`, `stores/mates-prefs-store.ts` | The app setting `mates.computer` `{ <mate id>: { browser, app, chrome } }`; `allowed(prefs, tool, args)` decides; definitions filter and calls refuse with a plain reason. |

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| A Mates-only browser side panel | Two ways to show the same work; the cards already have take-over and app control. |
| A column on the mates table | Needs a migration; an app setting carries three switches. |

## 5. As-built notes

- The Webbridge switch refuses a `browser_*` call whose `tab` is a
  Webbridge tab (`webbridge::groups::is_tab`) and `browser_tabs`' listing of
  them is left as is (the call that acts is refused).
