# Source control: review, commit and turn checkpoints — Spec

| | |
| --- | --- |
| Status | In progress |
| Area | `evo-worktrees`, desktop shell, desktop UI, engine |
| Created | 2026-11-19 |
| Updated | 2026-11-19 |
| Plan | [plan.md](plan.md) |
| Tasks | [tasks.md](tasks.md) |
| Related | [Git in the Files tool](../files-git/spec.md) · [Subagents and nested sessions](../subagents-and-nested-sessions/spec.md) (worktree review) · [Approval modes](../approval-modes/spec.md) · [Office v3 parity](../office-v3-parity/spec.md) (annotations) |

## 1. Summary

The agent changes files in the person's repository, but Evo only shows that
read-only: changed files in the Files tree, changed lines in the editor and a
count in the chat summary. To keep, drop or commit what the agent did, the
person leaves Evo for another tool or asks the agent to run `git`. This
feature adds a **Source control** tool that reviews the folder's changes as
diffs, stages, discards and commits them with a drafted message, and sends a
comment on any changed line to the chat. It also records a **checkpoint per
turn**, so the person sees which files a turn changed and can revert that
turn. It is the first of four phases (section 2.2); branches and sync,
history and pull requests come in later specs.

## 2. Scope

### 2.1 Goals

- One place to review every change in the folder a chat works in, as diffs.
- Stage, unstage and discard whole files and single hunks; commit what is
  staged, with a message the agent drafts and the person edits.
- Comment on changed lines and hand the comments to the agent, without
  typing file names and line numbers.
- Per turn: which files it changed, their diff, and **Revert this turn**.
- Safe by default: nothing is lost by a click, and the agent's work in
  progress is never committed or reverted under it.

### 2.2 Non-goals

- **Branches and sync** (switch, create, fetch, pull, push, publish): phase
  2, its own spec.
- **History** (log, file history, blame) and **merge conflict** resolution:
  phase 3.
- **Pull requests** and hosting providers (GitHub, GitLab): phase 4, possibly
  a plugin.
- **Git tools for the agent** (a structured `git_commit` and the like): the
  agent keeps using shell commands under the approval modes. See
  clarification 3.
- **Folders that are not repositories**: no checkpoints, no Source control
  content (an empty state says so). Files keeps working as today.
- **Turn checkpoints in nested repositories**: a turn's checkpoint covers
  the repository of the chat's folder only. Source control does list and
  work on nested repositories (FR-17), but a turn's changes inside one are
  neither listed under the reply nor reverted.
- **Worktree review** of subagents and nested sessions stays as it is; this
  feature does not replace Apply / Discard.

## 3. User scenarios

### US-1 Review what changed (priority P1)

A person whose agent just edited several files wants to read every change as
a diff, in Evo, so that they can judge the work without another tool.

Acceptance scenarios:

1. **Given** a chat in a repository with changed, new and deleted files,
   **when** the person opens Source control, **then** it lists them under
   Changes (and Staged, if any) with their status and line counts, and
   selecting one shows its diff.
2. **Given** a diff on screen, **when** the agent changes that file again,
   **then** the list and the diff follow within 2 s, keeping the person's
   scroll position and selection where the file still exists.
3. **Given** the person prefers it, **when** they switch the diff layout,
   **then** it shows inline or side by side, and the choice is kept.

### US-2 Keep part of the work and commit it (priority P1)

A person wants to keep some of the agent's changes, drop others and commit
the kept ones with a good message, so that the repository's history stays
clean.

Acceptance scenarios:

1. **Given** a changed file with three hunks, **when** the person stages one
   hunk and discards another, **then** the staged hunk shows under Staged,
   the discarded one is gone from the file, and the third stays under
   Changes.
2. **Given** staged changes, **when** the person asks for a message,
   **then** a drafted message in the style of the repository's recent
   commits fills the message box within 15 s, and the person can edit it.
3. **Given** a message and staged changes, **when** the person commits,
   **then** the commit is made with the repository's own identity and hooks,
   the staged list empties, and the summary card's branch shows the new
   commit.
4. **Given** a discard the person regrets, **when** they click Undo on the
   notice that follows it, **then** the discarded text comes back.

### US-3 Tell the agent what to fix, on the lines (priority P1)

A person reading a diff wants to point at lines and say what is wrong, so
that the agent fixes exactly that.

Acceptance scenarios:

1. **Given** a diff, **when** the person selects lines and writes a comment,
   **then** the comment joins the composer's pending batch, naming the file
   and lines; several comments make one batch.
2. **Given** a pending batch, **when** the person sends the next message
   (or clicks Send now), **then** the agent receives the comments with the
   quoted lines, and the batch clears.

### US-4 See and undo what one turn did (priority P1)

A person wants to know which files a given turn changed and, when that turn
went wrong, put those files back as they were before it, without losing the
turns before it.

Acceptance scenarios:

1. **Given** a turn that edited two files, **when** it finishes, **then**
   the turn shows "Changed 2 files" with their line counts, and each opens
   its diff for that turn alone.
2. **Given** that turn, **when** the person chooses Revert this turn and
   confirms, **then** both files are as they were when the turn started,
   and a note in the chat says the turn was reverted, so the agent knows.
3. **Given** a later turn or the person changed one of those files again,
   **when** the person reverts the earlier turn, **then** that file is
   listed as changed since and is skipped unless they choose to revert it
   anyway.

### US-5 Work safely while the agent runs (priority P2)

A person reviewing or committing while the agent is still working wants Evo
to keep them from committing or reverting half-done work.

Acceptance scenarios:

1. **Given** a turn running in the folder, **when** the person commits,
   **then** Evo says the agent is still working there and asks before it
   commits.
2. **Given** a turn running, **when** the person tries to revert any turn of
   that chat, **then** Revert waits until the turn has finished.

### Edge cases

- **No commits yet**: everything is new; commit makes the first commit.
- **Detached HEAD**: commit works and says the commit is on no branch.
- **Missing identity** (`user.name` / `user.email` unset): commit is refused
  with the reason and how to set it; nothing is staged or lost.
- **A hook fails** (pre-commit, commit-msg): the commit is not made, the
  hook's output shows, staged changes stay staged.
- **Binary and large files** (over 1 MB): listed with their status; no diff
  and no hunk actions, only whole-file stage, unstage and discard.
- **Renames**: shown as one renamed entry when git detects them.
- **Line endings only**: a file whose only change is CRLF vs LF says so
  instead of showing every line changed.
- **Merge or rebase in progress**: Source control says so, lists conflicted
  files and offers no commit until they are resolved (resolution itself is
  phase 3).
- **Two chats in one folder**: a turn's checkpoint shows whatever changed in
  the folder during that turn, also by the other chat or the person; the turn
  says when another chat ran in the folder at the same time.
- **A chat in a worktree**: Source control shows the worktree, and the
  person may commit there, on the worktree's branch. Apply still brings
  every change since the worktree started, committed or not, into the
  workspace uncommitted.
- **Ignored files**: never listed and never in a checkpoint.
- **`git` missing or failing**: Source control shows the error once, with a
  retry; nothing else in Evo breaks.
- **The folder stops being a repository** (`.git` removed): Source control
  shows its empty state; existing checkpoints of that folder are gone with it.

## 4. Requirements

### 4.1 Functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| FR-1 | A **Source control** tool of its own in the side panel, next to Files in the rail, shows the repository of the chat's folder: branch, Staged and Changes lists with status letters and line counts, and the selected file's diff. Outside a repository it shows an empty state. | Open it in a repository with changes: both lists and a diff show; in a plain folder: the empty state. |
| FR-2 | The diff shows inline or side by side, with syntax colours of the file's language and the unchanged context folded; the layout choice is kept across restarts. | Switch to side by side, restart: still side by side. |
| FR-3 | The lists and the diff refresh by themselves when files or the index change (by the agent, the person or another program), within 2 s, keeping selection and scroll. | Edit the shown file in another editor: the diff follows within 2 s. |
| FR-4 | Stage and unstage a whole file, all files, or one hunk. | Stage one hunk of three: one hunk under Staged, two under Changes. |
| FR-5 | Discard a whole file, all changes, or one hunk. Discarding all changes or a whole file asks first. Every discard can be undone once from the notice that follows it, for at least the rest of the session. | Discard a hunk, Undo: the hunk is back. |
| FR-6 | Commit what is staged with a message; **Amend** replaces the last commit (asks first when that commit is already on the upstream). The repository's identity, signing setting and hooks apply; a hook's output shows when it fails. | Commit with a failing pre-commit hook: no commit, its output shows, the files stay staged. |
| FR-7 | **Draft message** writes a commit message for the staged changes in the style of the repository's recent commits (for example Conventional Commits when they use it). It uses the chat's model in a side request that adds nothing to the chat's transcript or context (as Side chat does); with no engine running for the chat, the default model. | In a repository using `feat(scope):` subjects, the draft starts with a type and scope. |
| FR-8 | Selecting lines in a diff offers **Comment**: the comment joins the chat's pending annotations in the composer with the file, the line range and the quoted lines, as document annotations do. | Comment on two ranges, send: the agent's prompt holds both, with file and lines. |
| FR-9 | Every top-level turn of a chat in a repository records a **checkpoint**: the folder's tracked and untracked (not ignored) files as they were when the turn started. It is taken before the agent's first change in that turn. Subagents and nested sessions take none: a subagent in the chat's folder is part of its parent's turn, one in a worktree is reviewed with Apply / Discard. | Start a turn that edits a file: a checkpoint for that turn exists, holding the file's old text. |
| FR-10 | A finished turn that changed files shows them under the reply ("Changed 2 files"), with line counts; each opens the diff between the turn's start and its end. | A turn that edits two files lists both; one opens its diff for that turn only. |
| FR-11 | **Revert this turn** puts every file the turn changed back to its checkpoint, after a confirmation listing them. Files changed again after the turn are skipped and listed, unless the person picks **Revert anyway**. A revert is itself undoable once. It changes files only: the turn stays in the chat (rewinding the chat is Edit message or Fork from here). | Revert a turn whose file a later turn changed: that file is skipped and named. |
| FR-12 | After a revert, the chat gets a note saying which turn was reverted and which files, so the agent's next turn does not assume its changes are still there. | The next prompt's context includes the revert note. |
| FR-13 | While a turn runs in the folder, Commit and Amend ask first, and Revert is unavailable for that chat until the turn ends. | Click Commit during a run: a question names the running chat. |
| FR-14 | Checkpoints are kept for a chat's last 50 turns and removed when the chat is deleted. They never show up as the person's branches, tags or stashes. | Delete a chat: its checkpoints are gone; `git branch` and `git stash list` never list them. |
| FR-15 | Files' existing git decorations and the summary card's Git section open Source control at the file (or the list) they show. | Click a changed file in the summary card: Source control opens on its diff. |
| FR-16 | Keyboard: move through files, stage / unstage / discard the selected file or hunk, and commit (Ctrl/⌘+Enter in the message box); listed in the shortcuts list. | Commit with Ctrl+Enter; the shortcuts list shows the Source control group. |
| FR-17 | Source control finds the Git repositories of the chat's folder: the repository the folder is in, and repositories in its subfolders (their own `.git`, including submodules), up to 4 levels down, skipping dependency and build folders (`node_modules`, `target`, `dist`, `build`, `.venv`, `vendor`) and hidden folders. With more than one, a repository picker at the top lists each by its path in the folder (the folder's own first), its branch and its number of changes; the lists, diffs, actions and commit apply to the picked one, which is kept per folder. With one, no picker shows. A folder that is not itself in a repository but holds some shows them instead of "Not a Git repository". The list is read again when the tool opens and every few seconds while it shows. | A workspace with a repository at its top and two in subfolders shows three entries; picking one shows its changes and commits there. |
| FR-18 | The Staged and Changes lists show as a tree of folders (the default) or as a flat list, switched from the tool's header and kept across restarts. A folder row shows how many changed files it holds and their added and removed lines, opens and closes (kept while the app runs), and offers Stage, Unstage and Discard for everything under it (Discard asks first, with the number of files, and can be undone like any discard). A chain of folders with one child each shows as one row (`apps/desktop/src`). | A repository with changes in three folders shows a tree; staging a folder stages every file under it; switching to the list shows the same files flat. |
| FR-19 | The files and the diff share the tool's height, the line between them dragged to change it (kept across restarts); the diff can take the whole height (the commit box and the files hide; Esc or the same button brings them back). The diff's header names the file, says whether the change is staged, and offers Stage or Unstage, Discard and Open for the whole file. | Drag the line: the files get more room and keep it after a restart; Expand: the diff fills the tool, Esc returns. |
| FR-20 | Right-clicking a file row opens its menu: Show changes, Open file, Stage or Unstage, Discard changes (unstaged only), Attach to message, Copy path, Copy relative path, Open with default app, Show in the file manager. A folder row's menu: Expand or Collapse, Stage or Unstage its files, Discard its files (unstaged only), Copy path, Copy relative path, Show in the file manager. | Right-click a file and a folder: each menu shows its items, not the WebView's own menu. |

### 4.2 Non-functional

| ID | Requirement | Acceptance check |
| --- | --- | --- |
| NFR-1 | Taking a checkpoint never makes the turn start later: the prompt goes at once, and the agent's first change waits for the checkpoint only if it is not done yet. In a repository of 50,000 tracked files with 200 changed, a checkpoint takes under 1 s. | Measure on such a repository: first token time is unchanged; checkpoint ≤ 1 s. |
| NFR-2 | Git runs off the UI thread; the panel stays responsive (no frame over 100 ms) while a status, diff or checkpoint runs. | Trace while refreshing a large repository. |
| NFR-3 | No write to the repository happens without a person's click: stage, unstage, discard, commit, amend, revert. Checkpoints write only objects and Evo's own references, never the working tree, the index, branches or stashes. | Review of the write paths; a turn's checkpoint leaves `git status` unchanged. |
| NFR-4 | Checkpoints stay small: they reuse the repository's objects, and removed checkpoints are freed by git's normal cleanup. | 50 turns of small edits grow `.git` by about the size of the changed files, not of the tree. |
| NFR-5 | DESIGN.md holds: status colours from the existing tokens, one accent, weights 400/500/600. | Review in light and dark. |
| NFR-6 | Works with the `git` on PATH from version 2.30 on, on Windows, macOS and Linux; no new runtime dependency. | Run the checks with git 2.30 and the latest. |

## 5. Success criteria

- SC-1 A person reviews, partly discards and commits an agent's change
  without leaving Evo or typing a git command.
- SC-2 A bad turn is undone in two clicks, and the earlier turns' changes
  stay.
- SC-3 A comment on a diff line reaches the agent with the right file and
  lines every time.
- SC-4 No click in Source control loses work that cannot be brought back
  (Undo or a checkpoint).

## 6. Clarifications

### 6.1 Open

n/a — every question is answered (6.2).

### 6.2 Resolved

All proposed answers accepted by the person on 2026-11-19 ("Accept all").

- 2026-11-19 — Q: A Source control tool of its own, or part of Files? → A: its own tool, next to Files in the rail; Files keeps its tree and gutter markers and opens Source control at a file (FR-1, FR-15).
- 2026-11-19 — Q: Turn checkpoints in this phase? → A: yes (US-4, FR-9 – FR-14).
- 2026-11-19 — Q: Structured git tools for the agent now? → A: no; shell commands under the approval modes; structured tools with branches and sync in phase 2, where `push` always asks (2.2).
- 2026-11-19 — Q: Which model drafts the commit message? → A: the chat's model in a side request that adds nothing to the chat; the default model when the chat has no engine (FR-7).
- 2026-11-19 — Q: Commit in a chat's worktree? → A: yes, on the worktree's branch; Apply is unchanged (edge cases).
- 2026-11-19 — Q: Checkpoints for subagents and nested sessions? → A: no, top-level turns only (FR-9).
- 2026-11-19 — Q: Does Revert rewind the conversation? → A: no, files only, with a note to the agent (FR-11, FR-12).
- 2026-11-19 — Q: How many checkpoints, for how long? → A: a chat's last 50 turns, until the chat is deleted (FR-14).

## 7. History

- 2026-11-19 — Draft, from the person's request to enrich source control
  ("làm giàu tính năng source control"); phase 1 of 4 (review, commit, turn
  checkpoints).
- 2026-11-19 — Gate A: the person accepted every proposed answer; the
  answers are in FR-1, FR-7, FR-9, FR-11 and the edge cases.
- 2026-11-19 — Gate B: the person accepted the plan and ADR 0024; status
  `Accepted`.
- 2026-11-19 — FR-17 added at the person's request: a workspace's nested
  repositories are found and offered in Source control ("1 workspace phải
  detect ra project git và hiển thị ở sidepanel"); the non-goal on several
  repositories in one folder now covers turn checkpoints only.
- 2026-11-19 — FR-18 added at the person's request: the changed files as a
  tree ("tôi chưa thấy source control tree"), with folder actions.
- 2026-11-19 — FR-19 added at the person's request after a UI/UX audit:
  a draggable line between the files and the diff, the diff expanded to the
  whole tool, and file actions in the diff's header ("thêm khả năng kéo mở
  rộng, design lại layout cho hợp lý hơn").
- 2026-11-19 — FR-20 added at the person's request: right-click menus on
  file and folder rows ("chuột phải ko có gì").
