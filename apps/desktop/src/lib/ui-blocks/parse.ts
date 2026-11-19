// Reading an `evo-ui` block (docs/features/rich-replies/spec.md, FR-1, FR-2,
// FR-3, FR-14, NFR-4). A block is untrusted model output: it is parsed with
// JSON.parse and copied field by field into the typed shape, so unknown
// fields never reach the components.

import {
  BLOCK_TYPES,
  CALLOUT_TONES,
  CHART_KINDS,
  FORMAT_VERSION,
  LIMITS,
  NAMED_FORMATS,
  STEP_STATUSES,
  type Action,
  type BlockType,
  type Cell,
  type Format,
  type TableColumn,
  type TableRow,
  type UiBlock,
} from "./types";

/** Bodies longer than this are not parsed at all (they are shown as code). */
export const MAX_PARSE_BYTES = 512 * 1024;

export type InvalidReason = "json" | "not-object" | "unknown-type" | "missing-field" | "too-big";

export type ParseResult =
  /** Draw it. `dropped` items were invalid and left out. */
  | { status: "ok"; block: UiBlock; dropped: number }
  /** Show its text rendering with `note`: a newer version or over a limit. */
  | { status: "text"; block: UiBlock; dropped: number; note: string }
  /** Show the raw source with "Couldn't draw this block". */
  | { status: "invalid"; reason: InvalidReason; message: string; type?: string };

type Json = Record<string, unknown>;

const isObject = (value: unknown): value is Json => typeof value === "object" && value !== null && !Array.isArray(value);
const isNumber = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);
const text = (value: unknown): string | undefined => (typeof value === "string" ? value : undefined);
const oneOf = <T extends string>(options: readonly T[], value: unknown): T | undefined =>
  options.includes(value as T) ? (value as T) : undefined;

function format(value: unknown): Format | undefined {
  const named = oneOf(NAMED_FORMATS, value);
  if (named) return named;
  if (isObject(value) && typeof value.currency === "string" && /^[A-Za-z]{3}$/.test(value.currency)) {
    return { currency: value.currency.toUpperCase() };
  }
  return undefined;
}

function action(value: unknown): Action | undefined {
  if (!isObject(value)) return undefined;
  const keys = Object.keys(value);
  if (keys.length !== 1) return undefined;
  if (typeof value.send === "string" && value.send.trim()) return { send: value.send };
  if (typeof value.open === "string" && value.open.trim()) return { open: value.open };
  return undefined;
}

/** A required field is missing or wrong. */
class Missing extends Error {}

/** Builds one component from its JSON, counting dropped items and limits hit. */
class Reader {
  dropped = 0;
  over: string[] = [];

  constructor(readonly json: Json) {}

  list(key: string): unknown[] {
    const value = this.json[key];
    if (!Array.isArray(value)) throw new Missing(`"${key}" must be a list`);
    return value;
  }

  /** Keeps the items `read` accepts; at least one must remain. */
  items<T>(key: string, read: (item: unknown) => T | undefined, limit: number, noun = "items"): T[] {
    const all = this.list(key);
    const kept: T[] = [];
    for (const item of all) {
      const value = read(item);
      if (value === undefined) this.dropped++;
      else kept.push(value);
    }
    if (kept.length === 0) throw new Missing(`"${key}" has no valid ${noun}`);
    return this.cap(kept, limit, noun);
  }

  cap<T>(list: T[], limit: number, noun: string): T[] {
    if (list.length <= limit) return list;
    this.over.push(`${limit} of ${list.length} ${noun}`);
    return list.slice(0, limit);
  }

  required<T>(key: string, value: T | undefined, what: string): T {
    if (value === undefined) throw new Missing(`"${key}" must be ${what}`);
    return value;
  }
}

function readComponent(type: BlockType, r: Reader): UiBlock {
  const json = r.json;
  const base = { v: json.v as number, title: text(json.title), source: text(json.source) };
  switch (type) {
    case "stats":
      return {
        ...base,
        type,
        items: r.items(
          "items",
          (item) => {
            if (!isObject(item) || typeof item.label !== "string") return undefined;
            if (!isNumber(item.value) && typeof item.value !== "string") return undefined;
            return {
              label: item.label,
              value: item.value,
              format: format(item.format),
              delta: isNumber(item.delta) ? item.delta : undefined,
              deltaFormat: format(item.deltaFormat),
              hint: text(item.hint),
            };
          },
          LIMITS.stats,
          "tiles",
        ),
      };
    case "bars":
      return {
        ...base,
        type,
        format: format(json.format),
        max: isNumber(json.max) && json.max > 0 ? json.max : undefined,
        items: r.items(
          "items",
          (item) =>
            isObject(item) && typeof item.label === "string" && isNumber(item.value)
              ? { label: item.label, value: item.value, action: action(item.action) }
              : undefined,
          LIMITS.bars,
        ),
      };
    case "chart": {
      const kind = r.required("kind", oneOf(CHART_KINDS, json.kind), `one of ${CHART_KINDS.join(", ")}`);
      const x = r.list("x").filter((value): value is string | number => typeof value === "string" || isNumber(value));
      if (x.length === 0) throw new Missing(`"x" has no labels`);
      let series = r.items(
        "series",
        (item) => {
          if (!isObject(item) || typeof item.name !== "string" || !Array.isArray(item.values)) return undefined;
          if (item.values.length !== x.length) return undefined;
          return { name: item.name, values: item.values.map((value) => (isNumber(value) ? value : null)) };
        },
        LIMITS.chartSeries,
        "series",
      );
      const points = Math.floor(LIMITS.chartPoints / series.length);
      const xs = r.cap(x, points, "points per series");
      if (xs.length < x.length) series = series.map((one) => ({ ...one, values: one.values.slice(0, xs.length) }));
      return { ...base, type, kind, stacked: json.stacked === true ? true : undefined, x: xs, format: format(json.format), series };
    }
    case "table": {
      const columns = r.items(
        "columns",
        (item): TableColumn | undefined =>
          // A bare name is a column whose key and label are that name.
          typeof item === "string" && item
            ? { key: item, label: item }
            : isObject(item) && typeof item.key === "string"
              ? {
                  key: item.key,
                  label: text(item.label) ?? item.key,
                  format: format(item.format),
                  align: oneOf(["left", "right"] as const, item.align),
                }
              : undefined,
        LIMITS.tableColumns,
        "columns",
      );
      const rows = r.items(
        "rows",
        (item): TableRow | undefined => {
          if (!isObject(item)) return undefined;
          const cells: Record<string, Cell> = {};
          for (const column of columns) {
            const value = item[column.key];
            cells[column.key] = isNumber(value) || typeof value === "string" ? value : null;
          }
          return { cells, action: action(item.action) };
        },
        LIMITS.tableRows,
        "rows",
      );
      return { ...base, type, columns, rows };
    }
    case "steps":
      return {
        ...base,
        type,
        items: r.items(
          "items",
          (item) =>
            isObject(item) && typeof item.title === "string"
              ? { title: item.title, detail: text(item.detail), status: oneOf(STEP_STATUSES, item.status) }
              : undefined,
          Number.POSITIVE_INFINITY,
          "steps",
        ),
      };
    case "callout":
      return {
        ...base,
        type,
        tone: r.required("tone", oneOf(CALLOUT_TONES, json.tone), `one of ${CALLOUT_TONES.join(", ")}`),
        text: r.required("text", text(json.text), "text"),
      };
    case "actions":
      return {
        ...base,
        type,
        items: r.items(
          "items",
          (item) => {
            if (!isObject(item) || typeof item.label !== "string") return undefined;
            const what = action(item.action);
            return what ? { label: item.label, action: what, primary: item.primary === true ? true : undefined } : undefined;
          },
          LIMITS.actions,
          "buttons",
        ),
      };
  }
}

/** Removes keys whose value is undefined, so blocks compare and print cleanly. */
function clean<T>(value: T): T {
  if (Array.isArray(value)) return value.map(clean) as T;
  if (!isObject(value)) return value;
  const out: Json = {};
  for (const [key, inner] of Object.entries(value)) if (inner !== undefined) out[key] = clean(inner);
  return out as T;
}

/** Parses and validates one block's body (the text inside its fence). */
export function parseBlock(source: string): ParseResult {
  if (source.length > MAX_PARSE_BYTES) {
    return { status: "invalid", reason: "too-big", message: `The block is ${Math.round(source.length / 1024)} KB.` };
  }
  let json: unknown;
  try {
    json = JSON.parse(source);
  } catch (error) {
    return { status: "invalid", reason: "json", message: error instanceof Error ? error.message : "Not JSON." };
  }
  if (!isObject(json)) return { status: "invalid", reason: "not-object", message: "The block is not a JSON object." };
  const type = oneOf(BLOCK_TYPES, json.type);
  if (!type) {
    return {
      status: "invalid",
      reason: "unknown-type",
      message: typeof json.type === "string" ? `Unknown component "${json.type}".` : `"type" is missing.`,
      type: text(json.type),
    };
  }
  if (!isNumber(json.v) || !Number.isInteger(json.v) || json.v < 1) {
    return { status: "invalid", reason: "missing-field", message: `"v" must be the format version (1).`, type };
  }
  const reader = new Reader(json);
  let block: UiBlock;
  try {
    block = clean(readComponent(type, reader));
  } catch (error) {
    if (!(error instanceof Missing)) throw error;
    return { status: "invalid", reason: "missing-field", message: error.message, type };
  }
  const notes: string[] = [];
  if (json.v > FORMAT_VERSION) notes.push(`Made for a newer version of Evo (format ${json.v}).`);
  if (source.length > LIMITS.bodyBytes) notes.push(`Too large to draw (${Math.round(source.length / 1024)} KB).`);
  if (reader.over.length) notes.push(`Showing ${reader.over.join(", ")}.`);
  if (notes.length) return { status: "text", block, dropped: reader.dropped, note: notes.join(" ") };
  return { status: "ok", block, dropped: reader.dropped };
}

/** The component of a block still being written, once its `type` has arrived (FR-2). */
export function sniffType(partial: string): BlockType | undefined {
  const match = /"type"\s*:\s*"([a-z-]+)"/.exec(partial);
  return match ? oneOf(BLOCK_TYPES, match[1]) : undefined;
}
