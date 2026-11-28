import { describe, expect, test } from "bun:test";
import { CARD_LINK } from "@/components/file-refs";
import type { Card } from "@/lib/mates";
import { splitCardKeys, cardsIn, dateValue, dropIndex, dueFromDate, filterCards, labelsOf, linkCards, NO_FILTER, overdue } from "./board-logic";

// docs/features/evo-mates-threads-board/spec.md (FR-10, FR-14).
const card = (over: Partial<Card>): Card => ({
  id: "c",
  channelId: "ch",
  key: "LAUNCH-1",
  number: 1,
  columnId: "todo",
  position: 0,
  title: "Card",
  description: "",
  assignee: null,
  priority: "none",
  dueMs: null,
  labels: [],
  checklist: [],
  links: [],
  threadRoot: null,
  source: null,
  createdBy: "person",
  createdMs: 0,
  updatedMs: 0,
  ...over,
});

const cards = [
  card({ id: "a", key: "LAUNCH-1", number: 1, title: "Cut the build", assignee: "kai", priority: "high", labels: ["release"], position: 2 }),
  card({ id: "b", key: "LAUNCH-2", number: 2, title: "Write notes", assignee: "lin", labels: ["docs"], position: 1 }),
  card({ id: "c", key: "LAUNCH-3", number: 3, title: "Screenshots", columnId: "done", position: 0 }),
];

describe("board", () => {
  test("filters by assignee, label, priority and words", () => {
    expect(filterCards(cards, { ...NO_FILTER, assignee: "kai" }).map((c) => c.id)).toEqual(["a"]);
    expect(filterCards(cards, { ...NO_FILTER, assignee: "none" }).map((c) => c.id)).toEqual(["c"]);
    expect(filterCards(cards, { ...NO_FILTER, label: "docs" }).map((c) => c.id)).toEqual(["b"]);
    expect(filterCards(cards, { ...NO_FILTER, priority: "high" }).map((c) => c.id)).toEqual(["a"]);
    expect(filterCards(cards, { ...NO_FILTER, query: "launch-3" }).map((c) => c.id)).toEqual(["c"]);
    expect(filterCards(cards, { ...NO_FILTER, query: "build cut" }).map((c) => c.id)).toEqual(["a"]);
    expect(labelsOf(cards)).toEqual(["docs", "release"]);
  });

  test("a column's cards in order, and where a drop lands", () => {
    const todo = cardsIn(cards, "todo");
    expect(todo.map((c) => c.id)).toEqual(["b", "a"]);
    expect(dropIndex(todo, "a", "b")).toBe(0);
    expect(dropIndex(todo, "b", null)).toBe(1);
    expect(dropIndex(todo, "x", "a")).toBe(1);
  });

  test("overdue until the last column; dates round-trip", () => {
    const due = card({ dueMs: 1_000, columnId: "todo" });
    expect(overdue(due, "done", 2_000)).toBe(true);
    expect(overdue({ ...due, columnId: "done" }, "done", 2_000)).toBe(false);
    const ms = dueFromDate("2026-12-04")!;
    expect(dateValue(ms)).toBe("2026-12-04");
    expect(dueFromDate("Friday")).toBeNull();
  });

  test("KEY-n of a known board becomes a card link, not in code or links", () => {
    expect(linkCards("See LAUNCH-12, then OTHER-3.", ["LAUNCH"])).toBe(`See [LAUNCH-12](${CARD_LINK}LAUNCH-12), then OTHER-3.`);
    expect(linkCards("`LAUNCH-12` and [x](https://a/LAUNCH-1)", ["LAUNCH"])).toBe("`LAUNCH-12` and [x](https://a/LAUNCH-1)");
    expect(linkCards("PRELAUNCH-2 LAUNCH-2x", ["LAUNCH"])).toBe("PRELAUNCH-2 LAUNCH-2x");
    expect(linkCards("LAUNCH-2", [])).toBe("LAUNCH-2");
  });

  test("the person's words split at card keys", () => {
    expect(splitCardKeys("see LAUNCH-2 now", ["LAUNCH"])).toEqual([{ text: "see " }, { text: "LAUNCH-2", key: "LAUNCH-2" }, { text: " now" }]);
    expect(splitCardKeys("PRELAUNCH-2", ["LAUNCH"])).toEqual([{ text: "PRELAUNCH-2" }]);
  });

  // Board review #8: never inside code, URLs or links.
  test("card keys are not linked inside code, URLs or links", () => {
    const keys = ["LAUNCH"];
    const same = (text: string) => expect(linkCards(text, keys)).toBe(text);
    same("~~~\nLAUNCH-3\n~~~");
    same("```\nstill streaming LAUNCH-3");
    same("text\n\n    LAUNCH-3 in indented code");
    same("``a ` LAUNCH-3``");
    same("see https://tracker.example/issue?ticket=LAUNCH-3 now");
    same("see <https://tracker.example/LAUNCH-3>");
    same("tag #LAUNCH-3 and x=LAUNCH-3");
    same("![img](https://a/LAUNCH-3.png)");
    expect(linkCards("LAUNCH-3, then `LAUNCH-4`", keys)).toBe(`[LAUNCH-3](${CARD_LINK}LAUNCH-3), then \`LAUNCH-4\``);
  });
});
