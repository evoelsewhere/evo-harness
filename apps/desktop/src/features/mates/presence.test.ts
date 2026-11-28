import { describe, expect, test } from "bun:test";
import { channelPresence, presenceOf } from "./presence";

// docs/features/evo-mates-ui-v2/spec.md (FR-4, FR-5).
describe("presence", () => {
  test("each mood has its mark and word", () => {
    expect(presenceOf("idle")).toEqual({ presence: "idle", word: null });
    expect(presenceOf("working")).toEqual({ presence: "working", word: "Working…" });
    expect(presenceOf("thinking").presence).toBe("working");
    expect(presenceOf("waiting")).toEqual({ presence: "waiting", word: "Needs you" });
    expect(presenceOf("error")).toEqual({ presence: "error", word: "Couldn't reply" });
    expect(presenceOf("queued")).toEqual({ presence: "queued", word: "Queued" });
  });

  test("paused wins over idle and work, but not over needing the person", () => {
    expect(presenceOf("idle", true)).toEqual({ presence: "paused", word: "Paused" });
    expect(presenceOf("working", true).presence).toBe("paused");
    expect(presenceOf("waiting", true).presence).toBe("waiting");
  });

  test("a channel reads from its seats", () => {
    const seats = [
      { mateId: "a", seatChatId: "s-a" },
      { mateId: "b", seatChatId: "s-b" },
    ];
    expect(channelPresence(seats, { "s-a": "working" }, undefined)).toEqual({ working: ["a"], waiting: [], presence: "working" });
    expect(channelPresence(seats, { "s-a": "working", "s-b": "waiting" }, undefined).presence).toBe("waiting");
    expect(channelPresence(seats, {}, "working").presence).toBe("working");
    expect(channelPresence(seats, {}, undefined).presence).toBe("idle");
  });
});
