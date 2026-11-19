# 0024. Evo writes to the person's repository only on a click; turn checkpoints live under `refs/evo/`

| | |
| --- | --- |
| Status | Accepted |
| Date | 2026-11-19 |
| ADD sections | [4. Process view](../README.md#4-process-view), [7. Data view](../README.md#7-data-view), [9. Cross-cutting concepts](../README.md#9-cross-cutting-concepts) |
| Related | [Source control spec](../../features/source-control/spec.md), [plan](../../features/source-control/plan.md) · [0010](0010-engine-enforces-approval-mode.md) · [0018](0018-the-shell-hosts-engines-windows-attach.md) |

## Context

Until now Evo itself only read the person's repository: status, ignored
paths, a file's committed text. The only write was applying or discarding a
managed worktree after a review. The agent changes the repository through
its tools under the approval mode, but Evo's own code never staged,
committed or restored anything.

The source control feature needs Evo to write: stage, unstage, discard and
commit for the person, and restore files when a turn is reverted. It also
needs to remember the folder's state at the start and end of every turn, in
repositories that can hold 50,000 files, without slowing the turn and
without the person seeing new branches or stashes.

## Decision

1. **Writes to the repository happen only on the person's click.** The shell
   runs stage, unstage, discard and commit (through `evo-worktrees`) when the
   Source control panel asks; the engine restores files only for Revert this
   turn. No agent tool calls these paths; the agent keeps changing files
   through its own tools under the approval mode (ADR 0010).
2. **Turn checkpoints are git commits under `refs/evo/checkpoints/<chat>/…`**
   in the chat folder's own repository, made with a temporary index
   (`GIT_INDEX_FILE`), `write-tree`, `commit-tree` and `update-ref`. They
   never touch the working tree, the real index, branches, tags or stashes.
3. **The engine takes them**, because it can hold a run's first
   file-changing tool call until the start snapshot exists: a link at the end
   of the `before_tool_call` chain, after the approval gate.
4. Every git operation stays on the `git` CLI, as before; no git library is
   added.

## Consequences

- The person can review, commit and undo the agent's work without leaving
  Evo, and every turn can be reverted.
- Checkpoints share the repository's objects: cheap to take, freed by git's
  `gc` once Evo deletes their refs (the last 50 turns per chat; all on chat
  delete).
- `refs/evo/…` is visible to `git log --all` and `git for-each-ref`, and
  `git push --mirror` would push it. Contributors must not add pushes of
  `refs/evo/`; a future cleanup command may be needed for repositories whose
  chats were deleted outside Evo.
- Committing runs the repository's hooks, as any git client does; Evo never
  passes `--no-verify`.
- New code that writes to the repository goes through `evo-worktrees` and is
  reached only from a person's action (a shell command behind a click, or an
  engine command the UI sends); an agent-facing git tool needs its own spec
  and its own approval behaviour (planned with branches and sync).

## Alternatives considered

| Option | Why not |
| --- | --- |
| A shadow repository under `~/.evo` | Duplicates the project's objects per repository and needs its own `gc`. |
| `git stash create` / `stash store` | Untracked files need `-u`, which changes the working tree; stashes show in the person's stash list. |
| Copying files per turn | No dedupe; too slow for large repositories. |
| The shell takes checkpoints | It cannot hold the agent's first tool call until the snapshot exists. |
| libgit2 or gitoxide | A new native dependency for operations the CLI already does. |
