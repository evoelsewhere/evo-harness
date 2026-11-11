// Run with `bun test` from apps/desktop. Spec: docs/features/subagents-and-nested-sessions (FR-22).
import { expect, test } from "bun:test";
import { changedSinceReview, patchRows } from "./worktrees";

test("a patch line is told by its first character only inside a hunk", () => {
  const patch = [
    "diff --git a/list.md b/list.md",
    "index 1111111..2222222 100644",
    "--- a/list.md",
    "+++ b/list.md",
    "@@ -1,4 +1,4 @@ heading",
    " kept",
    "--- a rule that was removed",
    "+++ a line that was added",
    "-- sql comment",
    "\\ No newline at end of file",
    "diff --git a/logo.png b/logo.png",
    "GIT binary patch",
    "literal 12",
    "+abcdef",
    "-ghijkl",
  ].join("\n");
  expect(patchRows(patch).map((row) => row.kind)).toEqual([
    "meta",
    "meta",
    "meta",
    "meta",
    "meta",
    "context",
    "removed",
    "added",
    "removed",
    "meta",
    "meta",
    "meta",
    "meta",
    "meta",
    "meta",
  ]);
  const rows = patchRows(patch);
  expect(rows[6]?.text).toBe("-- a rule that was removed");
  expect(rows[8]?.text).toBe("- sql comment");
  expect(rows.filter((row) => row.file).length).toBe(2);
});

test("the refusal for a worktree that changed since the review is recognised", () => {
  expect(changedSinceReview("the worktree changed since you opened this review: look at the changes again, then apply them")).toBe(true);
  expect(changedSinceReview("the changes do not apply cleanly to your working folder")).toBe(false);
});
