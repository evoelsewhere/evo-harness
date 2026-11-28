import { describe, expect, test } from "bun:test";
import { forwardPrompt, threadMark, withThread } from "./message-markup";
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

// docs/features/evo-mates-messaging/spec.md (FR-10).
describe("forwarding", () => {
  test("a forward carries its source and the note, and reads back", () => {
    const prompt = forwardPrompt("Writer", "The intro is ready.", "check this");
    expect(prompt).toBe('<evo-mates-forward from="Writer">\nThe intro is ready.\n</evo-mates-forward>\ncheck this');
    expect(parsePrompt(prompt)).toEqual({ text: "check this", reply: null, forwarded: { from: "Writer", text: "The intro is ready." } });
    expect(forwardPrompt("A", "x", "")).toContain("(Forwarded for your information.)");
  });

  // docs/features/evo-mates-threads-board/spec.md (FR-1).
  test("a thread's mark round-trips", () => {
    const sent = withThread("r-1", "add arm64");
    expect(threadMark(sent)).toEqual({ root: "r-1", also: false, text: "add arm64" });
    expect(threadMark(withThread("r-1", "x", true)).also).toBe(true);
    expect(threadMark("plain").root).toBeNull();
  });
});
