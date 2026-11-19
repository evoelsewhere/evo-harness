// docs/features/ux-polish/spec.md (FR-9).
import { describe, expect, test } from "bun:test";
import { plainText } from "./plain-text";

describe("a markdown preview as plain text", () => {
  test("emphasis, code and links lose their marks", () => {
    expect(plainText("**Needs your attention** - **Unpushed commits:** `main` is ahead")).toBe(
      "Needs your attention - Unpushed commits: main is ahead",
    );
    expect(plainText("See [the docs](https://x.dev) and _this_ ~~old~~")).toBe("See the docs and this old");
  });
  test("headings, lists and quotes read as one line", () => {
    expect(plainText("# Summary\n\n- one\n- two\n> quoted")).toBe("Summary one two quoted");
  });
  test("code fences keep their content only", () => {
    expect(plainText("Run:\n```bash\nbun test\n```")).toBe("Run: bun test");
  });
  test("plain text is left alone", () => {
    expect(plainText("Tóm tắt từ hôm qua đến nay.")).toBe("Tóm tắt từ hôm qua đến nay.");
    expect(plainText("snake_case_name stays")).toBe("snake_case_name stays");
  });
});

// docs/features/rich-replies/spec.md (FR-4).
describe("a preview of a reply with UI blocks", () => {
  test("names each block instead of showing its JSON", () => {
    const reply = 'Spend this week:\n```evo-ui\n{"v":1,"type":"bars","title":"Cost per day","items":[{"label":"a","value":1}]}\n```\nMostly the 11th.';
    expect(plainText(reply)).toBe("Spend this week: [Cost per day] Mostly the 11th.");
  });

  test("a preview cut inside a block names its type", () => {
    expect(plainText('Done.\n```evo-ui\n{"v":1,"type":"table","columns":[{"key"')).toBe("Done. [table]");
  });
});
