import { describe, expect, test } from "bun:test";
import { messagePlainText, quoted, replyMarkdown } from "./message-text";

describe("messagePlainText", () => {
  test("drops emphasis, headings and links", () => {
    expect(messagePlainText("## Done\n\nThe **fix** is in [the store](src/store.ts) and _works_.")).toBe(
      "Done\n\nThe fix is in the store and works.",
    );
  });

  test("keeps code as it is", () => {
    expect(messagePlainText("Run `bun **test**`:\n\n```bash\nbun test **/*.ts\n```")).toBe("Run bun **test**:\n\nbun test **/*.ts");
  });

  test("lists, quotes, tasks and rules", () => {
    expect(messagePlainText("- one\n  * two\n> said\n- [x] done\n- [ ] todo\n\n---\n\nend")).toBe(
      "• one\n  • two\nsaid\n☑ done\n☐ todo\n\nend",
    );
  });

  test("tables become tab-separated rows", () => {
    expect(messagePlainText("| a | b |\n|---|:-:|\n| 1 | 2 |")).toBe("a\tb\n1\t2");
  });

  test("snake_case and multiplication are not emphasis", () => {
    expect(messagePlainText("set max_tokens_total to 2 * 3 * 4")).toBe("set max_tokens_total to 2 * 3 * 4");
  });
});

describe("quoted", () => {
  test("every line is quoted, with a line to write on after", () => {
    expect(quoted("first\n\nsecond\n")).toBe("> first\n>\n> second\n\n");
  });
});

// docs/features/rich-replies/spec.md (FR-4).
describe("a reply with UI blocks", () => {
  const reply = [
    "Here is the week.",
    "",
    "```evo-ui",
    '{"v":1,"type":"bars","title":"Cost per day","format":{"currency":"USD"},"items":[{"label":"10 Nov","value":168.89},{"label":"11 Nov","value":1004.12}]}',
    "```",
    "",
    "```evo-ui",
    '{"v":1,"type":"callout","tone":"warning","text":"Usage is **missing** for 14 Nov."}',
    "```",
  ].join("\n");

  test("copies as the blocks' text, not their JSON", () => {
    const text = messagePlainText(reply);
    expect(text).not.toContain('"type"');
    expect(text).toContain("Cost per day");
    expect(text).toMatch(/11 Nov: \$1,004\.12/);
    expect(text).toContain("Warning: Usage is missing for 14 Nov.");
  });

  test("as Markdown, the blocks become their text too", () => {
    const markdown = replyMarkdown(reply);
    expect(markdown).not.toContain("evo-ui");
    expect(markdown).toContain("Warning: Usage is **missing** for 14 Nov.");
  });

  test("a block that cannot be read keeps its source", () => {
    const broken = "Look:\n```evo-ui\n{not json\n```";
    expect(messagePlainText(broken)).toBe("Look:\n{not json");
  });
});
