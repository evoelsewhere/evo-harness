// docs/features/ux-polish/spec.md (FR-8).
import { describe, expect, test } from "bun:test";
import { suggestionsFor } from "./suggestions";

const labels = (place: Parameters<typeof suggestionsFor>[0]) => suggestionsFor(place).map((s) => s.label);

describe("new-chat suggestions by place", () => {
  test("outside a project: general tasks, no code tasks", () => {
    const shown = labels({ workspace: false, repo: false, changed: 0 });
    expect(shown).toHaveLength(4);
    expect(shown.some((label) => /project|bugs|tests/i.test(label))).toBe(false);
  });
  test("a folder that is not a repository: project tasks without git", () => {
    const shown = labels({ workspace: true, repo: false, changed: 0 });
    expect(shown[0]).toBe("Explain this project");
    expect(shown).not.toContain("Find bugs");
  });
  test("a repository with changes: reviewing them comes first, with the count", () => {
    const shown = labels({ workspace: true, repo: true, changed: 35 });
    expect(shown[0]).toBe("Review the 35 changed files");
    expect(shown).toHaveLength(4);
  });
  test("one changed file reads in the singular", () => {
    expect(labels({ workspace: true, repo: true, changed: 1 })[0]).toBe("Review the changed file");
  });
  test("a clean repository: the project tasks", () => {
    expect(labels({ workspace: true, repo: true, changed: 0 })).toEqual(["Explain this project", "Find bugs", "Write tests", "Summarize docs"]);
  });
});
