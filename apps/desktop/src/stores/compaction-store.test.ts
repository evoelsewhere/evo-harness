// docs/features/auto-compact-threshold/spec.md (FR-1).
import { describe, expect, test } from "bun:test";
import { DEFAULT_COMPACTION, clampPercent, readCompaction } from "./compaction-store";

describe("the saved auto-compact setting", () => {
  test("nothing saved: on at 90 %", () => {
    expect(readCompaction(undefined)).toEqual(DEFAULT_COMPACTION);
    expect(DEFAULT_COMPACTION).toEqual({ enabled: true, triggerPercent: 90 });
  });
  test("a saved value is read as it is", () => {
    expect(readCompaction({ enabled: false, triggerPercent: 60 })).toEqual({ enabled: false, triggerPercent: 60 });
  });
  test("the percent is kept inside the slider's range", () => {
    expect(clampPercent(5)).toBe(30);
    expect(clampPercent(100)).toBe(95);
    expect(clampPercent(59.6)).toBe(60);
    expect(readCompaction({ triggerPercent: 1 }).triggerPercent).toBe(30);
  });
  test("a part that is missing or the wrong type falls back to the default", () => {
    expect(readCompaction({ triggerPercent: "high", enabled: "no" })).toEqual(DEFAULT_COMPACTION);
    expect(readCompaction({ enabled: false })).toEqual({ enabled: false, triggerPercent: 90 });
    expect(readCompaction("x")).toEqual(DEFAULT_COMPACTION);
  });
});
