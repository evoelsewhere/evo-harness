// The rows of a channel's thread (docs/features/evo-mates/spec.md, FR-14 –
// FR-19, FR-58): the person's messages on the right, each mate's on the
// left under its name, consecutive messages of one author grouped, a time
// line when more than 15 minutes pass, and the channel's own lines (who
// answers, hand-offs, notices) centred between them.

import type { ChannelEntry } from "@/lib/mates";
import { timeLine } from "./thread-rows";

const GAP_MS = 15 * 60_000;

export type ChannelRow =
  | { kind: "time"; key: string; text: string }
  | {
      kind: "bubble";
      key: string;
      /** `person`, or the mate's id. */
      author: string;
      name: string;
      text: string;
      me: boolean;
      first: boolean;
      last: boolean;
      failed: boolean;
    }
  | { kind: "route"; key: string; text: string }
  | { kind: "handoff"; key: string; from: string; fromName: string; toName: string; handoff: string; text: string }
  | { kind: "notice"; key: string; text: string; stop: boolean };

/** `text` without markdown marks: bold, italics, code, headings, links. */
export function plainText(text: string): string {
  return text
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/(\*\*|__|`)/g, "")
    .replace(/(^|\s)[*_](\S[^*_]*\S|\S)[*_](?=\s|$|[.,!?;:])/g, "$1$2")
    .replace(/^\s*#+\s*/gm, "");
}

/** One plain line for a hand-off: "Planner assigns Writer: draft the intro". */
export function handoffLine(row: { fromName: string; toName: string; handoff: string; text: string }): string {
  const text = plainText(row.text.replace(/^\s*\[assign\]\s*/i, "")).replace(/\s+/g, " ").trim();
  const short = text.length > 90 ? `${text.slice(0, 89)}…` : text;
  return `${row.fromName} ${row.handoff} ${row.toName}${short ? `: ${short}` : ""}`;
}

export function channelRows(entries: ChannelEntry[], nameOf: (mate: string) => string, now = Date.now()): ChannelRow[] {
  const rows: ChannelRow[] = [];
  let lastAt: number | null = null;
  entries.forEach((entry, index) => {
    const key = `${entry.kind}:${index}`;
    if (entry.kind === "message" && (lastAt === null || entry.at - lastAt > GAP_MS)) {
      rows.push({ kind: "time", key: `time:${index}`, text: timeLine(entry.at, now) });
    }
    if (entry.kind === "message") lastAt = entry.at;
    switch (entry.kind) {
      case "message": {
        const me = entry.author === "person";
        rows.push({
          kind: "bubble",
          key: entry.id || key,
          author: entry.author,
          name: me ? "You" : entry.name || nameOf(entry.author),
          text: entry.text,
          me,
          first: true,
          last: true,
          failed: Boolean(entry.status && entry.status !== "done"),
        });
        break;
      }
      case "route":
        // The line says who answers; a message nobody answers says why.
        rows.push({ kind: "route", key, text: entry.line });
        break;
      case "handoff":
        rows.push({
          kind: "handoff",
          key,
          from: entry.from,
          fromName: entry.fromName,
          toName: entry.toName,
          handoff: entry.handoff,
          text: entry.text,
        });
        break;
      case "notice":
        rows.push({ kind: "notice", key, text: entry.text, stop: entry.notice === "limit" || entry.notice === "stopped" });
        break;
    }
  });
  // Group runs of one author: only the first shows the name, only the last the tail.
  for (let i = 0; i < rows.length; i++) {
    const row = rows[i];
    if (row.kind !== "bubble") continue;
    const prev = rows[i - 1];
    const next = rows[i + 1];
    row.first = !(prev?.kind === "bubble" && prev.author === row.author);
    row.last = !(next?.kind === "bubble" && next.author === row.author);
  }
  return rows;
}

/** The `@` word being typed at `caret` in `text`, if any: its start and what follows the `@`. */
export function mentionAt(text: string, caret: number): { start: number; query: string } | null {
  const before = text.slice(0, caret);
  const match = /(^|\s)@([^\s@]*)$/.exec(before);
  if (!match) return null;
  return { start: caret - match[2].length - 1, query: match[2] };
}

/** `text` with the `@` word at `start` (up to `caret`) replaced by `@name `. */
export function insertMention(text: string, start: number, caret: number, name: string): { text: string; caret: number } {
  const inserted = `@${name} `;
  return { text: text.slice(0, start) + inserted + text.slice(caret), caret: start + inserted.length };
}

/** Pieces of `text` split at mentions of `names` (and `@everyone`), for drawing them out. */
export function mentionParts(text: string, names: string[]): { text: string; mention: boolean }[] {
  const known = [...names, "everyone"].filter(Boolean).sort((a, b) => b.length - a.length);
  if (known.length === 0) return [{ text, mention: false }];
  const escaped = known.map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const pattern = new RegExp(`(^|[^\\p{L}\\p{N}])(@(?:${escaped.join("|")}))(?![\\p{L}\\p{N}])`, "giu");
  const parts: { text: string; mention: boolean }[] = [];
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    const start = (match.index ?? 0) + match[1].length;
    if (start > last) parts.push({ text: text.slice(last, start), mention: false });
    parts.push({ text: match[2], mention: true });
    last = start + match[2].length;
  }
  if (last < text.length) parts.push({ text: text.slice(last), mention: false });
  return parts;
}

/** Markdown with each mention in bold (a mention already in bold stays as it is). */
export function boldMentions(markdown: string, names: string[]): string {
  return mentionParts(markdown, names)
    .map((part, i, all) => {
      if (!part.mention) return part.text;
      const before = all[i - 1]?.text ?? "";
      return before.endsWith("**") ? part.text : `**${part.text}**`;
    })
    .join("");
}
