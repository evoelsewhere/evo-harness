# Engines run without a window: the shell hosts them — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-11 |
| ADD sections | [4. Process view](../../architecture/README.md#4-process-view), [6. Runtime view](../../architecture/README.md#6-runtime-view), [9. Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts), [12. Risks and technical debt](../../architecture/README.md#12-risks-and-technical-debt) |
| ADRs | [0018 The shell hosts engines; windows attach to them as viewers](../../architecture/decisions/0018-the-shell-hosts-engines-windows-attach.md), [0003](../../architecture/decisions/0003-engine-runs-as-a-child-process-per-chat.md), [0014](../../architecture/decisions/0014-agent-tools-by-layer.md), [0016](../../architecture/decisions/0016-memory-indexing-runs-in-a-short-lived-engine-process.md) |

## 1. Context and constraints

The shell already owns the engine processes. What it lacks is the decisions
about them:

- **`apps/desktop/src-tauri/src/engine.rs`**
  - `engine_start` is the only spawn path, and only a window calls it. It
    kills an older process, calls `MeshHub::on_start`, bumps the generation
    and spawns `evo-engine --mode rpc`. The arguments are the session,
    sandbox, approval mode and `ChatFlags`. On Windows it puts the process in
    the job object.
  - A reader task handles each stdout line in order: `log_message`,
    `WorkingChats::on_line`, emit `engine://message`, `mesh::on_line`,
    `agent_tools::on_line`.
  - A 250 ms poll detects exit and calls `mesh::on_exit`.
  - There is no idle trim and no restart policy.
- **`apps/desktop/src/stores/engine-store.ts` is the host today.**
  - `launch()` is reached from `activate`, `start`, `onHost` (`mesh://host`),
    `startTaskChat`, `restartIdleEngines`, `moveChat` and worktree moves.
  - On `ready` it calls `startOnDefault` (`set_model` from the chat row), then
    `onReadyNested`. That runs `configureSessionTools`
    (`agent_mcp_chat_config`, then `mesh_session_tools` for top-level chats
    and `agent_tools_definitions`, then `configure_external_tools`), sends a
    `pendingPrompts` first prompt, and calls `mesh_hosted`.
  - It also holds:
    - `trimIdle` (`MAX_IDLE_ENGINES = 4`, per window)
    - the mail restart limiter `mayRestartForMail` (3 restarts in 120 s)
    - the wakes: `scheduleWake`, `checkWake`, `startAutomatedRun`,
      `WAKE_DELAY_MS = 2000`, `MAX_AUTO_WAKES = 5`, and the guards
      `composing`, `hasOpenPermission` and archived
    - turn recording: `recordFinishedTurn` → `chat_record_turn` →
      `evo_store::chats::record_turn`, plus `countAgent`/`pendingUsage` for
      subagent cost
    - settings fan-out: `broadcastApprovalMode`, `broadcastProviders`,
      `reload_resources` on `memory://changed`/`plugins://changed`, and
      `restartIdleEngines` on `SANDBOX_CHANGED`
- **Scheduler.** The runner is `stores/scheduler-store.ts` `startScheduler`
  (main window only, from `app/use-app-bootstrap.ts`). It runs missed tasks,
  then ticks every 20 s, and writes the whole `scheduler.tasks` setting. The
  shell's `agent_tools/scheduler/mod.rs` writes the same setting; it already
  has `schedule.rs` (`next_run`, `schedule_from`). So there are two writers.
- **Mesh hub** (`crates/evo-mesh/src/hub.rs`, adapter `src-tauri/src/mesh.rs`)
  - It does no IO.
  - `Effect::Host {chat, cwd, restore}` becomes `mesh://host`, and each
    window's `onHost` reacts to it.
  - `mesh_pending_chats` is called by the main window at start.
  - Prompt mail (`Deliver::Prompt`) is durable in the `mail` table.
- **Questions.**
  - The engine emits `ui_dialog_request` once and waits for
    `ui_dialog_response`. It emits `ui_dialog_closed` when the request ends
    another way. The hub already tracks `waiting` from these.
  - Shell tools ask through `ToolCtx::ask` → `ask_person` → `agent://ask`,
    answered by `agent_ask_answer`.
- **Notifications.**
  - `agent_tools/user.rs` sends OS notifications from Rust (`send_system`,
    `any_window_focused`, `MUTED`).
  - Bell entries live in the webview's persisted `notification-store.ts`.
- **Lifecycle** (`system.rs`, `lib.rs`)
  - With the tray on (default), closing hides `main` (`on_close_requested`).
  - `Destroyed` on a `chat-…` window stops that chat's engine.
  - There is no `RunEvent::ExitRequested` handling.
  - `show_main` only shows an existing window. `main` comes from
    `tauri.conf.json` (`visible: false`).
- **Usage.** The engine writes `usage_events` itself (`usage_log.rs`). Turn
  cost in the desktop is summed by the transcript reducer (`applyUsage`, from
  `message_end` usage), plus `extra-usage` for subagents.
- **Settings in window storage.** `ui-store.ts` (zustand `persist`) holds
  `autoStartSessions` (pushed through `mesh_configure`) and
  `chatConnectorsOff` (pushed through `agent_mcp_chat_config`).

Constraint: the desktop and its engine sidecar ship together, so a protocol
addition can be required in the same release. A stale dev sidecar must fail
visibly, not silently.

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | deviation, [ADR 0018](../../architecture/decisions/0018-the-shell-hosts-engines-windows-attach.md) | The shell takes over host decisions the webview makes today: start, set-up, restart, trim, wake, turn record, scheduled runs. These are lifecycle decisions, not agent decisions. Prompts, steering, answers and cancels stay with the person (NFR-2). ADD §4, §6, §12 change when 0018 is accepted (T39). |
| Layers point down; library crates stay free of engine/desktop concerns | ok | `evo-mesh` stays IO-free. Turn totals are computed in `evo-engine`. The supervisor lives in the shell. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | ok | One additive field, `agent_settled.turn`, with a `turnTotals` capability in `ready`. Engine, `src/engine/protocol.ts` and the shell are updated in phase 6. |
| Models come from models.dev; no hardcoded model facts | ok | The model comes from the chat row or the engine's default. Cost comes from the engine's provider pricing. |
| Real engine in the app; settings and credentials reload live | ok | The fan-out moves to the shell and reaches engines no window holds (FR-8). |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Approvals are never answered by the shell (FR-9). The sandbox config and approval mode are passed exactly as `engine_start` does. See 3.7. |
| Desktop UI structure and `DESIGN.md` | ok | Only `engine-store.ts` talks to the engine client; it now also calls the supervisor commands. There is no new UI except the quit question and the tray menu. |
| Accepted ADRs are not edited | ok | 0003 and 0014 stay. 0018 amends 0003's "per open chat" and takes the decision 0014 deferred. The specs amended are mesh FR-21 – FR-24, subagents FR-19, Scheduler FR-12 – FR-16 and NFR-2, and approval-modes (open questions without a viewer) (T40). |

## 3. Design

### 3.1 Overview

```text
            windows (viewers)                         shell (owner)
 ┌──────────────────────────────┐        ┌──────────────────────────────────────┐
 │ engine-store.ts              │ attach │ supervisor.rs  EngineSupervisor        │
 │  render, prompts, answers,   ├───────►│  ensure / setup / restart / trim       │
 │  composing, focus            │ detach │  work tracker   viewers   wakes        │
 │                              │◄───────┤  turn recorder  questions  notices     │
 └──────────────────────────────┘ events │  scheduler runner (scheduler/run.rs)   │
                                          │         │ spawn, stdin       ▲ stdout  │
                                          │         ▼                    │         │
                                          │   evo-engine (one per chat with work   │
                                          │   or a viewer)                         │
                                          │   mesh hub: Effect::Host → ensure      │
                                          └──────────────────────────────────────┘
```

`EngineSupervisor` wraps `EngineState`. Everything that starts an engine
calls `ensure(chat, why)`: a window, the hub, the Scheduler or an app start.
On `ready` the supervisor does the setup a window does today. Then it keeps a
small per-chat record (status, viewers, work, open questions, wake guard),
fed by the same stdout lines it already reads. Windows ask the supervisor and
listen. They never start or stop an engine on their own.

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| desktop shell | `apps/desktop/src-tauri/src/supervisor.rs` (new) | `EngineSupervisor`: `ensure`, setup on `ready`, viewers, work, restart limiter, idle trim, quit check | FR-1 – FR-4, FR-11, NFR-1, NFR-3 |
| desktop shell | `apps/desktop/src-tauri/src/supervisor/rpc.rs` (new) | Shell-side request/response to an engine (`id` prefix `shell-`), with a timeout | FR-1, FR-8 |
| desktop shell | `apps/desktop/src-tauri/src/supervisor/wakes.rs` (new) | Port of the wake logic: 2 s gather, guards, 5 in a row, prompt texts | FR-5 |
| desktop shell | `apps/desktop/src-tauri/src/supervisor/questions.rs` (new) | Open-question registry per chat: engine dialogs and shell asks | FR-9 |
| desktop shell | `apps/desktop/src-tauri/src/supervisor/turns.rs` (new) | Turn records from `agent_settled.turn`; unread from viewers and focus; run notifications | FR-6, FR-10 |
| desktop shell | `apps/desktop/src-tauri/src/supervisor/settings.rs` (new) | Fan-out of approval mode, providers, resources, sandbox restarts; settings moved from window storage | FR-8, NFR-4 |
| desktop shell | `apps/desktop/src-tauri/src/engine.rs` | `engine_start` becomes `start_process` (private), called only by the supervisor; reader task feeds the supervisor | FR-1 |
| desktop shell | `apps/desktop/src-tauri/src/mesh.rs` | `apply`: `Effect::Host` → `supervisor.ensure`; pending chats hosted from `setup` | FR-4 |
| desktop shell | `apps/desktop/src-tauri/src/agent_tools/scheduler/run.rs` (new), `mod.rs` | Runner: 20 s tick, missed runs, busy skip, first prompt as hub mail, run records; the only writer of `scheduler.tasks` | FR-7 |
| desktop shell | `apps/desktop/src-tauri/src/agent_tools/mod.rs` | `ask_person` registers with `questions.rs`; viewers come from `AgentTools.windows` (`agent_window_chat`) | FR-9, FR-6 |
| desktop shell | `apps/desktop/src-tauri/src/agent_tools/user.rs` | `send_system` is reused for run notices, with a click target | FR-9, FR-10 |
| desktop shell | `apps/desktop/src-tauri/src/system.rs`, `lib.rs` | Tray menu with working/waiting lines, quit question, `ExitRequested`, main window rebuilt by `show_main`, chat windows detach on `Destroyed` | FR-2, FR-11, FR-12 |
| engine | `crates/evo-engine/src/modes.rs` | `agent_settled.turn` totals (own usage and subagent usage settled since the last turn); `turnTotals` capability | FR-6 |
| engine | `crates/evo-engine/src/rpc/protocol.rs` | Capability flag in the `ready` header | FR-6 |
| desktop UI | `apps/desktop/src/stores/engine-store.ts` | `launch` → `engine_attach` / `engine_ensure`; host, setup, trim, wake, record, fan-out code removed, one phase at a time | FR-2, NFR-1 |
| desktop UI | `apps/desktop/src/engine/client.ts`, `src/engine/protocol.ts` | New commands and events; `agent_settled.turn` type | FR-2, FR-6 |
| desktop UI | `apps/desktop/src/stores/scheduler-store.ts`, `app/use-app-bootstrap.ts` | Page edits through shell commands; `startScheduler` removed | FR-7 |
| desktop UI | `apps/desktop/src/stores/notification-store.ts`, `features/notifications/notify.ts` | Drain shell-held bell entries; open-chat target | FR-9, FR-10 |
| desktop UI | `apps/desktop/src/stores/ui-store.ts` | `autoStartSessions` and `chatConnectorsOff` read from and written to the shell; one-time import of stored values | FR-8, NFR-4 |
| desktop UI | `apps/desktop/src/features/shell/quit-dialog.tsx` (new) | "N chats are working. Quit anyway?" when a window is open | FR-11 |

### 3.3 Interfaces

**Tauri commands (new).** All take `chat` as the chat id.

```ts
// Make sure the chat's engine runs; start it if needed. Replaces engine_start for windows.
engine_ensure(chat, cwd?: string, options?: ChatFlags, restore: boolean): { generation: number; started: boolean }
// This window shows the chat / no longer shows it. Attach returns what to draw at once.
engine_attach(chat): AttachState
engine_detach(chat): void
type AttachState = {
  generation: number | null;            // null: no engine runs
  status: "starting" | "ready" | "exited" | "error" | "stopped";
  working: boolean;                      // a run, running agents, pending mail or prompt
  questions: OpenQuestion[];             // every open question of the chat
};
type OpenQuestion = {
  requestId: string; source: "engine" | "shell";
  kind: "permission" | "questions" | "choice";
  title: string; body?: string; options: string[]; ui?: unknown; at: number;
};
engine_set_composing(chat, composing: boolean): void   // a draft sits in this chat's composer
engine_working(): { chat: string; title: string; state: "working" | "waiting" }[]
quit_confirmed(): void                                 // the quit dialog's "Quit anyway"
scheduler_save(task: ScheduledTask): ScheduledTask     // create or update (page)
scheduler_delete(id: string): void
scheduler_run_now(id: string): void
notifications_take(): BellEntry[]                      // entries the shell recorded with no window
settings_import_window(values: { autoStartSessions?: boolean; chatConnectorsOff?: Record<string, string[]> }): void
```

`engine_start` stops being a command. `engine_stop`, `engine_send` and
`engine_running` stay. `engine_stop` is still the person's "stop this chat":
it stops the engine and its work.

**Tauri events (new).** The existing ones are unchanged: `engine://message`
and `engine://exit` (all windows), `scheduler://changed` and `mesh://prompted`.

```ts
"engine://state"           { chat, generation, status, reason?: "viewer" | "mail" | "schedule" | "restart" | "start" }
"engine://question"        { chat, question: OpenQuestion }
"engine://question-closed" { chat, requestId }
"app://open-chat"          { chat }                    // a notification or the tray asked to show a chat
"app://quit-requested"     { working: number }         // a window shows the quit dialog
"notify://entry"           { entry: BellEntry }        // a bell entry while a window is open
```

`mesh://host` is no longer emitted (FR-4); `onHost` is deleted.

**Shell → engine.** These are existing RPC commands:
- `configure_external_tools`, `set_model`, `set_approval_mode`,
  `reload_providers`, `reload_resources`
- `prompt`, sent through hub mail (`Deliver::Prompt`), never directly
- a mesh `subscribe` at level `state` from the subscriber address `shell`,
  for node states (running agents, `meta.wake`)

Shell requests carry ids `shell-<n>`. Windows filter replies by their own
`ID_PREFIX` and ignore these.

**Engine protocol (additive).** `agent_settled` gains `turn`:

```jsonc
{ "type": "agent_settled", "outcome": "completed", …,
  "turn": {
    "provider": "…", "model": "…",
    "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0,
    "costUsd": 0.0,            // the run's responses
    "childCostUsd": 0.0,       // subagents and plugin children settled since the last turn
    "childTokens": 0,
    "preview": "…"             // start of the final text; absent for incognito
  } }
```

- `ready` advertises `"turnTotals": true`.
- `turn` is absent on a compaction run.
- A shell meeting an engine without the capability (a stale dev sidecar)
  logs `engine has no turn totals: rebuild the sidecar` once, records no
  turn, and lets the window record as before.

**Settings keys** (`app_settings`, `evo_store::settings`):

| Key | Type | Default | Was |
| --- | --- | --- | --- |
| `mesh.autoStartSessions` | bool | `false` | `ui-store` `autoStartSessions` |
| `connectors.chatOff` | object `{chat: [server]}` | `{}` | `ui-store` `chatConnectorsOff` |
| `notifications.inbox` | array of `BellEntry`, at most 100 | `[]` | — (shell-held bell entries) |
| `scheduler.tasks` | array | unchanged | single writer: the shell |

### 3.4 Data and storage

- **No `evo-store` migration.** The new state goes into `app_settings` keys
  (3.3) or stays in memory.
  - Turns use the existing `chats` columns (`record_turn`).
  - First prompts use the existing `mail` table.
  - The other open feature,
    [session-roles-and-mail](../session-roles-and-mail/spec.md), is expected
    to take `0013`. This feature takes no number.
- **In memory, in the supervisor, per chat:**
  - status and generation
  - viewers (window labels)
  - working flags: run active, running agents, pending prompt mail, and
    the hub's `waiting`
  - open questions
  - wake state: pending set, timer, wakes in a row
  - restart times
  - last settled turn id

  All of it is lost on quit, which is fine: engines stop on quit.
- **Bell entries** recorded while no window is open go to
  `notifications.inbox`. The next window drains them with
  `notifications_take`.

### 3.5 Behaviour

**Ensure and set up (FR-1).**

```text
ensure(chat, why):
  if running(chat): return
  folder = worktree_path ?? workspace path ?? home        (as chatFolder)
  flags  = ChatFlags::for_engine(row, options)
  start_process(chat, folder, flags) → status=starting, emit engine://state
on ready(chat):
  set_model(row.provider, row.model)   if the row names a model
  configure_external_tools(session tools if top-level + agent_tools_definitions)
       ── refused because a run is active → retry on agent_settled
  mesh subscribe(level=state, from=shell)
  hub.on_hosted(chat) → flush mail (a session's brief, a scheduled prompt, notes)
  status=ready, emit engine://state
```

A window's own work is the same call: `launch()` becomes `engine_attach`,
then `engine_ensure` when no engine runs. A window still restores and draws
the transcript from `get_messages`.

**Work and idle trim (FR-3).** A chat has work while any of these holds:
- a run is active (`WorkingChats`)
- an agent node is running (from the `shell` subscription)
- the hub holds mail or a prompt for it
- it has an open question

After each `agent_settled` and each `engine_detach`, trim runs. It takes the
chats with no work and no viewer, sorts them by last activity, and keeps the
newest 4. The rest stop (`stop_process`), which is not the person's
`engine_stop`.

**Restart limiter (FR-4).** On exit with pending work, `ensure(chat,
restart)` runs if the chat had fewer than 3 restarts in the last 120 s.
Otherwise the shell logs, notifies "The engine keeps stopping · {title}" and
leaves the mail waiting.

**Mesh hosting (FR-4).**
- `mesh::apply`: `Effect::Host {chat, cwd, restore}` calls `ensure`. When
  `cwd` is given (a new worktree session), it is the folder.
- `supervisor::setup` hosts `mesh_pending_chats` at app start (archived
  chats excluded). Hosting no longer waits for the window.

**Wakes (FR-5), a port of `checkWake`.**
1. The shell subscription shows `meta.wake` on the session node, or the hub
   reports a note or mail for an idle chat.
2. The wake waits 2 s, gathering everything that arrives.
3. It is skipped while the chat has a draft (`engine_set_composing`), an
   open question or is archived.
4. A chat may be woken at most 5 times in a row without the person. The
   count resets when a window sends a prompt (the shell sees a `prompt` with
   no `shell-` id).
5. The wake runs one automated turn with the same `<background-agents-done>`,
   `<nested-sessions-done>` and `<mail-arrived>` texts, sent as hub
   `Deliver::Prompt` mail so it is durable.

The texts move as constants with tests that pin them.

**Turn records (FR-6).**
- On `agent_settled` with `turn`, the shell calls `chats::record_turn` once
  per run id. Totals:
  - `costUsd = turn.costUsd + turn.childCostUsd`
  - `tokens = input + output + cacheRead + cacheWrite + childTokens`
  - `preview` (cut by `record_turn`), and the provider and model
- `unread` is true unless a window that holds the chat is focused and shows
  it. Viewers come from `agent_window_chat`; focus from
  `any_window_focused`.
- The engine adds `childCostUsd` for background children that settled after
  their turn. Those are counted in the next turn, as subagents FR-6 says
  today.
- Parity is checked by a test that runs the same faux run through the TS
  reducer's numbers and the engine totals (T24).

**Open questions (FR-9).**
- `ui_dialog_request` adds to the registry; `ui_dialog_closed`, or a
  `ui_dialog_response` written by a window, removes it.
- `ask_person` registers before it emits `agent://ask` and removes on answer.
- If no focused window holds the chat:
  - `send_system` sends "Approval needed · {title}" (or "A question from
    {title}"), with an action target that emits `app://open-chat` and calls
    `show_main`;
  - a bell entry is pushed;
  - the tray line updates.
- Nothing times out.

**Scheduler (FR-7).** `scheduler/run.rs` replaces `startScheduler`.
- **Start.** At `setup`, runs that were missed run once (`trigger: missed`),
  also with `--hidden`.
- **Tick.** Every 20 s it starts due tasks.
  - It skips a task whose last run's chat has work.
  - Otherwise it creates the task chat (`chats::create_chat`, `scheduled`)
    and posts the prompt as hub mail `Deliver::Prompt` to that chat. Mail
    calls `ensure`.
  - It appends the run, disables a `once` task, sets `nextRunAt`, writes the
    setting, emits `scheduler://changed` and notifies.
- **Single writer.** The page and the agent tools edit through the shell
  (`scheduler_save` / `scheduler_delete` / `scheduler_run_now`, and the
  existing tools in `mod.rs`). All of them go through one mutex around
  read-modify-write.

**Settings fan-out (FR-8).**
- `settings.rs` hooks in where the shell already sees each change:
  - `db.rs` `settings_set` dispatches by key. `approvalMode` sends
    `set_approval_mode` to every ready engine. `sandbox` restarts engines with
    no work; the others restart on settle. This replaces the webview's
    `sandbox://changed` → `restartIdleEngines`.
  - An engine's `providers_changed` line (seen by the reader task) sends
    `reload_providers` to every other engine: idle ones at once, busy ones on
    settle. This replaces `broadcastProviders`.
  - Where `plugins://changed` (`plugins.rs`) and `memory://changed`
    (`memory_index.rs`) are emitted, `reload_resources` goes to the idle
    engines.
- `mesh.autoStartSessions` replaces the in-memory flag set by
  `mesh_configure`.
- `connectors.chatOff` is read by `agent_mcp_chat_config` when the
  supervisor configures tools.

**Lifecycle and tray (FR-11, FR-12).**
- `RunEvent::ExitRequested` calls `prevent_exit` while the tray is on.
- Quit from the tray, or closing the last window with the tray off, asks
  `engine_working()` first. When it is not empty:
  - a visible window shows the quit dialog (`app://quit-requested`);
  - with none, `show_main` builds the main window and asks there.
- `quit_confirmed` runs `quit_app`.
- `show_main` rebuilds `main` with `WebviewWindowBuilder` from the
  `tauri.conf.json` values when it is gone.
- Chat windows: `Destroyed` calls `engine_detach` for their chat instead of
  stopping its engine.
- The tray menu gains lines that are disabled while empty: "2 chats working"
  and "1 chat waiting for you". Clicking a waiting line opens that chat.

**Cut-over order (NFR-1).** Each responsibility moves in one phase, and the
TypeScript code for it is deleted in the same phase. Both never run.
1. ensure and setup
2. hosting
3. viewers and trim
4. questions
5. wakes
6. turn records
7. settings
8. Scheduler
9. lifecycle

Until phase 6, windows keep recording turns. From phase 6 only the shell
records.

### 3.6 Errors and edge cases

- **A window reloads.** The engine keeps running. The window calls
  `engine_attach` and gets status, `working` and every open question
  (NFR-3). `pendingPrompts` no longer exists: first prompts are hub mail.
- **A setup request fails** (an engine dies before `ready`). The restart
  limiter applies. A failing `configure_external_tools` during a run is
  retried on `agent_settled`, as today.
- **The person stops a chat** (`engine_stop`). Its mail stays in the hub,
  which does not host it again until a new message or the person opens it,
  the same as today's archived and stopped rules. Trim never stops a chat
  with work.
- **Incognito chats.** No preview is recorded (the engine omits it, and
  `record_turn` nulls it). Notifications say "A chat" instead of its title.
  The run log stays redacted as today.
- **A stale sidecar without `turnTotals`.** See 3.3. The window keeps
  recording, and the shell logs once.
- **Two windows show one chat.** One engine, two viewers. Unread is false if
  either is focused on it.
- **Quit while an approval waits.** It counts as working ("waiting for
  you"). Quitting ends the run, the same as quitting today.

### 3.7 Security and trust

- **Unseen work.**
  - Engines can now run with no window. They still run only for work the
    person or an agent they allowed started: a session they started or
    allowed, mail inside their own trees, or a task they scheduled.
  - The tray and the sidebar always show working and waiting chats (NFR-5).
  - Quitting stops everything, after one question.
- **Approvals.** The shell records and announces questions; it never
  answers them. Approval mode, the read-only guard, hooks and the shell gate
  run unchanged in the engine and in `gate.rs`. A run that needs the person
  waits.
- **Sandbox and trust.** The supervisor starts engines with the same
  `--sandbox-config`, `--approval-mode` and `ChatFlags` as `engine_start`.
  Project trust checks stay in the engine.
- **Notification contents.**
  - Titles only, never message text.
  - Incognito chats show "A chat".
  - The mute flag (`MUTED`, Do not disturb) is respected for every notice the
    shell sends.
- **Automated prompts.** The shell sends only the fixed wake texts and the
  Scheduler's stored prompt, both as mail the person can see in the
  transcript (event chips, not user bubbles: mesh FR-28).

### 3.8 Configuration

| Setting | Type | Default | Stored | Applies |
| --- | --- | --- | --- | --- |
| `system.tray` | bool | `true` | `app_settings` (existing) | live; with it off, closing the last window asks when work runs |
| `mesh.autoStartSessions` | bool | `false` | `app_settings` (moved) | live |
| `connectors.chatOff` | object | `{}` | `app_settings` (moved) | next tool set-up of that chat |
| idle engines kept | — | 4 | constant `IDLE_ENGINES` in `supervisor.rs` | — |

The UI is unchanged: **Settings → Sandbox** (start sessions without asking),
the connector menu in the composer, and **Settings → General** (tray).

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Keep the hidden main webview as the host, fix the gaps there | It still depends on a webview's timers and memory, and duplicates every decision per window (NFR-1, NFR-3). See ADR 0018. |
| A daemon process outside the app | A second lifecycle and IPC for no gain while the app runs; running with evo quit is a non-goal. |
| Sum `usage_events` per run in the shell for turn cost | Rows are written by a background thread with no run id, so totals race the `agent_settled` line; the engine already knows its run's usage. |
| Keep `engine_start` for windows beside `ensure` | Two ways to start an engine bring back two owners; the trim and the restart limiter could not see window-started engines. |
| A new `evo-store` table for open questions and bell entries | Questions die with their engine and the app; bell entries are a bounded list. A setting key is enough. |
| Let windows keep the Scheduler page's writes | Two writers of one JSON array lose updates (seen today); the shell must be the only writer. |

## 5. As-built notes

### Phases 1 and 2 (2026-11-12)

- **`engine_ensure` takes `restart`.** It is not in 3.3. Windows still
  restart engines (moving a chat, changing a draft's flags, a sandbox
  change, the restart banner), so there is one start path for both.
- **`engine://state` comes in Phase 1, not T9,** and carries `restore`.
  Windows wait for `ready` before sending prompts or a model pick.
- **Windows still hold engines the shell starts.** Until Phases 3, 5 and 6
  move trim, wakes and turn records, the main window takes over the runtime
  of an engine the shell started:
  - live, on `engine://state` with reason `mail` or `restart`;
  - when it connects, every running engine of a saved chat.

  The window starts, sets up and restarts nothing itself.
- **The shell sends a chat's tools again** when they change: the connector
  pool, the shell's own tool list, a chat's connector switches, or a parent
  set or removed. Windows send the switches before starting an engine.
- **When the restart limit is hit**, it is only logged; the notification
  comes with Phase 4.
- **Run in the app** (separate identifier, copy of the data). All passed:
  - a new chat got the default model and its session, browser and terminal
    tools from the shell;
  - two nested sessions were started by the hub through the supervisor,
    without session tools, and mail between them was delivered;
  - mail left waiting at quit was delivered when the app started again,
    before the window loaded, and the window then held all three engines.
- **To do in Phase 3: mail during a worktree Apply.** An engine stopped on
  purpose around Apply is started again by new mail, possibly in the middle
  of the Apply. The viewers/work record should hold a chat while its
  worktree is being applied.

### Phases 3 and 4 (2026-11-12)

What was built differs from 3.3 as follows.

- **New commands:** `engine_hold(chat, held)` keeps an engine stopped while
  its worktree is applied or discarded, so mail waits. `engine_questions(chat)`
  serves the open questions when a transcript is rebuilt.
- **`OpenQuestion`** carries `toolCallId` and `callId`. `engine://state`
  gains status `stopped` and reasons `idle` and `detached`. The client
  methods are `attachChat` and `detachChat`.
- **The shell's mesh subscription is at `summary`, not `state`.** An engine
  has one subscriber (its stdout), and a later `state` subscribe would lower
  the windows' level.
- **Notices live in `supervisor/notices.rs`,** not in `turns.rs`.
- **Trim waits 10 s (`IDLE_GRACE`)** after an engine was used or set up.
  This stands in for prompts the shell cannot see until Phases 5 and 8.
- **Questions end with their engine,** the shell tools' too: their call can
  no longer be answered.
- **The webview's own notices are removed:** "Reply ready", "A run failed",
  "Approval needed" and "The engine stopped". A focused window that shows
  the chat gets no toast for them, only its banner. "The engine could not
  start" stays, because it answers that window's own request.
- **Run in the app** (separate identifier, copy of the data, approval mode
  ask). All passed:
  - A nested session that no window showed asked to run a command. The
    shell kept the question, logged `shell notice: approval` and put
    "Approval needed · Writer" in the bell. Opening the session showed the
    question at once, and Allow finished the run.
  - A chat window was closed in the middle of a long reply. The engine kept
    running, the main window took it over, and the reply finished.
- **Known limits:**
  - A bell entry made before the main window's listeners exist (an engine
    hosted at app start) is not kept.
  - If the window that set an Apply hold closes mid-Apply, the hold lasts
    until the chat is opened again.

### Phases 5 and 6 (2026-11-12)

What was built differs from 3.3 and 3.5 as follows.

- **Wakes** (`supervisor/wakes.rs`).
  - The shell keeps the session node's wake list from its mesh subscription.
  - A gather starts again on a node change, `agent_settled`, a closed
    question, a cleared draft and the end of set-up.
  - A wake is hub mail to the chat itself (`Deliver::Prompt`).
  - Drafts are flagged per window (`engine_set_composing(chat|null,
    composing)`); a closed or reloaded window clears its flags.
  - A chat with no row (a draft) is never woken.
- **The `turn` field** is `{provider, model, input, output, cacheRead,
  cacheWrite, costUsd, childCostUsd, childTokens, preview?}`.
  - `preview` is at most 400 characters and absent for incognito.
  - Child usage comes from a ledger in the engine's mesh router: a child's
    usage is counted when its node settles or closes. Plugin children are
    counted too; side questions are not.
  - Children still running when an engine exits lose their unsettled usage.
- **Pricing in the test provider:** the `faux` provider now prices usage at
  the model's base rates. The default faux model has no price, so other
  tests still see zero.
- **Old engines:** windows record turns only for an engine whose `ready`
  lacks `turnTotals` (a stale sidecar). That record no longer includes
  background children.
- **Windows still take over engines the shell hosts,** for a scheduled
  task's first prompt (until Phase 8) and for that fallback record.
- **Run in the app** (separate identifier, copy of the data, no window on
  the chats). All passed:
  - A nested session ran three turns. Each was recorded with its cost,
    tokens, preview and unread.
  - Its idle parent was woken once per child turn: six turns in all, three
    prompts and three wakes.

### Phases 7 and 8 (2026-11-12)

What was built differs from 3.3 and 3.5 as follows.

- **Settings fan-out** (`supervisor/settings.rs`).
  - `settings_set` dispatches `approvalMode` and `sandbox`. The sandbox
    change waits 600 ms for a burst to end; it also sets the hub's session
    limit and sends `set_limits`.
  - A provider change is a `providers_changed` line or a window's successful
    provider command. A change made any other way does not reach the other
    engines.
  - Settings that arrive while an engine is starting or in a run are sent
    when it is set up or settles.
  - Windows hear `engine://reloaded`. `engine_reload_resources` is new.
  - `mesh_configure` is removed.
  - `ui-store.ts` keeps `autoStartSessions` and `chatConnectorsOff` in the
    app settings, imported once from local storage.
- **The Scheduler runner** (`agent_tools/scheduler/run.rs`).
  - It starts the task's engine itself (reason `schedule`) before posting the
    prompt as hub mail. The hub's own start would reopen the session as
    saved and skip the default-model set-up.
  - An engine that cannot start keeps its chat; its id is recorded with the
    error, and no prompt is sent.
  - `scheduler_save` takes the page's partial input.
- **Fixed during the run in the app: the clock.** The runner used
  `evo_store::now()`, which counts seconds, while every task time is in
  milliseconds. No task ever came due, and "Run now" recorded a time in
  seconds. The clock helper `now_ms` in `scheduler/mod.rs`, which had
  returned seconds since before this feature (so tasks the agent's tools made
  carried seconds), now returns milliseconds, and the runner uses it.
  - Task times stored in seconds are read as milliseconds (`in_milliseconds`
    on load).
  - `scheduler_save` stores a schedule in its normal form (a `once` time as
    milliseconds even when given as a date and time).
  - Tests: `task_times_are_milliseconds`,
    `task_times_written_in_seconds_are_read_as_milliseconds`,
    `a_page_schedule_is_stored_in_its_normal_form`.
- **Run in the app** (separate identifier, copy of the data, the person's
  own task switched off). Both passed:
  - A task missed 10 minutes earlier ran once at start (trigger `missed`):
    its chat was made, answered, and recorded, and the one-time task
    switched itself off.
  - A task saved to run 65 s later started 2 s after its time, was answered
    and recorded with its cost, and switched itself off.
- **Not run in the app:** the settings fan-out to an engine no window holds
  (unit tests only).

### Phase 9 (2026-11-12)

What was built differs from 3.3 as follows.

- **New commands and event.** `quit_dismissed`, `quit_pending` and
  `app_open_pending` let a rebuilt main window, which has no listeners yet,
  read the question or the chat to open when it loads. `app://working
  {chats}` keeps the sidebar's working and waiting marks current.
- **`show_main`** finds the window with `get_window`, because
  `get_webview_window` misses a window that holds browser tabs. It rebuilds
  `main` from its `tauri.conf.json` entry, so `additionalBrowserArgs` are
  kept.
- **What counts as working.** Mail for a chat with no engine does not
  count. Background-agent reports do, so a chat stopped at the
  five-wakes-in-a-row limit stays "working" until the person's next prompt.
- **Not done:** on macOS the app menu's own Quit (`mac-menu.tsx`) still
  quits without asking.
- **Run in the app** (separate identifier, copy of the data). Passed:
  - A second launch showed a hidden main window and focused it.
  - With the tray off, closing the main window during a run asked "1 chat
    is working. Quit anyway?". Cancel kept the run going; "Quit anyway"
    exited and left no engine running.
  - With the tray off and a chat window open, closing `main` destroyed it.
    A second launch rebuilt it with its sidebar, and its session store
    showed the chat working in the other window.
- **Not run in the app:** the tray's menu lines and its Quit, because CDP
  cannot click the tray. Unit tests cover them.
- **Check note:** one full `test:shell` run had 5 failures (4 in
  `agent_tools::app::sessions`, 1 in `chat_flags`). They pass on their own,
  and three later full runs passed (610/610), so they look like tests that
  fail when the machine is busy.

### Phase 10 (2026-11-12)

- **T35, every window closed or hidden** (tray on). A parent started a
  background agent and a nested session, and the main window was closed. The
  agent's report and the session's note reached the parent inside its
  running turn, the session's turn and the parent's were recorded, and a
  scheduled task came due and ran.
- **T36, three windows open.** Each turn was recorded once and each child
  turn woke the parent once. Reloading the main window ten times across a
  scheduled start sent the task's prompt once.
- **T37, review.** See the NFR-2 row in tasks.md.
- **T38, unused code removed.** The commands `mesh_pending_chats` and
  `mesh_hosted` are gone; the supervisor hosts and flushes mail itself. A
  stale comment in `window.rs` now says that closing a chat window detaches.
