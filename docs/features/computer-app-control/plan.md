# Computer app control in the background — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-10 |
| ADD sections | [4. Process view](../../architecture/README.md#4-process-view), [7. Data view](../../architecture/README.md#7-data-view), [9. Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts), [12. Risks](../../architecture/README.md#12-risks-and-technical-debt) |
| ADRs | [0015 The shell drives other desktop apps in the background only](../../architecture/decisions/0015-agent-drives-desktop-apps-in-the-background-only.md), [0013](../../architecture/decisions/0013-app-side-agent-tools-run-in-the-rust-shell.md), [0014](../../architecture/decisions/0014-agent-tools-by-layer.md), [0010](../../architecture/decisions/0010-engine-enforces-approval-mode.md) |

## 1. Context and constraints

**What exists on `main`:**

- **Shell tools.** App-side tools run in the shell. Each has a `Tool` entry in
  the registry, a `Class` and one approval gate
  (`apps/desktop/src-tauri/src/agent_tools/mod.rs`, `gate.rs`, ADR 0013).
- **The browser stage.** The agent's browser renders a 1280×800 stage parked
  off screen and streams JPEG frames (`browser://frame`) and pointer events
  (`browser://pointer`) to floating cards (`apps/desktop/src-tauri/src/browser.rs`,
  `browser/stage.rs`; `apps/desktop/src/features/cards/`,
  `stores/cards-store.ts`, `stores/agent-events.ts`).
- **The cards.** They already have the virtual cursor, chips, open-in-panel
  and per-chat ownership ([floating cards](../floating-cards/spec.md)).
- **No code from v3 is on `main`.**

**What exists in v3 (tag `v3.1.1`):** the Rust backend
`desktop/src-tauri/src/computer_app/`, about 13k lines for Windows and macOS.
It was driven by a Python tool over a WebSocket. Its window listing, UI
Automation tree, refs, patterns, posted input, typing, keys, popups, parking
journal, policy, interrupts and live tests port almost unchanged. The Python
and WebSocket layers do not.

**Research and spikes:** [research.md](research.md). The findings this plan
relies on:

| # | Finding | What this plan does with it |
| --- | --- | --- |
| R1 | Plain Chromium stops painting fully off screen | an already-open Chromium-based window keeps one pixel topmost on screen |
| R2 | One pixel works only if the window *moves* there with topmost set in the same call | position and z-order are set in one `SetWindowPos`; re-asserted with a 1 px nudge |
| R3 | Electron apps keep painting off screen | the corner pixel is not used for them |
| R4 | The DevTools pipe works for Edge and Electron, with input and fresh screenshots while hidden | it is the page channel |
| R5 | Launching Chromium takes the foreground | apps start minimized without activation |
| R6 | v3: posted input fails in WPF, GTK, Tk and VCL, and on hover-tracking buttons | refs first, read-back, honest limits |

**Platform constraints:**

- Windows input isolation is per session (ADR 0015).
- `PrintWindow` reads native windows off screen. Windows.Graphics.Capture gave
  stale frames off screen in v3.
- UIPI blocks posting to elevated processes.
- On macOS, a window cannot leave the displays, and menu commands need an
  active app (spec FR-29).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | The shell runs the tool's native actions (ADR 0013). The agent's decisions stay in the engine's loop and the bundled skill. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | All native code is in `apps/desktop/src-tauri`. `evo-store` gets one table (3.4). No library crate learns about apps. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | n/a | The tool is an external tool over the existing `configure_external_tools` and `external_tool_request` lines. |
| Models come from models.dev; no hardcoded model facts | ok | Whether a model takes images is read from models.dev to decide whether `screenshot` and `marks` return pictures. No model ID appears in code. |
| Real engine in the app; settings reload live | ok | Changing a setting re-sends the tool definitions to every engine (3.8). |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | deviation, covered by [ADR 0015](../../architecture/decisions/0015-agent-drives-desktop-apps-in-the-background-only.md) | A new trust boundary (other programs), and a gate class that asks in every mode under "Ask every time". All app output is marked untrusted (FR-25). |
| Desktop UI structure and `DESIGN.md` | ok | New UI is a card body, card controls and one settings section. Components use store selectors and actions, and only `engine-store.ts` talks to the engine. |

## 3. Design

### 3.1 Overview

```text
 engine (chat) ──external_tool_request computer_app──► shell: agent_tools/computer
                                                          │  policy · gate (Class::Control) · batch runner
                                                          │  per-chat worker thread (owns refs)
                                                          ▼
                                       ComputerBackend (trait) ── WindowsBackend │ MacBackend (phase 5)
                                          │            │             │
                     channel 1: page ◄────┘   channel 2: UIA/MSAA   channel 3: posted messages
                     (DevTools pipe, apps                (refs, patterns, tree)
                      evo started)
                                          │
              stage: window off screen │ 1 px topmost corner (R1, R2) │ journal in evo.db
                                          │
       capture (PrintWindow │ page screenshot) ──computer://frame──► cards (kind "app")
       pointer targets ──────────────────────computer://pointer──► AgentCursor
       person's input in the card ◄──computer_input── app canvas
```

One `computer_app` call runs as an ordered batch on the chat's own worker
thread:

1. Each acting action passes the gate.
2. The action is aimed: a ref resolves to an element, a point to the element
   under it.
3. It is delivered through the first channel that applies.
4. The result is read back.
5. If no change is seen, the next channel is tried once.

The window stays off screen for the whole attachment. Pictures stream to the
card while one shows it. Release puts the window back.

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| desktop shell | `apps/desktop/src-tauri/src/agent_tools/computer/mod.rs` | `computer_app` tool: schema, batch runner, skip-after-failure, results with channel and read-back, untrusted marking, timeouts | FR-2, FR-15, FR-17, FR-18, FR-19, FR-25, FR-26 |
| desktop shell | `…/computer/{action,keys,policy,interrupt,workers,apps}.rs` | ported from v3: closed action enum, key parsing and blocked keys, protected processes and allow/block lists, Stop tickets, per-chat workers, app catalog search | FR-3, FR-4, FR-12, FR-22, FR-23, FR-26 |
| desktop shell | `…/computer/backend.rs` | `ComputerBackend` trait and the platform factory | FR-2, NFR-1 |
| desktop shell | `…/computer/marks.rs` | set-of-marks: numbered boxes for visible refs drawn on the screenshot | FR-14 |
| desktop shell | `…/computer/readback.rs` | before/after tree diff for focus, value, selection and new windows; `type` confirmation | FR-17, FR-18 |
| desktop shell | `…/computer/zoom.rs` | crop ×2 and map coordinates back | FR-20 |
| desktop shell | `…/computer/win/stage.rs` | from v3 `parking.rs`: off-screen move, stage size, 1 px topmost corner for Chromium windows without a page channel (one `SetWindowPos`, re-asserted per action and frame), journal, restore, popup follow hook | FR-6, FR-7, FR-8, FR-9 |
| desktop shell | `…/computer/win/page.rs` | page channel: start a Chromium or Electron app with `--remote-debugging-pipe` and `--disable-features=CalculateNativeWinOcclusion`, minimized without activation; DevTools input and screenshots; closed with the app | FR-16, FR-23, NFR-6 |
| desktop shell | `…/computer/win/{uia_tree,uia_patterns,uia_input,page_tree,catalog,listing,registry,target,lifecycle,popups,typing,keyboard,messages,text_surface,pointer,web_fill,capture}.rs` | ported from v3: channels 2 and 3, capture with `PrintWindow`, app lifecycle | FR-2, FR-9, FR-10, FR-14, FR-15, FR-21, FR-23 |
| desktop shell | `…/computer/mac/*` (phase 5) | ported from v3 without temporary activation; capture through ScreenCaptureKit | FR-28, FR-29, NFR-1 |
| desktop shell | `apps/desktop/src-tauri/src/computer.rs` | Tauri commands and events for the UI (3.3); start-up journal restore; release on quit | FR-1, FR-8, FR-10, FR-12, FR-13, FR-28 |
| desktop shell | `apps/desktop/src-tauri/src/browser/stage.rs` | `Streams` keyed by surface id so app frames share the throttle | FR-10, NFR-8 |
| desktop shell | `apps/desktop/src-tauri/src/agent_tools/gate.rs` | `Class::Control`: asks in every mode while `computer_apps.permission = "ask"`, else follows the mode like `Command`; refusals remembered per chat and action until the run ends | FR-24 |
| desktop shell | `apps/desktop/src-tauri/src/agent_tools/{mod.rs,activity.rs}` | register the tool only while enabled; `computer_` activity kind | FR-1, FR-11 |
| desktop shell | `apps/desktop/src-tauri/src/engine.rs` | on a chat's `agent_end`, release its app | FR-7 |
| crate | `crates/evo-store/src/migrations/0010_computer_parked.sql`, `crates/evo-store/src/computer.rs` | the journal table | FR-8 |
| engine | `crates/evo-engine/bundled-skills/computer-app-control/` | the skill, ported and rewritten for the channel order and marks | FR-27 |
| engine | `crates/evo-engine/src/` (skill listing) | a bundled skill whose front matter names `requires-tools` is offered only when the chat has those tools | FR-27 |
| desktop UI | `apps/desktop/src/stores/computer-store.ts` | per chat: attached app, title, icon, state (`active` · `stopped`), web flag | FR-10, FR-12 |
| desktop UI | `apps/desktop/src/stores/{cards-store,cards-sync,agent-events}.ts` | `CardKind` `"app"`, keyed `app:<chat>`; ensure on attach, remove on release | FR-10 |
| desktop UI | `apps/desktop/src/features/cards/{card-body,card,stage-controller,stage-stream}.tsx/ts`, `app-canvas.tsx` (new) | app card body: frames, `AgentCursor`, the person's input → `computer_input`; **Stop**, **Allow again**, **Show the app** and **Close** in the card header | FR-10, FR-11, FR-12, FR-13 |
| desktop UI | `apps/desktop/src/features/settings/sections/computer-apps-settings.tsx`, `settings-registry.ts` | **Settings → Computer apps**: switch, approval, app picker, macOS permissions | FR-1, FR-28 |
| desktop UI | `apps/desktop/src/components/ai-elements/{tool-kinds.ts,tool.tsx}` | transcript row kind `computer` with one line per action | FR-2 |
| docs | `apps/desktop/README.md` | UI map: the app card, the settings section | — |

### 3.3 Interfaces

**Agent tool `computer_app`** (the model sees this schema):

```json
{
  "name": "computer_app",
  "inputSchema": {
    "type": "object",
    "required": ["actions"],
    "properties": {
      "actions": {
        "type": "array", "minItems": 1, "maxItems": 50,
        "items": {
          "type": "object", "required": ["action"],
          "properties": {
            "action": { "enum": ["list_windows","attach","detach","status","screenshot","snapshot","find","zoom",
                                  "click","hover","scroll","drag","type","key","invoke","set_value","wait",
                                  "search_apps","open_app","close_app","kill_app"] },
            "window_id": { "type": "integer" }, "app": { "type": "string" }, "title": { "type": "string" },
            "query": { "type": "string" }, "limit": { "type": "integer" },
            "ref": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" },
            "to_x": { "type": "number" }, "to_y": { "type": "number" },
            "region": { "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4 },
            "marks": { "type": "boolean" },
            "button": { "enum": ["left","right","middle"] }, "clicks": { "type": "integer" },
            "modifiers": { "type": "array", "items": { "enum": ["shift","ctrl","alt","cmd"] } },
            "direction": { "enum": ["up","down","left","right"] }, "amount": { "type": "integer" },
            "text": { "type": "string" }, "key": { "type": "string" }, "repeat": { "type": "integer" },
            "value": { "type": "string" }, "direct": { "type": "boolean" },
            "seconds": { "type": "number" }, "max_depth": { "type": "integer" }, "max_elements": { "type": "integer" }
          }
        }
      }
    }
  }
}
```

Result shape:

- Text holds one line per action, as in v3 (for example `Clicked e12 "Save"
  → Button in "Report" (via UI Automation: invoke) · changed: focus →
  Edit "Name"`).
- Every line ends with `via <channel>` and, for acting actions, `changed: …`
  or `no visible change`.
- An image part is added for `screenshot` and `zoom`.
- `structuredContent` is `{ actions: [{ action, ok, channel, changed, note }] }`
  for the transcript row.
- The whole result is wrapped as untrusted app content (FR-25).
- `open_app` with `"page": true` (the default for web-based apps) starts the
  app with the page channel. Restarting an already-open app is never done by
  the tool: the agent asks with `ask_user` first and then calls `close_app`
  and `open_app` (FR-16).

**Tauri events** (shell → owning window):

| Event | Payload | Requirements |
| --- | --- | --- |
| `computer://attached` | `{ chat, surface: "app:<chat>", app, title, icon, web: bool, page: bool }` | FR-10 |
| `computer://released` | `{ chat, surface, reason: "detach"·"stop"·"reveal"·"close"·"turn"·"gone"·"quit" }` | FR-7, FR-10 |
| `computer://state` | `{ chat, state: "active"·"stopped" }` | FR-12 |
| `computer://frame` | `{ surface, seq, data (base64 JPEG), width, height, late: bool }`, the same shape as `browser://frame` plus `late` | FR-10, NFR-3 |
| `computer://pointer` | `{ surface, chat, kind: "move"·"down"·"up"·"wheel", x, y }`, the same shape as `browser://pointer` | FR-11 |

**Tauri commands** (UI → shell):

`computer_stage(surface, on, fps)`, `computer_input(chat, event)` (the event
uses the same mouse, wheel and key shapes as `browser_cdp` input),
`computer_stop(chat)`, `computer_resume(chat)`, `computer_reveal(chat)`,
`computer_close(chat)`, `computer_list_apps(query)`, `computer_permissions()`,
`computer_request_permission(kind)` and `computer_restart()`.

None of these exists for the agent. Stop, reveal, close and the permission
commands act only on the person's clicks (FR-12, FR-28).

**Older engines:** none are affected. The tool is offered through
`configure_external_tools`, which every current engine supports.

### 3.4 Data and storage

- **`evo.db` table `computer_parked`** (migration `0010_computer_parked.sql`):
  - columns `hwnd INTEGER`, `pid INTEGER`, `process_started INTEGER`,
    `placement TEXT` (JSON: rect, show state, style bits, topmost),
    `chat TEXT`, `parked_at TEXT`;
  - a row is written before the window moves and removed after it is back;
  - the shell reads the table at start-up and restores each row whose window
    is still the same process and still off screen (FR-8).
  - On macOS the key is the window number and pid.
- **Settings** (`evo_store::settings`, 3.8).
- **In memory, per chat, in the worker:**
  - the attached window, its refs (COM / `AXUIElement` objects, never shared
    across threads) and its page channel handle;
  - the Stop ticket, and refusals for the run.

  All of it is dropped on release.

### 3.5 Behaviour

```mermaid
sequenceDiagram
  participant E as engine (chat)
  participant S as shell computer tool
  participant B as backend (worker)
  participant A as app window
  participant U as card (UI)
  E->>S: external_tool_request computer_app [attach 44, snapshot]
  S->>S: policy check · gate (Class::Control)
  S->>B: attach
  B->>A: journal row · move off screen (or 1 px topmost corner)
  S-->>U: computer://attached → card
  U->>S: computer_stage(app:chat, on, 10)
  B-->>U: computer://frame (PrintWindow or page screenshot)
  B->>A: snapshot (UIA / AX / DOM via page channel)
  S-->>E: result (tree, refs, untrusted)
  E->>S: [click e12]
  S->>S: gate
  S-->>U: computer://pointer move (≈200 ms before)
  B->>A: channel 1 → 2 → 3 (first that applies)
  B->>B: read back; no change? next channel once
  S-->>E: "Clicked e12 … via UIA invoke · changed: dialog 'Save as'"
  E-->>S: agent_end
  S->>B: release → window back · journal row removed
  S-->>U: computer://released (turn)
```

**Channel choice per action:**

1. The page channel when the app was started with one.
2. For a ref, the element's pattern (Invoke, Value, Toggle, SelectionItem,
   ExpandCollapse, Scroll) or MSAA default action.
3. Posted messages for known-good window classes, and v3's surface paths
   (text caret, grid shift+click).
4. Coordinates only for a point with no element; the result is flagged.

Channel 2 skips any action for which it has no pattern.

**Limits.** An action whose only remaining channel is known to be dropped (v3's
toolkit list, hover-only targets, OLE drag, raw input) is not posted
blindly. It returns `cannot be done in the background: <reason>` (FR-19).

**Picture freshness (NFR-3):**

- Native windows use `PrintWindow`.
- Page-channel apps use DevTools screenshots or a screencast.
- An already-open Chromium window without a page channel sits at the 1 px
  topmost corner. After each acting action, two frames 150 ms apart are
  compared to the frame before the action. When none differs and a change
  was expected, the frame carries `late: true`, and the picture in the result
  says "may be late".

**Corner pixel (R2):**

- Placement sets the position and `HWND_TOPMOST` in one `SetWindowPos` with
  `SWP_NOACTIVATE`.
- Before each action and frame, the shell checks the rectangle and the
  topmost bit. If either has drifted, it nudges the window by 1 px and back.
- Release clears topmost before restoring the placement.

**Starting apps (R5, FR-23):**

- Native apps start through `ShellExecuteExW` with `SW_SHOWMINNOACTIVE`, as
  in v3.
- Web-based apps start through `CreateProcessW` with `STARTUPINFO.wShowWindow
  = SW_SHOWMINNOACTIVE` and the two pipe handles.
- The first window is restored with `SW_SHOWNOACTIVATE`, then staged.
- The person's foreground is checked before and after. If it changed, the
  shell does not take it back (that would be taking input); the result
  carries a note, and the launch test fails (T-level check).

**Concurrency.**

- One worker thread per chat runs that chat's actions in order.
- Capture runs on a separate blocking task. It waits only for the typing gate
  (from v3: a capture during a cell editor opening dropped characters).
- Stop invalidates the chat's ticket. Long actions check it between
  characters, drag steps, wheel notches and before the cursor approach.

**Turn end.** `engine.rs` already reads `agent_end`. It calls
`computer::release_chat(chat, "turn")`. `attach` refusals after **Close**
last until that point.

### 3.6 Errors and edge cases

| Case | Behaviour |
| --- | --- |
| App hangs on an action | The action times out (60 s + 20 ms per character), is interrupted where it got to, and only that chat waits; frames keep flowing (FR-26, NFR-5). |
| Window closes or the process exits | `gone` release; the card closes; the agent is told; the journal row is removed. |
| Person brings the window back from the taskbar | The next action re-stages it; only **Show the app** hands it back (spec edge case). |
| evo is killed | Rows stay in `computer_parked`; the next start restores them (FR-8). |
| evo quits with a hung app | Restores run on their own threads; quitting waits at most 3 s, and the rest stay journalled (NFR-5). |
| The page channel is refused (the app ignores the flags, or Electron fuses block them) | The app is driven through channel 2, it gets the corner pixel if it is Chromium, and the result says `page channel unavailable`. |
| A WebView2 host | No page channel: the browser process is not evo's child, so a pipe cannot be inherited, and a port would break NFR-6. It is driven through channel 2, and WebView2 keeps painting off screen (v3). |
| Elevated, UWP CoreWindow or protected app | Refused at `list_windows` and `attach` (FR-4). |
| Model without image input | `screenshot`, `zoom` and `marks` return a text note; the tree still works. |
| Password field | `type` and `set_value` are refused with the reason (FR-21). |
| Ref from a closed dialog or a replaced element | Refused with the reason (FR-14). |
| macOS permission missing | `list_windows` reports it; actions fail with which permission to grant (FR-28). |

### 3.7 Security and trust

- The tool is absent unless the person enabled it. Every acting call passes
  `Class::Control`, and the prompt lists each action as it will run
  (FR-24).
- Protected processes, elevated processes and evo itself are refused at
  every entry point (`list_windows`, `attach`, `open_app`, `close_app`,
  `kill_app`).
- `open_app` starts only catalog entries; paths and command lines from the
  agent are refused (FR-23).
- The page channel is a pipe inherited by the app evo started, never a
  network port (NFR-6). The agent never gets raw DevTools access. Only the
  tool's actions use it: no script evaluation, cookies, storage or network.
- Results are untrusted app content (FR-25). The skill repeats that app text
  is data.
- No clipboard API is called (FR-21).
- A unit test fails if the computer module references `SendInput`,
  `SetCursorPos`, `SetForegroundWindow`, `BlockInput`, `keybd_event` or
  `mouse_event` (ADR 0015).
- Stop is enforced in the shell. The agent cannot call `computer_resume`.

### 3.8 Configuration

| Key (`evo.db` settings) | Type | Default | Edited in |
| --- | --- | --- | --- |
| `computer_apps.enabled` | bool | `false` | **Settings → Computer apps** |
| `computer_apps.permission` | `"ask"` · `"allow"` | `"ask"` | same |
| `computer_apps.allowed` | JSON array of executable names | `[]` (any) | same, app picker |
| `computer_apps.blocked` | JSON array of executable names | `[]` | same, app picker |

Changes apply live:

- The gate and the policy read the settings on every call.
- Turning the feature on or off re-sends `configure_external_tools` to every
  running engine, so the tool appears or disappears on the next call.
- Turning it off also releases every attached app.

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Virtual monitor, brief real-input take-over, transparent window | rejected by the owner (spec 6.2, ADR 0015) |
| Windows.Graphics.Capture for native windows | stale frames off screen (v3 measurement); `PrintWindow` stays |
| Always-on corner pixel for every app | not needed for native or Electron apps (R3), and more of the window would sit at the person's screen corner |
| DevTools over a loopback port | reachable by other local users; the pipe works (R4). WebView2 hosts go without a page channel instead |
| Keep the v3 Python tool and WebSocket bridge | it was the source of timeouts and reconnect failures; app-side tools run in the shell (ADR 0013) |
| A grounding model for coordinates now | refs, marks, zoom and read-back come first. A grounding model is reconsidered after the benchmark (SC-1) shows where accuracy is still lost |

## 5. As-built notes

Phase 1 (2026-11-10):

- **Port.**
  - The v3 core and Windows backend are under
    `apps/desktop/src-tauri/src/agent_tools/computer/`.
  - `windows` is raised from 0.58 to 0.62, the version `webview2-com`
    already uses; `src/apps.rs` needed two signature fixes for it.
  - `image` and `walkdir` are added.
  - App icons in the catalog come from `src/apps.rs::shell_icon`, not from
    `file_icon_provider`.
- **The macOS backend** is not ported yet (phase 7). On macOS the factory
  returns the unsupported backend.
- **The journal** is `evo.db`'s `computer_parked` (migration 0010), reached
  through a `Journal` trait. The app uses `src/computer.rs::DbJournal`; tests
  use a `MemoryJournal`.
  - Its columns are `window`, `pid`, `placement_json`, `chat_id` and
    `parked_at`. The planned `process_started` column was not needed: the
    window handle plus the pid identify the window, as in v3.
  - `placement_json` also carries `cornered`, so a crashed run's corner
    window has its topmost bit cleared when it is put back.
  - Shell start-up restores the journal; `RunEvent::Exit` releases every
    window.
- **v3's transparent peek is removed** (`win/peek.rs`, its live tests
  `drags_in_a_covered_page` and `a_layered_window_keeps_its_opacity_after_a_peek`).
  The corner replaces it:
  - `needs_corner` covers every Chromium top-level window
    (`Chrome_WidgetWin*`, Electron included: harmless, one pixel);
  - `corner_place` picks a monitor corner, top corners first, where exactly
    one pixel shows;
  - `move_to_corner` sets the position and `HWND_TOPMOST` in one
    `SetWindowPos`;
  - `keep_out_of_view` re-asserts the corner on every action;
  - `is_off_screen` now means "shows at most one pixel".
- **Attach order.** A Chromium window is parked into its corner before its
  page tree is awaited, so a window that opened completely covered gets its
  render host there. `wake_page` re-moves a cornered window instead of making
  it transparent.
- **`attach` hides by default** (`hide` defaults to true; v3's default came
  from a setting). Live tests that check the shown mode pass `"hide": false`.
- **Showing the app to the person.** `reveal` no longer calls
  `SetForegroundWindow`: it returns the `hwnd`, and the person's own click
  (phase 4, `src/computer.rs`) brings it forward. The T1 test now forbids the
  call anywhere in the module.
- **Known gap.** VS Code 1.145 (Electron 43) ignores posted shortcuts
  (`drives_vscode_by_its_keys`); see tasks Phase 1 results. To be covered by
  the page channel (T24).

Phase 2 (2026-11-10):

- **The tool.** `agent_tools/computer/tool.rs` (`computer_app`) is a port of
  v3's Python tool, now running in the shell:
  - parsing, with typed text composed to NFC through `unicode-normalization`;
  - one approval line per action;
  - skipping the rest of a batch after a failure (a `detach` still runs);
  - timeouts of 60 s plus 20 ms per character, interrupting the worker;
  - the allow and block lists, checked before attach, open, close and kill,
    and again on the first reading or acting step of each call;
  - the result text, ending with the untrusted-content note unless the call
    only waited.

  `zoom` and `marks` are not in the schema yet: they come with T22 and T23.
- **The gate.** It is `gate::check_control`, apart from `check`, because its
  rule depends on the feature setting (`ToolCtx::gate_control`).
  - Refused actions are kept per chat and forgotten on the chat's
    `agent_end`, with its release.
  - The prompt's options are "Allow", "Allow for this chat" and "Deny". The
    chat grant is the same in-memory grant the other shell tools use.
- **Release (FR-7).** `agent_tools::on_line` releases a chat's app on
  `agent_end`, and `EngineState::stop` does it when a chat's engine stops
  (the chat closed).
- **Settings (FR-1).** `settings_set` calls `computer::settings_changed` for
  the `computer_apps.*` keys. On `computer_apps.enabled` it emits
  `agent-tools://changed`; `stores/engine-store.ts` then re-sends the tools
  to every ready chat, and switching the feature off also releases every app.
  `agent_tools_definitions` offers `computer_app` only while the feature is
  on, and only on Windows.
- **The pointer event** is `computer://pointer` with
  `{chat, surface: "app:<chat>", x, y, phase}`. `x` and `y` are fractions of
  the window, as v3's card used them, not pixels as the browser's
  `browser://pointer` has. The card (T29) maps them.

Phase 3 (2026-11-10):

- **Read-back** (`readback.rs`, `win/uia_tree.rs::probe_state`).
  - Every acting action except `hover` is wrapped by two probes of the
    window: its title, dialog, popups, and up to 250 tree lines without refs
    or positions, with `[focused]` marked. A probe never hands out or retires
    refs.
  - The diff goes into the result as `changed` and as a `changed: …` line.
  - Cost: two tree walks and a 250 ms settle per acting action.
- **The ladder** is in the tool (`tool.rs::next_channel`, `limit_line`).
  - A plain left click by ref with no visible change is tried once more on
    the other channel: an accessibility press gets a posted mouse click (the
    backend's internal `"channel": "mouse"`), and a mouse click gets `invoke`.
  - Typing, keys and values are never retried.
  - Any acting action with no visible change adds a line that the step may
    not have reached the app and must not be reported as done.
- **Marks and zoom.**
  - `screenshot` with `marks: true` walks the tree as a snapshot does (fresh
    refs). It boxes and numbers controls of interactive roles, up to 150,
    using a built-in 3×5 digit font, and returns the marked lines with the
    picture.
  - `zoom` crops at most 640×400 of the screenshot and enlarges it ×2.
    Pointer actions with `in_zoom: true` are mapped back in the tool
    (`zoom.rs`, per chat, cleared by the next screenshot).
- **The page channel** (`win/page.rs`).
  - Chromium takes the pipes as inherited Windows handles through
    `--remote-debugging-pipe --remote-debugging-io-pipes=<read>,<write>`,
    verified with Edge 157 and VS Code 1.145 / Electron 43.
  - `CreateProcessW` passes only those two handles
    (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`) and starts the app with
    `SW_SHOWMINNOACTIVE`.
  - `open_app` uses this path when the catalog program, or its Start menu
    shortcut's target, is a Chromium browser or an Electron app (`resources\app.asar`).
  - Clicks go to such an app as DevTools mouse events. They are mapped from
    screen to CSS pixels by the client origin, the page's
    `cssLayoutViewport` and any toolbar height. An accessibility press left
    Edge's focus on `BODY`.
  - Keys go as `Input.dispatchKeyEvent` with DOM key, code and virtual-key
    codes; text goes as `Input.insertText`, with line breaks as Shift+Enter.
  - The page target is chosen by the window's title.
  - **Focus emulation** (`Emulation.setFocusEmulationEnabled`) is switched
    on when the page is attached: an app started without activation never
    has page focus, and Chromium drops input to it. It changes nothing
    outside the page.
- **Starting an app keeps the front window where it is.**
  - VS Code (Electron) ignored the start-up show state and took the person's
    foreground even while they were active.
  - `start_with_channel` now calls `LockSetForegroundWindow(LSFW_LOCK)`
    before the start and unlocks once the app's window has shown (or after
    20 s); the person pressing Alt or clicking lifts it too. The live test
    then kept the person's front window.
  - This call keeps the foreground; it never takes it, so ADR 0015 holds and
    T1 does not forbid it.
  - `open_app` also reports `took_foreground` when an app took the front
    anyway; evo never takes it back.
- **Late pictures (NFR-3).** For web content without a page channel, the
  window's picture is fingerprinted before and after each acting action. A
  tree change with an unchanged picture sets `picture_may_be_late`, and
  screenshots carry "this picture may be late" until a later action shows a
  fresh picture.

Phase 4 (2026-11-10):

- **Frames.** An app's pictures are captured, not pushed, so they do not go
  through the browser's `Streams` as the plan had it. `computer_stage`
  (`src/computer.rs`) runs one task per chat:
  - it captures `preview_frame` (1280 px wide at most) at the asked rate
    (8 per second from `StageController`, 15 at most);
  - it skips a picture whose bytes did not change;
  - it emits `computer://frame {surface, chat, seq, data, width, height, title}`;
  - it stops when the card is not shown (NFR-8), and when the app is no
    longer attached: then it emits `released` with reason `gone`, unless
    control was stopped.
- **Events.** The module emits the following through an `AppHandle` set at
  start-up:
  - `computer://attached` from the tool after a successful attach;
  - `computer://released` on detach, turn end or engine stop (`turn`),
    Close or Show the app (`close`), a window that went away (`gone`),
    resume without an app (`resume`), and evo quitting or the feature off
    (`chat: null`);
  - `computer://state` on Stop and Allow again.
- **Close and Show the app.** Both refuse `attach` until the turn ends
  (`computer::mark_closed`, cleared by `release_chat`). Show the app first
  restores the window (the backend's `reveal`), then hands it back, then
  brings it to the front from `src/computer.rs` on the person's click. The
  `SetForegroundWindow` call lives outside `agent_tools/computer`, so T1
  still holds.
- **The person's input** (`computer_input`) goes through the same backend
  actions as the agent's, without approval:
  - clicks and wheel carry fractions of the picture (`"fraction": true`,
    mapped in `pointer_target`);
  - keys use the key parser's names (`features/cards/app-input.ts`);
  - printable characters go as text.
- **The pointer** reaches the card as fractions. `AgentCursor` scales them by
  the picture's size (`Pointer.fraction`).
- **Reload.** App cards come back from the session storage. `restoreAppCards`
  asks `computer_status` for each, and drops a card whose chat has no app
  attached or stopped.
- **Transcript.** Kind `computer` uses the `AppWindowIcon` and a one-line
  label such as "Used an app · click e12, key ctrl+s", and shows screenshots.
  The tool sends no `structuredContent`: it showed as raw JSON under each
  step.
- **Settings → Computer apps** (`sections/computer-apps-settings.tsx`):
  - the switch, Approval, and an app list from `computer_list_apps` with
    Default / Allowed / Blocked per app;
  - a typed executable can be blocked;
  - on platforms other than Windows the page says the feature is not
    available there.

Phase 5 (2026-11-10):

- **`requires-tools`.** It is a new, optional front matter field of a skill
  (`evo_harness::types::Skill::requires_tools`): a YAML list or a
  comma-separated string. `AgentHarness::compose_prompt` filters the
  `<available_skills>` listing through `skills::skills_for_tools`, so a skill
  is listed only while all its tools are active. External tools such as
  `computer_app` are in the active list once `configure_external_tools` adds
  them, so switching the feature on or off changes the listing on the next
  turn.
- **The skill.** It is in `crates/evo-engine/bundled-skills/computer-app-control/`,
  ported from v3.
  - `SKILL.md`, `input-channels.md`, `actions.md` and
    `events-and-errors.md` are rewritten for v4: the page, web content and
    native channels; `marks`, `zoom` and `in_zoom`; the `changed:` read-back
    lines; the one-step ladder and "Do not report it as done"; the card's
    Stop, Allow again, Show the app and Close; Settings → Computer apps.
  - The surface guides are unchanged, apart from the page channel in
    `web-content.md`. They still mention macOS in places, for phase 7.

Phase 6 (2026-11-09, in progress):

- **Benchmark harness.** `computer/win/benchmark.rs` (test only) runs the
  live bench and live test cases as recorded cases, times every action
  through `benchmark::timed`, and samples the person's input every 10 ms
  (`InputMonitor`: pointer, front window, modifier keys,
  `GetLastInputInfo`). A change within 100 ms of the person's own input is
  theirs; the harness pauses the monitor while it starts or ends a process
  (`harness_pause`).
- **Only the harness's own windows.** `live_bench::start_private` takes a new
  process's window only when its title starts with the launch's `title`
  hint, and `end_private` ends the process only when it holds no other
  window. Word started for the bench can reopen a document the person was
  working on.
- **Apps activate their own windows (FR-30).** Driven by background input,
  Excel activates a new workbook and its dialogs, and WinForms activates a
  modal dialog; the front window then left the person's window. evo makes
  no foreground call for the agent (ADR 0015); instead `win/front.rs` holds
  `LockSetForegroundWindow(LSFW_LOCK)` while an action runs and for 1.5 s
  after. Holds are counted across chats and the page channel's start, so
  the lock lifts only when the last one ends. The lock blocks programs, not
  the person: their clicks and Alt+Tab still move the front window. A
  process that is not in front can take the lock (checked on Windows 11).
  Measured (smoke run 4): the lock did not stop Excel's "Create Table"
  dialog, opened by a UI Automation invoke, from taking the front, so FR-30
  is not met yet.
- **Latency (T46).** The by-ref medians of the first smoke runs (about
  1 – 2 s) missed NFR-4's 300 ms. Where the time went, per step
  (`EVO_COMPUTER_TRACE`): two tree walks, a fixed 250 ms wait, the card
  cursor's 220 ms travel and 150 ms after each click. Now:
  - walks read each element with one cached call (`walk_cache`), and a
    native window's tree is fetched whole in one call (`walk_cached`); web
    content stays element by element, as a page can be far larger than
    what a walk lists;
  - a pointer action reads its "before" during the cursor's travel
    (`before_input`, called by `Target::travel` and ahead of each input
    sent without one); an action that sent input without that read
    reports no `changed` rather than a wrong one;
  - after the input, the read waits for the app (`settle`) and 40 ms, and
    looks again 200 ms later only when nothing changed yet;
  - the travel is 160 ms, matching the card cursor's 150 ms glide.

  Excel still takes about 1 s an action, almost all of it handing out its
  tree twice; NFR-4 now counts evo's part only, and the whole time is
  reported per app. Reusing one action's "after" as the next "before"
  would halve it, at the risk of counting a late change of the app toward
  the next action; not done.
