// docs/features/chat-split-view/spec.md (FR-2, FR-7, FR-13; T7).
import { describe, expect, test } from "bun:test";
import { dropTarget, previewRect } from "./drop-geometry";

const area = { left: 100, top: 50, width: 1000, height: 600 };
const one = { panes: null, focus: 0 as const, ratio: 0.5, narrow: false };
const two = { panes: ["a", "b"] as [string, string], focus: 1 as const, ratio: 0.3, narrow: false };
const chat = { kind: "chat" as const, chat: "c" };

describe("where a dragged chat lands", () => {
  test("outside the area: nowhere", () => {
    expect(dropTarget(area, 50, 100, one, chat)).toBeNull();
    expect(dropTarget(area, 500, 700, one, chat)).toBeNull();
  });
  test("one pane: the half under the pointer opens a pane there", () => {
    expect(dropTarget(area, 300, 300, one, chat)).toEqual({ kind: "open", side: "left" });
    expect(dropTarget(area, 700, 300, one, chat)).toEqual({ kind: "open", side: "right" });
  });
  test("two panes: the pane under the pointer, split where the divider is", () => {
    expect(dropTarget(area, 350, 300, two, chat)).toEqual({ kind: "replace", index: 0 });
    expect(dropTarget(area, 450, 300, two, chat)).toEqual({ kind: "replace", index: 1 });
  });
  test("a narrow area shows one pane: a drop replaces the focused one", () => {
    expect(dropTarget(area, 150, 300, { ...two, narrow: true }, chat)).toEqual({ kind: "replace", index: 1 });
  });
});

describe("where a dragged pane lands (FR-7)", () => {
  const pane = (index: 0 | 1) => ({ kind: "pane" as const, index });
  test("over the other pane: the panes swap", () => {
    expect(dropTarget(area, 900, 300, two, pane(0))).toEqual({ kind: "swap", into: 1 });
  });
  test("over itself, with one pane, or narrow: nothing", () => {
    expect(dropTarget(area, 200, 300, two, pane(0))).toBeNull();
    expect(dropTarget(area, 900, 300, one, pane(0))).toBeNull();
    expect(dropTarget(area, 900, 300, { ...two, narrow: true }, pane(0))).toBeNull();
  });
});

describe("the preview of a drop", () => {
  test("covers the half that opens, or the pane that changes", () => {
    expect(previewRect(area, { kind: "open", side: "right" }, one)).toEqual({ left: 500, top: 0, width: 500, height: 600 });
    expect(previewRect(area, { kind: "replace", index: 0 }, two)).toEqual({ left: 0, top: 0, width: 300, height: 600 });
    expect(previewRect(area, { kind: "swap", into: 1 }, two)).toEqual({ left: 300, top: 0, width: 700, height: 600 });
  });
  test("narrow: the whole area", () => {
    expect(previewRect(area, { kind: "replace", index: 1 }, { ...two, narrow: true })).toEqual({ left: 0, top: 0, width: 1000, height: 600 });
  });
});
