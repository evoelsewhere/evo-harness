# App updates — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-11 |

## 1. Phases

### Phase 0 — Release key (owner)

Done when: the GitHub secrets exist and the public key is known. Only the
owner can do this phase (plan 3.8).

- [x] T1 [NFR-2] Generate the key pair outside the repository, add `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to the repository's secrets, keep an offline backup, hand over the public key — owner — check: both secrets listed under the repository's Actions secrets — key pair generated and the public key handed over (2026-11-10); both secrets set by the owner in the repository settings (2026-11-10, confirmed by the owner; their values cannot be read back)

### Phase 1 — Shell updater

Done when: a release build of the shell checks, downloads, verifies and
reports state through `updates_state` and `updates://state`; tests pass.

- [x] T2 [FR-9] Test first: `EngineState::working()` follows `agent_start` / `agent_end` and forgets a chat whose engine stops or exits — `apps/desktop/src-tauri/src/engine.rs` (tests module) — check: `bun run test:shell` fails for the missing method
- [x] T3 [FR-9] Track working chats in the stdout loop, `stop`, `stop_all` and the exit path — `apps/desktop/src-tauri/src/engine.rs` — check: T2 passes
- [ ] ~~T4~~ waived 2026-11-10 by the owner (not run; failures are reported as bugs) — [FR-10] Move the exit steps (connectors, controlled apps, engines) into `system::shutdown` and call it from `RunEvent::Exit` — `apps/desktop/src-tauri/src/system.rs`, `apps/desktop/src-tauri/src/lib.rs` — check: `cargo check`; quitting from the tray still stops every `evo-engine` process (manual) — code done and `cargo check` passed; the manual quit not run yet (the installed app was running)
- [x] T5 [FR-4, FR-12, FR-13] Tests first for the pure parts of `updates.rs`: the outcome of a check (`Some`, `None`, `TargetsNotFound` with a higher / equal / lower recorded version, other errors, automatic vs by hand), cache folder cleanup by version, the 6-hour due time from `lastCheckedAt` — `apps/desktop/src-tauri/src/updates.rs` (tests module) — check: `bun run test:shell` fails for the right reasons
- [x] T6 [FR-1, FR-2, FR-4, FR-12, FR-13, FR-14, NFR-5] State machine, comparator, timer (30 s after setup, wake every 10 min, due after 6 h, reads `updates.autoCheck` each time), `unavailable` in debug builds, `updates_state` and `updates_check` (main window only), `updates://state` — `apps/desktop/src-tauri/src/updates.rs`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src-tauri/Cargo.toml` — check: T5 passes; `cargo clippy`
- [x] T7 [FR-6, NFR-6] Test first: a file is accepted only with a signature from the configured key; a changed byte or another key is refused (test key pair generated in the test, never committed) — `apps/desktop/src-tauri/src/updates.rs` — check: `bun run test:shell` fails, then passes after T8 (a throwaway test key's public half and one signature are in the test; no private key)
- [x] T8 [FR-5, FR-6, NFR-6] Download with progress (throttled), write to the cache under a temporary name then rename, second verification with `minisign-verify` on the file read back, `failed { verify }` deletes the folder — `apps/desktop/src-tauri/src/updates.rs` — check: T7 passes
- [ ] ~~T9~~ waived 2026-11-10 by the owner (not run; failures are reported as bugs) — [FR-6, FR-15, NFR-2, NFR-4] `plugins.updater` in the config (`pubkey` from T1, endpoint, `windows.installMode: "passive"`); debug-only `EVO_UPDATES_ENDPOINT` / `EVO_UPDATES_PUBKEY` — `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/src/updates.rs` — check: a release build starts; `bun run build:dev` still works without the private key (NFR-7) — endpoint, install mode and debug overrides done; the owner's public key is in the config and checked by `the_app_is_built_with_a_valid_release_key_and_endpoint`; a debug NSIS installer builds without the private key (2026-11-10); a release build not started yet
- [x] T10 [FR-8, FR-9, FR-10] `updates_install(force)`: `confirm` when chats are working; read and verify the file; Windows: `install` on a blocking thread with `on_before_exit` = `system::shutdown`; macOS: `install`, `system::shutdown`, `restart`; failures before shutdown leave the app running — `apps/desktop/src-tauri/src/updates.rs` — check: `cargo clippy`; covered end to end by T24

### Phase 2 — Desktop UI

Done when: **Settings → About** shows every state, **Settings → General**
has the switch, a ready update raises one notification with a working
**Restart to update** button.

- [x] T11 [P] [FR-7] Notification actions: `actions` on `AppNotification` and `notify()`, `registerNotificationCommand`, buttons on toasts and bell rows, hidden when no handler is registered; update the notifications plan and note its T10 (the mechanism; **Approve** on approvals is still open there) — `apps/desktop/src/stores/notification-store.ts`, `apps/desktop/src/features/notifications/{notify.ts,toaster.tsx,notification-center.tsx,open-target.ts}`, `docs/features/notifications/{plan.md,tasks.md}` — check: `bun run typecheck`; manual: a test notification's button runs its command
- [x] T12 [P] [FR-3, FR-7, FR-9, FR-12, FR-13] Updates store: load `updates_state`, listen to `updates://state`, `check()`, `install(force)`, notifications on `ready`, `notice` and `failed { verify }` with key `updates`, registered command `updates.install`; started from the bootstrap in the main window only — `apps/desktop/src/stores/updates-store.ts`, `apps/desktop/src/app/use-app-bootstrap.ts` — check: `bun run typecheck`
- [x] T13 [FR-3, FR-9, FR-11, FR-12, FR-14] The **Updates** row in **About** with every state of plan 3.3, progress, last check time, **Check for updates**, **Restart to update**, **What's new**, the download-page link for `notice` and install failures, and the confirmation dialog — `apps/desktop/src/features/settings/sections/update-row.tsx`, `apps/desktop/src/features/settings/sections/system-settings.tsx` — check: `bun run build`; README screenshots harness renders each state (manual)
- [ ] ~~T14~~ waived 2026-11-10 by the owner (not run; failures are reported as bugs) — [P] [FR-2] **Updates** group in **General** with **Check for updates automatically** (`updates.autoCheck`, default on); turning it on calls `updates_check` — `apps/desktop/src/features/settings/sections/general-settings.tsx` — check: `bun run typecheck`; manual: the setting survives a restart — typecheck passed; the restart check not run yet
- [x] T15 [FR-3] UI map rows for the About row, the General switch and the update notification — `apps/desktop/README.md` — check: review

### Phase 3 — Release pipeline

Done when: a tag build on a fork (with its own key) publishes a release
whose `latest.json` lists Windows and both macOS builds (signed case) or
Windows only (unsigned case), with URLs that download and signatures that
verify.

- [x] T16 [FR-15] Test first: `updater_manifest.py` on sample platform files and a sample asset list — complete set, a missing Windows file, an ineligible macOS file, an asset name not in the release — `.github/scripts/updater_manifest.py`, `.github/scripts/test_updater_manifest.py` — check: `python -m unittest discover -s .github/scripts -p "test_*.py"` fails, then passes after T17
- [x] T17 [FR-15] The script: read the platform files, match assets by name to their download URLs, write `latest.json` (`version`, `notes` = release URL, `pub_date` RFC 3339, `platforms`), fail when Windows or a macOS file is missing or an eligible entry is incomplete — `.github/scripts/updater_manifest.py` — check: T16 passes
- [ ] ~~T18~~ waived 2026-11-10 by the owner (not run; failures are reported as bugs) — [FR-15, NFR-2, NFR-7] Build job: on tag builds require the signing secrets and add `createUpdaterArtifacts: true` to the bundle override; manual runs add it only when the secrets exist; pass the secrets to the `tauri build` steps only — `.github/workflows/desktop-packages.yml` — check: a manual run without secrets still builds; the log shows no key material — written; not run (needs a workflow run)
- [ ] ~~T19~~ waived 2026-11-10 by the owner (not run; failures are reported as bugs) — [FR-15] Build job: write the platform file (Windows: the setup file and its `.sig`; macOS: rename `*.app.tar.gz` to `evo-harness_<version>_<arch>.app.tar.gz`, eligible only when the Apple identity was used; Linux: not eligible), upload it and the macOS archive with the artifacts — `.github/workflows/desktop-packages.yml` — check: artifacts of a manual run contain the expected files — written (the macOS archive is moved out of the DMG folder first); not run
- [ ] ~~T20~~ waived 2026-11-10 by the owner (not run; failures are reported as bugs) — [FR-15] Release job: after the asset upload, run the script against `gh release view --json assets`, upload `latest.json`, then publish as latest; a script failure leaves the release a draft — `.github/workflows/desktop-packages.yml` — check: a fork tag build publishes `latest.json`; a fork build with the Windows platform file removed stays a draft — written; not run

### Phase 4 — Docs and end-to-end verification

Done when: every row of the verification table has a result.

- [x] T21 [NFR-2] ADD sections 2, 4, 7, 10 and 12 and ADR 0017 agree with what was built; set ADR 0017 to `Accepted` with gate B — `docs/architecture/README.md`, `docs/architecture/decisions/` — check: review
- [ ] ~~T22~~ waived 2026-11-10 by the owner (not run; failures are reported as bugs) — [NFR-3] Read the request a check makes (debug build against a local HTTPS server or a proxy) — check: one GET of `latest.json`, plugin user agent, no identifiers
- [ ] ~~T23~~ waived 2026-11-10 by the owner (not run; failures are reported as bugs) — [FR-14] Development build and browser preview show "not available in this build" and make no request — check: manual, with the network panel
- [ ] ~~T24~~ waived 2026-11-10 by the owner (not run; failures are reported as bugs) — [FR-1, FR-5, FR-6, FR-7, FR-8, FR-9, FR-10, FR-11, NFR-1, NFR-4, NFR-6] End to end on a fork: install release N, publish N+1, wait for the automatic check (with the interval shortened by `EVO_UPDATES_*` in a debug build, then once with a release build), restart with and without a working chat, on Windows and on macOS (both architectures if the hardware exists); also: an altered file, a cut-off download, quitting with an update ready — check: manual, results recorded below
- [ ] ~~T25~~ waived 2026-11-10 by the owner (not run; failures are reported as bugs) — [FR-12] Linux package: install release N from the DEB, publish N+1, check — check: notice with the download page, nothing downloaded
- [ ] ~~T26~~ standing rule, not a step to finish ([ADR 0017](../../architecture/decisions/0017-app-updates-from-signed-github-releases.md), consequences) — [FR-15] After every upgrade of `tauri-plugin-updater`: rerun T24's Windows case and T25 (the comparator-before-lookup behaviour) — check: noted in the upgrade's commit

### Phase 5 — New-version dialog and check mode (2026-11-11)

Done when: an automatic check that finds a version opens the dialog with
the notes in sections, once per version, and About chooses automatic or
manual checks.

- [x] T27 [FR-16, FR-15] Test first: `--notes` puts the release notes in the manifest, empty notes fall back to the release URL — `.github/scripts/updater_manifest.py`, `.github/scripts/test_updater_manifest.py`, `.github/workflows/desktop-packages.yml` — check: `python -m unittest discover -s .github/scripts -p "test_*.py"`
- [x] T28 [FR-16] The comparator records the release's notes and date; `UpdateState.release`; `release_notes` drops a bare link — `apps/desktop/src-tauri/src/updates.rs` — check: `bun run test:shell` (`release_notes_are_markdown_or_nothing`, `the_state_is_sent_as_the_plan_writes_it`)
- [x] T29 [FR-16] Test first: parse the notes into sections, scopes, commits and the compare link; other shapes return null — `apps/desktop/src/features/updates/changelog.ts`, `changelog.test.ts` — check: `bun test src/features/updates`
- [x] T30 [FR-11, FR-16, FR-17, FR-9] The dialog, the restart question moved into `UpdatesHost` (main window only), `openWhatsNew`, once-per-version opening after automatic checks (`updates.dialogShownFor`) — `apps/desktop/src/features/updates/`, `apps/desktop/src/stores/updates-store.ts`, `apps/desktop/src/app/app.tsx` — check: `bun run typecheck`; browser preview with the store filled by hand
- [x] T31 [FR-2] **Check for updates: Automatically / Manually** in About; the General switch removed; About's search words — `apps/desktop/src/features/settings/sections/update-row.tsx`, `system-settings.tsx`, `general-settings.tsx`, `settings-registry.ts` — check: browser preview
- [x] T32 UI map row — `apps/desktop/README.md` — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | T24: automatic check finds N+1 | not run — waived by the owner (2026-11-10) |
| FR-2 | T14 manual; T6 timer reads the setting; T31 | partly: the switch renders and saves `updates.autoCheck` in the screenshot harness (2026-11-10); the About control switches between Automatically and Manually and the Status line follows, in the browser preview (2026-11-11); saving from the real app, restart and live check not run |
| FR-3 | T13 every state; T24 | partly: every state rendered in the screenshot harness with fake IPC, light and dark (2026-11-10); T24 not run; the rest waived by the owner (2026-11-10) |
| FR-4 | T5 tests (equal and lower versions) | passed (2026-11-10): `a_release_without_our_platform_is_announced_only_when_newer`, `a_newer_release_is_downloaded_or_announced` |
| FR-5 | T24: chat keeps streaming during the download | not run — waived by the owner (2026-11-10) |
| FR-6 | T7 tests; T24 altered file | partly: `only_a_file_signed_with_the_release_key_is_accepted` passed (2026-11-10); T24 not run; the rest waived by the owner (2026-11-10) |
| FR-7 | T24: one bell entry, button works | partly: the toast's **Restart to update** opens About and asks (screenshot harness, 2026-11-10); T24 not run; the rest waived by the owner (2026-11-10) |
| FR-8 | T24: nothing installed without a click after 24 h | not run — waived by the owner (2026-11-10) |
| FR-9 | T2 test; T24 confirm and cancel with a working chat | partly: `working_chats_follow_runs_and_forget_stopped_engines` passed; the dialog shows the count (harness) (2026-11-10); T24 not run; the rest waived by the owner (2026-11-10) |
| FR-10 | T24: reopens on N+1 with chats and settings | not run — waived by the owner (2026-11-10) |
| FR-11 | T24: **What's new** opens the release notes; T30 | partly: **What's new** opens the dialog over Settings in the browser preview (2026-11-11); T24 not run |
| FR-12 | T5 tests; T25 | partly: decision tests passed (2026-11-10); T25 not run; the rest waived by the owner (2026-11-10) |
| FR-13 | T5 tests; T24 offline check by hand | partly: failed states render with the reason and a retry (harness, 2026-11-10); T24 not run; the rest waived by the owner (2026-11-10) |
| FR-14 | T23 | not run — waived by the owner (2026-11-10) |
| FR-15 | T16 tests; T20 fork release | partly: 10 manifest tests passed (2026-11-10); T20 not run; the rest waived by the owner (2026-11-10) |
| FR-16 | T27, T28, T29 tests; T30 preview | partly: 12 manifest tests, `release_notes_are_markdown_or_nothing`, 5 parser tests passed; the dialog rendered ready (sections, counts, Restart to update), downloading (progress) and no-notes states in the browser preview (2026-11-11); a real release's `latest.json` with notes not seen yet |
| FR-17 | T30 | not run: needs an automatic check against a release with notes (debug build with `EVO_UPDATES_ENDPOINT`) |
| NFR-1 | T24 on Windows and macOS; T25 on Linux | not run — waived by the owner (2026-11-10) |
| NFR-2 | T18 log review; T21 | not run — waived by the owner (2026-11-10) |
| NFR-3 | T22 | not run — waived by the owner (2026-11-10) |
| NFR-4 | T24 on a standard Windows account | not run — waived by the owner (2026-11-10) |
| NFR-5 | T6: first check 30 s after setup; T24 startup with a slow network | not run — waived by the owner (2026-11-10) |
| NFR-6 | T8; T24 cut-off download | not run — waived by the owner (2026-11-10) |
| NFR-7 | T9 `bun run build:dev` without the key; T18 manual run without secrets | partly: `tauri build --debug --bundles nsis` without the key built the installer, no update files (2026-11-10); T18 not run; the rest waived by the owner (2026-11-10) |
