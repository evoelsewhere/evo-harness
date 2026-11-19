// A block as text (docs/features/rich-replies/spec.md, FR-4, FR-14): what
// copy, quote, previews and notifications show in place of a block's JSON.
// Also finds the `evo-ui` fences in a reply.

import { formatValue, type FormatOptions } from "./format";
import { parseBlock, sniffType } from "./parse";
import { BLOCK_FENCE, type UiBlock } from "./types";

export interface BlockTextOptions extends FormatOptions {
  /** Said at the end, e.g. that the block was truncated. */
  note?: string;
}

const capital = (word: string) => word.charAt(0).toUpperCase() + word.slice(1);

function body(block: UiBlock, options: FormatOptions): string[] {
  const format = (value: Parameters<typeof formatValue>[0], how?: Parameters<typeof formatValue>[1]) =>
    formatValue(value, how, options);
  switch (block.type) {
    case "stats":
      return block.items.map((item) => {
        let line = `${item.label}: ${format(item.value, item.format)}`;
        if (item.delta !== undefined) {
          line += ` (${formatValue(item.delta, item.deltaFormat ?? item.format, { ...options, signed: true })})`;
        }
        return item.hint ? `${line} — ${item.hint}` : line;
      });
    case "bars":
      return block.items.map((item) => `${item.label}: ${format(item.value, block.format)}`);
    case "chart":
      return [
        ["", ...block.series.map((series) => series.name)].join("\t"),
        ...block.x.map((x, index) =>
          [String(x), ...block.series.map((series) => format(series.values[index], block.format))].join("\t"),
        ),
      ];
    case "table":
      return [
        block.columns.map((column) => column.label).join("\t"),
        ...block.rows.map((row) => block.columns.map((column) => format(row.cells[column.key], column.format)).join("\t")),
      ];
    case "steps":
      return block.items.map(
        (item, index) =>
          `${index + 1}. ${item.status ? `[${item.status}] ` : ""}${item.title}${item.detail ? ` — ${item.detail}` : ""}`,
      );
    case "callout":
      return [`${capital(block.tone)}: ${block.text}`];
    case "actions":
      return [block.items.map((item) => `[${item.label}]`).join(" ")];
  }
}

/** A block as plain text, line by line. */
export function blockText(block: UiBlock, options: BlockTextOptions = {}): string {
  const lines = [...(block.title ? [block.title] : []), ...body(block, options)];
  if (block.source) lines.push(`Source: ${block.source}`);
  if (options.note) lines.push(`(${options.note})`);
  return lines.join("\n");
}

/** An `evo-ui` fence in a reply. `start`/`end` index the reply with `\n` line ends. */
export interface FoundBlock {
  start: number;
  end: number;
  /** The text between the fences. */
  source: string;
  /** False when the reply ends before the closing fence (still streaming, or cut). */
  closed: boolean;
}

const OPEN = /^ {0,3}(`{3,}|~{3,})(.*)$/;
const CLOSE = /^ {0,3}(`{3,}|~{3,})[ \t]*$/;

/** The `evo-ui` fences of a reply, skipping blocks shown inside other code fences. */
export function findBlocks(markdown: string): FoundBlock[] {
  const text = markdown.replace(/\r\n?/g, "\n");
  const found: FoundBlock[] = [];
  let fence: { marker: string; start: number; bodyStart: number; block: boolean } | null = null;
  let offset = 0;
  for (const line of text.split("\n")) {
    const lineEnd = offset + line.length;
    if (fence) {
      const close = CLOSE.exec(line);
      if (close && close[1][0] === fence.marker[0] && close[1].length >= fence.marker.length) {
        if (fence.block) {
          found.push({
            start: fence.start,
            end: lineEnd,
            source: text.slice(fence.bodyStart, Math.max(fence.bodyStart, offset - 1)),
            closed: true,
          });
        }
        fence = null;
      }
    } else {
      const open = OPEN.exec(line);
      // A backtick fence's info string may not contain a backtick.
      if (open && !(open[1][0] === "`" && open[2].includes("`"))) {
        fence = { marker: open[1], start: offset, bodyStart: lineEnd + 1, block: open[2].trim() === BLOCK_FENCE };
      }
    }
    offset = lineEnd + 1;
  }
  if (fence?.block) {
    found.push({ start: fence.start, end: text.length, source: text.slice(Math.min(fence.bodyStart, text.length)), closed: false });
  }
  return found;
}

/** The text of a block's source, or undefined when it cannot be read. */
export function blockTextOf(source: string, options: FormatOptions = {}): string | undefined {
  const result = parseBlock(source);
  if (result.status === "invalid") return undefined;
  return blockText(result.block, { ...options, note: result.status === "text" ? result.note : undefined });
}

/** A block's name for a one-line preview: `[title]`, `[type]`, `[block]`, or "" for a cut block of no known type. */
export function blockTitleOf(source: string, closed = true): string {
  const result = parseBlock(source);
  if (result.status !== "invalid") return `[${result.block.title ?? result.block.type}]`;
  const type = sniffType(source);
  if (type) return `[${type}]`;
  return closed ? "[block]" : "";
}

/**
 * The reply with each `evo-ui` block replaced: by its text (`"text"`, for
 * copy and quote; a block that cannot be read keeps its source) or by its
 * name (`"title"`, for previews). Line ends become `\n`.
 */
export function replaceBlocks(markdown: string, mode: "text" | "title", options: FormatOptions = {}): string {
  const text = markdown.replace(/\r\n?/g, "\n");
  const found = findBlocks(text);
  if (found.length === 0) return text;
  let out = "";
  let at = 0;
  for (const block of found) {
    const original = text.slice(block.start, block.end);
    const replacement =
      mode === "title" ? blockTitleOf(block.source, block.closed) : (blockTextOf(block.source, options) ?? original);
    out += text.slice(at, block.start) + replacement;
    at = block.end;
  }
  return out + text.slice(at);
}
