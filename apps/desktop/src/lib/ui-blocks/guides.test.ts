// Every example the engine's `ui_guide` teaches the model is a block this app
// draws (docs/features/rich-replies/spec.md, FR-11, FR-1).
import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { parseBlock } from "./parse";
import { findBlocks } from "./text";
import { BLOCK_TYPES } from "./types";

const GUIDES = join(import.meta.dir, "../../../../../crates/evo-engine/src/tools/ui_guide");
const files = readdirSync(GUIDES).filter((name) => name.endsWith(".md"));

describe("the ui_guide examples", () => {
  test("every component has a guide", () => {
    for (const type of BLOCK_TYPES) expect(files).toContain(`${type}.md`);
  });

  for (const file of files) {
    const examples = findBlocks(readFileSync(join(GUIDES, file), "utf8"));
    if (examples.length === 0) continue;
    test(`${file}: its examples are drawn`, () => {
      for (const example of examples) {
        expect(example.closed).toBe(true);
        const result = parseBlock(example.source);
        if (result.status !== "ok") throw new Error(`${file}: ${JSON.stringify(result)}`);
        expect(result.dropped).toBe(0);
      }
    });
  }

  test("each component guide shows its own component", () => {
    for (const type of BLOCK_TYPES) {
      const examples = findBlocks(readFileSync(join(GUIDES, `${type}.md`), "utf8"));
      const types = examples.map((example) => {
        const result = parseBlock(example.source);
        return result.status === "invalid" ? undefined : result.block.type;
      });
      expect(types).toContain(type);
    }
  });
});
