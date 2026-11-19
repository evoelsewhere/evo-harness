// docs/features/source-control/spec.md (FR-8; T19).
import { describe, expect, test } from "bun:test";
import { composeCodeComments, describeCodeComment, MAX_QUOTE_LINES, type CodeComment } from "./code-comments";
import { composeAnnotatedMessage, parseAnnotatedMessage, type DocumentAnnotation } from "./document-annotations";

const comment = (over: Partial<CodeComment> = {}): CodeComment => ({
  id: "c1",
  file: "src/a.ts",
  from: 12,
  to: 13,
  quote: "const a = 1;\nconst b = a;\n",
  instruction: "Rename these.",
  ...over,
});

describe("comments on changed lines", () => {
  test("each names its file and lines, quotes them and says what to do", () => {
    const text = composeCodeComments("Fix these please", [comment(), comment({ id: "c2", file: "b.rs", from: 4, to: 4, quote: "x", instruction: "" })]);
    expect(text).toBe(
      [
        "Fix these please",
        "",
        "Comments on the changes:",
        "1. src/a.ts lines 12–13",
        "   > const a = 1;",
        "   > const b = a;",
        "   Rename these.",
        "2. b.rs line 4",
        "   > x",
        "   (no instruction: look at these lines)",
      ].join("\n"),
    );
    expect(composeCodeComments("only text", [])).toBe("only text");
    expect(composeCodeComments("", [comment()]).startsWith("Comments on the changes:")).toBe(true);
  });

  test("a long quote is cut", () => {
    const quote = Array.from({ length: MAX_QUOTE_LINES + 5 }, (_, i) => `line ${i}`).join("\n");
    const text = composeCodeComments("", [comment({ quote })]);
    expect(text).toContain("… 5 more lines");
    expect(text).not.toContain(`line ${MAX_QUOTE_LINES}\n`);
  });

  test("with office annotations, the office block stays last and still parses", () => {
    const annotation: DocumentAnnotation = {
      id: "o1",
      file: "deck.pptx",
      slide: 2,
      shapes: [],
      area: { x: 1, y: 2, w: 3, h: 4 },
      text: "Revenue",
      instruction: "Bigger",
      label: "Slide 2",
    };
    const sent = composeAnnotatedMessage(composeCodeComments("Both", [comment()]), [annotation]);
    const parsed = parseAnnotatedMessage(sent);
    expect(parsed?.annotations.length).toBe(1);
    expect(parsed?.text).toContain("1. src/a.ts lines 12–13");
  });

  test("labels", () => {
    expect(describeCodeComment({ file: "a", from: 3, to: 3 })).toBe("a · line 3");
    expect(describeCodeComment({ file: "a", from: 3, to: 9 })).toBe("a · lines 3–9");
  });
});
