import { describe, expect, test } from "bun:test";
import { plainPreview } from "./plain-preview";

// docs/features/evo-mates-ui-v2/spec.md (FR-4); evo-mates QA G2.
describe("plain previews", () => {
  test("a code block previews as its code", () => {
    expect(plainPreview("```js\n1,2,3\n```")).toBe("1,2,3");
  });

  test("Markdown marks go, the words stay", () => {
    expect(plainPreview("**This week**\n\n- **Launch:** Friday\n- see [the notes](docs/notes.md)")).toBe(
      "This week Launch: Friday see the notes",
    );
    expect(plainPreview("# Title\n> quoted _word_ and `code`")).toBe("Title quoted word and code");
    expect(plainPreview("| a | b |\n| --- | --- |\n| 1 | 2 |")).toBe("a b 1 2");
  });

  test("snake_case and file names survive", () => {
    expect(plainPreview("Saved to snake_case_name.md")).toBe("Saved to snake_case_name.md");
  });

  test("long text is cut with an ellipsis", () => {
    expect(plainPreview("x".repeat(200), 20)).toBe(`${"x".repeat(19)}…`);
    expect(plainPreview(null)).toBe("");
  });
});
