// docs/features/chat-split-view/spec.md (FR-1, FR-4 – FR-7, FR-11 – FR-13; T1).
import { describe, expect, test } from "bun:test";
import {
  NO_SPLIT,
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

const two = (left: string, right: string, focus: 0 | 1 = 1, ratio = 0.5): Layout => ({ panes: [left, right], focus, ratio });

describe("opening a pane", () => {
  test("a chat dropped on the right half opens beside the active chat and gets the focus", () => {
    expect(openPane(NO_SPLIT, "a", "b", "right")).toEqual(two("a", "b", 1));
  });
  test("on the left half it opens on the left", () => {
    expect(openPane(NO_SPLIT, "a", "b", "left")).toEqual(two("b", "a", 0));
  });
  test("the active chat itself, or no active chat, opens nothing", () => {
    expect(openPane(NO_SPLIT, "a", "a", "right")).toEqual(NO_SPLIT);
    expect(openPane(NO_SPLIT, null, "b", "right")).toEqual(NO_SPLIT);
  });
  test("with two panes, a drop replaces the chat of the pane it lands on", () => {
    expect(openPane(two("a", "b", 1), "b", "c", "left")).toEqual(two("c", "b", 0));
  });
  test("a chat a pane already shows only gets the focus (FR-11)", () => {
    expect(openPane(two("a", "b", 1), "b", "a", "right")).toEqual(two("a", "b", 0));
  });
});

describe("arranging panes", () => {
  test("replace keeps the other pane and focuses the replaced one", () => {
    expect(replacePane(two("a", "b", 0), 1, "c")).toEqual(two("a", "c", 1));
  });
  test("swap mirrors the panes, the focus and the ratio (FR-7)", () => {
    expect(swapPanes(two("a", "b", 1, 0.3))).toEqual(two("b", "a", 0, 0.7));
  });
  test("close leaves the other chat alone and says which one stays (FR-6)", () => {
    expect(closePane(two("a", "b", 1), 1)).toEqual({ layout: NO_SPLIT, remaining: "a" });
    expect(closePane(NO_SPLIT, 0)).toEqual({ layout: NO_SPLIT, remaining: null });
  });
});

describe("following the active chat (FR-4)", () => {
  test("the focused pane's chat changes nothing", () => {
    const layout = two("a", "b", 1);
    expect(followActive(layout, "b")).toBe(layout);
  });
  test("the other pane's chat moves the focus there", () => {
    expect(followActive(two("a", "b", 1), "a")).toEqual(two("a", "b", 0));
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
  });
  test("both gone: no split", () => {
    expect(dropMissing(two("a", "b"), () => false)).toEqual({ layout: NO_SPLIT, remaining: null });
  });
  test("both there: unchanged", () => {
    const layout = two("a", "b");
    expect(dropMissing(layout, () => true)).toEqual({ layout, remaining: null });
  });
});

describe("sizes (FR-5, FR-13)", () => {
  test("the ratio keeps both panes at their minimum width", () => {
    expect(clampRatio(0.1, 1000)).toBeCloseTo(0.36);
    expect(clampRatio(0.95, 1000)).toBeCloseTo(0.64);
    expect(clampRatio(0.5, 1000)).toBe(0.5);
  });
  test("an area too small for two panes", () => {
    expect(isNarrow(700)).toBe(true);
    expect(isNarrow(800)).toBe(false);
    expect(clampRatio(0.2, 600)).toBe(0.5);
  });
});
