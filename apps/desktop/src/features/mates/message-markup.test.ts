import { describe, expect, test } from "bun:test";
import { composePrompt, excerpt, parsePrompt } from "./message-markup";

// docs/features/evo-mates/spec.md (FR-60, FR-61).
describe("mate message markup", () => {
  test("a plain message stays as typed", () => {
    expect(composePrompt("hello", null, [])).toBe("hello");
    expect(parsePrompt("hello")).toEqual({ text: "hello", reply: null });
  });

  test("a reply quotes the message and reads back", () => {
    const sent = composePrompt("make it shorter", { from: "Writer", text: "Here is the <draft> intro" }, []);
    expect(sent).toContain('<evo-mates-reply from="Writer">Here is the draft intro</evo-mates-reply>');
    expect(parsePrompt(sent)).toEqual({ text: "make it shorter", reply: { from: "Writer", text: "Here is the draft intro" } });
  });

  test("reactions become a note the thread hides", () => {
    const sent = composePrompt("thanks", null, [{ message: "e1:reply", reaction: "heart", excerpt: "The draft", atMs: 1 }]);
    expect(sent).toContain("The person reacted ❤️ to your message: \"The draft\"");
    expect(parsePrompt(sent)).toEqual({ text: "thanks", reply: null });
  });

  test("an excerpt is one line and short", () => {
    expect(excerpt("a\n\nb   c")).toBe("a b c");
    expect(excerpt("x".repeat(200), 10)).toHaveLength(10);
  });
});
