import { describe, expect, test } from "bun:test";
import { previewable, previewOf } from "./file-preview";

// docs/features/evo-mates-ui-v2/spec.md (FR-30): thumbnails on the Files page.
describe("file thumbnails", () => {
  test("Markdown previews as a page with its headings", () => {
    expect(previewOf("notes.md", "# Release 4.14\n\n- **Build:** done\n```\ncode\n```\nSee [docs](x.md)")).toEqual({
      kind: "page",
      lines: [
        { text: "Release 4.14", heading: true },
        { text: "• Build: done", heading: false },
        { text: "code", heading: false },
        { text: "See docs", heading: false },
      ],
    });
  });

  test("a CSV previews as a small table", () => {
    expect(previewOf("sizes.csv", 'market,size\n"EU",12\nUS,30\n')).toEqual({
      kind: "table",
      rows: [
        ["market", "size"],
        ["EU", "12"],
        ["US", "30"],
      ],
    });
  });

  test("other files have no text preview", () => {
    expect(previewable("deck.pptx")).toBe(false);
    expect(previewOf("deck.pptx", "PK…")).toEqual({ kind: "none" });
    expect(previewOf("empty.md", "\n\n")).toEqual({ kind: "none" });
  });
});
