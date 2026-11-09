# Computer app control in the background — Research notes

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Updated | 2026-11-09 |
| Sources in the repository | tag `v3.1.1`: `desktop/src-tauri/src/computer_app/`, `app/agent/tools/builtin/computer_app_tool.py`, `app/agent/builtin_skills/computer-app-control/`, `documents/features/computer-app-control.md` |

These notes record what v3 did and why its accuracy was low. They also cover
the options weighed for v4 and the spike results the [plan](plan.md) relies
on. They are background for the plan and the tasks, and are removed once the
feature is implemented.

## 1. What v3 built

v3 had a `computer_app` tool driving one window per chat. The window was
"parked" off screen and shown in a floating card with a virtual cursor.

| Area | Windows | macOS |
| --- | --- | --- |
| Choosing an app | `list_windows`, `attach`, `search_apps`, `open_app`, `close_app`, `kill_app`; allow/block lists; protected and elevated processes refused | same; terminals, script tools and System Settings refused |
| Keeping it off screen | `SetWindowPos` past the virtual screen's right edge; fixed 1280×800 "stage"; journal `computer_app_parked.json` for crash recovery; an `EVENT_OBJECT_SHOW` hook pulls dialogs and popups after it | moved to the bottom-right corner with one point visible |
| Capture | `PrintWindow(PW_RENDERFULLCONTENT)` plus open popups; Windows.Graphics.Capture rejected (stale frames off screen) | CGWindowList; ScreenCaptureKit for Chromium |
| Reading | UI Automation tree, refs `eN` never reused; Chromium pages through their MSAA/IAccessible2 tree | AX tree |
| Input | posted messages (`PostMessage`), modifiers held via `AttachThreadInput` + `SetKeyboardState`, `WM_CHAR` typing, UIA/MSAA for buttons, special paths for grids and text | AX actions, `CGEventPostToPid` as fallback; menu shortcuts briefly activate the app |
| Card | JPEG poll ~6 fps, pointer events, Stop / Allow again / Show the app / Close | same |
| Command path | Python tool → WebSocket → chat UI → Tauri `invoke` | same |

Measured by v3's `live_bench` (latest three runs):

- Notepad: 7/7 in all three.
- Word: 9/11, 11/11 and 11/11.
- Excel: 16/16 and 15/16, after a first, slower run that failed 4 cases.
- The failing cases moved between runs: ctrl+click in a grid, a ribbon toggle,
  and a right click.
- About 2 typing runs in 10 reordered a character.

## 2. Why accuracy was low

1. **Posted input is not real input.** Messages posted to a window skip the
   system's input queue. There is no real hover tracking, no guaranteed
   ordering against the app's own work, and no real key state. v3 documented
   these failures:
   - WPF, GTK, Tk and LibreOffice (VCL) dropping much of it;
   - self-drawn buttons that only take a click while "hovered";
   - key tips never appearing;
   - OLE drag and drop following the real cursor;
   - raw-input apps and UWP surfaces ignoring it.

   Each failure gained its own workaround, and each workaround is
   probabilistic. That is why failures moved between runs.
2. **Off-screen windows stop painting.** Chromium (and so Electron, Edge and
   WebView-based apps) classifies a window wholly outside the screen as
   occluded. It then stops painting, stops updating accessibility values, and
   drops pointer moves during drags. DirectComposition content gave stale
   Windows.Graphics.Capture frames. **The model then decided from an old
   picture**, which is the hardest failure to see. v3 worked around it with a
   transparent, click-through, topmost moment and a one-pixel nudge.
3. **Coordinates depend on the model.** Clicking at screenshot coordinates
   only works when the model grounds well and the image is not rescaled. v3
   had to cap images at 1568 px and 1.15 MP because larger ones were scaled
   again by the model API, and its clicks then landed short. evo lets the
   person pick models from any provider, so grounding quality cannot be
   assumed.
4. **A long command path.** The chain of Python tool, WebSocket, chat UI and
   `invoke` added timeouts, reconnects and "two windows own one session"
   failures unrelated to the app. In v4, app-side tools run in the Rust shell
   ([ADR 0013](../../architecture/decisions/0013-app-side-agent-tools-run-in-the-rust-shell.md)),
   so this goes away.

## 3. Options weighed for v4

| Option | Renders and captures? | Isolates input? | Verdict |
| --- | --- | --- | --- |
| Virtual monitor (IddCx driver) | yes, it is a real display | no: one pointer and one foreground per session | **rejected by the owner** (spec 6.2): it needs a signed driver and a one-time elevation, and it does not solve input |
| Borrowing the real pointer and keyboard briefly (`SendInput` while the person is idle) | n/a | no, by definition | **rejected by the owner** (spec 6.2) |
| Transparent, click-through, topmost "ghost" window | yes (Chromium keeps painting) | no | **rejected by the owner** (spec 6.2): it counts as hiding |
| Minimize or cloak, or another virtual desktop | no: minimized windows do not render; cloaked windows count as occluded in Chromium | no | rejected |
| Hidden desktop (`CreateDesktop`) | not composited; only PrintWindow + posted input | yes | rejected: it is the hidden-VNC malware technique, and security products flag it |
| Second session (RDP loopback) | yes | yes | rejected: client editions allow one session without patches |
| OS agent session (Windows agent workspace, preview) | yes | yes | not available to third parties yet; kept as a future backend (spec section 2.2) |
| Synthetic pen or touch device (`InjectSyntheticPointerInput`) | n/a | unclear: it goes through the shared input path; whether it moves the cursor or takes the foreground is not documented | rejected for now: it targets screen points, which an off-screen window has none of |
| Second mouse cursor | n/a | n/a | Windows has none. All mice merge into one cursor, and the MultiPoint SDK only drew fake cursors inside its own apps |
| **Off screen + background-only channels + accuracy layer** | yes for native apps (PrintWindow); yes for web apps through a page channel; at risk for web apps already open | yes | **chosen** (spec) |

## 4. The chosen direction in technical terms

These are candidates for `plan.md`, not decisions yet.

### 4.1 Input channels, in order

| Order | Channel | Used for | Note |
| --- | --- | --- | --- |
| 1 | Page channel: DevTools protocol over a pipe | web-based apps evo starts (Chromium, Electron, WebView2 hosts) | It is the channel evo's agent browser already uses. Input and screenshots work regardless of window visibility. Electron takes `--remote-debugging-pipe`; WebView2 hosts take `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`. **To verify:** apps that disable remote debugging (Electron fuses), and whether a pipe is possible for WebView2 or only a loopback port |
| 2 | UI Automation and MSAA (Windows), AX (macOS) | native apps, WPF, Office, forms, dialogs, already-open web apps | v3 code. WPF supports UIA well even though it drops posted mouse input |
| 3 | Posted messages | Win32 and WinForms classes known to accept them | v3 code, limited to known-good classes |
| 4 | Coordinates | canvases and apps with no tree | last resort; the result is flagged |

### 4.2 Keeping pictures current without hiding

- **Native apps:** PrintWindow is correct off screen (v3 measured it).
- **Web apps started by evo:** take the picture from the page channel, and
  start the app with `--disable-features=CalculateNativeWinOcclusion` so it
  keeps painting.
- **Web apps already open:** keep one pixel topmost on screen in a corner
  (spec FR-6, FR-16). A spike must verify that Chromium then counts the window
  as visible. If one still lags, its pictures are labelled "may be late".

### 4.3 Accuracy layer (provider-agnostic)

1. **Set-of-marks.** A screenshot with visible elements numbered by their
   refs. The model picks `click e17` instead of a point. Hybrid tree plus
   picture is the standard approach:
   - UFO2 reaches it with UIA plus vision;
   - Cua's `som` mode sends an indexed tree plus a screenshot.
2. **A 1:1 stage.** The 1280×800 stage fits common model image limits, so the
   picture is not rescaled and coordinates need no mapping. A small adapter
   handles models that answer in a 0–1000 grid; it is chosen by capability,
   never by model ID.
3. **Zoom when unsure.** A region crop at 2×. Published refinement methods
   report +13 to +19 points on ScreenSpot-Pro: ZoomClick and AutoFocus.
   UI-Zoomer found that zooming on every target hurts, so zoom only for small
   or dense targets.
4. **Read-back after every action.** Diff the tree (value, selection, focus,
   new window). On no effect, move one channel down once and report. Never
   retype blindly.
5. **Optional later:** a pluggable grounding model (UI-TARS, GUI-Actor,
   Holo2 class) for weak models, measured only after 1–4.
6. **Benchmark:** port v3's `live_bench`. Run every case N times by surface
   kind and add an input-isolation monitor (spec SC-1, SC-2, NFR-2).

### 4.4 Reusing floating cards

On `main`, the browser stage is:

- a real WebView2 at 1280×800 parked at −10000;
- CDP `Page.startScreencast` producing JPEG frames;
- `Streams` throttling them;
- `browser://frame {tab, seq, data, width, height}` events;
- a canvas, an `AgentCursor` and `browser://pointer` events.

The payloads and the throttle are already generic in shape. Needed for apps:

- a `CardKind` `"app"`;
- an app frame source emitting the same frame and pointer shapes (or a
  shared `stage://` naming);
- a card body branch for an app canvas;
- an input command for the person's input in the card;
- a `computer_` prefix in the activity mapping;
- tool registration in the shell registry with its class.

The engine and `evo-protocol` stay unchanged.

## 5. Port map from v3

| v3 | v4 |
| --- | --- |
| `computer_app/{action,keys,policy,interrupt,workers,apps,geometry}.rs` | keep; move under the shell's agent tools |
| `win/{uia_tree,uia_patterns,uia_input,page_tree,catalog,listing,registry,target,lifecycle,popups,typing,keyboard,messages,text_surface}.rs` | keep as channels 2 and 3 |
| `win/parking.rs` | keep (stage, journal, popup hook); drop the transparent-topmost repaint trick |
| `win/capture.rs` | keep PrintWindow for native apps; page channel for web apps |
| `win/pointer.rs`, `web_fill.rs` | keep for already-open web apps; evo-started ones use the page channel |
| `win/live_bench.rs`, `live_tests.rs` | keep; extend to repeated runs plus the input monitor |
| `mac/*` | keep; drop the temporary activation for menu shortcuts (spec FR-29); move capture to ScreenCaptureKit |
| Python tool, `direct_computer_bridge`, WebSocket, `computerAppBridge.ts` | drop: the tool runs in the Rust shell |
| `computer-app-control` skill | keep the method; rewrite the input sections for the channel order and set-of-marks |

## 6. Spikes before the plan

Run 2026-11-09 on Windows 11 with three monitors. Each app ran as a separate
instance with a throw-away profile, so none of the person's open windows or
profiles were touched. Apps tested:

- Edge 157, standing in for a Chromium app the person already has open;
- VS Code 1.145 on Electron 43.

A full-window overlay changed colour every 10 animation frames. For each
window placement the spike measured:

- the page's frame rate and `document.visibilityState`;
- whether `PrintWindow` pictures changed;
- whether a DevTools screenshot showed a colour set just before it;
- whether a DevTools click plus `Input.insertText("Tiếng Việt")` reached a
  textarea;
- the person's foreground window before and after.

| App, placement | Visibility | Frames/s | PrintWindow fresh | DevTools picture fresh | DevTools input |
| --- | --- | --- | --- | --- | --- |
| Edge, on screen | visible | 70 | yes | yes | yes |
| Edge, fully off screen | **hidden** | **0** | **no** | yes | yes |
| Edge, 1 px corner, not topmost (moved there) | visible | 72 | yes | yes | yes |
| Edge, 1 px corner, topmost (moved there) | visible | 70 | yes | yes | yes |
| Edge, 1 px corner reached by a z-order change only (first run) | hidden | 0 | no | — | yes |
| Edge, fully off screen, `--disable-features=CalculateNativeWinOcclusion` | visible | 69 | yes | yes | yes |
| VS Code, fully off screen, no flag | visible | 71 | yes | yes | yes |
| VS Code, 1 px corner, topmost | visible | 72 | yes | yes | yes |

Findings:

1. **One pixel is enough, but only after a move.** A Chromium window with one
   pixel of its frame on a monitor counts as visible and paints at full rate.
   Chromium re-evaluates occlusion when a window moves, not when only its
   z-order changes. In the first run the corner was placed, then made topmost
   without a move, and the window stayed hidden. v3 saw the same thing ("only
   rechecks whether a window shows when it moves").

   Rule for the plan: set the position and `HWND_TOPMOST` in one
   `SetWindowPos` call. Re-assert it, with a one-pixel move, whenever the
   window is found hidden or covered.
2. **Fully off screen, plain Chromium stops painting.** Its frame rate drops to
   0 and `PrintWindow` returns the old picture. This confirms the v3 failure
   behind "the model sees an old state".
3. **Electron apps keep painting off screen**, as VS Code did with no flag.
   Electron's default turns native occlusion off. The corner pixel is
   therefore needed mainly for Chromium browsers and their installed web apps,
   and possibly for WebView2 hosts (not measured, see 7).
4. **The occlusion flag works.** With
   `--disable-features=CalculateNativeWinOcclusion`, Edge kept painting fully
   off screen. An app evo starts can stay fully off screen.
5. **The DevTools channel works off screen.** Input (a click, Vietnamese text)
   reached the page in every placement, including hidden. `Page.captureScreenshot`
   returned a fresh picture even while the page was hidden, because it forces a
   paint. Transport:
   - `--remote-debugging-pipe` worked for Edge and for VS Code (Electron 43),
     so no network port is needed (spec NFR-6);
   - `--remote-debugging-port` also worked for VS Code.
6. **Starting a Chromium app takes the foreground.** Edge's new window became
   the person's foreground window at launch. `open_app` must start apps
   without activation, as v3 did with `SW_SHOWMINNOACTIVE`, then restore them
   without activation. It must also verify that Chromium honours the start-up
   show state (spec FR-5, US-8).
7. **No change to the person's foreground** from any placement or DevTools
   input. Only launches changed it (finding 6).

Not run:

- **WebView2 hosts.** The ones installed were running with the person's
  accounts, and restarting them was out of bounds. The
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` route is documented, and evo's own
  end-to-end tests use it.
- **Electron apps that turn remote debugging off with fuses.** None was at
  hand.
- **Spike 3**, re-running v3's failing cases. It needs v3's benchmark ported,
  which is product code, so it becomes the first phase of `tasks.md`.

The spike scripts are not kept in the repository. They are a Win32 helper in
Python for placement and `PrintWindow` sampling, and a Node driver for the
DevTools pipe and port.

## 7. Sources

- Chromium native window occlusion on Windows: https://chromium.googlesource.com/chromium/src/+/main/docs/windows_native_window_occlusion_tracking.md
- `SendInput`: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- `InjectSyntheticPointerInput`: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-injectsyntheticpointerinput
- Windows agent workspace security: https://learn.microsoft.com/en-us/windows/security/book/operating-system-agentic-security
- Hidden desktop detection: https://attack.mitre.org/detectionstrategies/DET0128
- Windows MultiPoint Mouse: https://en.wikipedia.org/wiki/Windows_MultiPoint_Mouse
- Background input without foreground (Cua): https://cua.ai/docs/concepts/the-no-foreground-contract
- macOS window internals (Cua): https://cua.ai/blog/inside-macos-window-internals
- UFO2: https://arxiv.org/abs/2504.14603
- OmniParser v2: https://www.microsoft.com/en-us/research/articles/omniparser-v2-turning-any-llm-into-a-computer-use-agent/
- GUI-Actor: https://arxiv.org/html/2506.03143v1
- Holo2: https://huggingface.co/blog/Hcompany/introducing-holo2-235b-a22b
- Zoom refinement: https://arxiv.org/pdf/2512.05941, https://arxiv.org/pdf/2604.14113
- ScreenSpot-Pro leaderboard: https://benchlm.ai/benchmarks/screenspot-pro
