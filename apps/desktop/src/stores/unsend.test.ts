import { describe, expect, test } from "bun:test";
import { unsendOutcome } from "./engine-store";

// docs/features/evo-mates-messaging/spec.md (FR-9); review 1a #4.
describe("taking a queued message back", () => {
  test("a message the mate already took in stays, and says so", () => {
    expect(unsendOutcome({ outcome: "alreadyconsumed" })).toBe("alreadyconsumed");
  });
  test("taken back, or no answer", () => {
    expect(unsendOutcome({ outcome: "cancelled" })).toBe("cancelled");
    expect(unsendOutcome({})).toBe("cancelled");
    expect(unsendOutcome(null)).toBe("failed");
  });
});
