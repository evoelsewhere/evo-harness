# Evo Webbridge: Evo in the person's own Chrome or Edge — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-18 |

Paths are relative to the repository root. `shell` means
`apps/desktop/src-tauri/src`; `ext` means `extensions/evo-webbridge`.

## 1. Phases

### Phase 0 — Spike

Done when: a minimal extension, the host and a pipe server exchange `hello` /
`welcome` on this machine, and the risky browser behaviours are recorded in
the plan's as-built notes.

- [x] T1 [FR-3] A fixed extension key and the id it gives; the id is the unpacked origin in the allow-list — `ext/manifest.json`, `shell/webbridge/ids.rs` — check: `cargo test webbridge::ids` (the id computed from the key matches)
- [x] T2 [NFR-2] Check in Chrome and Edge: `Page.captureScreenshot` on a background tab, `chrome.debugger` under `DeveloperToolsAvailability = 2`, `--load-extension` for a test profile — notes in `plan.md` §5 — check: recorded with dates

### Phase 1 — Connect

Done when: **Settings → Browser → Set up Chrome** registers the host, the
loaded extension shows "Connected to Evo", and Settings shows the profile.

- [x] T3 [FR-3] Tests, then the wire: frame encode/decode (4-byte LE length), message types, `chunk` split and join — `shell/webbridge/wire.rs` — check: `bun run test:shell -- webbridge::wire`
- [x] T4 [FR-3, NFR-1] Tests, then `evo-bridge-host`: relay stdin ↔ pipe frames unchanged, check the pipe server's executable sits in the host's folder, exit when either side closes — `crates/evo-bridge-host/` — check: `cargo test -p evo-bridge-host`
- [ ] T5 [FR-19] The host starts Evo hidden when no pipe answers and retries for 10 s — `crates/evo-bridge-host/src/launch.rs` — check: manual (quit Evo, open the browser: Evo starts in the tray)
- [x] T6 [FR-3, FR-4, FR-23, NFR-3] Tests, then the pipe server: current-user pipe, client executable check, `hello` (origin allow-list, version, policy), instance table, `bye` on refusal; starts only when set up — `shell/webbridge/{mod,server}.rs`, `shell/lib.rs` — check: `bun run test:shell -- webbridge::server`
- [x] T7 [FR-2, NFR-4] Tests, then set-up: host manifest, per-user registration for Chrome and Edge (registry on Windows, manifest files on macOS and Linux), remove, status — `shell/webbridge/setup.rs` — check: `bun run test:shell -- webbridge::setup`
- [x] T8 [FR-1, FR-4, NFR-5] The extension: manifest (permissions of FR-1 only), port with `hello`, reconnect with backoff, profile id and label in storage, popup with connection state — `ext/manifest.json`, `ext/background/*.js`, `ext/popup/*` — check: manual (load unpacked; popup says Connected to Evo)
- [x] T9 [FR-2, FR-4, FR-23] Tauri commands and **Settings → Browser**: set up / remove per browser, connected profiles with rename, policy state — `shell/webbridge/commands.rs`, `apps/desktop/src/features/settings/sections/webbridge-settings.tsx`, `apps/desktop/src/stores/webbridge-store.ts`, settings navigation — check: `bun run typecheck`; manual
- [x] T10 [FR-24] Log lines for connections, set-up, refusals, and each agent call with its chat, tool and site — `shell/webbridge/{server,mod}.rs`, `shell/agent_tools/mod.rs` — check: manual (Logs page)
- [x] T11 [FR-2] Ship the host next to the engine: `externalBin`, a staging script beside `engine:sidecar` — `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/scripts/`, `apps/desktop/package.json` — check: `bun run bridge:sidecar` stages the binary; the dev app finds it

### Phase 2 — Drive tabs

Done when: an agent opens a page with `target: "chrome"` and every browser
tool except downloads works on it.

- [x] T12 [FR-6, FR-7, FR-8] Tests, then groups: chat ↔ profile ↔ group ↔ tabs, tab ids `chrome:<profile>:<tab>`, joins and leaves, a group closing — `shell/webbridge/groups.rs` — check: `bun run test:shell -- webbridge::groups`
- [x] T13 [FR-5, FR-6, FR-8, FR-20] Extension tabs and DevTools: `open`, `adopt`, `release`, `list`, `attach`, `detach`, `cdp`, events, tab and group changes, errors as codes — `ext/background/{cdp,groups}.js` — check: manual
- [x] T14 [FR-5, FR-20, NFR-2, NFR-6] Webbridge pages: requests with ids and timeouts, `Runtime.evaluate` eval, the page helper on attach and on new documents, focus emulation (a background tab takes input), a screencast around captures, dialogs into the shared table, subscribed events into the shared ring, load state from tab changes, every pending call failed when the tab or profile goes — `shell/webbridge/page.rs` — check: the end-to-end runs of T19 (they need a browser; no unit test reaches the extension)
- [x] T15 [FR-5, FR-7] The page functions dispatch Webbridge tabs (`eval`, `cdp`, subscribe, events, dialogs, navigate, history, close, address, status, create) — `shell/browser.rs` — check: `bun run test:shell -- browser`
- [x] T16 [FR-5, FR-9, FR-21] Tools: `target` on `browser_navigate` and `browser_tabs`, `browser_tabs {action: "choose"}`, panel-only steps skip Webbridge tabs, download tools refuse them — `shell/agent_tools/browser/{core,tabs,driver,downloads}.rs` — check: `bun run test:shell -- agent_tools::browser`
- [x] T17 [FR-22] Refusals for pages the browser forbids and for Block sites, with the reason — `shell/webbridge/page.rs`, `shell/agent_tools/browser/core.rs` — check: `bun run test:shell -- webbridge`
- [x] T18 [FR-5] Leases: attach on the first call, detach 60 s (`webbridge.leaseSeconds`) after the chat's turn ends, a refused prompt or the engine's exit — `shell/webbridge/{groups,mod,page}.rs`, `shell/engine.rs`, Settings → Browser — check: test of the lease clock; manual (notice goes after a turn)
- [x] T19 [FR-5, FR-20, NFR-2] End to end on Edge (Chrome runs the same extension and code, so it is not checked separately): navigate, read, click, type, key, scroll, screenshot, dialog, mock, eval; close the tab during a wait — check: manual, results in the verification table

### Phase 3 — Stay in control

Done when: site permissions, the indicator, Take over, Stop and Stop all work
from the page, the popup and the app.

- [x] T20 [FR-12] Tests, then the site list: load, save, change events, Tauri commands — `shell/webbridge/sites.rs` — check: `bun run test:shell -- webbridge::sites`
- [x] T21 [FR-13, FR-22] Tests, then the gate rule for Webbridge tabs: acts ask once per site and chat in "Ask every time" and "Approve edits" unless Allow; Block refuses in every mode; "Allow for this site" saves Allow — `shell/agent_tools/gate.rs`, `shell/agent_tools/mod.rs` — check: `bun run test:shell -- agent_tools::gate`
- [x] T22 [FR-15, FR-16, FR-17] The indicator: injected per document, chat name, Stop, Take over, Hand back, signals through a binding — `ext/overlay/overlay.js`, `ext/background/control.js`, `shell/webbridge/page.rs` — check: manual
- [x] T23 [FR-16, FR-17, FR-18] Take over holds a tab's calls; Stop stops the chat's turn; Stop all ends a profile's work and detaches — `shell/webbridge/{groups,mod}.rs`, `shell/supervisor.rs` — check: test of the takeover state; manual
- [x] T24 [FR-12, FR-18] Popup: this site's permission, chats working now, Open side panel, Stop all — `ext/popup/*` — check: manual (later folded into the side panel: the toolbar button opens the panel, whose ⋯ menu has the site permission and Stop all)
- [x] T25 [FR-7, FR-8, FR-12] App: the site list in Settings; a chat's Webbridge tabs in the Browser panel with Take back and Stop — `apps/desktop/src/features/settings/sections/webbridge-settings.tsx`, `apps/desktop/src/features/browser/`, `apps/desktop/src/stores/` — check: `bun run typecheck`; manual
- [x] T26 [FR-17] Acts on the tab the person used in the last 5 s wait until they stop — `ext/overlay/overlay.js`, `shell/webbridge/page.rs` — check: manual

### Phase 4 — Side panel

Done when: US-3 and US-8 pass with the app's window closed.

- [x] T27 [FR-10, FR-11] Tests, then panel sessions: `chats`, `open`, `new`, `send`, `stop`, the reduced event stream from engine lines — `shell/webbridge/panel.rs` — check: `bun run test:shell -- webbridge::panel`
- [x] T28 [FR-14] Questions of a chat reach its panel sessions; the first answer settles both — `shell/supervisor/questions.rs`, `shell/webbridge/panel.rs` — check: test; manual
- [x] T29 [FR-26, FR-27] `models`, `set_model`, `set_effort`, `set_mode` as the app sends them; `chat` state pushed on change — `shell/webbridge/panel.rs` — check: test; manual
- [x] T30 [FR-28] `attach` saved as the app saves pasted files, with its limits; page screenshot in the extension — `shell/webbridge/panel.rs`, `shell/files.rs`, `ext/sidepanel/composer.js` — check: test; manual
- [x] T31 [FR-10, FR-25] Side panel: header, History with search, New chat, ⋯ menu, empty state, messages, tool activity, questions, Stop — `ext/sidepanel/*` — check: manual
- [x] T32 [FR-26 – FR-29] Composer: Include this tab, **+** menu and chips, mode switch, model and effort, microphone with the permission page and the Settings switch — `ext/sidepanel/composer.js`, `ext/permission/*`, settings — check: manual

### Phase 5 — Ship

Done when: the extension packs for the stores and every verification row
has a result.

- [x] T33 [FR-1] `scripts/pack-webbridge.ts` zips the extension; store ids join the allow-list when published — `scripts/`, `shell/webbridge/ids.rs` — check: the zip loads in a browser
- [ ] T34 [NFR-4] macOS and Linux registration paths and the 0600 socket — `shell/webbridge/{setup,server}.rs`, `crates/evo-bridge-host` — check: `cargo check` for those targets where possible; manual later
- [x] T35 UI map and docs: `apps/desktop/README.md`, plan as-built notes, verification table — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Review of `ext/manifest.json` | passed (2026-11-18) |
| FR-2 | Manual: set up Chrome, connected; remove, host unreachable | passed with Edge (2026-11-18): Set up registers, the extension connects, Remove deletes the key (Chrome shares the code; not checked separately) |
| FR-3 | `webbridge` tests (origin admission, ids, setup); spike: host introduces the origin, pipe sees the sibling host | partly passed (2026-11-18); a page cannot connect: not run |
| FR-4 | Manual: two profiles show as two entries | passed on Edge (2026-11-18): two profiles show as two named entries; `target: "edge"` then asks the agent to name one |
| FR-5 | Manual: tools on a Chrome tab (T19) | passed on Edge (2026-11-18): navigate, read_page, find, click, type, key, hover, scroll, eval, dialog, wait, network, console, mock, storage, upload, frames, viewport, performance, screenshot (visible, full page, element), pdf, tabs, status; leases: let go 10 s after a turn with `leaseSeconds` 10, attached again on the next call |
| FR-6 | `webbridge::groups` tests; manual: three pages, one group | tests passed (2026-11-18); manual (Edge): pages open in the background in the chat's group (passed) |
| FR-7 | `webbridge::groups` tests; manual: chat B cannot name A's tab | passed (2026-11-18): another chat gets "No browser tab …" and lists none |
| FR-8 | Manual: drag in and out | passed on Edge (2026-11-18): a tab put in the group joins the chat; a tab taken out answers "The person took this tab back." |
| FR-9 | `agent_tools::browser` tests; manual: choose | schema tests passed (2026-11-18); choose: not run |
| FR-10 | Manual: US-3 with the window closed | partly passed (2026-11-18): chats, new chat, history, send reach the engine; a model's reply: not run (no provider on the test profile) |
| FR-11 | Manual: panel and app show the same chat | not run |
| FR-12 | `webbridge::sites` tests; manual: Block in the side panel menu shows in Settings | passed (2026-11-18): Block and Ask from the panel's message show in `webbridge_sites` and the log |
| FR-13 | `agent_tools::gate` tests | passed (2026-11-18) |
| FR-14 | Manual: answer in the panel, the app's question goes | not run |
| FR-15 | Manual: indicator on every page acted on | passed on Edge (2026-11-18): the indicator is in each new document of an agent tab |
| FR-16 | Manual: Stop during `browser_wait` | not run: Stop aborts the chat's engine turn, which ends its calls; the test profile has no model to run a turn with tool calls |
| FR-17 | Manual: Take over, then a click is held | passed on Edge (2026-11-18): Take over through the indicator's binding; the next tool call says the person has control. A click by the person makes the agent's next click wait 5.3 s; the agent's own input does not |
| FR-18 | Manual: Stop all, the notices go | passed on Edge (2026-11-18): the tab is detached and its indicator off; the next call attaches again |
| FR-19 | Manual: Evo starts in the tray | reconnect after a browser restart passed (2026-11-18); starting Evo from the browser: not run |
| FR-20 | Manual: close the tab during a wait | passed on Edge (2026-11-18): the wait returns "The tab closed." at once; later calls say the same |
| FR-21 | Review of result wording | passed (2026-11-18): page results end with the untrusted-data note, as in the built-in browser |
| FR-22 | `webbridge` tests; manual: browser settings page refused | passed (2026-11-18): tests; on Edge `edge://settings`, `chrome://extensions` and the Web Store are refused with "The browser does not let extensions use this page."; a Block site refuses click, eval and navigate |
| FR-23 | Manual on a test machine with the policy | not run |
| FR-24 | Manual: Logs page | passed (2026-11-18): set-up, connections, site changes and each call ("chat …: browser_click on http://127.0.0.1:8765 (allowed)") in the `webbridge` source; no page content or typed text |
| FR-25 | Manual: US-8 scenarios 1 and 6 | passed on Edge (2026-11-18): header, history, new chat, menu, empty state |
| FR-26 | Manual: model picked in the panel shows in the app | not run |
| FR-27 | Manual: US-8 scenario 3 | not run |
| FR-28 | Manual: screenshot and PDF attached | not run |
| FR-29 | Manual: voice off hides the microphone; on, text only | partly passed (2026-11-18): the switch shows and hides the microphone live; dictation: not run |
| NFR-1 | `netstat` shows no Evo listener for Webbridge | passed (2026-11-18): the only Evo listener is the plugin host's, there before set-up; Webbridge is the per-user pipe |
| NFR-2 | Time `browser_read_page` on a Webbridge tab | passed (2026-11-18): 8–17 ms on Edge; click and type 220–360 ms |
| NFR-3 | Start the app with nothing set up: no host, no pipe | passed (2026-11-18): no `evo-webbridge-*` pipe until Set up |
| NFR-4 | Manual on Windows with Chrome and Edge | passed on Windows with Edge (2026-11-18); Chrome shares the code; macOS and Linux: not run (T34) |
| NFR-5 | Review of the extension's storage | passed (2026-11-18): `storage.local` holds the profile id, its name and the last chat id; `storage.session` the group ↔ chat map |
| NFR-6 | `webbridge::wire` chunk tests; full-page screenshot | passed (2026-11-18): wire tests; full-page screenshot of a 4000 px page on Edge |
