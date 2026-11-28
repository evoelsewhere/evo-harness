import { describe, expect, test } from "bun:test";
import type { ChannelEntry } from "@/lib/mates";
import { boldMentions, channelRows, handoffLine, handoffSummary, handoffVerb, insertMention, mentionAt, mentionParts, retroFailure } from "./channel-rows";

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
    expect(rows.map((r) => r.kind)).toEqual(["day", "time", "bubble", "route", "bubble", "bubble", "bubble"]);
    const bubbles = rows.filter((r) => r.kind === "bubble");
    expect(bubbles.map((r) => (r.kind === "bubble" ? [r.name, r.me, r.first, r.last] : null))).toEqual([
      ["You", true, true, true],
      ["Planner", false, true, false],
      ["Planner", false, false, true],
      ["Writer", false, true, true],
    ]);
  });

  test("a message that steered a working mate says so", () => {
    const rows = channelRows([{ ...message("person", "@Writer shorter", 0), steers: "w" }], nameOf, at(1));
    const bubble = rows.find((r) => r.kind === "bubble");
    expect(bubble?.kind === "bubble" ? bubble.steers : null).toBe("Writer");
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

// docs/features/learning-layers/spec.md (FR-21): a retro is a card, a job end shows nothing.
test("a retro is a card and a job end shows nothing", () => {
  const rows = channelRows(
    [
      { kind: "message", id: "m1", author: "person", text: "Go", at: 1 },
      { kind: "job", id: "j1", message: "m1", status: "limit", handoffs: 12, at: 2 },
      {
        kind: "retro",
        id: "r1",
        at: 3,
        job: "j1",
        status: "limit",
        handoffs: 12,
        well: ["Fast"],
        wrong: ["Looped"],
        lessons: [{ mate: "w1", name: "Writer", text: "Answer first.", skill: "answering", row: 7 }],
      },
    ],
    (id) => id,
    10,
  );
  expect(rows.map((row) => row.kind)).toEqual(["day", "time", "bubble", "retro"]);
  const retro = rows[3];
  expect(retro.kind === "retro" && retro.card.lessons[0].row).toBe(7);
});

// learning-layers FR-19, FR-24 (qa-2026-11-28, C4): Run retro says why it did not run.
describe("a retro that did not run", () => {
  test("says what to do in plain words", () => {
    expect(retroFailure("no retro: no model")).toBe("No model for the retro: pick a Background model in Settings › Evo Mates, or a model for the lead.");
    expect(retroFailure("no retro: no job")).toBe("Nothing to look back on yet.");
    expect(retroFailure("no retro: a learning step for personal is already running")).toBe("A retro is already running.");
    expect(retroFailure("boom")).toBe("The retro did not run.");
  });
});

// docs/features/evo-mates-ui-v2/spec.md (FR-11, FR-20).
describe("v2 channel rows", () => {
  test("hand-offs carry both mates and read as a verb and a summary", () => {
    const rows = channelRows(
      [
        { kind: "route", message: "m", picked: ["p"], line: "Planner, the lead", at: at(0) },
        { kind: "handoff", message: "m", handoff: "assigns", from: "p", fromName: "Planner", to: "w", toName: "Writer", text: "[assign] Draft the **intro**", at: at(1) },
      ] as ChannelEntry[],
      nameOf,
    );
    const route = rows.find((r) => r.kind === "route");
    expect(route?.kind === "route" ? route.picked : []).toEqual(["p"]);
    const handoff = rows.find((r) => r.kind === "handoff");
    expect(handoff?.kind === "handoff" ? [handoff.from, handoff.to] : []).toEqual(["p", "w"]);
    expect(handoffVerb("assigns")).toBe("assigned");
    expect(handoffSummary("[assign] Draft the **intro**")).toBe("Draft the intro");
  });

  test("a new day opens with its separator", () => {
    const rows = channelRows([message("person", "a", 0), message("person", "b", 60 * 24 + 5)], nameOf);
    expect(rows.filter((r) => r.kind === "day")).toHaveLength(2);
  });
});

// docs/features/evo-mates-messaging/spec.md (FR-2).
describe("seen by", () => {
  test("the person's line lists the mates its route picked", () => {
    const rows = channelRows(
      [message("person", "plan it", 0), { kind: "route", message: "person-0", picked: ["p", "w"], line: "Planner and Writer", at: at(0) }] as ChannelEntry[],
      nameOf,
    );
    const mine = rows.find((r) => r.kind === "bubble" && r.me);
    expect(mine?.kind === "bubble" ? mine.seenBy : null).toEqual(["p", "w"]);
  });
});

// docs/features/evo-mates-messaging/spec.md (FR-5, FR-6).
describe("quotes and files", () => {
  test("a mate's line quotes the line it answers; a hand-off keeps its files", () => {
    const rows = channelRows(
      [
        { kind: "message", id: "r-1", author: "p", name: "Planner", text: "Is the build green?", at: at(0) },
        { kind: "message", id: "r-2", author: "w", name: "Writer", text: "Yes", quote: "r-1", at: at(1) },
        { kind: "message", id: "r-3", author: "w", name: "Writer", text: "Odd", quote: "gone", at: at(2) },
        { kind: "handoff", message: "m", handoff: "tells", from: "w", fromName: "Writer", to: "p", toName: "Planner", text: "Shot", files: ["C:/a.png"], at: at(3) },
      ] as ChannelEntry[],
      nameOf,
    );
    const answer = rows.find((r) => r.kind === "bubble" && r.key === "r-2");
    expect(answer?.kind === "bubble" ? answer.quote : null).toEqual({ id: "r-1", name: "Planner", text: "Is the build green?" });
    const odd = rows.find((r) => r.kind === "bubble" && r.key === "r-3");
    expect(odd?.kind === "bubble" ? odd.quote : "x").toBeUndefined();
    const handoff = rows.find((r) => r.kind === "handoff");
    expect(handoff?.kind === "handoff" ? handoff.files : []).toEqual(["C:/a.png"]);
  });
});
