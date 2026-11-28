import { describe, expect, test } from "bun:test";
import { APP_LINK } from "@/components/file-refs";
import { firstMessageLink, linkPlaces, messageLink, splitLinks } from "./links-logic";
import { parsePrompt, parseShare, sharePrompt } from "./message-markup";

// docs/features/evo-mates-threads-board/spec.md (FR-16, FR-17).
const places = [
  { word: "#Launch room", path: "channel/launch" },
  { word: "#Launch", path: "channel/other" },
  { word: "@Kai", path: "mate/kai" },
];
const placeOf = (chat: string) => (chat === "chan-launch" ? "#Launch room" : null);

describe("links between conversations", () => {
  test("a message link round-trips", () => {
    expect(firstMessageLink(`see ${messageLink("chan-launch", "k1")} please`)).toEqual({ chat: "chan-launch", line: "k1" });
    expect(firstMessageLink("nothing")).toBeNull();
  });

  test("places and message links become app links, the longest name first", () => {
    expect(linkPlaces("Ask @Kai in #Launch room.", places, placeOf)).toBe(`Ask [@Kai](${APP_LINK}mate/kai) in [#Launch room](${APP_LINK}channel/launch).`);
    expect(linkPlaces("see evo://mates/chan-launch/k1", places, placeOf)).toBe(`see [message in #Launch room](${APP_LINK}message/chan-launch/k1)`);
    expect(linkPlaces("`#Launch room` and [x](https://a)", places, placeOf)).toBe("`#Launch room` and [x](https://a)");
    expect(linkPlaces("@Kaiser", places, placeOf)).toBe("@Kaiser");
  });

  test("the person's text splits at cards, message links and channels", () => {
    expect(splitLinks("LAUNCH-2 in #launch room: evo://mates/chan-launch/k1", ["LAUNCH"], places.slice(0, 2))).toEqual([
      { text: "LAUNCH-2", card: "LAUNCH-2" },
      { text: " in " },
      { text: "#launch room", path: "channel/launch" },
      { text: ": " },
      { text: "evo://mates/chan-launch/k1", path: "message/chan-launch/k1" },
    ]);
    expect(splitLinks("plain", [], [])).toEqual([{ text: "plain" }]);
  });

  test("shared lines are marked and read back apart from the note", () => {
    const prompt = sharePrompt("#Launch room", messageLink("chan-launch", "k1"), [
      { author: "Kai", text: "Build is green." },
      { author: "Lin", text: "Notes next." },
    ], "Can you check these?");
    expect(prompt).toContain('instruction="false"');
    const { shared, text } = parseShare(prompt);
    expect(shared).toEqual({ from: "#Launch room", link: "evo://mates/chan-launch/k1", lines: 2, text: "Kai: Build is green.\nLin: Notes next." });
    expect(text).toBe("Can you check these?");
    expect(parsePrompt(prompt).shared?.lines).toBe(2);
    expect(parsePrompt(prompt).text).toBe("Can you check these?");
  });
});
