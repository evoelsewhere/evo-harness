// Run with `bun test` from apps/desktop. Spec: docs/features/floating-cards (FR-7).
import { describe, expect, test } from "bun:test";
import { noteUserInput, recentUserInput } from "./user-input";

describe("the person's last input on a picture", () => {
  test("a page the person has not touched has none", () => {
    expect(recentUserInput("browser-never")).toBe(false);
  });

  test("a press counts for a moment, for that page only", () => {
    noteUserInput("browser-1");
    expect(recentUserInput("browser-1")).toBe(true);
    expect(recentUserInput("browser-2")).toBe(false);
    expect(recentUserInput("browser-1", 0)).toBe(false);
  });
});
