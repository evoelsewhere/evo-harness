// docs/features/chat-split-view/spec.md (FR-1, FR-4 – FR-7, FR-11 – FR-13, FR-15 – FR-18; T1, T14).
import { describe, expect, test } from "bun:test";
import {
  NO_SPLIT,
  arrange,
  clampRatio,
  closePane,
  dropMissing,
  followActive,
  isNarrow,
  openPane,
  replacePane,
  swapPanes,
  type Layout,
} from "./split-rules";

const many = (panes: string[], focus = panes.length - 1, ratio = 0.5, rowRatio = 0.5): Layout => ({ panes, focus, ratio, rowRatio });
const two = (left: string, right: string, focus = 1, ratio = 0.5): Layout => many([left, right], focus, ratio);

describe("opening a pane", () => {
  test("one pane: a chat dropped on the right opens beside the active chat and gets the focus", () => {
    expect(openPane(NO_SPLIT, "a", "b", 1)).toEqual(two("a", "b", 1));
  });
  test("on the left it opens on the left", () => {
    expect(openPane(NO_SPLIT, "a", "b", 0)).toEqual(two("b", "a", 0));
  });
  test("the active chat itself, or no active chat, opens nothing", () => {
    expect(openPane(NO_SPLIT, "a", "a", 1)).toEqual(NO_SPLIT);
    expect(openPane(NO_SPLIT, null, "b", 1)).toEqual(NO_SPLIT);
  });
  test("with room left, a drop adds a pane at that place and focuses it (FR-15)", () => {
    expect(openPane(two("a", "b", 1), "b", "c", 2)).toEqual(many(["a", "b", "c"], 2));
    expect(openPane(two("a", "b", 1), "b", "c", 0)).toEqual(many(["c", "a", "b"], 0));
    expect(openPane(many(["a", "b", "c"], 2), "c", "d", 3)).toEqual(many(["a", "b", "c", "d"], 3));
  });
  test("a full grid replaces the pane at that place", () => {
    const full = many(["a", "b", "c", "d"], 3);
    expect(openPane(full, "d", "e", 1)).toEqual(many(["a", "e", "c", "d"], 1));
    expect(openPane(full, "d", "e", 4)).toEqual(many(["a", "b", "c", "e"], 3));
  });
  test("a chat a pane already shows only gets the focus (FR-11)", () => {
    expect(openPane(two("a", "b", 1), "b", "a", 2)).toEqual(two("a", "b", 0));
  });
});

describe("arranging panes", () => {
  test("replace keeps the others and focuses the replaced one", () => {
    expect(replacePane(two("a", "b", 0), 1, "c")).toEqual(two("a", "c", 1));
    expect(replacePane(many(["a", "b", "c"], 0), 2, "d")).toEqual(many(["a", "b", "d"], 2));
  });
  test("swap of two mirrors the panes, the focus and the ratio (FR-7)", () => {
    expect(swapPanes(two("a", "b", 1, 0.3))).toEqual(two("b", "a", 0, 0.7));
  });
  test("swap in a grid trades two places; the focus goes with its chat; sizes stay (FR-16)", () => {
    expect(swapPanes(many(["a", "b", "c", "d"], 2, 0.3, 0.6), 0, 2)).toEqual(many(["c", "b", "a", "d"], 0, 0.3, 0.6));
    expect(swapPanes(many(["a", "b", "c"], 1), 0, 2)).toEqual(many(["c", "b", "a"], 1));
  });
  test("swap with itself or a place that is not there changes nothing", () => {
    const layout = many(["a", "b", "c"]);
    expect(swapPanes(layout, 1, 1)).toBe(layout);
    expect(swapPanes(layout, 1, 3)).toBe(layout);
  });
  test("close of two leaves the other chat alone and says which one stays (FR-6)", () => {
    expect(closePane(two("a", "b", 1), 1)).toEqual({ layout: NO_SPLIT, remaining: "a" });
    expect(closePane(NO_SPLIT, 0)).toEqual({ layout: NO_SPLIT, remaining: null });
  });
  test("close in a grid keeps the order and the focus on the chat that had it (FR-15)", () => {
    expect(closePane(many(["a", "b", "c"], 2), 0)).toEqual({ layout: many(["b", "c"], 1), remaining: "c" });
    expect(closePane(many(["a", "b", "c", "d"], 1), 3)).toEqual({ layout: many(["a", "b", "c"], 1), remaining: "b" });
  });
  test("closing the focused pane focuses the one that takes its place, or the last", () => {
    expect(closePane(many(["a", "b", "c", "d"], 1), 1)).toEqual({ layout: many(["a", "c", "d"], 1), remaining: "c" });
    expect(closePane(many(["a", "b", "c"], 2), 2)).toEqual({ layout: many(["a", "b"], 1), remaining: "b" });
  });
});

describe("following the active chat (FR-4)", () => {
  test("the focused pane's chat changes nothing", () => {
    const layout = two("a", "b", 1);
    expect(followActive(layout, "b")).toBe(layout);
  });
  test("another pane's chat moves the focus there", () => {
    expect(followActive(many(["a", "b", "c"], 2), "a")).toEqual(many(["a", "b", "c"], 0));
  });
  test("any other chat replaces the focused pane", () => {
    expect(followActive(two("a", "b", 1), "c")).toEqual(two("a", "c", 1));
  });
  test("one pane, or no active chat, is left as it is", () => {
    expect(followActive(NO_SPLIT, "c")).toBe(NO_SPLIT);
    const layout = two("a", "b");
    expect(followActive(layout, null)).toBe(layout);
  });
});

describe("chats that go away (FR-12)", () => {
  test("a pane whose chat is gone closes", () => {
    expect(dropMissing(two("a", "b"), (id) => id !== "b")).toEqual({ layout: NO_SPLIT, remaining: "a" });
    expect(dropMissing(many(["a", "b", "c", "d"], 3), (id) => id !== "b")).toEqual({
      layout: many(["a", "c", "d"], 2),
      remaining: "d",
    });
  });
  test("the focused pane's chat gone: the focus stays at its place", () => {
    expect(dropMissing(many(["a", "b", "c"], 1), (id) => id !== "b")).toEqual({ layout: many(["a", "c"], 1), remaining: "c" });
  });
  test("all gone: no split", () => {
    expect(dropMissing(two("a", "b"), () => false)).toEqual({ layout: NO_SPLIT, remaining: null });
  });
  test("all there: unchanged", () => {
    const layout = many(["a", "b", "c"]);
    expect(dropMissing(layout, () => true)).toEqual({ layout, remaining: null });
  });
});

describe("sizes (FR-5, FR-13)", () => {
  test("the ratio keeps both sides at their minimum", () => {
    expect(clampRatio(0.1, 1000)).toBeCloseTo(0.36);
    expect(clampRatio(0.95, 1000)).toBeCloseTo(0.64);
    expect(clampRatio(0.5, 1000)).toBe(0.5);
    expect(clampRatio(0.05, 1000, 300)).toBeCloseTo(0.3);
  });
  test("an area too small for two sides", () => {
    expect(isNarrow(700)).toBe(true);
    expect(isNarrow(800)).toBe(false);
    expect(clampRatio(0.2, 600)).toBe(0.5);
  });
});

describe("how the panes fit the area (FR-13, FR-17)", () => {
  test("one pane, or an area not measured yet: the grid", () => {
    expect(arrange(1, 300, 300)).toBe("grid");
    expect(arrange(4, 0, 0)).toBe("grid");
  });
  test("two panes: side by side, or the focused one when narrow", () => {
    expect(arrange(2, 1000, 200)).toBe("grid");
    expect(arrange(2, 700, 900)).toBe("focus");
  });
  test("three or four panes: two rows when the area is high enough", () => {
    expect(arrange(3, 900, 700)).toBe("grid");
    expect(arrange(4, 900, 700)).toBe("grid");
  });
  test("too low for two rows: one row when every pane fits, else the focused one", () => {
    expect(arrange(3, 1200, 500)).toBe("row");
    expect(arrange(4, 1200, 500)).toBe("focus");
    expect(arrange(4, 1500, 500)).toBe("row");
  });
  test("narrow: the focused pane, whatever the height", () => {
    expect(arrange(4, 700, 900)).toBe("focus");
  });
});
