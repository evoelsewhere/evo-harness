import { describe, expect, test } from "bun:test";
import { pausedLine } from "./composer";

// docs/features/evo-mates/spec.md (FR-45, FR-80).
describe("the paused line", () => {
  test("names the daily limit when that is why the mate paused", () => {
    expect(pausedLine("Penny", 2)).toBe("Paused: Penny reached today's limit of $2.00. Resume lets it go on today.");
    expect(pausedLine("Penny", null)).toBe("Penny is paused. It does no work until you resume it.");
  });
});
