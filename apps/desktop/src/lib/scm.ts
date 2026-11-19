// The person's git operations on a chat's folder, through the shell
// (src-tauri/src/source_control.rs → crates/evo-worktrees/src/scm.rs), and the
// pure helpers the Source control tool uses
// (docs/features/source-control/spec.md, FR-1 – FR-7; plan 3.3).

import { invoke } from "@tauri-apps/api/core";

export type ScmFileStatus = "added" | "modified" | "deleted" | "renamed" | "untracked";

export interface ScmFile {
  path: string;
  status: ScmFileStatus;
  /** A renamed file's old path. */
  from?: string;
  /** Lines; `null` for binary files. */
  added: number | null;
  removed: number | null;
}

export interface ScmStatus {
  root: string;
  branch: string | null;
  head: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  state: "normal" | "merge" | "rebase" | "cherry-pick" | "revert";
  staged: ScmFile[];
  unstaged: ScmFile[];
  conflicted: string[];
}

export type Area = "staged" | "unstaged";

/** A Git repository found for a chat's folder (FR-17). */
export interface Repository {
  root: string;
  /** Where it is in the folder, forward slashes; `.` for the one the folder is in. */
  path: string;
  branch: string | null;
  /** Changed, staged and new files. */
  changed: number;
}

export interface Hunk {
  id: string;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
}

export interface FileDiff {
  path: string;
  old: string | null;
  new: string | null;
  hunks: Hunk[];
  hash: string;
  binary: boolean;
  tooLarge: boolean;
  eolOnly: boolean;
}

export type Target = { kind: "all" } | { kind: "file"; path: string } | { kind: "hunk"; path: string; hunk: string; expected: string };

export type CommitResult =
  | { commit: string; branch: string | null }
  | { needsConfirm: { running: string[]; pushed: boolean } };

export const scm = {
  /** The repository `cwd` is in, then those in its subfolders; empty when none (FR-17). */
  repositories: (cwd: string) => invoke<Repository[]>("scm_repositories", { cwd }),
  /** `null` outside a repository. */
  status: (cwd: string) => invoke<ScmStatus | null>("scm_status", { cwd }),
  diff: (cwd: string, path: string, area: Area) => invoke<FileDiff>("scm_diff", { cwd, path, area }),
  /** A turn's diff: `from` and `to` are its checkpoint commits. */
  diffBetween: (cwd: string, from: string, to: string, path: string) =>
    invoke<FileDiff>("scm_diff_between", { cwd, from, to, path }),
  stage: (cwd: string, target: Target) => invoke<void>("scm_stage", { cwd, target }),
  unstage: (cwd: string, target: Target) => invoke<void>("scm_unstage", { cwd, target }),
  /** Resolves with a token that `undoDiscard` takes (this app run). */
  discard: (cwd: string, target: Target) => invoke<{ undo: string }>("scm_discard", { cwd, target }),
  undoDiscard: (cwd: string, undo: string) => invoke<void>("scm_undo_discard", { cwd, undo }),
  commit: (cwd: string, options: { message: string; amend: boolean; confirmRunning: boolean }) =>
    invoke<CommitResult>("scm_commit", { cwd, options }),
  recentSubjects: (cwd: string, n = 20) => invoke<string[]>("scm_recent_subjects", { cwd, n }),
  /** Remove a deleted chat's checkpoints from its folder's repository. */
  dropCheckpoints: (cwd: string, chat: string) => invoke<void>("checkpoints_drop", { cwd, chat }),
  /** Other chats that ran in the same folder while `runId` ran (this app run). */
  turnOverlap: (chat: string, runId: string) => invoke<string[]>("turn_overlap", { chat, runId }),
};

/** The draft input's limit: the staged diff is cut at a file boundary below it. */
export const DRAFT_DIFF_MAX = 64 * 1024;

/**
 * Join file diffs for the commit-message draft, keeping whole files while
 * they fit in `max` characters; a cut says how many files were left out.
 */
export function joinForDraft(parts: { path: string; text: string }[], max = DRAFT_DIFF_MAX): string {
  const kept: string[] = [];
  let size = 0;
  let left = 0;
  for (const part of parts) {
    if (size + part.text.length > max) {
      left++;
      continue;
    }
    kept.push(part.text);
    size += part.text.length;
  }
  const joined = kept.join("\n");
  return left > 0 ? `${joined}\n[${left} more ${left === 1 ? "file" : "files"} not shown]` : joined;
}

/** A unified diff of one file from its two texts, for the draft (no context folding). */
export function patchText(diff: FileDiff): string {
  if (diff.binary || diff.tooLarge) return `--- ${diff.path}\n(binary or large file changed)`;
  const old = diff.old ?? "";
  const neu = diff.new ?? "";
  const oldLines = old.split("\n");
  const newLines = neu.split("\n");
  const out = [`--- a/${diff.path}`, `+++ b/${diff.path}`];
  for (const hunk of diff.hunks) {
    out.push(`@@ -${hunk.oldStart},${hunk.oldLines} +${hunk.newStart},${hunk.newLines} @@`);
    for (const line of oldLines.slice(Math.max(0, hunk.oldStart - 1), hunk.oldStart - 1 + hunk.oldLines)) out.push(`-${line}`);
    for (const line of newLines.slice(Math.max(0, hunk.newStart - 1), hunk.newStart - 1 + hunk.newLines)) out.push(`+${line}`);
  }
  return out.join("\n");
}

/** The selection after a refresh: the same file when it is still listed, else none. */
export function keptSelection(
  selected: { path: string; area: Area } | null,
  status: Pick<ScmStatus, "staged" | "unstaged"> | null,
): { path: string; area: Area } | null {
  if (!selected || !status) return null;
  const list = selected.area === "staged" ? status.staged : status.unstaged;
  if (list.some((file) => file.path === selected.path)) return selected;
  // Staged or unstaged meanwhile: follow it to the other list.
  const other = selected.area === "staged" ? status.unstaged : status.staged;
  return other.some((file) => file.path === selected.path)
    ? { path: selected.path, area: selected.area === "staged" ? "unstaged" : "staged" }
    : null;
}

/** The hunk whose new-side lines hold `line` (1-based); the nearest one above otherwise. */
export function hunkAt(hunks: Hunk[], line: number): Hunk | null {
  let found: Hunk | null = null;
  for (const hunk of hunks) {
    const end = hunk.newStart + Math.max(hunk.newLines, 1) - 1;
    if (line >= hunk.newStart && line <= end) return hunk;
    if (hunk.newStart <= line) found = hunk;
  }
  return found;
}

/** The letter and tone a file's status shows with. */
export function statusMark(status: ScmFileStatus): { letter: string; tone: "added" | "modified" | "deleted" } {
  switch (status) {
    case "added":
    case "untracked":
      return { letter: status === "untracked" ? "U" : "A", tone: "added" };
    case "deleted":
      return { letter: "D", tone: "deleted" };
    case "renamed":
      return { letter: "R", tone: "modified" };
    default:
      return { letter: "M", tone: "modified" };
  }
}

/** A signature of a status, to skip re-rendering when nothing changed. */
export function statusSignature(status: ScmStatus | null): string {
  if (!status) return "";
  const files = (list: ScmFile[]) => list.map((f) => `${f.status}:${f.path}:${f.added}:${f.removed}`).join("|");
  return [status.root, status.branch, status.head, status.ahead, status.behind, status.state, files(status.staged), files(status.unstaged), status.conflicted.join("|")].join("\n");
}

// ── The tree of changes (FR-18) ──────────────────────────────────────────────

export interface TreeFolder {
  kind: "folder";
  /** Its path in the repository (a joined chain names its deepest folder). */
  path: string;
  /** What the row shows: one folder, or a chain of single-child folders (`apps/desktop/src`). */
  name: string;
  depth: number;
  /** Changed files under it, and their lines. */
  count: number;
  added: number;
  removed: number;
  children: TreeNode[];
}

export interface TreeFile {
  kind: "file";
  path: string;
  name: string;
  depth: number;
  file: ScmFile;
}

export type TreeNode = TreeFolder | TreeFile;

const byName = (a: TreeNode, b: TreeNode) =>
  a.kind !== b.kind ? (a.kind === "folder" ? -1 : 1) : a.name.localeCompare(b.name, undefined, { sensitivity: "base" });

/**
 * The files as a tree: folders before files, each folder with the number of
 * changed files under it and their lines, chains of folders that hold only
 * one folder joined into one row.
 */
export function changeTree(files: ScmFile[]): TreeNode[] {
  interface Draft {
    folders: Map<string, Draft>;
    files: ScmFile[];
  }
  const root: Draft = { folders: new Map(), files: [] };
  for (const file of files) {
    const parts = file.path.split("/");
    let node = root;
    for (const part of parts.slice(0, -1)) {
      let next = node.folders.get(part);
      if (!next) {
        next = { folders: new Map(), files: [] };
        node.folders.set(part, next);
      }
      node = next;
    }
    node.files.push(file);
  }
  const build = (draft: Draft, prefix: string, depth: number): TreeNode[] => {
    const nodes: TreeNode[] = [];
    for (const [segment, child] of draft.folders) {
      // Join a chain of folders that each hold one folder and no files.
      let name = segment;
      let path = prefix ? `${prefix}/${segment}` : segment;
      let current = child;
      while (current.files.length === 0 && current.folders.size === 1) {
        const [nextName, next] = [...current.folders][0];
        name = `${name}/${nextName}`;
        path = `${path}/${nextName}`;
        current = next;
      }
      const children = build(current, path, depth + 1);
      const files = children.flatMap(function leaves(node): ScmFile[] {
        return node.kind === "file" ? [node.file] : node.children.flatMap(leaves);
      });
      nodes.push({
        kind: "folder",
        path,
        name,
        depth,
        count: files.length,
        added: files.reduce((sum, f) => sum + (f.added ?? 0), 0),
        removed: files.reduce((sum, f) => sum + (f.removed ?? 0), 0),
        children,
      });
    }
    for (const file of draft.files) {
      nodes.push({ kind: "file", path: file.path, name: file.path.split("/").pop() ?? file.path, depth, file });
    }
    return nodes.sort(byName);
  };
  return build(root, "", 0);
}

/** The rows on screen: every node whose folders are all open (`closed` holds closed folders' paths). */
export function visibleRows(nodes: TreeNode[], closed: ReadonlySet<string>): TreeNode[] {
  const rows: TreeNode[] = [];
  const walk = (list: TreeNode[]) => {
    for (const node of list) {
      rows.push(node);
      if (node.kind === "folder" && !closed.has(node.path)) walk(node.children);
    }
  };
  walk(nodes);
  return rows;
}
