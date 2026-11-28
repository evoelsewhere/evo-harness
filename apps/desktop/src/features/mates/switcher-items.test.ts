import { describe, expect, test } from "bun:test";
import { rankSwitcher, type SwitcherItem } from "./switcher-items";

// docs/features/evo-mates-ui-v2/spec.md (FR-7, NFR-5).
const items: SwitcherItem[] = [
  { key: "mate:kai", kind: "mate", label: "Kai", detail: "Engineer", at: 3 },
  { key: "mate:ava", kind: "mate", label: "Ava", detail: "Product lead", at: 9 },
  { key: "channel:launch", kind: "channel", label: "Launch room", detail: "Ava, Kai", at: 5 },
  { key: "action:new-channel", kind: "action", label: "New channel", keywords: ["add", "create"] },
  { key: "action:new-mate", kind: "action", label: "New mate", keywords: ["add", "hire"] },
];

describe("the quick switcher", () => {
  test("nothing typed: recent conversations, then actions", () => {
    expect(rankSwitcher(items, "").map((i) => i.key)).toEqual([
      "mate:ava",
      "channel:launch",
      "mate:kai",
      "action:new-channel",
      "action:new-mate",
    ]);
  });

  test("a name's start ranks first", () => {
    expect(rankSwitcher(items, "ka")[0].key).toBe("mate:kai");
  });

  test("words and keywords find actions", () => {
    expect(rankSwitcher(items, "new ch")[0].key).toBe("action:new-channel");
    expect(rankSwitcher(items, "hire").map((i) => i.key)).toEqual(["action:new-mate"]);
  });

  test("a role finds its mate", () => {
    expect(rankSwitcher(items, "engineer")[0].key).toBe("mate:kai");
  });

  test("no match lists nothing", () => {
    expect(rankSwitcher(items, "zzz")).toEqual([]);
  });

  test("fast enough for a big team (NFR-5)", () => {
    const many: SwitcherItem[] = Array.from({ length: 70 }, (_, i) => ({ key: `mate:${i}`, kind: "mate", label: `Mate ${i}`, detail: "Role", at: i }));
    const start = performance.now();
    for (let i = 0; i < 100; i++) rankSwitcher(many, "mate 4");
    expect((performance.now() - start) / 100).toBeLessThan(5);
  });
});
