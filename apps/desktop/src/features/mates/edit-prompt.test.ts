import { describe, expect, test } from "bun:test";
import { replaceVisibleText } from "./edit-prompt";

// docs/features/evo-mates-messaging/spec.md (FR-7); review 1a #8.
describe("editing the person's words", () => {
  test("keeps the quote and changes only the words", () => {
    const prompt = '<evo-mates-reply from="Ava">plan monday</evo-mates-reply>\nplan monday';
    expect(replaceVisibleText(prompt, "plan monday", "plan tuesday")).toBe('<evo-mates-reply from="Ava">plan monday</evo-mates-reply>\nplan tuesday');
  });

  test("never edits inside a pasted block", () => {
    const prompt = '<pasted-text lines="1">\nok\n</pasted-text>\nok';
    expect(replaceVisibleText(prompt, "ok", "yes")).toBe('<pasted-text lines="1">\nok\n</pasted-text>\nyes');
  });

  test("words not found: refused, nothing dropped", () => {
    expect(replaceVisibleText("hello", "bye", "x")).toBeNull();
    expect(replaceVisibleText("hello", "", "x")).toBeNull();
  });
});
