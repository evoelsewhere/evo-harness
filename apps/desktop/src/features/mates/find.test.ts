import { describe, expect, test } from "bun:test";
import { findLabel, matchOffsets, stepMatch } from "./find";

// docs/features/evo-mates-ui-v2/spec.md (FR-15).
describe("find in a conversation", () => {
  test("finds every occurrence, ignoring case", () => {
    expect(matchOffsets("Links, links and LINKS", "links")).toEqual([0, 7, 17]);
    expect(matchOffsets("aaaa", "aa")).toEqual([0, 2]);
    expect(matchOffsets("anything", "  ")).toEqual([]);
  });

  test("says where it is", () => {
    expect(findLabel(0, 2, "links")).toBe("1 of 2");
    expect(findLabel(0, 0, "links")).toBe("No matches");
    expect(findLabel(0, 0, "")).toBe("");
  });

  test("Enter and Shift+Enter wrap round", () => {
    expect(stepMatch(1, 2, 1)).toBe(0);
    expect(stepMatch(0, 2, -1)).toBe(1);
    expect(stepMatch(0, 0, 1)).toBe(0);
  });
});
