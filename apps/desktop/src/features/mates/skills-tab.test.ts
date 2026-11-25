import { describe, expect, test } from "bun:test";
import { nextSkills, skillOn } from "./skills-tab";

// docs/features/evo-mates/spec.md (FR-44).
describe("a mate's skill switches", () => {
  const known = ["docx", "pdf", "review"];

  test("switching one off from every skill keeps the others", () => {
    expect(nextSkills(null, known, "pdf", false)).toEqual(["docx", "review"]);
  });

  test("switching the last one back on returns to every skill", () => {
    expect(nextSkills(["docx", "review"], known, "pdf", true)).toBeNull();
  });

  test("an empty list stays a list", () => {
    expect(nextSkills(["pdf"], known, "pdf", false)).toEqual([]);
    expect(nextSkills([], known, "review", true)).toEqual(["review"]);
  });

  test("a skill is on with no list or when the list names it", () => {
    expect(skillOn({ name: "pdf" }, null)).toBe(true);
    expect(skillOn({ name: "pdf" }, ["pdf"])).toBe(true);
    expect(skillOn({ name: "pdf" }, [])).toBe(false);
  });
});
