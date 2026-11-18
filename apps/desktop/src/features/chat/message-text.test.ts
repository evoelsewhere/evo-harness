import { describe, expect, test } from "bun:test";
import { messagePlainText, quoted } from "./message-text";

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
