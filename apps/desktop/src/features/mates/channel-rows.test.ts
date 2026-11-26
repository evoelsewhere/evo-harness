import { describe, expect, test } from "bun:test";
import type { ChannelEntry } from "@/lib/mates";
import { boldMentions, channelRows, handoffLine, insertMention, mentionAt, mentionParts } from "./channel-rows";

// docs/features/evo-mates/spec.md (FR-14 – FR-19, FR-58).
const at = (minutes: number) => 1_700_000_000_000 + minutes * 60_000;
const message = (author: string, text: string, minutes: number, name?: string): ChannelEntry => ({
  kind: "message",
  id: `${author}-${minutes}`,
  author,
  name,
  text,
  at: at(minutes),
});
const nameOf = (id: string) => ({ p: "Planner", w: "Writer" })[id] ?? id;

describe("channel rows", () => {
  test("people on both sides, runs grouped, a time line first", () => {
    const rows = channelRows(
      [
        message("person", "Plan the launch", 0),
        { kind: "route", message: "m1", picked: ["p"], line: "Planner, the lead", at: at(0) },
        message("p", "On it", 1),
        message("p", "Done: three steps", 2),
        message("w", "Intro drafted", 3, "Writer"),
      ],
      nameOf,
      at(4),
    );
    expect(rows.map((r) => r.kind)).toEqual(["time", "bubble", "route", "bubble", "bubble", "bubble"]);
    const bubbles = rows.filter((r) => r.kind === "bubble");
    expect(bubbles.map((r) => (r.kind === "bubble" ? [r.name, r.me, r.first, r.last] : null))).toEqual([
      ["You", true, true, true],
      ["Planner", false, true, false],
      ["Planner", false, false, true],
      ["Writer", false, true, true],
    ]);
  });

  test("a gap of more than 15 minutes adds a time line", () => {
    const rows = channelRows([message("person", "a", 0), message("p", "b", 16)], nameOf, at(17));
    expect(rows.filter((r) => r.kind === "time")).toHaveLength(2);
  });

  test("hand-offs and notices read plainly", () => {
    expect(handoffLine({ fromName: "Planner", toName: "Writer", handoff: "assigns", text: "[assign] draft the intro" })).toBe(
      "Planner assigns Writer: draft the intro",
    );
    const rows = channelRows([{ kind: "notice", notice: "limit", text: "Stopped at the limit", at: at(0) }], nameOf, at(1));
    expect(rows[0]).toEqual({ kind: "notice", key: "notice:0", text: "Stopped at the limit", stop: true });
  });

  test("a failed reply is marked", () => {
    const rows = channelRows([{ ...message("p", "half", 0), status: "failed" } as ChannelEntry], nameOf, at(1));
    const bubble = rows.find((r) => r.kind === "bubble");
    expect(bubble?.kind === "bubble" && bubble.failed).toBe(true);
  });
});

describe("mentions in the box", () => {
  test("the @ word at the caret", () => {
    expect(mentionAt("hi @wr", 6)).toEqual({ start: 3, query: "wr" });
    expect(mentionAt("@", 1)).toEqual({ start: 0, query: "" });
    expect(mentionAt("mail a@b", 8)).toBeNull();
    expect(mentionAt("hi @writer done", 15)).toBeNull();
  });

  test("picking a member replaces the word", () => {
    expect(insertMention("hi @wr please", 3, 6, "Writer")).toEqual({ text: "hi @Writer  please", caret: 11 });
  });
});

describe("mentions and marks in the thread", () => {
  test("mentions are found by member name, not inside words or emails", () => {
    const names = ["Researcher", "New mate"];
    expect(mentionParts("Ok @Researcher, ask @new mate and @everyone.", names)).toEqual([
      { text: "Ok ", mention: false },
      { text: "@Researcher", mention: true },
      { text: ", ask ", mention: false },
      { text: "@new mate", mention: true },
      { text: " and ", mention: false },
      { text: "@everyone", mention: true },
      { text: ".", mention: false },
    ]);
    expect(mentionParts("mail x@Researcher.com", names)).toEqual([{ text: "mail x@Researcher.com", mention: false }]);
    expect(mentionParts("@Researchers", names)).toEqual([{ text: "@Researchers", mention: false }]);
  });

  test("a mate's markdown gets its mentions in bold once", () => {
    expect(boldMentions("Hi @Writer!", ["Writer"])).toBe("Hi **@Writer**!");
    expect(boldMentions("Hi **@Writer**!", ["Writer"])).toBe("Hi **@Writer**!");
  });

  test("hand-off lines drop markdown marks", () => {
    expect(handoffLine({ fromName: "Writer", toName: "Researcher", handoff: "asks", text: "Call me **Bút Nhỏ**, _ok_? `x`" })).toBe(
      "Writer asks Researcher: Call me Bút Nhỏ, ok? x",
    );
  });
});
