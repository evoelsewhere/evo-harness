import { describe, expect, test } from "bun:test";
import { isQuiet, muteUntil } from "./mates-prefs-store";

// docs/features/evo-mates-messaging/spec.md (FR-15).
describe("mutes", () => {
  test("quiet while muted, or below All", () => {
    const now = 1_000;
    expect(isQuiet(undefined, now)).toBe(false);
    expect(isQuiet({ until: 2_000 }, now)).toBe(true);
    expect(isQuiet({ until: 500 }, now)).toBe(false);
    expect(isQuiet({ until: null }, now)).toBe(true);
    expect(isQuiet({ level: "mentions" }, now)).toBe(true);
    expect(isQuiet({ level: "all" }, now)).toBe(false);
  });

  test("mute lengths", () => {
    const now = new Date(2026, 10, 29, 22, 15);
    expect(muteUntil("hour", now)).toBe(now.getTime() + 3_600_000);
    expect(new Date(muteUntil("tomorrow", now)!).getHours()).toBe(8);
    expect(new Date(muteUntil("tomorrow", now)!).getDate()).toBe(30);
    expect(muteUntil("always", now)).toBeNull();
  });
});
