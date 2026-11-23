# Quick entry — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-23 |

## 1. Phases

### Phase 1 — The box

Done when: the shortcut and the tray click open the box, Enter starts a new
chat in the main window, Esc and blur keep the text, and Settings turns it
off and changes the shortcut.

- [x] T1 [FR-2, FR-3, FR-4, FR-6] Unit tests for the pure decisions: tray click target, placement, pending text, default shortcut — `apps/desktop/src-tauri/src/quick_entry.rs` — check: `bun run test:shell quick_entry`
- [x] T2 [FR-1, FR-2, FR-4, FR-5, FR-6, FR-7, FR-9] Shell module: window, toggle, blur, global shortcut, settings, hand-off and commands; plugin, capability and wiring — `apps/desktop/src-tauri/src/quick_entry.rs`, `src/lib.rs`, `src/system.rs`, `Cargo.toml`, `capabilities/quick-entry.json` — check: `cargo check`, `bun run test:shell`
- [x] T3 [FR-3] Tray left click toggles the box while quick entry is on — `apps/desktop/src-tauri/src/system.rs` — check: T1's test; manual tray click
- [x] T4 [FR-5, FR-7, FR-8, FR-10, NFR-1] The box page — `apps/desktop/quick-entry.html`, `apps/desktop/src/quick-entry/`, `apps/desktop/vite.config.ts` — check: `bun run build`; manual check
- [x] T5 [FR-5, FR-6] Main window starts the chat from `app://quick-entry` and `quick_entry_pending` — `apps/desktop/src/stores/engine-store.ts` — check: manual check in the running app
- [x] T6 [FR-9] Settings › General: Quick entry switch and shortcut recorder — `apps/desktop/src/features/settings/sections/general-settings.tsx` — check: manual check in Settings
- [x] T7 UI map and features index — `apps/desktop/README.md`, `docs/features/README.md` — check: read

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Debug app on scratch data: a `quick-entry.html` page loads beside the main page at start | passed (2026-11-23) |
| FR-2 | Manual check: shortcut from another app toggles the box | not run — `Ctrl+Alt+Space` was taken by another app on the test machine; default moved to `Alt+Shift+Space`, not yet pressed |
| FR-3 | Unit test `a_tray_click_opens_the_box_only_while_quick_entry_is_on`; manual tray click | unit passed (2026-11-23); manual: the person's check passed (2026-11-23) |
| FR-4 | Unit test `the_box_opens_centred_in_the_upper_part_of_the_monitor`; the box sat at (620, 264) on a 1920×1200 monitor | passed (2026-11-23); two monitors not run |
| FR-5 | Manual check: Enter starts a new chat with the text | passed (2026-11-23, the person's check: the main window's engine started a run with the sent text) |
| FR-6 | Unit test `text_sent_without_a_main_window_is_taken_once`; manual check with the main window gone | unit passed (2026-11-23); manual not run |
| FR-7 | Manual check: Esc and blur keep the text | blur hid the box when another app took focus (2026-11-23); Esc not run by the agent |
| FR-8 | Unit test `the_box_grows_with_its_text_up_to_a_limit`; manual long text | unit passed (2026-11-23); manual not run |
| FR-9 | Settings reported `quickEntryShortcutOk: false` for the taken `Ctrl+Alt+Space`; recording a new shortcut | taken-shortcut report passed (2026-11-23); recording not run |
| FR-10 | Manual check: light and dark | not run |
| NFR-1 | `bun run build`: the quick entry page loads its own 2.4 kB entry plus shared React/style chunks, no engine code | passed (2026-11-23) |
| NFR-2 | Windows checked; macOS and Linux | Windows passed (2026-11-23); macOS, Linux not run |
