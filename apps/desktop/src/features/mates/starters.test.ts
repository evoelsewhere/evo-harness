import { describe, expect, test } from "bun:test";
import { SCRATCH_STARTERS, startersFor } from "./starters";

// docs/features/evo-mates/spec.md (FR-74).
describe("starters", () => {
  const templates = [{ id: "writer", starters: ["Draft a post", "Shorten this"] }];
  test("a mate from a role gets its role's starters", () => {
    expect(startersFor({ template: "writer" }, templates)).toEqual(["Draft a post", "Shorten this"]);
  });
  test("a mate from scratch, or from a role that is gone, gets the set-up starters", () => {
    expect(startersFor({ template: null }, templates)).toEqual(SCRATCH_STARTERS);
    expect(startersFor({ template: "gone" }, templates)).toEqual(SCRATCH_STARTERS);
  });
});
