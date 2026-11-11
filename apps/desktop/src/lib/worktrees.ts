// Managed Git worktrees (src-tauri/src/worktrees.rs, crates/evo-worktrees):
// where a subagent or nested session worked in isolation, and what to do
// with its changes.

import { invoke } from "@tauri-apps/api/core";

export interface WorktreeInfo {
  path: string;
  branch: string;
  location: "repository" | "userData";
  /** Uncommitted changes, or commits beyond where it started. */
  changed: boolean;
}

export interface FileChange {
  path: string;
  status: "added" | "modified" | "deleted";
  added: number | null;
  removed: number | null;
}

export interface WorktreeChanges {
  base: string;
  files: FileChange[];
  added: number;
  removed: number;
  /** For display: cut when long, and bytes that are not UTF-8 show as U+FFFD. */
  patch: string;
  truncated: boolean;
  /** Names the whole patch: pass it to `apply` to apply exactly what was shown. */
  hash: string;
}

export interface WorktreeApplied {
  files: number;
  added: number;
  removed: number;
  /** False: the changes are applied, but the worktree's folder is still in use and stays for now. */
  cleaned: boolean;
}

export interface PatchRow {
  /** The line without its +/− marker. */
  text: string;
  kind: "added" | "removed" | "context" | "meta";
  /** The first line of a file's part of the patch. */
  file: boolean;
}

/**
 * A patch's lines for display. Only a hunk's lines are told by their first
 * character: a file's header (`---`, `+++`, a binary patch's data) has + and −
 * too, and a removed line may itself start with `--`.
 */
export function patchRows(patch: string): PatchRow[] {
  let inHunk = false;
  return patch.split("\n").map((line) => {
    const file = line.startsWith("diff --git ");
    const hunk = line.startsWith("@@");
    if (file) inHunk = false;
    else if (hunk) inHunk = true;
    if (!inHunk || hunk || line.startsWith("\\")) return { text: line, kind: "meta", file };
    if (line.startsWith("+")) return { text: line.slice(1), kind: "added", file };
    if (line.startsWith("-")) return { text: line.slice(1), kind: "removed", file };
    return { text: line, kind: "context", file };
  });
}

/** `apply` refuses with this when the worktree is not what `expected` names any more. */
export function changedSinceReview(error: unknown): boolean {
  return String(error).includes("changed since you opened this");
}

export const worktrees = {
  /** A new worktree of `cwd`'s repository (Settings → Sandbox picks where). */
  create: (cwd: string) => invoke<{ path: string; branch: string; cwd: string; base: string }>("worktree_create", { cwd }),
  /** Empty outside a repository; rejects when git fails inside one. */
  list: (cwd: string) => invoke<WorktreeInfo[]>("worktree_list", { cwd }),
  /** What the worktree changed since it was cut from the folder `cwd` is in. */
  changes: (cwd: string, path: string) => invoke<WorktreeChanges>("worktree_changes", { cwd, path }),
  /**
   * Bring the changes into the folder `cwd` is in (uncommitted, all or
   * nothing) and remove the worktree. `expected` is the `hash` that was
   * reviewed: a worktree that changed since is refused.
   */
  apply: (cwd: string, path: string, expected?: string) =>
    invoke<WorktreeApplied>("worktree_apply", { cwd, path, expected: expected ?? null }),
  /** `cleaned` false: the branch is gone, the folder is still in use and stays for now. */
  discard: (cwd: string, path: string) => invoke<{ cleaned: boolean }>("worktree_discard", { cwd, path }),
};
