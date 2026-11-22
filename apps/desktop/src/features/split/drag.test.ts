// docs/features/chat-split-view/spec.md (FR-2, FR-7, FR-13, FR-15 – FR-17; T7, T15).
import { describe, expect, test } from "bun:test";
import { dropTarget, paneRects, previewRect, type AreaLayout } from "./drop-geometry";

const area = { left: 100, top: 50, width: 1000, height: 600 };
const base = { focus: 1, ratio: 0.5, rowRatio: 0.5, arrangement: "grid" as const };
const one: AreaLayout = { ...base, panes: null };
const two: AreaLayout = { ...base, panes: ["a", "b"], ratio: 0.3 };
const three: AreaLayout = { ...base, panes: ["a", "b", "c"], focus: 2 };
const four: AreaLayout = { ...base, panes: ["a", "b", "c", "d"], focus: 3 };
const chat = { kind: "chat" as const, chat: "z" };
/** A point inside the area given in the area's own coordinates. */
const at = (x: number, y: number) => [area.left + x, area.top + y] as const;

describe("the boxes of the panes", () => {
  test("two panes: side by side around the divider", () => {
    expect(paneRects(2, "grid", 0.3, 0.5, 1000, 600)).toEqual([
      { left: 0, top: 0, width: 296, height: 600 },
      { left: 304, top: 0, width: 696, height: 600 },
    ]);
  });
  test("three panes: two above, one full width below", () => {
    const rects = paneRects(3, "grid", 0.5, 0.5, 1000, 600);
    expect(rects[0]).toEqual({ left: 0, top: 0, width: 496, height: 296 });
    expect(rects[2]).toEqual({ left: 0, top: 304, width: 1000, height: 296 });
  });
  test("four panes: a 2 x 2 grid", () => {
    const rects = paneRects(4, "grid", 0.5, 0.5, 1000, 600);
    expect(rects[3]).toEqual({ left: 504, top: 304, width: 496, height: 296 });
  });
  test("a row: equal panes; the focused pane: the whole area", () => {
    expect(paneRects(3, "row", 0.5, 0.5, 1000, 300).map((r) => r.width)).toEqual([328, 328, 328]);
    expect(paneRects(3, "focus", 0.5, 0.5, 1000, 300)[1]).toEqual({ left: 0, top: 0, width: 1000, height: 300 });
  });
});

describe("where a dragged chat lands", () => {
  test("outside the area: nowhere", () => {
    expect(dropTarget(area, 50, 100, one, chat)).toBeNull();
    expect(dropTarget(area, 500, 700, one, chat)).toBeNull();
  });
  test("one pane: the half under the pointer opens a pane there", () => {
    expect(dropTarget(area, ...at(200, 300), one, chat)).toEqual({ kind: "open", at: 0 });
    expect(dropTarget(area, ...at(600, 300), one, chat)).toEqual({ kind: "open", at: 1 });
  });
  test("two panes: the middle of a pane replaces its chat, split where the divider is", () => {
    expect(dropTarget(area, ...at(150, 300), two, chat)).toEqual({ kind: "replace", index: 0 });
    expect(dropTarget(area, ...at(650, 300), two, chat)).toEqual({ kind: "replace", index: 1 });
  });
  test("near a side edge a pane opens there (FR-15)", () => {
    expect(dropTarget(area, ...at(5, 300), two, chat)).toEqual({ kind: "open", at: 0 });
    expect(dropTarget(area, ...at(290, 300), two, chat)).toEqual({ kind: "open", at: 1 });
    expect(dropTarget(area, ...at(995, 300), two, chat)).toEqual({ kind: "open", at: 2 });
  });
  test("near the bottom edge of the last row a pane opens last", () => {
    expect(dropTarget(area, ...at(600, 595), two, chat)).toEqual({ kind: "open", at: 2 });
    expect(dropTarget(area, ...at(500, 595), three, chat)).toEqual({ kind: "open", at: 3 });
  });
  test("the bottom edge of the top row is only a pane's middle", () => {
    expect(dropTarget(area, ...at(250, 290), three, chat)).toEqual({ kind: "replace", index: 0 });
  });
  test("a full grid only replaces", () => {
    expect(dropTarget(area, ...at(5, 300), four, chat)).toEqual({ kind: "replace", index: 0 });
    expect(dropTarget(area, ...at(995, 595), four, chat)).toEqual({ kind: "replace", index: 3 });
  });
  test("over a divider: the nearest pane", () => {
    expect(dropTarget(area, ...at(500, 100), four, chat)).toEqual({ kind: "replace", index: 0 });
    expect(dropTarget(area, ...at(700, 300), four, chat)).toEqual({ kind: "replace", index: 1 });
  });
  test("showing only the focused pane: a drop replaces it", () => {
    expect(dropTarget(area, ...at(50, 300), { ...two, arrangement: "focus" }, chat)).toEqual({ kind: "replace", index: 1 });
  });
});

describe("where a dragged pane lands (FR-7, FR-16)", () => {
  const pane = (index: number) => ({ kind: "pane" as const, index });
  test("over another pane: the two swap", () => {
    expect(dropTarget(area, ...at(800, 300), two, pane(0))).toEqual({ kind: "swap", into: 1 });
    expect(dropTarget(area, ...at(800, 500), four, pane(0))).toEqual({ kind: "swap", into: 3 });
  });
  test("over itself, with one pane, or showing one pane: nothing", () => {
    expect(dropTarget(area, ...at(100, 300), two, pane(0))).toBeNull();
    expect(dropTarget(area, ...at(800, 300), one, pane(0))).toBeNull();
    expect(dropTarget(area, ...at(800, 300), { ...two, arrangement: "focus" }, pane(0))).toBeNull();
  });
});

describe("the preview of a drop", () => {
  test("one pane: the half that opens", () => {
    expect(previewRect(area, { kind: "open", at: 1 }, one)).toEqual({ left: 504, top: 0, width: 496, height: 600 });
  });
  test("a new pane fills its place in the grid it makes", () => {
    const tall = { ...area, height: 700 };
    expect(previewRect(tall, { kind: "open", at: 2 }, two)).toEqual({ left: 0, top: 354, width: 1000, height: 346 });
    expect(previewRect(tall, { kind: "open", at: 3 }, three).left).toBe(504);
  });
  test("too low for the grid it would make: the whole area", () => {
    expect(previewRect(area, { kind: "open", at: 2 }, two)).toEqual({ left: 0, top: 0, width: 1000, height: 600 });
  });
  test("a replaced or swapped pane: its box", () => {
    expect(previewRect(area, { kind: "replace", index: 0 }, two)).toEqual({ left: 0, top: 0, width: 296, height: 600 });
    expect(previewRect(area, { kind: "swap", into: 1 }, two)).toEqual({ left: 304, top: 0, width: 696, height: 600 });
  });
  test("one pane showing: the whole area", () => {
    expect(previewRect(area, { kind: "replace", index: 1 }, { ...two, arrangement: "focus" })).toEqual({ left: 0, top: 0, width: 1000, height: 600 });
  });
});
