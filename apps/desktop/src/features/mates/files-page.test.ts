import { describe, expect, test } from "bun:test";
import { filterFiles, groupByDay, type MateFile } from "./files-page";

// docs/features/evo-mates/spec.md (FR-84).
describe("files page", () => {
  const files: MateFile[] = [
    { mateId: "a", path: "/a/report.pdf", name: "report.pdf", size: 1, modified: 2 },
    { mateId: "b", path: "/b/chart.png", name: "chart.png", size: 1, modified: 1 },
  ];
  test("a mate filter and a search narrow the list", () => {
    expect(filterFiles(files, "", "b").map((f) => f.name)).toEqual(["chart.png"]);
    expect(filterFiles(files, "REPORT", null).map((f) => f.name)).toEqual(["report.pdf"]);
    expect(filterFiles(files, "", null)).toHaveLength(2);
  });
});

// docs/features/evo-mates-ui-v2/spec.md (FR-30).
describe("files by day", () => {
  test("newest day first, newest file first", () => {
    const now = new Date(2026, 10, 28, 12).getTime();
    const at = (daysAgo: number, hour: number) => new Date(2026, 10, 28 - daysAgo, hour).getTime();
    const files: MateFile[] = [
      { mateId: "a", path: "/a/old.md", name: "old.md", size: 1, modified: at(1, 9) },
      { mateId: "a", path: "/a/new.md", name: "new.md", size: 1, modified: at(0, 10) },
      { mateId: "a", path: "/a/newer.md", name: "newer.md", size: 1, modified: at(0, 11) },
    ];
    expect(groupByDay(files, now).map((g) => [g.label, g.files.map((f) => f.name)])).toEqual([
      ["Today", ["newer.md", "new.md"]],
      ["Yesterday", ["old.md"]],
    ]);
  });
});
