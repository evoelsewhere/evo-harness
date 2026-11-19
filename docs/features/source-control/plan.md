# Source control: review, commit and turn checkpoints — Plan

| | |
| --- | --- |
| Spec | [spec.md](spec.md) |
| Tasks | [tasks.md](tasks.md) |
| Updated | 2026-11-19 |
| ADD sections | [4. Process view](../../architecture/README.md#4-process-view), [6.1 A prompt](../../architecture/README.md#61-a-prompt), [7. Data view](../../architecture/README.md#7-data-view), [9. Cross-cutting concepts](../../architecture/README.md#9-cross-cutting-concepts) |
| ADRs | [0024 Evo writes to the person's repository only on a click; checkpoints live under `refs/evo/`](../../architecture/decisions/0024-repository-writes-and-turn-checkpoints.md) · [0010 Engine enforces the approval mode](../../architecture/decisions/0010-engine-enforces-approval-mode.md) · [0018 The shell hosts engines](../../architecture/decisions/0018-the-shell-hosts-engines-windows-attach.md) |

## 1. Context and constraints

What exists:

- **`crates/evo-worktrees`** runs the `git` CLI (`git` / `git_raw` in
  `src/lib.rs`) for managed worktrees, `repo_status`, `ignored_paths` and
  `file_at_head`. `worktree_apply` already applies a reviewed patch with an
  expected hash and a rollback snapshot (`src/snapshot.rs`). It is the only
  crate that talks to git; the shell exposes it as Tauri commands
  (`apps/desktop/src-tauri/src/worktrees.rs`: `git_status`, `git_ignored`,
  `git_file_base`, `worktree_*`). All are read-only except worktree
  apply/discard.
- **Files git** ([files-git](../files-git/plan.md)): tree decorations and
  the editor gutter (`features/files/git-state.ts`, `git-gutter.ts`),
  refreshed from the folder watch of `stores/files-store.ts`.
- **Summary card** (`features/chat/summary-card.tsx`): `GitSection` lists
  changed files, branch, ahead/behind.
- **Worktree review** (`components/ai-elements/worktree-review.tsx`): a
  patch view with `patchRows` (`lib/worktrees.ts`).
- **Engine tool gate**: every tool call passes the `before_tool_call`
  chain; the approval gate is its last link and `approval::classify` sorts
  calls into read / edit / command (`crates/evo-engine/src/approval.rs`,
  ADR 0010).
- **Context notes**: `lane.next_run(message)` puts a message in front of
  the session's next run (used for mesh mail, `crates/evo-engine/src/mesh.rs`).
- **Side questions** (`crates/evo-engine/src/side_questions.rs`): tool-free,
  in-memory side conversations on the chat's model that never write the
  session; Side chat uses them through `side_question`.
- **Annotations** (`stores/annotations-store.ts`,
  `lib/document-annotations.ts`): a per-chat batch in the composer, folded
  into the next message; today only office annotations.
- **Side panel tools** are listed in `features/panels/panel-registry.ts`
  and drawn in `features/panels/right-panel.tsx`.
- **Editor**: CodeMirror 6 (`features/files/code-editor.tsx`) with
  `@codemirror/language-data`.

Constraints: git 2.30+ on PATH (NFR-6; `git restore` needs 2.23). Windows
paths and case-insensitive file systems (the `key` helper in
`evo-worktrees`). Repositories can be large (NFR-1: 50,000 tracked files).

## 2. Architecture check

| Rule | Result | Notes |
| --- | --- | --- |
| The engine owns agent logic; the shell relays and provides native features | ok | Checkpoints, the tool gate, Revert this turn (run state, the note to the agent) and the drafted message are agent concerns: engine. Stage, unstage, discard, commit and diffs are the person's git operations: shell, through `evo-worktrees`. |
| Layers point down; library crates stay free of engine/desktop concerns | ok | `evo-worktrees` gains `checkpoint` and `scm` modules that know git only: no chats, sessions or UI. Chat ids are opaque strings in ref names. |
| Protocol changes update `evo-protocol`, the engine and the desktop together | ok | New commands `list_checkpoints`, `revert_turn`, `draft_commit_message`; new field `checkpoint` on `agent_settled`; all three layers in one task phase (3.3). |
| Models come from models.dev; no hardcoded model facts | ok | The draft uses the chat's current model through side questions; no model named. |
| Real engine in the app; settings and credentials reload live | ok | No new engine setting. The diff layout is a UI preference. |
| Untrusted input stays untrusted; permissions, sandbox and project trust stay in the path | ok | Repository content (diffs, commit subjects) reaches a model only in the tool-free draft, whose output is a suggestion the person edits. Git hooks run on commit as they do from any git client; Evo adds no hook. The checkpoint gate never lets a call through that the approval gate blocked. See 3.7 and ADR 0024. |
| Desktop UI structure and `DESIGN.md` | ok | New `features/source-control/` with its own store; components read selectors, only `engine-store.ts` talks to the engine client; status colours from existing tokens (NFR-5). |
| New trust boundary or persistent store → ADD + ADR | deviation, covered | Evo starts writing to the person's repository (index, commits, working tree on a click) and keeps refs there: ADR 0024; ADD sections 7 and 9 updated with it. |

## 3. Design

### 3.1 Overview

```text
 Source control panel ──invoke──► shell scm commands ──► evo-worktrees::scm ──► git
   (diff, stage, discard,          (src-tauri/src/source_control.rs)
    commit, undo discard)

 chat turn ─► engine: run starts ─► checkpoint "start" (async, evo-worktrees::checkpoint)
                 │  before_tool_call: edit/command waits for it
                 └► run settles ─► checkpoint "end" ─► agent_settled.checkpoint
                                                     └► session entry "checkpoint"
 Revert this turn ─► engine revert_turn ─► restore files ─► note via next_run
 Draft message    ─► engine draft_commit_message ─► side question on the chat's model
```

Two owners. The **shell** does what the person clicks in Source control:
git reads and writes on the chat's folder, with no agent involved. The
**engine** does what belongs to a turn: it checkpoints the folder around
each top-level run, holds the run's first file-changing call until the
checkpoint is taken, reverts a turn and tells the agent so, and drafts commit
messages on the chat's model.

### 3.2 Components

| Layer | Path | Responsibility | Requirements |
| --- | --- | --- | --- |
| crate | `crates/evo-worktrees/src/checkpoint.rs` | `snapshot`, `changes_between`, `restore`, `prune`, `drop_chat` on `refs/evo/checkpoints/…` (3.4) | FR-9 – FR-11, FR-14, NFR-1, NFR-3, NFR-4 |
| crate | `crates/evo-worktrees/src/scm.rs` | status with staged/unstaged split, file diff with hunk ids, stage / unstage / discard (file, all, hunk), keep-before-discard blobs, commit / amend, recent subjects, repository state (merge, rebase, detached, no commits) | FR-1 – FR-6, edge cases |
| engine | `crates/evo-engine/src/checkpoints.rs` | per-run checkpoint task, the `before_tool_call` link, `agent_settled.checkpoint`, the `checkpoint` session entry, `list_checkpoints`, `revert_turn` with its note | FR-9 – FR-13 |
| engine | `crates/evo-engine/src/commit_message.rs` | `draft_commit_message` through side questions | FR-7 |
| engine | `crates/evo-engine/src/modes.rs`, `rpc/protocol.rs` | dispatch of the three commands | — |
| protocol | `crates/evo-protocol/src/protocol.rs` | command and event shapes (3.3) | — |
| desktop shell | `apps/desktop/src-tauri/src/source_control.rs` | Tauri commands over `evo_worktrees::scm`; undo tokens; "is a turn running in this folder" from the supervisor for FR-13; `checkpoints_drop` on chat delete; the overlap record for two chats in one folder | FR-1 – FR-6, FR-13, FR-14 |
| desktop shell | `apps/desktop/src-tauri/src/supervisor.rs` | run spans per folder (from `agent_start` / `agent_settled` it already follows) for the overlap note | edge case "two chats" |
| desktop UI | `apps/desktop/src/features/source-control/` | panel: lists, diff (`@codemirror/merge`), hunk actions, commit box, draft button, line comments, keyboard | FR-1 – FR-8, FR-16 |
| desktop UI | `apps/desktop/src/stores/source-control-store.ts` | per-folder status, selection, refresh from the folder watch, undo notices | FR-3, FR-5 |
| desktop UI | `apps/desktop/src/lib/scm.ts` | typed `invoke` wrappers | — |
| desktop UI | `apps/desktop/src/lib/document-annotations.ts`, `stores/annotations-store.ts` | a `code` annotation kind next to the office one | FR-8 |
| desktop UI | `apps/desktop/src/features/chat/chat-turn.tsx`, `turn-changes.tsx`, `transcript.ts` | "Changed N files" under a reply, Revert this turn, the revert note line | FR-10 – FR-12 |
| desktop UI | `apps/desktop/src/stores/engine-store.ts` | `agent_settled.checkpoint` into the turn, `list_checkpoints` on restore, `revertTurn`, `draftCommitMessage` | FR-7, FR-10 – FR-12 |
| desktop UI | `features/panels/panel-registry.ts`, `features/chat/summary-card.tsx`, `features/files/*`, `features/shell/shortcuts.ts`, `stores/ui-store.ts` | the rail entry, opening at a file, the shortcut group, the layout preference | FR-1, FR-2, FR-15, FR-16 |

### 3.3 Interfaces

**Engine commands** (JSONL, `evo-protocol`; the dispatcher matches `type`):

```jsonc
// → list_checkpoints: the chat's kept checkpoints, newest last
{ "type": "list_checkpoints", "id": 1 }
// ← { "checkpoints": [Checkpoint] }

// → revert_turn: put the turn's files back; `force` reverts files changed since
{ "type": "revert_turn", "id": 2, "checkpoint": "c-17", "force": false, "paths": null }
// ← { "reverted": ["src/a.ts"], "skipped": [{ "path": "src/b.ts", "reason": "changed-since" }],
//     "undo": "c-17-undo" }        // a checkpoint id: revert_turn with it undoes the revert
// error "busy" while a run is active (FR-13)

// → draft_commit_message
{ "type": "draft_commit_message", "id": 3,
  "diff": "<staged diff, at most 64 KiB, cut at a file boundary>",
  "subjects": ["feat(chat): …", "fix(files): …"] }   // the last 20 commit subjects
// ← { "message": "feat(scm): …\n\n…" }
```

```ts
interface Checkpoint {
  id: string;                 // "c-<seq>", per chat
  promptEntryId?: string;     // the session entry of the turn's prompt (to place it in the transcript)
  runId: string;
  startedAt: number;
  files: { path: string; status: "added" | "modified" | "deleted" | "renamed"; from?: string; added: number; removed: number }[];
  complete: boolean;          // false: the start snapshot was late or failed (3.6)
  reverted?: { at: number; undo: string };
}
```

**Event**: `agent_settled` of a top-level run gains `checkpoint: Checkpoint`
when the folder is a repository. The same object is appended as a session
entry `customType: "checkpoint"` (it outlives the process and the window).

**Older peers**: an engine without these commands answers "unknown command";
the window then shows no changed-files row, no Revert and a draft button that
says this engine cannot draft. A window without this feature ignores the new
field and entries.

**Shell commands** (Tauri, `src-tauri/src/source_control.rs`; `cwd` is the
chat's folder):

```ts
scm_status(cwd): ScmStatus | null              // RepoStatus + staged/unstaged split + state: "clean"|"merge"|"rebase"|…
scm_diff(cwd, path, area: "staged"|"unstaged"): FileDiff
  // { old: string|null, new: string|null, hunks: Hunk[], hash, binary, tooLarge, eolOnly }
  // Hunk: { id, oldStart, oldLines, newStart, newLines }  — id = hash of header + lines
scm_diff_between(cwd, from: string, to: string, path): FileDiff  // a turn's diff (commit ids from the checkpoint)
scm_stage(cwd, target: Target) / scm_unstage(cwd, target) : void
scm_discard(cwd, target): { undo: string }      // an undo token for this app run
scm_undo_discard(cwd, undo): void
  // Target: { all: true } | { path } | { path, hunk, expectedHash }  — a changed diff refuses (like worktree_apply)
scm_commit(cwd, { message, amend, confirmRunning }): { commit, branch } | { needsConfirm: { running: string[], pushed: boolean } }
scm_recent_subjects(cwd, n): string[]
checkpoints_drop(cwd, chat): void               // on chat delete
turn_overlap(chat, runId): string[]             // other chats that ran in the folder meanwhile (this app run)
```

**Annotations**: `type Annotation = DocumentAnnotation | CodeAnnotation`,
`CodeAnnotation = { id, kind: "code", file, from, to, quote, instruction, label }`.
Code comments are sent as plain text, after the office block when both are
present:

```text
Comments on the changes:
1. src/a.ts lines 12–18
   > (quoted lines)
   Rename this; it shadows the import.
```

### 3.4 Data and storage

**Checkpoints** are git commits in the chat folder's repository, reachable
only from Evo's refs:

```text
refs/evo/checkpoints/<chat-id>/<seq>/start
refs/evo/checkpoints/<chat-id>/<seq>/end
refs/evo/checkpoints/<chat-id>/<seq>/undo     (only after a revert)
```

A snapshot asks `git --no-optional-locks status --porcelain=v2 -z
--untracked-files=all` which files differ from `HEAD` (staged, unstaged,
renamed, conflicted or new; ignored files are not listed), reads `HEAD` into
a temporary index (`GIT_INDEX_FILE`; `read-tree` leaves its cache tree
complete), runs `update-index --add --remove -z --stdin` with only those
paths, then `git write-tree`, `git commit-tree <tree> -p HEAD` (no parent
before the first commit; Evo's own identity in the environment, so it works
in a repository with none) and `git update-ref`. Only changed files are
hashed and only their folders' trees rebuilt. The real index (status takes
no optional lock), the working tree, branches and stashes are not touched
(NFR-3). Unchanged files reuse existing objects (NFR-4); `git update-ref -d`
frees a checkpoint for git's normal `gc`. Files go through the repository's
filters, as `git add` and `git checkout` do, so a reverted file is written as
a checkout would write it (with `core.autocrlf` too).

Retention (FR-14): after each new checkpoint the engine deletes the chat's
refs beyond the newest 50 turns. Deleting a chat calls `checkpoints_drop`,
which removes `refs/evo/checkpoints/<chat-id>/`. A deleted worktree takes its
refs with it only when the repository goes; refs live in the shared ref store
of the repository.

The checkpoint **metadata** (files, counts, revert state) is the session
entry `checkpoint` in the chat's transcript file (ADR 0007); no `evo.db`
table, no migration.

**Undo of a discard** (FR-5): before a discard the shell stores the current
content of every touched path as a blob (`git hash-object -w`) and keeps
`token → [(path, blob | absent)]` in memory for the app run. Undo writes them
back. Blobs left unreferenced are collected by `gc`.

**Diff layout** (FR-2): `ui-store` `scmDiffLayout: "inline" | "split"`,
persisted with the other UI preferences.

### 3.5 Behaviour

**A turn's checkpoint** (FR-9, NFR-1):

```text
prompt ─► run starts (top-level, main lane, folder in a repository)
   ├─► spawn snapshot(start)                         ── off the run's task
   ├─► model streams …
   └─► tool call ─► before_tool_call chain … approval gate ─► checkpoint link:
             read → go on
             edit / command / unknown → await start snapshot (≤ 10 s), then go on
run settles ─► snapshot(end) ─► changes_between(start, end) ─► checkpoint
   ├─► agent_settled { …, checkpoint }   (an empty `files` list: no row is shown)
   └─► append entry "checkpoint", prune to 50
```

The checkpoint link sits after the approval gate, so a call the person
refuses never waits for it, and a blocked call is never let through by it.
Subagents run in their own harness and get no link (FR-9).

**Revert this turn** (FR-11 – FR-13): the window asks for confirmation
listing the turn's files, then sends `revert_turn`. The engine refuses with
`busy` while a run is active. Otherwise it snapshots the folder as `undo`,
and for each file of the turn compares the working copy with the turn's
`end` version: equal → restore the `start` version (`git restore
--source=<start> --worktree -- <path>`, or delete it when absent in `start`);
different → skip with `changed-since` unless `force`. It updates the
checkpoint entry and queues a note with `lane.next_run`:

```text
<turn-reverted checkpoint="c-17">The person reverted your changes from an earlier turn ("<prompt, first 80 chars>"): src/a.ts, src/b.ts are back as they were before it. Skipped (changed since): src/c.ts.</turn-reverted>
```

The transcript shows it as an event line (`noteKind` "turn-reverted"), not
as a prompt. Undo is `revert_turn` with the `undo` checkpoint: it restores
those paths to the `undo` snapshot.

**Source control panel** (FR-1 – FR-6): the store reads `scm_status` for the
active chat's folder (its worktree, else its workspace) when the panel opens
and on every folder-watch burst (300 ms debounce, as Files) and on
`.git/index` / `HEAD` changes (FR-3). Selecting a file reads `scm_diff`; the
diff shows in `@codemirror/merge` (unified for inline, `MergeView` for side
by side) with the language from `language-data`. Hunk actions are drawn at
the hunks' new-side line ranges from `scm_diff`; each sends the hunk id and
the file diff's hash, and a changed diff is refused and re-read. The
selection and scroll stay when the path is still listed.

**Commit** (FR-6, FR-13): `scm_commit` first asks the supervisor whether a
run is active in the folder (any chat) and, for amend, whether `HEAD` is on
the upstream (`git merge-base --is-ancestor HEAD @{u}`); either returns
`needsConfirm`, and the UI asks, then calls again with `confirmRunning`.
The commit is `git commit -F <tmp message file>` (plus `--amend`), so hooks,
identity and signing are the repository's. Output of a failing hook is
returned as the error.

**Draft** (FR-7): the panel builds the staged diff (`scm_diff` per staged
file, joined and cut to 64 KiB) and `scm_recent_subjects(20)`, and calls
`draftCommitMessage` in `engine-store`, which wakes a deferred engine
(ux-polish FR-14) and sends `draft_commit_message`. The engine asks one
tool-free side question on the chat's model under an owner of its own (side
questions keep one thread per owner, so the draft never joins the Side chat
thread, and the thread is cleared after each draft), with a fixed
instruction: follow
the subjects' style, describe the staged change, no invented facts. The
answer fills the message box; nothing is committed.

**Comments** (FR-8): selecting lines in the diff's new side shows a small
**Comment** button; the comment goes to `annotations-store` for the chat as a
`code` annotation and shows in the composer chip, like office annotations.

**Turn row** (FR-10): `engine-store` puts `agent_settled.checkpoint` on the
current turn; on restore it reads `list_checkpoints` and attaches each to
the turn whose prompt entry id matches. `turn-changes.tsx` shows "Changed N
files" with counts; a file opens Source control in turn mode
(`scm_diff_between(start, end)`); "Revert this turn" and, when reverted,
"Reverted · Undo". When `turn_overlap` names other chats, the row says
"Another chat worked in this folder at the same time".

### 3.6 Errors and edge cases

| Case | What happens |
| --- | --- |
| Folder not in a repository | No checkpoint, no `checkpoint` field; the panel's empty state (FR-1). |
| Start snapshot slower than 10 s or failing | The call goes on; the checkpoint is `complete: false`, its row says changes before the snapshot may be missing, and Revert warns. Logged with the error. |
| `git` missing / failing | Panel: one error with Retry. Engine: no checkpoint, logged once per engine. |
| Engine crash mid-run | No end snapshot: on restart, the engine closes an open `start` with an end snapshot taken then and marks it `complete: false`. |
| Run cancelled (Stop) | Settles as usual: the end snapshot is taken, Revert works. |
| Merge / rebase in progress, conflicts | Status says so; commit disabled; checkpoints still taken (they read the files, not the index state). |
| Binary, > 1 MB, EOL-only, renames | From `scm_diff`: no hunks for binary / large; `eolOnly` shows a line instead of a diff; renames via `git diff -M`. |
| Diff changed between view and click | The shell refuses with "changed since shown"; the panel re-reads. |
| Two chats in one folder | Checkpoints show the folder's changes; `turn_overlap` adds the note (known for runs in this app run only). |
| Chat deleted while its folder is gone | `checkpoints_drop` fails quietly (logged). |
| Revert while a run is active | `busy`; the button is disabled with "Wait for the turn to finish". |
| Missing identity, failing hook | Git's message (hook output) in the commit box; staged files stay staged. |

### 3.7 Security and trust

- **Writes only on a click** (NFR-3, ADR 0024): every write command is a
  shell command called from a person's action in the panel, or an engine
  command (`revert_turn`) called from the Revert button. No agent tool is
  added; the agent cannot call `scm_*` or `revert_turn`.
- **Checkpoints** write objects and `refs/evo/…` only. They are visible to
  `git log --all` and would be pushed by `git push --mirror`; the ADR records
  this. They hold what the working tree held, untracked non-ignored files
  included — the same content git would see on `git add -A`.
- **Draft message**: the diff and subjects are untrusted repository content.
  They go to a tool-free side question; the result is plain text placed in an
  editable box and never run. The chat's model and provider are the ones the
  person already uses for that chat.
- **Hooks**: committing runs the repository's hooks, as any git client does;
  Evo neither adds nor skips hooks (`--no-verify` is never passed).
- **Approval modes** keep their meaning: the checkpoint link never allows a
  call the approval gate blocked.
- **Paths** from the UI are resolved inside the repository root and refused
  when outside it, as `git_file_base` does.

### 3.8 Configuration

| Key | Type | Default | Where | Applies |
| --- | --- | --- | --- | --- |
| `scmDiffLayout` | `"inline" \| "split"` | `"inline"` | `ui-store`, persisted UI preferences | live |

Retention (50 turns), the snapshot wait (10 s) and the draft input cap
(64 KiB) are constants, not settings.

### 3.9 Repositories in a workspace (FR-17)

`evo_worktrees::scm::repositories(dir)` returns the repository `dir` is
in (if any), then every repository found under `dir`: a folder holding a
`.git` folder or file (a submodule or worktree), walked breadth first up to 4
levels, never into `.git`, hidden folders, `node_modules`, `target`, `dist`,
`build`, `.venv` or `vendor`, nor below a repository already found inside
`dir` (its own nested ones are found through its listing), at most 50. Each
entry is `{ root, path, branch, changed }` (`path`: relative to `dir`, `.`
for the enclosing repository; `changed`: the count from one
`git status --porcelain -z`, run for the entries in parallel). The shell
exposes it as `scm_repositories(cwd)`. The Source control store reads it on
open and every 5 s while shown (the status of the picked repository keeps its
1.5 s poll), keeps the picked root per folder (`picked[cwd]`), and runs every
`scm_*` command with the picked root as `cwd` (every command accepts any
folder inside a repository). The panel shows a picker (`Select`) above the
branch row when there are two or more.

### 3.10 The tree of changes (FR-18)

`lib/scm.ts` builds the tree from a list (`changeTree`): folders sorted
before files, each folder with its file count and summed line counts, and
chains of single-child folders joined into one row. The panel draws it with
an indent per level; the open folders are kept per repository in the store
for the app run, `scmListMode: "tree" | "list"` in `ui-store` (saved).
Folder actions use the file targets with the folder's path: `git add -A --
<folder>` and `git restore --staged -- <folder>` act on everything below;
`scm::discard` expands a folder to the unstaged files under it, keeping each
for Undo. The keyboard moves through the visible rows (files and folders).

## 4. Alternatives considered

| Option | Why not |
| --- | --- |
| Checkpoints in a separate shadow repository under `~/.evo` | Copies every object of the project once per repository (large), loses git's dedupe with the project's own objects, and needs its own `gc`. |
| `git stash create` / `stash store` | Leaves untracked files out unless `-u`, which changes the working tree; stashes show in the person's `git stash list` (spec FR-14). |
| File copies (as `snapshot.rs` does for apply) | No dedupe and no cheap "what changed": copying a 50,000-file tree per turn breaks NFR-1. |
| The shell takes checkpoints | The shell cannot hold the agent's first tool call until the snapshot exists; the turn would race it. |
| libgit2 / gitoxide instead of the CLI | A new native dependency (and Windows build work) for little gain; every other git use is the CLI. |
| Revert in the shell | It would need the run state and a way into the chat's context; both are the engine's (agent logic). |
| A structured `git_*` tool for the agent now | Out of scope (spec 2.2, clarification 3). |
| Hand-written diff view instead of `@codemirror/merge` | Side by side, syntax colours and folding come with the editor family the app already ships. |

## 5. As-built notes

- **Desktop (phases 4–5)**: the tool refreshes by polling (1.5 s while
  visible, on focus, when a turn starts or ends; the repository list every
  5 s), as the Files tool does, not from a folder watch. The diff is
  `@codemirror/merge` (`unifiedMergeView` / `MergeView`); the editor look
  moved to `lib/codemirror-style.ts` so Files and Source control share it.
  Hunk actions are block widgets above each hunk's first new line; their
  label gives the new-side line range (git's ranges include context).
  Comments on lines are their own kind (`lib/code-comments.ts`, a `code` list
  in `annotations-store`), composed into the message text before any office
  annotation block, which must stay last to parse. The checkpoint types live
  with the transcript (`features/chat/transcript.ts`), not in
  `engine/protocol.ts`.
- **FR-17**: `scm::repositories` / `scm_repositories` as in 3.9. The commit
  check for a running turn compares the chat folders with the picked
  repository's root exactly, so a turn in the workspace folder does not ask
  before a commit in a nested repository.
- **Fixed in the running app (2026-11-19)**: after switching workspaces the
  diff kept a file of the previous repository (the store now drops a
  selection its repository does not list, also when the status did not
  change, and `select` ignores such paths); the commit box scrolled with the
  lists (it now stays put and only the lists scroll); a file with no
  changes showed an empty editor (it now says so); in a short window the
  diff got no height and its editor drew over the file header (the diff
  now sits in a definite box with a 10rem minimum, the list can shrink, the
  message box stops at 8rem, and a very short panel scrolls as a whole).
- **UI/UX audit (2026-11-19)** and what changed:
  1. Hunk buttons were off screen with long lines (the bar spans the
     content's width): diffs now wrap lines, and the buttons sit next to
     the label.
  2. The files were a flat list: a tree is the default (FR-18), with a
     list switch.
  3. Tree rows did not line up: file rows keep the chevron's place and get
     a file icon, like folders of their level.
  4. A drafted commit message was lost on a window reload: messages are
     kept per repository in `localStorage` (`evo.scm.messages`).
  5. A grey Commit gave no reason: its tooltip says what is missing.
  6. Wholly new lines were tinted twice (line and text): only the line is.
  7. Conflicted files did nothing: a click opens the file.
  8. A clean folder said "Changes 0 / No changes": it now says the folder
     matches the last commit (or that everything is staged).
  9. The repository picker took a row: it is the branch row now.
  10. The files and the diff had fixed shares: a draggable line (kept), an
      Expand for the diff, file actions in the diff's header (FR-19).
  11. Side by side in a panel under 520 px is unreadable: it falls back to
      inline there.
  12. A chat picked up running had no folder, so Source control said "No
      folder": `launch` now fills the chat's folder when none was given.
  13. Two buttons named only "Undo" (the discard notice and a reverted
      turn) were easy to confuse, also for screen readers: each now says
      what it undoes.
  14. Right-click showed the WebView's own menu: file and folder rows have
      their own (FR-20).
  15. "Attach to message" from the menu did nothing after a hot update in
      development: `composerDraft` was a module object, and a reloaded copy
      of `chat-composer.tsx` made a second one the mounted composer did not
      listen to. It is one object per window now (`globalThis`).
  Still open: keyboard over folder rows, and next/previous change in a
  diff.
- **Seen while verifying, not changed**: twice a Copilot request answered
  408 ("Timed out reading request body"); the window showed the error while
  the engine kept the run active (provider retries) until `abort`. The
  checkpoint's end snapshot then followed normally.

- **Snapshot (3.4)**: the first version copied the real index and ran
  `git add -A` on the copy; on a 50,000-file repository that took 2.95 s
  on Windows (`add -A` 1.4 s, `write-tree` 1 s). Reading `HEAD` into the
  temporary index and updating only the paths `status` lists gives the same
  tree in about 0.85 s (status 0.11 s, read-tree 0.11 s, update-index 0.33 s,
  write-tree 0.30 s). The final version also finds the top folder, runs
  the status and reads `HEAD` into the temporary index at the same time
  (each git start costs 70–140 ms on Windows), takes `HEAD` from the
  status's `branch.oid` instead of a `rev-parse`, and skips the checksum and
  fsync of the throw-away index. Measured: 0.90–1.06 s with 200 changed
  files in 50,000 (NFR-1's 1 s is met only just); a turn that changes a few
  files takes far less. The window does not wait for it: the agent's first
  edit does, and only if the model answered faster (phase 2).
- **Shell commands (3.3), as built** (phase 3):
  - `Target` is the crate's tagged shape, `{ kind: "all" }`,
    `{ kind: "file", path }` or `{ kind: "hunk", path, hunk, expected }`
    (`expected` is the file diff's `hash`), not `{ all: true } | { path } |
    { path, hunk, expectedHash }`.
  - `scm_commit` takes `{ cwd, options: { message, amend, confirmRunning } }`
    (`amend` and `confirmRunning` default to false); an empty message is
    refused before git runs.
  - Paths are relative to the repository's top folder (as `scm_status`
    lists them) or absolute inside it (compared without case on Windows and
    macOS); `..`, empty paths and paths outside are refused in the shell
    before git runs, and again by `evo-worktrees`.
  - `scm_diff_between` takes `from` / `to` as an object id or a ref name
    only (no leading `-`, no `..`, no revision expressions), so a value can
    never become a git option.
  - Undo tokens are `discard-<n>`, kept for the last 50 discards of the app
    run, used once, and only in the folder the discard was made in; a
    failed undo keeps its token.
  - `scm_recent_subjects` reads at most 200 subjects.
  - `checkpoints_drop` answers success when the folder is gone or no longer
    in a repository (logged at info); a git failure is logged as a warning
    and returned as the error.
  - `turn_overlap` is a supervisor command (`supervisor.rs`), not one of
    `source_control.rs`.
- **Run spans (T14)**: `agent_start` carries no `runId`, so a span opens at
  `agent_start` and takes its run id from `agent_settled`. A compaction's
  settle (`compact: true`) closes the open span without keeping it. An
  engine that exits or is restarted ends its open span then. Folders match
  exactly (by `folder_key`: forward slashes, no `\\?\` prefix or trailing
  slash, no case on Windows and macOS), not a parent or child folder.
  Times are milliseconds and ends count as overlapping. The last 1,000
  finished spans are kept over the whole app.
- **Engine (phase 2, T6 – T12), as built**:
  - `Checkpoint` (3.3) also carries `seq` (the turn number), `start` and
    `end` (the snapshot commits, for `scm_diff_between`; `end` is absent
    only while a turn is open), and `reverted.paths` (the files the revert
    put back, which an undo restores). A file's `added` / `removed` are
    absent for a binary file. Wire types: `evo_protocol::protocol::{Checkpoint,
    CheckpointFile, CheckpointRevert, RevertTurnRequest, RevertTurnResult,
    DraftCommitMessageRequest, …}`; the three commands are in
    `SUPPORTED_COMMANDS`.
  - A revert appends the checkpoint entry again with `reverted` set (an undo
    appends it without); the newest entry of an id wins. `list_checkpoints`
    returns, oldest first, the checkpoints whose refs are still kept.
  - `revert_turn` with `c-<seq>-undo` answers `undo: null`. A revert that put
    nothing back records nothing and queues no note. An undo made before the
    next run takes the queued `<turn-reverted …>` note back (nothing reaches
    the agent); after that run, the undo queues a second note with the same
    opening tag (`The person undid their revert of an earlier turn …`).
  - Only `prompt` runs get a checkpoint; `resume_pending` and `compact` do
    not (a resumed turn has no prompt of its own; its interrupted turn is
    closed `complete: false` when the engine starts).
  - The chat id in ref names is the engine's session id. "In a repository"
    is checked without starting git (a `.git` in the folder or above), so a
    plain folder costs nothing per turn.
  - The link sits after the approval gate in `session.rs` and waits on one
    engine-wide gate (`checkpoints::global()`). Subagents inherit the tool
    chain: an edit of a subagent waits for its parent's start snapshot, and
    it takes none of its own. The RPC loop gets the chat's `Checkpoints`
    through `RpcProviders::checkpoints` (the host sets it; tests and
    embedded hosts leave it `None` and take none).
  - The end snapshot runs before `agent_settled` is written, so a large
    repository delays the settle by about the snapshot time (NFR-1).
  - **Draft (FR-7)**: a side-chat thread takes questions of at most 20,000
    bytes, less than the 64 KiB draft input, so the draft is one tool-free
    side request outside every thread (`side_questions::ask_once`, on the
    chat's current model): no owner thread exists, so there is nothing to
    clear and the Side chat thread is never touched. The conversation is not
    sent, only the fixed instruction, the subjects and the diff; a fenced
    answer is unwrapped. The engine cuts the diff to 64 KiB at a file
    boundary (else a line) itself too.
  - Tests: `crates/evo-engine/src/checkpoints/tests.rs` (the link, the
    note), `crates/evo-engine/src/modes/checkpoints_tests.rs` (runs over
    RPC against temporary repositories: checkpoints, `revert_turn_*`, the
    draft beside a Side chat thread) and
    `crates/evo-engine/src/commit_message/tests.rs`.
