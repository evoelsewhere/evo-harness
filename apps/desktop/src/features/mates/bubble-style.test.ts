import { describe, expect, test } from "bun:test";
import { BUBBLE_TEXT, panelFloats } from "./bubble-style";

// The owner found Evo Mates scrolling sideways (qa-2026-11-28, H1): a mate's
// bubble with a long URL or word grew past the thread, in a mate's
// conversation and in a channel. Both bubbles share this style.
describe("a bubble's text", () => {
  test("breaks anywhere a long URL or word needs it, and may shrink", () => {
    expect(BUBBLE_TEXT.split(" ")).toContain("[overflow-wrap:anywhere]");
    expect(BUBBLE_TEXT.split(" ")).toContain("min-w-0");
    // Tables scroll inside their frame and keep whole words.
    expect(BUBBLE_TEXT.split(" ")).toContain("[&_table]:[overflow-wrap:normal]");
  });
});

// ux.md 2: the mate panel floats over the conversation in a narrow window
// (H2); Evo Mates UI v2 docks it only from 1100 px (FR-31).
describe("the mate panel", () => {
  test("floats over the conversation in a narrow window", () => {
    expect(panelFloats(1099)).toBe(true);
    expect(panelFloats(1100)).toBe(false);
  });
});
