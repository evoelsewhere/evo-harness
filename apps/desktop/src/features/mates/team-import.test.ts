import { describe, expect, test } from "bun:test";
import { teamFileName } from "./team-import";

// docs/features/evo-mates/spec.md (FR-20).
describe("team file names", () => {
  test("a name becomes a safe file name", () => {
    expect(teamFileName("Launch room")).toBe("Launch room.evo-team.md");
    expect(teamFileName('a/b:c*"d')).toBe("abcd.evo-team.md");
    expect(teamFileName("   ")).toBe("team.evo-team.md");
  });
});
