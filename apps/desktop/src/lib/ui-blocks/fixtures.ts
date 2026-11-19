// Example `evo-ui` blocks shared by the ui-blocks tests
// (docs/features/rich-replies/spec.md, FR-1, FR-3, FR-14).

/** One valid block per stage-1 component, as the model would write it. */
export const VALID = {
  stats: JSON.stringify({
    v: 1,
    type: "stats",
    title: "Usage, 10–16 Nov",
    source: "Usage page",
    items: [
      { label: "Tokens", value: 3180000000, format: "compact" },
      { label: "Cost", value: 2752.69, format: { currency: "USD" }, delta: 12.5, deltaFormat: "percent" },
      { label: "Active days", value: "7/7", hint: "every day" },
    ],
  }),
  bars: JSON.stringify({
    v: 1,
    type: "bars",
    title: "Cost per day",
    format: { currency: "USD" },
    items: [
      { label: "10 Nov", value: 168.89, action: { send: "Show details for 10 Nov" } },
      { label: "11 Nov", value: 1004.12 },
      { label: "14 Nov", value: 2.35 },
    ],
  }),
  chart: JSON.stringify({
    v: 1,
    type: "chart",
    kind: "line",
    title: "Requests",
    x: ["Mon", "Tue", "Wed"],
    format: "number",
    series: [
      { name: "api", values: [120, 340, null] },
      { name: "web", values: [80, 95, 110] },
    ],
  }),
  table: JSON.stringify({
    v: 1,
    type: "table",
    title: "Slowest packages",
    columns: [
      { key: "name", label: "Package" },
      { key: "tests", label: "Tests", format: "number", align: "right" },
      { key: "time", label: "Duration", format: "duration", align: "right" },
    ],
    rows: [
      { name: "evo-engine", tests: 812, time: 74.2, action: { open: "crates/evo-engine/src/lib.rs:1" } },
      { name: "evo-agent", tests: 140, time: 9.5 },
    ],
  }),
  steps: JSON.stringify({
    v: 1,
    type: "steps",
    items: [
      { title: "Build", status: "done" },
      { title: "Test", detail: "3 suites left", status: "current" },
      { title: "Release" },
    ],
  }),
  callout: JSON.stringify({
    v: 1,
    type: "callout",
    tone: "warning",
    text: "OpenAI usage is **missing** for 14 Nov.",
  }),
  actions: JSON.stringify({
    v: 1,
    type: "actions",
    items: [
      { label: "Show 11 Nov", action: { send: "Show details for 11 Nov" }, primary: true },
      { label: "Open the log", action: { open: "logs/usage.log" } },
    ],
  }),
} as const;

/** Blocks that cannot be drawn, by why. */
export const INVALID = {
  json: `{"v":1,"type":"stats","items":[{"label":"a","value":1}`,
  notObject: `[1, 2, 3]`,
  unknownType: JSON.stringify({ v: 1, type: "sparkle", items: [] }),
  noType: JSON.stringify({ v: 1, items: [] }),
  noVersion: JSON.stringify({ type: "stats", items: [{ label: "a", value: 1 }] }),
  missingField: JSON.stringify({ v: 1, type: "chart", kind: "line", x: ["a"] }),
  allItemsBad: JSON.stringify({ v: 1, type: "bars", items: [{ label: "a" }, { value: 2 }] }),
} as const;

/** A newer format version of a known component. */
export const NEWER = JSON.stringify({
  v: 2,
  type: "bars",
  items: [{ label: "a", value: 1 }],
  glow: true,
});

/** One bad item among good ones, and an unknown field. */
export const SOME_BAD = JSON.stringify({
  v: 1,
  type: "bars",
  sparkle: "ignored",
  items: [
    { label: "a", value: 1 },
    { label: "b", value: "lots" },
    { label: "c", value: 3, action: { run: "rm -rf /" } },
  ],
});

/** A table over its row limit. */
export function bigTable(rows: number): string {
  return JSON.stringify({
    v: 1,
    type: "table",
    columns: [{ key: "n", label: "N", format: "number" }],
    rows: Array.from({ length: rows }, (_, n) => ({ n })),
  });
}
