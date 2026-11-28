import { describe, expect, test } from "bun:test";
import { shortcodeAt, slashQuery, typingWords, wrapSelection } from "./composer-keys";
import { replaceShortcodes, searchEmoji } from "./emoji";
import { matchSlash, parseCheckin, parseSlash } from "./slash-commands";

// docs/features/evo-mates-messaging/spec.md (FR-1, FR-12, FR-13).
describe("the message box", () => {
  test("Ctrl+B wraps the selection and unwraps it again", () => {
    const once = wrapSelection("make it bold", 8, 12, "**");
    expect(once).toEqual({ text: "make it **bold**", start: 10, end: 14 });
    expect(wrapSelection(once.text, once.start, once.end, "**").text).toBe("make it bold");
  });

  test("a :code being typed", () => {
    expect(shortcodeAt("great :thu", 10)).toEqual({ start: 6, query: "thu" });
    expect(shortcodeAt("at 10:30", 8)).toBeNull();
    expect(shortcodeAt(":t", 2)).toBeNull();
  });

  test("emoji by shortcode and word", () => {
    expect(searchEmoji("thu")[0].e).toBe("👍");
    expect(searchEmoji("like")[0].e).toBe("👍");
    expect(replaceShortcodes("ship it :rocket: :nope:")).toBe("ship it 🚀 :nope:");
  });

  test("the typing line's words", () => {
    expect(typingWords(["Writer"])).toBe("Writer is typing…");
    expect(typingWords(["Writer", "Analyst"])).toBe("Writer and Analyst are typing…");
    expect(typingWords(["Planner", "Writer", "Analyst"])).toBe("Planner and 2 others are typing…");
    expect(typingWords([])).toBe("");
  });
});

describe("slash commands", () => {
  test("the menu filters as you type, per box", () => {
    expect(slashQuery("/che")).toBe("che");
    expect(slashQuery("/checkin every")).toBeNull();
    expect(matchSlash("p", "mate").map((c) => c.id)).toEqual(["pause"]);
    expect(matchSlash("", "channel").map((c) => c.id)).toEqual(["stop", "handoff", "clear-quote"]);
  });

  test("a typed command and its words", () => {
    expect(parseSlash("/pause")?.command.id).toBe("pause");
    expect(parseSlash("/checkin every weekday 9:00 inbox")?.rest).toBe("every weekday 9:00 inbox");
    expect(parseSlash("/nothing")).toBeNull();
    expect(parseSlash("hello /pause")).toBeNull();
  });

  test("check-in schedules", () => {
    expect(parseCheckin("every weekday 9:00 inbox")).toEqual({ repeat: { kind: "weekdays", time: "09:00" }, text: "inbox" });
    expect(parseCheckin("every day at 5:30 pm wrap up")).toEqual({ repeat: { kind: "daily", time: "17:30" }, text: "wrap up" });
    expect(parseCheckin("every mon,fri 16:00 weekly review")).toEqual({ repeat: { kind: "weekly", time: "16:00", days: [1, 5] }, text: "weekly review" });
    expect(parseCheckin("sometimes")).toBeNull();
    expect(parseCheckin("every day 25:00 nope")).toBeNull();
  });
});
