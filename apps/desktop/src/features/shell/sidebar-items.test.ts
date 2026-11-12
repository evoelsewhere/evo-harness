// docs/features/sidebar-items/spec.md (FR-1, FR-5, NFR-3).
import { describe, expect, test } from "bun:test";
import { parseSidebarShown } from "@/stores/ui-store";
import { SIDEBAR_ITEMS, isShown } from "./sidebar-items";

const byId = (id: string) => SIDEBAR_ITEMS.find((item) => item.id === id)!;

describe("sidebar items", () => {
  test("ids are unique", () => {
    expect(new Set(SIDEBAR_ITEMS.map((item) => item.id)).size).toBe(SIDEBAR_ITEMS.length);
  });

  test("with no saved choice only Usage is shown", () => {
    expect(SIDEBAR_ITEMS.filter((item) => isShown(item, {})).map((item) => item.id)).toEqual(["usage"]);
  });

  test("a saved choice wins over the default, either way", () => {
    expect(isShown(byId("scheduler"), { scheduler: true })).toBe(true);
    expect(isShown(byId("usage"), { usage: false })).toBe(false);
  });

  test("ids that are not items are ignored", () => {
    const saved = { gone: true, usage: false };
    expect(SIDEBAR_ITEMS.filter((item) => isShown(item, saved))).toEqual([]);
  });
});

describe("parseSidebarShown", () => {
  test("keeps boolean choices only", () => {
    expect(parseSidebarShown({ usage: false, scheduler: "yes", customize: true })).toEqual({ usage: false, customize: true });
  });

  test("anything else is no choice", () => {
    for (const value of [null, undefined, "usage", 3, ["usage"]]) expect(parseSidebarShown(value)).toEqual({});
  });
});
