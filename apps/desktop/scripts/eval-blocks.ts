// Scores the rich-replies block-use eval (docs/features/rich-replies/assets/eval.md):
// for each saved answer, how many `evo-ui` blocks it has and whether the
// desktop draws them. Usage: bun scripts/eval-blocks.ts <answers-dir>
// where <answers-dir> holds A01.txt … A20.txt and B01.txt … B20.txt.

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { parseBlock } from "../src/lib/ui-blocks/parse";
import { findBlocks } from "../src/lib/ui-blocks/text";

const dir = process.argv[2];
if (!dir) {
  console.error("usage: bun scripts/eval-blocks.ts <answers-dir>");
  process.exit(2);
}

const rows: { file: string; blocks: number; drawn: number; types: string[]; failed: boolean }[] = [];
for (const file of readdirSync(dir).filter((name) => /^[AB]\d\d\.txt$/.test(name)).sort()) {
  const text = readFileSync(join(dir, file), "utf8");
  const found = findBlocks(text);
  const results = found.map((block) => parseBlock(block.source));
  rows.push({
    file,
    blocks: found.length,
    drawn: results.filter((result) => result.status === "ok").length,
    types: results.map((result) => (result.status === "invalid" ? `invalid:${result.reason}` : result.block.type)),
    failed: /^\[(timeout|exit \d+)\]/m.test(text),
  });
}

for (const row of rows) {
  console.log(`${row.file}\tblocks=${row.blocks}\tdrawn=${row.drawn}\t${row.types.join(",")}${row.failed ? "\tFAILED RUN" : ""}`);
}
const set = (name: string) => rows.filter((row) => row.file.startsWith(name));
const withBlock = (list: typeof rows) => list.filter((row) => row.blocks > 0).length;
const a = set("A");
const b = set("B");
const allBlocks = rows.reduce((sum, row) => sum + row.blocks, 0);
const drawn = rows.reduce((sum, row) => sum + row.drawn, 0);
console.log(
  `\nSet A: ${withBlock(a)}/${a.length} with a block (need ≥ 16)` +
    `\nSet B: ${withBlock(b)}/${b.length} with a block (need ≤ 2)` +
    `\nBlocks drawn by the desktop: ${drawn}/${allBlocks}` +
    `\nFailed runs: ${rows.filter((row) => row.failed).map((row) => row.file).join(", ") || "none"}`,
);
