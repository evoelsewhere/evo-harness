// docs/features/source-control/spec.md (FR-3, FR-7; T15).
import { describe, expect, test } from "bun:test";
import { changeTree, hunkAt, joinForDraft, keptSelection, patchText, statusMark, statusSignature, visibleRows, type FileDiff, type ScmStatus } from "./scm";

const status = (staged: string[], unstaged: string[]): ScmStatus => ({
  root: "/r",
  branch: "main",
  head: "abc1234",
  upstream: null,
  ahead: 0,
  behind: 0,
  state: "normal",
  staged: staged.map((path) => ({ path, status: "modified", added: 1, removed: 0 })),
  unstaged: unstaged.map((path) => ({ path, status: "modified", added: 1, removed: 0 })),
  conflicted: [],
});

describe("source control helpers", () => {
  test("a selection stays on its file, follows it to the other list, or goes", () => {
    const selected = { path: "a.ts", area: "unstaged" as const };
    expect(keptSelection(selected, status([], ["a.ts", "b.ts"]))).toEqual(selected);
    expect(keptSelection(selected, status(["a.ts"], []))).toEqual({ path: "a.ts", area: "staged" });
    expect(keptSelection(selected, status([], ["b.ts"]))).toBeNull();
    expect(keptSelection(null, status([], ["a.ts"]))).toBeNull();
  });

  test("the hunk at a line, or the nearest above", () => {
    const hunks = [
      { id: "1", oldStart: 1, oldLines: 5, newStart: 1, newLines: 5 },
      { id: "2", oldStart: 22, oldLines: 7, newStart: 22, newLines: 7 },
      { id: "3", oldStart: 40, oldLines: 3, newStart: 40, newLines: 0 },
    ];
    expect(hunkAt(hunks, 3)?.id).toBe("1");
    expect(hunkAt(hunks, 28)?.id).toBe("2");
    expect(hunkAt(hunks, 30)?.id).toBe("2");
    expect(hunkAt(hunks, 40)?.id).toBe("3");
    expect(hunkAt([], 3)).toBeNull();
  });

  test("the draft input keeps whole files under the limit", () => {
    const parts = [
      { path: "a", text: "a".repeat(40) },
      { path: "b", text: "b".repeat(40) },
      { path: "c", text: "c".repeat(10) },
    ];
    const joined = joinForDraft(parts, 60);
    expect(joined).toContain("a".repeat(40));
    expect(joined).not.toContain("b");
    expect(joined).toContain("c".repeat(10));
    expect(joined).toContain("[1 more file not shown]");
    expect(joinForDraft(parts, 1000)).not.toContain("not shown");
  });

  test("a patch for the draft shows the hunks' lines", () => {
    const diff: FileDiff = {
      path: "x.txt",
      old: "one\ntwo\nthree\n",
      new: "one\nTWO\nthree\n",
      hunks: [{ id: "h", oldStart: 2, oldLines: 1, newStart: 2, newLines: 1 }],
      hash: "x",
      binary: false,
      tooLarge: false,
      eolOnly: false,
    };
    expect(patchText(diff)).toBe("--- a/x.txt\n+++ b/x.txt\n@@ -2,1 +2,1 @@\n-two\n+TWO");
    expect(patchText({ ...diff, binary: true })).toContain("binary or large");
  });

  test("status letters and signatures", () => {
    expect(statusMark("untracked")).toEqual({ letter: "U", tone: "added" });
    expect(statusMark("deleted").letter).toBe("D");
    expect(statusSignature(status(["a"], []))).toBe(statusSignature(status(["a"], [])));
    expect(statusSignature(status(["a"], []))).not.toBe(statusSignature(status([], ["a"])));
    expect(statusSignature(null)).toBe("");
  });
});

describe("the tree of changes", () => {
  const file = (path: string, added = 1, removed = 0) => ({ path, status: "modified" as const, added, removed });
  const tree = changeTree([
    file("apps/desktop/src/a.ts", 2, 1),
    file("apps/desktop/src/lib/b.ts", 3),
    file("crates/x/src/lib.rs", 5, 2),
    file("README.md"),
    file("AGENTS.md"),
  ]);

  test("folders come first, files after, by name; single-child chains join", () => {
    expect(tree.map((n) => `${n.kind}:${n.name}`)).toEqual(["folder:apps/desktop/src", "folder:crates/x/src", "file:AGENTS.md", "file:README.md"]);
    const apps = tree[0];
    expect(apps.kind === "folder" && apps.path).toBe("apps/desktop/src");
    expect(apps.kind === "folder" && apps.children.map((n) => `${n.kind}:${n.name}:${n.depth}`)).toEqual(["folder:lib:1", "file:a.ts:1"]);
  });

  test("a folder counts the files and lines under it", () => {
    const apps = tree[0];
    expect(apps.kind === "folder" && [apps.count, apps.added, apps.removed]).toEqual([2, 5, 1]);
  });

  test("closed folders hide what is under them", () => {
    expect(visibleRows(tree, new Set()).map((n) => n.path)).toEqual([
      "apps/desktop/src",
      "apps/desktop/src/lib",
      "apps/desktop/src/lib/b.ts",
      "apps/desktop/src/a.ts",
      "crates/x/src",
      "crates/x/src/lib.rs",
      "AGENTS.md",
      "README.md",
    ]);
    expect(visibleRows(tree, new Set(["apps/desktop/src"])).map((n) => n.path)).toEqual([
      "apps/desktop/src",
      "crates/x/src",
      "crates/x/src/lib.rs",
      "AGENTS.md",
      "README.md",
    ]);
  });
});
