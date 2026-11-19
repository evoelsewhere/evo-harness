# Source control: review, commit and turn checkpoints — Tasks

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Plan | [plan.md](plan.md) |
| Updated | 2026-11-19 |

## 1. Phases

### Phase 1 — Git building blocks in `evo-worktrees`

Done when: checkpoints (snapshot, changes, restore, prune, drop) and the
person's git operations (status split, diff with hunks, stage / unstage /
discard with undo, commit / amend) pass their tests against real temporary
repositories, and a checkpoint leaves `git status` unchanged.

- [x] T1 [P] [FR-9, FR-11, FR-14, NFR-3, NFR-4] Failing tests for `checkpoint`: a snapshot leaves the index, working tree, branches and stashes unchanged; holds untracked files and leaves ignored ones out; works before the first commit; `changes_between` lists added / modified / deleted / renamed with counts; `restore` puts files back, skips files changed since (unless forced) and deletes files absent at the start; `prune` keeps the newest 50; `drop_chat` removes every ref of a chat — `crates/evo-worktrees/src/checkpoint_tests.rs` — check: `cargo test -p evo-worktrees checkpoint` fails for the missing module
- [x] T2 [FR-9, FR-11, FR-14, NFR-3, NFR-4] `checkpoint` module: `HEAD` read into a temporary index, only the paths `status` lists updated, `write-tree`, `commit-tree`, `update-ref` under `refs/evo/checkpoints/<chat>/<seq>/…`; `changes_between`, `restore` (`git restore --source --worktree`), `prune`, `drop_chat` — `crates/evo-worktrees/src/{checkpoint.rs,lib.rs}` — check: T1 passes
- [x] T3 [P] [FR-1, FR-4, FR-5, FR-6, edge cases] Failing tests for `scm`: status split into staged and unstaged with repository state (clean, merge, rebase, detached, no commits); file diff with stable hunk ids and a hash, `binary`, `tooLarge`, `eolOnly`, renames; stage / unstage / discard of a hunk, a file and all; a stale hash is refused; discard returns blobs that restore the text; commit and amend; a failing hook keeps the files staged and returns its output; missing identity is refused; recent subjects — `crates/evo-worktrees/src/scm_tests.rs` — check: `cargo test -p evo-worktrees scm` fails for the missing module
- [x] T4 [P] [FR-1, FR-4, FR-5, FR-6, edge cases] `scm` module — `crates/evo-worktrees/src/scm.rs` (its `mod` line in `lib.rs` after T2) — check: T3 passes
- [x] T5 [NFR-1, NFR-4] An ignored test that builds a 50,000-file repository with 200 changed files and times a checkpoint (≤ 1 s) and the object growth over 50 small turns — `crates/evo-worktrees/src/checkpoint_tests.rs` — check: `cargo test -p evo-worktrees checkpoint_large -- --ignored` passes and prints the times (`EVO_BIG_REPO=<folder>` keeps the repository between runs)

### Phase 2 — Protocol and engine

Done when: a top-level run in a repository records a checkpoint that
`agent_settled` and `list_checkpoints` return, the first edit or command
waits for it, `revert_turn` restores and tells the agent, and
`draft_commit_message` answers without touching the session.

- [x] T6 [FR-7, FR-10, FR-11] Command and event shapes (`list_checkpoints`, `revert_turn`, `draft_commit_message`, `agent_settled.checkpoint`, the `checkpoint` entry) in the protocol crate, the engine's `Command` enum and the desktop's protocol types — `crates/evo-protocol/src/protocol.rs`, `crates/evo-engine/src/rpc/protocol.rs`, `apps/desktop/src/engine/protocol.ts` — check: `cargo test -p evo-protocol`; `bun run typecheck` (Rust part done and checked 2026-11-19; `protocol.ts` and its typecheck are the desktop's, done separately)
- [x] T7 [FR-9, FR-10, FR-14] Failing engine tests (faux provider, temporary repository): a top-level run snapshots at start and end; an edit or command call waits for the start snapshot and a read does not; a call the approval gate blocks never waits; `agent_settled` carries the checkpoint and a `checkpoint` entry is appended; `list_checkpoints` returns it after a restart; a subagent run and a folder outside a repository record none; a snapshot over 10 s lets the call go and marks `complete: false` — `crates/evo-engine/src/checkpoints.rs` (tests module) — check: `cargo test -p evo-engine checkpoints` fails
- [x] T8 [FR-9, FR-10, FR-14, NFR-1] Checkpoint task per top-level run, the `before_tool_call` link after the approval gate, the end snapshot and entry, pruning, `list_checkpoints`; closing an open start after a crash — `crates/evo-engine/src/{checkpoints.rs,modes.rs,agent_session.rs}` — check: T7 passes
- [x] T9 [FR-11, FR-12, FR-13] Failing tests for `revert_turn`: `busy` during a run; files changed since are skipped, `force` reverts them; an `undo` checkpoint restores the revert; the `<turn-reverted>` note is queued for the next run and the entry records the revert — `crates/evo-engine/src/checkpoints.rs` — check: `cargo test -p evo-engine revert_turn` fails
- [x] T10 [FR-11, FR-12, FR-13] `revert_turn` — `crates/evo-engine/src/{checkpoints.rs,modes.rs}` — check: T9 passes
- [x] T11 [P] [FR-7] Failing tests for `draft_commit_message`: answers on the chat's model through a side question of its own owner, writes nothing to the session, leaves the Side chat thread alone, cuts input over 64 KiB — `crates/evo-engine/src/commit_message.rs` — check: `cargo test -p evo-engine commit_message` fails
- [x] T12 [P] [FR-7] `draft_commit_message` — `crates/evo-engine/src/{commit_message.rs,modes.rs}` — check: T11 passes

### Phase 3 — Shell

Done when: the Source control commands work over Tauri on a real
repository, commit asks while a run is active in the folder, overlapping
runs are reported, and deleting a chat drops its checkpoints.

- [x] T13 [FR-1 – FR-6, FR-13] `source_control.rs`: `scm_status`, `scm_diff`, `scm_diff_between`, `scm_stage`, `scm_unstage`, `scm_discard`, `scm_undo_discard`, `scm_commit` (with `needsConfirm`), `scm_recent_subjects`, `checkpoints_drop`; paths kept inside the repository; registered in `lib.rs` — `apps/desktop/src-tauri/src/{source_control.rs,lib.rs}` — check: `bun run test:shell` (tests for path refusal, undo tokens, needsConfirm)
- [x] T14 [FR-13, edge case "two chats"] Run spans per folder from the `agent_start` / `agent_settled` lines the supervisor follows; `folder_running(cwd)` for commit; `turn_overlap(chat, runId)` — `apps/desktop/src-tauri/src/supervisor.rs` — check: `bun run test:shell` (supervisor tests with the fake host)

### Phase 4 — Desktop: the Source control tool

Done when: in the running app the tool lists changes, shows inline and side
by side diffs, stages / unstages / discards by hunk and file with Undo,
commits with a drafted message, and line comments reach the composer.

- [x] T15 [FR-1, FR-3] `lib/scm.ts` wrappers; `stores/source-control-store.ts` (status of the active chat's folder, refresh on the folder watch and `.git` changes with a 300 ms debounce, selection kept); tests for the pure helpers (selection after refresh, hunk-to-line mapping, the 64 KiB cut at a file boundary) — `apps/desktop/src/{lib/scm.ts,lib/scm.test.ts,stores/source-control-store.ts}` — check: `bun test src/lib/scm.test.ts`
- [x] T16 [FR-2] Add `@codemirror/merge`; the diff view (inline / side by side, language colours, folded context, `eolOnly` / binary / large states); `scmDiffLayout` in `ui-store` — `apps/desktop/{package.json,bun.lock}`, `src/features/source-control/diff-view.tsx`, `src/stores/ui-store.ts` — check: `bun run typecheck`; running app
- [x] T17 [FR-1, FR-4, FR-5] The panel: rail entry, Staged / Changes lists, file and hunk actions, discard confirmations, Undo notices, empty, error and merge / rebase states — `src/features/source-control/{source-control-panel.tsx,change-list.tsx,index.ts}`, `src/features/panels/{panel-registry.ts,right-panel.tsx}` — check: running app
- [x] T18 [FR-6, FR-7, FR-13] Commit box: message, Commit, Amend, the running-turn and pushed-commit questions, hook output, Draft message through `engine-store.draftCommitMessage` (wakes a deferred engine) — `src/features/source-control/commit-box.tsx`, `src/stores/engine-store.ts` — check: running app
- [x] T19 [FR-8] Tests, then the `code` annotation kind (compose as plain text after the office block, label), the composer chip counting both kinds, the diff's Comment button — `src/lib/{document-annotations.ts,document-annotations.test.ts}`, `src/stores/annotations-store.ts`, `src/features/source-control/diff-view.tsx`, `src/features/chat/chat-composer.tsx` — check: `bun test src/lib/document-annotations.test.ts`; running app
- [x] T20 [FR-15, FR-16] Open Source control at a file from the summary card and Files; the keyboard actions and the shortcuts group — `src/features/chat/summary-card.tsx`, `src/features/files/files-panel.tsx`, `src/features/shell/shortcuts.ts`, `src/stores/ui-store.ts` — check: running app; the shortcuts list

### Phase 5 — Desktop: turns

Done when: a finished turn shows its changed files, opens their turn diff,
reverts with confirmation and Undo, says when another chat worked in the
folder, and the revert note shows as an event line.

- [x] T21 [FR-10, FR-12] Tests, then the turn's checkpoint from `agent_settled` and from `list_checkpoints` on restore (matched by the prompt's entry id), and the `turn-reverted` note kind — `src/features/chat/{transcript.ts,transcript-checkpoints.test.ts}`, `src/stores/engine-store.ts` — check: `bun test src/features/chat/transcript-checkpoints.test.ts`
- [x] T22 [FR-10, FR-11, FR-13] "Changed N files" row, turn diff in Source control (`scm_diff_between`), Revert this turn with its confirmation, Reverted · Undo, the overlap line; `revertTurn` in `engine-store` — `src/features/chat/{turn-changes.tsx,chat-turn.tsx}`, `src/stores/engine-store.ts` — check: running app

### Phase 5b — Repositories in a workspace (FR-17)

Done when: a workspace with a top repository and nested ones lists them all
in Source control, and picking one shows and commits its changes.

- [x] T25 [FR-17] Failing tests, then `scm::repositories`: the enclosing repository first, nested ones by path, skipped folders, depth and count limits, a folder that is not a repository but holds some, change counts — `crates/evo-worktrees/src/{scm.rs,scm_tests.rs}` — check: `cargo test -p evo-worktrees repositories`
- [x] T26 [FR-17] `scm_repositories(cwd)` command — `apps/desktop/src-tauri/src/{source_control.rs,lib.rs}` — check: `bun run test:shell`; `cargo check`
- [x] T27 [FR-17] The store's repository list and pick (per folder), and the picker in the panel — `apps/desktop/src/{lib/scm.ts,stores/source-control-store.ts,features/source-control/source-control-panel.tsx}` — check: `bun run typecheck`; running app

### Phase 5c — The tree of changes (FR-18)

Done when: the lists show as a tree with folder actions, a list mode stays
available, and a folder discard can be undone.

- [x] T28 [FR-18] Failing test, then `scm::discard` of a folder: every unstaged file under it goes, each kept for Undo; staged changes stay — `crates/evo-worktrees/src/{scm.rs,scm_tests.rs}` — check: `cargo test -p evo-worktrees discard`
- [x] T29 [FR-18] Tests, then `changeTree` (sorting, counts, joined single-child folders) — `apps/desktop/src/lib/{scm.ts,scm.test.ts}` — check: `bun test src/lib/scm.test.ts`
- [x] T30 [FR-18] The tree in the panel: folder rows with counts and actions, open/closed kept, the tree/list switch (`scmListMode`), keyboard over visible rows — `apps/desktop/src/features/source-control/{change-tree.tsx,source-control-panel.tsx}`, `apps/desktop/src/stores/{source-control-store.ts,ui-store.ts}` — check: `bun run typecheck`; running app
- [x] T31 [FR-19] The resizable files / diff split (`scmSplit`), Expand with Esc, the diff header's file actions, the branch row as the repository switch — `apps/desktop/src/features/source-control/source-control-panel.tsx`, `apps/desktop/src/stores/ui-store.ts` — check: `bun run typecheck`; running app
- [x] T32 [FR-20] Right-click menus on file and folder rows — `apps/desktop/src/features/source-control/{change-tree.tsx,source-control-panel.tsx}` — check: `bun run typecheck`; running app

### Phase 6 — Docs and verification

Done when: the UI map, the as-built notes and every verification row are
filled.

- [x] T23 UI map row for Source control and the turn row — `apps/desktop/README.md` — check: review
- [ ] T24 Run every check in section 2 and record the results; as-built notes in the plan — `docs/features/source-control/{tasks.md,plan.md,spec.md}` — check: review

## 2. Verification

| Requirement | Test or check | Result |
| --- | --- | --- |
| FR-1 | Running app: a repository with changes shows both lists and a diff; a plain folder shows the empty state | running app (debug build on a copy of the person's data, a demo repository with a nested one, WebView2 CDP, 2026-11-19): lists and a diff for the demo repository; "Not a Git repository" path not run — passed (2026-11-19), empty state not run |
| FR-2 | Running app: side by side kept after a restart; language colours; folded context | Running app: inline diff with language colours and folded context — passed (2026-11-19); side by side and restart not run |
| FR-3 | Running app: an edit in another editor shows within 2 s, selection kept | Running app: the lists followed the agent's edits within the 1.5 s poll — passed (2026-11-19); an outside editor not tried |
| FR-4 | `cargo test -p evo-worktrees scm`; running app: one hunk of three staged | `cargo test -p evo-worktrees scm`; running app: the Stage button on a hunk's bar staged it — passed (2026-11-19) |
| FR-5 | `cargo test -p evo-worktrees scm`; running app: discard a hunk, Undo brings it back; whole-file discard asks | `cargo test -p evo-worktrees scm`; running app: discarding a new file asked first, removed it, Undo brought it back — passed (2026-11-19) |
| FR-6 | `cargo test -p evo-worktrees scm` (hook, identity, amend); running app: commit and amend | `cargo test -p evo-worktrees scm`; running app: Commit made `fix: change greeting from "Hello" to "Hi"` — passed (2026-11-19); Amend in the app not run |
| FR-7 | `cargo test -p evo-engine commit_message`; running app: a `feat(scope):` repository gets a typed draft | `cargo test -p evo-engine commit_message` (6); running app: in a repository with `feat:` / `chore:` subjects the draft was `fix: change greeting from "Hello" to "Hi"` in 2.5 s (a first try timed out at the provider) — passed (2026-11-19) |
| FR-8 | `bun test src/lib/document-annotations.test.ts`; running app: two comments reach the prompt with file and lines | `bun test src/lib/code-comments.test.ts` (4) passed (2026-11-19); running app not run (the person took over the window) |
| FR-9 | `cargo test -p evo-worktrees checkpoint`; `cargo test -p evo-engine checkpoints` | `cargo test -p evo-worktrees checkpoint`; `cargo test -p evo-engine checkpoints` (16); running app: refs `…/1/{start,end}`, `…/2/{start,end}` made per turn — passed (2026-11-19) |
| FR-10 | `bun test src/features/chat/transcript-checkpoints.test.ts`; running app: "Changed 2 files", each opens its turn diff | `bun test src/features/chat/transcript-checkpoints.test.ts` (5); running app: "Changed 2 files +4 −1" under the turn — passed (2026-11-19); opening a file from it not run |
| FR-11 | `cargo test -p evo-engine revert_turn`; running app: a changed-since file is skipped and named; Undo restores | `cargo test -p evo-engine revert_turn` (4); running app: Revert this turn asked, put greet.ts back and removed farewell.ts, Reverted · Undo restored both — passed (2026-11-19); changed-since in the app not run |
| FR-12 | `cargo test -p evo-engine revert_turn` (note queued); running app: the next turn sees the note | `cargo test -p evo-engine revert_turn`; running app: the next message's run held `<turn-reverted checkpoint="c-2">…</turn-reverted>`, shown as "You reverted an earlier turn's changes" — passed (2026-11-19) |
| FR-13 | `bun run test:shell` (needsConfirm); `cargo test -p evo-engine revert_turn` (busy); running app | `bun run test:shell` (needsConfirm) and `cargo test -p evo-engine revert_turn` (busy) passed (2026-11-19); running app not run |
| FR-14 | `cargo test -p evo-worktrees checkpoint` (prune, drop); running app: a deleted chat leaves no `refs/evo/checkpoints/<id>` | `cargo test -p evo-worktrees checkpoint` (prune, drop) passed (2026-11-19); deleting a chat in the app not run |
| FR-15 | Running app: the summary card and Files open Source control at the file | Running app not run |
| FR-16 | Running app: Ctrl/⌘+Enter commits; the shortcuts list has the group | Running app not run (the shortcuts group is in the list) |
| FR-17 | `cargo test -p evo-worktrees repositories`; running app: a workspace with three repositories lists three and commits in the picked one | `cargo test -p evo-worktrees repositories` (3); running app: demo repository and `packages/web` listed as "This folder · master · 2 changed" and "packages/web · master · 1 changed", picking one showed its change — passed (2026-11-19) |
| FR-18 | `cargo test -p evo-worktrees discard`; `bun test src/lib/scm.test.ts`; running app: a tree with folder counts, staging a folder, the list switch | `cargo test -p evo-worktrees discard` and `bun test src/lib/scm.test.ts` passed (2026-11-19); running app: the tree with joined folders, counts and aligned rows; Discard on the `util` folder row asked "2 files go back…", removed both new files and left `src/greet.ts` alone, and its Undo put them back — passed (2026-11-19); the list switch in the app not run |
| FR-19 | Running app: drag the line, Expand, Esc | passed (2026-11-19): the line dragged from y 532 to 700 kept `scmSplit` 60 (and back to 40); Expand gave the diff the whole tool, Esc brought the files back |
| FR-20 | Running app: right-click a file row and a folder row | passed (2026-11-19): a staged file showed Show changes, Open file, Unstage, Attach to message, Copy path, Copy relative path, Open with default app, Show in Explorer; a staged folder showed Collapse, Unstage 2 files, Copy path, Copy relative path, Show in Explorer |
| NFR-1 | `cargo test -p evo-worktrees checkpoint_large -- --ignored` (≤ 1 s); running app: first-token time unchanged | crate: passed (2026-11-19) at 0.96 s first / 0.90 s next, but runs ranged 0.90–1.06 s on this Windows machine, so the 1 s bound is marginal; running app not run |
| NFR-2 | Running app: performance trace while refreshing a large repository, no frame over 100 ms | not run |
| NFR-3 | `cargo test -p evo-worktrees checkpoint` (status unchanged); review of every write path | crate test passed (2026-11-19); review not done |
| NFR-4 | `cargo test -p evo-worktrees checkpoint_large -- --ignored` (object growth) | passed (2026-11-19): 50 turns of one small edit added 726 KiB to a 4,590 KiB repository |
| NFR-5 | Review against DESIGN.md in light and dark | passed (2026-11-19): light and dark screenshots of the tool, the tree and a diff; achromatic rows, status colours from the tokens |
| NFR-6 | The crate tests with git 2.30 and the latest git | not run |
