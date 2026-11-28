import { describe, expect, test } from "bun:test";
import { mateMood, moodWord, seenNow } from "./mates-store";

// docs/features/evo-mates/spec.md (FR-6, FR-32); found playing a daily user
// and an offline one (qa-2026-11-28, B6, F4).
describe("a mate's state", () => {
  test("a last turn that failed reads Couldn't reply", () => {
    expect(mateMood(undefined, false, false, true)).toBe("error");
    expect(moodWord(mateMood(undefined, false, false, true))).toBe("Couldn't reply");
  });

  test("work and waiting win over an earlier failure", () => {
    expect(mateMood("working", false, false, true)).toBe("working");
    expect(mateMood("waiting", false, false, true)).toBe("waiting");
    expect(mateMood(undefined, true, false, true)).toBe("working");
  });
});

describe("a reply the person is looking at", () => {
  test("is read when its conversation is shown in a focused window", () => {
    expect(seenNow({ chat: "c", shown: "c", unread: true, focused: true })).toBe(true);
  });

  test("stays unread elsewhere or while the window is in the background", () => {
    expect(seenNow({ chat: "c", shown: "d", unread: true, focused: true })).toBe(false);
    expect(seenNow({ chat: "c", shown: "c", unread: true, focused: false })).toBe(false);
    expect(seenNow({ chat: "c", shown: "c", unread: false, focused: true })).toBe(false);
  });
});
