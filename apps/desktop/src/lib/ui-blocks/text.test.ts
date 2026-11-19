// docs/features/rich-replies/spec.md (FR-4, FR-14).
import { describe, expect, test } from "bun:test";
import { INVALID, VALID, bigTable } from "./fixtures";
import { parseBlock } from "./parse";
import { blockText, findBlocks, replaceBlocks } from "./text";

const en = { locale: "en-US" };
const textOf = (source: string) => {
  const result = parseBlock(source);
  if (result.status === "invalid") throw new Error(result.message);
  return blockText(result.block, en);
};

describe("each component as text", () => {
  test("stats: one line per tile, with its change and hint", () => {
    expect(textOf(VALID.stats)).toBe(
      ["Usage, 10–16 Nov", "Tokens: 3.18B", "Cost: $2,752.69 (+12.5%)", "Active days: 7/7 — every day", "Source: Usage page"].join("\n"),
    );
  });

  test("bars: label and value", () => {
    expect(textOf(VALID.bars)).toBe(["Cost per day", "10 Nov: $168.89", "11 Nov: $1,004.12", "14 Nov: $2.35"].join("\n"));
  });

  test("chart: its data as tab-separated rows", () => {
    expect(textOf(VALID.chart)).toBe(["Requests", "\tapi\tweb", "Mon\t120\t80", "Tue\t340\t95", "Wed\t—\t110"].join("\n"));
  });

  test("table: header and rows, tab-separated", () => {
    expect(textOf(VALID.table)).toBe(
      ["Slowest packages", "Package\tTests\tDuration", "evo-engine\t812\t1m 14s", "evo-agent\t140\t9.5s"].join("\n"),
    );
  });

  test("steps: numbered, with status and detail", () => {
    expect(textOf(VALID.steps)).toBe(["1. [done] Build", "2. [current] Test — 3 suites left", "3. Release"].join("\n"));
  });

  test("callout: its tone and text", () => {
    expect(textOf(VALID.callout)).toBe("Warning: OpenAI usage is **missing** for 14 Nov.");
  });

  test("actions: the button labels", () => {
    expect(textOf(VALID.actions)).toBe("[Show 11 Nov] [Open the log]");
  });

  test("a note is kept at the end", () => {
    const result = parseBlock(bigTable(502));
    if (result.status !== "text") throw new Error("not text");
    const lines = blockText(result.block, { ...en, note: result.note }).split("\n");
    expect(lines.length).toBe(1 + 500 + 1);
    expect(lines.at(-1)).toBe("(Showing 500 of 502 rows.)");
  });
});

const reply = (body: string, fence = "```") => `Here is the week.\n\n${fence}evo-ui\n${body}\n${fence}\n\nMostly the 11th.`;

describe("blocks in a reply", () => {
  test("are found with their source and whether they are closed", () => {
    const found = findBlocks(reply(VALID.callout));
    expect(found.length).toBe(1);
    expect(found[0].source).toBe(VALID.callout);
    expect(found[0].closed).toBe(true);
  });

  test("a tilde fence and a longer fence count; other languages do not", () => {
    expect(findBlocks(reply(VALID.callout, "~~~")).length).toBe(1);
    expect(findBlocks(reply(VALID.callout, "````")).length).toBe(1);
    expect(findBlocks("```json\n{}\n```").length).toBe(0);
  });

  test("a block shown inside another code fence is not a block", () => {
    const doc = ["````markdown", "```evo-ui", VALID.callout, "```", "````"].join("\n");
    expect(findBlocks(doc).length).toBe(0);
  });

  test("an unclosed block runs to the end", () => {
    const found = findBlocks(`Text\n\`\`\`evo-ui\n{"v":1,"type":"table"`);
    expect(found).toMatchObject([{ closed: false, source: `{"v":1,"type":"table"` }]);
  });

  test("CRLF line ends", () => {
    expect(findBlocks(reply(VALID.callout).replace(/\n/g, "\r\n"))[0].source).toBe(VALID.callout);
  });
});

describe("a reply with its blocks replaced", () => {
  test("by their text, for copying", () => {
    expect(replaceBlocks(reply(VALID.callout), "text", en)).toBe(
      "Here is the week.\n\nWarning: OpenAI usage is **missing** for 14 Nov.\n\nMostly the 11th.",
    );
  });

  test("by their title, for previews", () => {
    expect(replaceBlocks(reply(VALID.bars), "title")).toBe("Here is the week.\n\n[Cost per day]\n\nMostly the 11th.");
    expect(replaceBlocks(reply(VALID.steps), "title")).toBe("Here is the week.\n\n[steps]\n\nMostly the 11th.");
  });

  test("an invalid block keeps its source as text, and shows as [block] in previews", () => {
    expect(replaceBlocks(reply(INVALID.json), "text", en)).toBe(reply(INVALID.json));
    expect(replaceBlocks(reply(INVALID.json), "title")).toBe("Here is the week.\n\n[stats]\n\nMostly the 11th.");
    expect(replaceBlocks(reply(INVALID.notObject), "title")).toBe("Here is the week.\n\n[block]\n\nMostly the 11th.");
  });

  test("a truncated preview ending inside a block gets its type", () => {
    expect(replaceBlocks(`Done.\n\`\`\`evo-ui\n{"v":1,"type":"chart","x":[1,`, "title")).toBe("Done.\n[chart]");
    expect(replaceBlocks(`Done.\n\`\`\`evo-ui\n{"v":1,"ty`, "title")).toBe("Done.\n");
  });

  test("a reply without blocks is unchanged", () => {
    const plain = "No blocks here.\n```ts\nconst a = 1;\n```";
    expect(replaceBlocks(plain, "text")).toBe(plain);
    expect(replaceBlocks(plain, "title")).toBe(plain);
  });
});
