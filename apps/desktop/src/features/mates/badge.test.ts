import { describe, expect, test } from "bun:test";
import { badgeCount, badgeText } from "./badge";

// docs/features/evo-mates-messaging/spec.md (FR-16).
describe("the icon badge", () => {
  test("counts unread and waiting places once each, not hidden ones", () => {
    const places = [
      { chat: "a", seats: [], hidden: false },
      { chat: "b", seats: [], hidden: false },
      { chat: "c", seats: ["seat-1", null], hidden: false },
      { chat: "d", seats: [], hidden: true },
    ];
    expect(badgeCount(places, new Set(["a", "d"]), { b: "working", "seat-1": "waiting" })).toBe(2);
    expect(badgeCount(places, new Set(), {})).toBe(0);
  });

  test("says 9+ past nine", () => {
    expect(badgeText(3)).toBe("3");
    expect(badgeText(12)).toBe("9+");
  });
});
