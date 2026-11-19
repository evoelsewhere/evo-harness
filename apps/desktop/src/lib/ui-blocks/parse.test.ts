// docs/features/rich-replies/spec.md (FR-1, FR-3, FR-14, NFR-4).
import { describe, expect, test } from "bun:test";
import { INVALID, NEWER, SOME_BAD, VALID, bigTable } from "./fixtures";
import { MAX_PARSE_BYTES, parseBlock, sniffType } from "./parse";

describe("a valid block of each component", () => {
  for (const [type, source] of Object.entries(VALID)) {
    test(`${type} parses`, () => {
      const result = parseBlock(source);
      expect(result.status).toBe("ok");
      if (result.status !== "invalid") {
        expect(result.block.type).toBe(type as never);
        expect(result.dropped).toBe(0);
      }
    });
  }

  test("keeps the title, source and the component's fields", () => {
    const result = parseBlock(VALID.stats);
    if (result.status === "invalid" || result.block.type !== "stats") throw new Error("not stats");
    expect(result.block.title).toBe("Usage, 10–16 Nov");
    expect(result.block.source).toBe("Usage page");
    expect(result.block.items[1]).toEqual({
      label: "Cost",
      value: 2752.69,
      format: { currency: "USD" },
      delta: 12.5,
      deltaFormat: "percent",
    });
  });

  test("keeps actions that are allowed", () => {
    const bars = parseBlock(VALID.bars);
    if (bars.status === "invalid" || bars.block.type !== "bars") throw new Error("not bars");
    expect(bars.block.items[0].action).toEqual({ send: "Show details for 10 Nov" });
    const table = parseBlock(VALID.table);
    if (table.status === "invalid" || table.block.type !== "table") throw new Error("not table");
    expect(table.block.rows[0].action).toEqual({ open: "crates/evo-engine/src/lib.rs:1" });
  });
});

describe("blocks that cannot be drawn", () => {
  test("invalid JSON", () => {
    expect(parseBlock(INVALID.json)).toMatchObject({ status: "invalid", reason: "json" });
  });

  test("not an object", () => {
    expect(parseBlock(INVALID.notObject)).toMatchObject({ status: "invalid", reason: "not-object" });
  });

  test("an unknown or missing type", () => {
    expect(parseBlock(INVALID.unknownType)).toMatchObject({ status: "invalid", reason: "unknown-type", type: "sparkle" });
    expect(parseBlock(INVALID.noType)).toMatchObject({ status: "invalid", reason: "unknown-type" });
  });

  test("no format version", () => {
    expect(parseBlock(INVALID.noVersion)).toMatchObject({ status: "invalid", reason: "missing-field", type: "stats" });
  });

  test("a missing required field", () => {
    expect(parseBlock(INVALID.missingField)).toMatchObject({ status: "invalid", reason: "missing-field", type: "chart" });
  });

  test("every item invalid", () => {
    expect(parseBlock(INVALID.allItemsBad)).toMatchObject({ status: "invalid", reason: "missing-field", type: "bars" });
  });

  test("the message says what is wrong", () => {
    const result = parseBlock(INVALID.missingField);
    if (result.status !== "invalid") throw new Error("valid");
    expect(result.message).toContain("series");
  });
});

describe("a newer version of a known component", () => {
  test("is shown as text, not drawn", () => {
    const result = parseBlock(NEWER);
    expect(result.status).toBe("text");
    if (result.status === "text") {
      expect(result.block.type).toBe("bars");
      expect(result.note).toContain("newer");
    }
  });
});

describe("bad items and unknown fields", () => {
  test("a bad item is dropped and counted; a bad action is dropped", () => {
    const result = parseBlock(SOME_BAD);
    if (result.status !== "ok" || result.block.type !== "bars") throw new Error("not ok");
    expect(result.block.items.map((item) => item.label)).toEqual(["a", "c"]);
    expect(result.dropped).toBe(1);
    expect(result.block.items[1].action).toBeUndefined();
  });

  test("unknown fields are ignored", () => {
    const result = parseBlock(SOME_BAD);
    if (result.status === "invalid") throw new Error("invalid");
    expect("sparkle" in result.block).toBe(false);
  });

  test("an invalid format is dropped, not the block", () => {
    const result = parseBlock(JSON.stringify({ v: 1, type: "bars", format: "furlongs", items: [{ label: "a", value: 1 }] }));
    if (result.status !== "ok" || result.block.type !== "bars") throw new Error("not ok");
    expect(result.block.format).toBeUndefined();
  });

  test("a currency code must be three letters", () => {
    const result = parseBlock(
      JSON.stringify({ v: 1, type: "bars", format: { currency: "dollars" }, items: [{ label: "a", value: 1 }] }),
    );
    if (result.status !== "ok" || result.block.type !== "bars") throw new Error("not ok");
    expect(result.block.format).toBeUndefined();
  });

  test("table columns given as names are their own key and label", () => {
    const result = parseBlock(
      JSON.stringify({ v: 1, type: "table", columns: ["File", "Lines"], rows: [{ File: "a.rs", Lines: 12 }] }),
    );
    if (result.status !== "ok" || result.block.type !== "table") throw new Error("not ok");
    expect(result.block.columns).toEqual([
      { key: "File", label: "File" },
      { key: "Lines", label: "Lines" },
    ]);
    expect(result.block.rows[0].cells).toEqual({ File: "a.rs", Lines: 12 });
  });

  test("a chart series whose length differs from x is dropped", () => {
    const result = parseBlock(
      JSON.stringify({
        v: 1,
        type: "chart",
        kind: "bar",
        x: ["a", "b"],
        series: [
          { name: "ok", values: [1, 2] },
          { name: "short", values: [1] },
        ],
      }),
    );
    if (result.status !== "ok" || result.block.type !== "chart") throw new Error("not ok");
    expect(result.block.series.map((series) => series.name)).toEqual(["ok"]);
    expect(result.dropped).toBe(1);
  });
});

describe("limits", () => {
  test("a table over 500 rows is shown as text, truncated", () => {
    const result = parseBlock(bigTable(600));
    expect(result.status).toBe("text");
    if (result.status === "text" && result.block.type === "table") {
      expect(result.block.rows.length).toBe(500);
      expect(result.note).toContain("500 of 600");
    }
  });

  test("more than 8 tiles is over the limit", () => {
    const items = Array.from({ length: 9 }, (_, n) => ({ label: `t${n}`, value: n }));
    const result = parseBlock(JSON.stringify({ v: 1, type: "stats", items }));
    expect(result.status).toBe("text");
    if (result.status === "text" && result.block.type === "stats") expect(result.block.items.length).toBe(8);
  });

  test("more than 4 buttons is over the limit", () => {
    const items = Array.from({ length: 5 }, (_, n) => ({ label: `b${n}`, action: { send: `b${n}` } }));
    const result = parseBlock(JSON.stringify({ v: 1, type: "actions", items }));
    expect(result.status).toBe("text");
  });

  test("a chart over 1,000 points keeps the first x values", () => {
    const x = Array.from({ length: 600 }, (_, n) => n);
    const series = [
      { name: "a", values: x },
      { name: "b", values: x },
    ];
    const result = parseBlock(JSON.stringify({ v: 1, type: "chart", kind: "line", x, series }));
    expect(result.status).toBe("text");
    if (result.status === "text" && result.block.type === "chart") {
      expect(result.block.x.length).toBe(500);
      expect(result.block.series[0].values.length).toBe(500);
    }
  });

  test("a body over 32 KB is shown as text", () => {
    const source = bigTable(4000);
    expect(source.length).toBeGreaterThan(32 * 1024);
    const result = parseBlock(source);
    expect(result.status).toBe("text");
    if (result.status === "text") expect(result.note).toContain("Too large");
  });

  test("a body too large to read is not parsed", () => {
    const source = `{"v":1,"type":"callout","tone":"info","text":"${"x".repeat(MAX_PARSE_BYTES)}"}`;
    expect(parseBlock(source)).toMatchObject({ status: "invalid", reason: "too-big" });
  });
});

describe("the type of a block still being written", () => {
  test("is read from the partial body", () => {
    expect(sniffType(`{"v":1,"type":"chart","kind":"li`)).toBe("chart");
    expect(sniffType(`{ "type" : "table"`)).toBe("table");
  });

  test("is unknown before it arrives or when it is not a component", () => {
    expect(sniffType(`{"v":1,"ty`)).toBeUndefined();
    expect(sniffType(`{"type":"sparkle"}`)).toBeUndefined();
  });
});
