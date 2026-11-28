import { describe, expect, test } from "bun:test";
import { filterFiles, type MateFile } from "./files-page";

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
