// The Webbridge side panel's port of the block text stays in step with this
// module (docs/features/rich-replies/spec.md, FR-28, FR-4).
import { describe, expect, test } from "bun:test";
// @ts-expect-error: a plain JS module of the browser extension.
import { blockText as portText } from "../../../../../extensions/evo-webbridge/sidepanel/ui-blocks.js";
import { INVALID, VALID } from "./fixtures";
import { blockTextOf } from "./text";

describe("the side panel's block text", () => {
  for (const [type, source] of Object.entries(VALID)) {
    test(`${type} reads the same as on the desktop`, () => {
      expect(portText(source)).toBe(blockTextOf(source)!);
    });
  }

  test("a block it cannot read shows its name, never its JSON", () => {
    expect(portText(INVALID.json)).toBe("[stats]");
    expect(portText(INVALID.notObject)).toBe("[block]");
    expect(portText(INVALID.unknownType)).toBe("[block]");
  });
});
