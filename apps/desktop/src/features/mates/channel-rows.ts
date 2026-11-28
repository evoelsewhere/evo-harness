// The rows of a channel's thread (docs/features/evo-mates/spec.md, FR-14 –
// FR-19, FR-58): the person's messages on the right, each mate's on the
// left under its name, consecutive messages of one author grouped, a time
// line when more than 15 minutes pass, and the channel's own lines (who
// answers, hand-offs, notices) centred between them.

import type { ChannelEntry, RetroCard } from "@/lib/mates";
import { timeLine } from "./thread-rows";

function dayOf(at: number): number {
  const d = new Date(at);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** The verb of a hand-off as its card says it (evo-mates-ui-v2 FR-20). */
export function handoffVerb(kind: string): string {
  if (kind === "assigns") return "assigned";
  if (kind === "asks") return "asked";
  if (kind === "tells") return "told";
  return kind;
}

/** A hand-off's text in one plain line, without its "[assign]" mark. */
export function handoffSummary(text: string, max = 90): string {
  const plain = plainText(text.replace(/^\s*\[assign\]\s*/i, "")).replace(/\s+/g, " ").trim();
  return plain.length > max ? `${plain.slice(0, max - 1)}…` : plain;
}

const GAP_MS = 15 * 60_000;

export type ChannelRow =
  /** A day's separator (evo-mates-ui-v2 FR-11). */
  | { kind: "day"; key: string; at: number }
  | { kind: "time"; key: string; text: string; at: number }
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
      /** A message of the person that went into this mate's turn (FR-56). */
      steers?: string;
      /** The mates that took this message of the person in (evo-mates-messaging FR-2). */
      seenBy?: string[];
      /** The line a mate's line answers, with its author and words (FR-5). */
      quote?: { id: string; name: string; text: string };
    }
  /** Who answers; `picked` are the mates' ids, for their faces (evo-mates-ui-v2 FR-20). */
  | { kind: "route"; key: string; text: string; picked: string[] }
  | { kind: "handoff"; key: string; from: string; fromName: string; to: string; toName: string; handoff: string; text: string; files: string[] }
  | { kind: "notice"; key: string; text: string; stop: boolean }
  | { kind: "retro"; key: string; card: RetroCard };

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
  // Each line by its id, for the quotes that point at it (FR-5).
  const lines = new Map<string, { name: string; text: string }>();
  for (const entry of entries) {
    if (entry.kind === "message") lines.set(entry.id, { name: entry.author === "person" ? "You" : entry.name || nameOf(entry.author), text: entry.text });
  }
  let lastAt: number | null = null;
  entries.forEach((entry, index) => {
    const key = `${entry.kind}:${index}`;
    if (entry.kind === "message" && (lastAt === null || dayOf(entry.at) !== dayOf(lastAt))) {
      rows.push({ kind: "day", key: `day:${index}`, at: entry.at });
    }
    if (entry.kind === "message" && (lastAt === null || entry.at - lastAt > GAP_MS || dayOf(entry.at) !== dayOf(lastAt))) {
      rows.push({ kind: "time", key: `time:${index}`, text: timeLine(entry.at, now), at: entry.at });
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
          steers: entry.steers ? nameOf(entry.steers) : undefined,
          quote: entry.quote && lines.has(entry.quote) ? { id: entry.quote, ...lines.get(entry.quote)! } : undefined,
        });
        break;
      }
      case "route":
        // The line says who answers; a message nobody answers says why.
        rows.push({ kind: "route", key, text: entry.line, picked: entry.picked ?? [] });
        break;
      case "handoff":
        rows.push({
          kind: "handoff",
          key,
          from: entry.from,
          fromName: entry.fromName,
          to: entry.to,
          toName: entry.toName,
          handoff: entry.handoff,
          text: entry.text,
          files: entry.files ?? [],
        });
        break;
      case "notice":
        rows.push({ kind: "notice", key, text: entry.text, stop: entry.notice === "limit" || entry.notice === "stopped" });
        break;
      case "retro":
        rows.push({
          kind: "retro",
          key: entry.id || key,
          card: {
            job: entry.job,
            status: entry.status,
            handoffs: entry.handoffs ?? 0,
            well: entry.well ?? [],
            wrong: entry.wrong ?? [],
            lessons: entry.lessons ?? [],
          },
        });
        break;
    }
  });
  // Who took each of the person's messages in: the mates whose turn ran on it,
  // known by their lines that answer it (FR-2, A-2; review 1a #9). The view adds
  // the mates at work on the latest message.
  const picked = new Map<string, string[]>();
  for (const entry of entries) {
    if (entry.kind === "message" && entry.author !== "person" && entry.replyTo)
      picked.set(entry.replyTo, [...(picked.get(entry.replyTo) ?? []), entry.author]);
  }
  for (const row of rows) {
    if (row.kind === "bubble" && row.me && picked.has(row.key)) row.seenBy = Array.from(new Set(picked.get(row.key)));
  }
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

/** Why **Run retro** did not run, in plain words (learning-layers FR-19, FR-24). */
export function retroFailure(error: string): string {
  const reason = error.replace(/^Error:\s*/, "").replace(/^no retro:\s*/, "");
  if (reason === "no model") return "No model for the retro: pick a Background model in Settings › Evo Mates, or a model for the lead.";
  if (reason === "no job") return "Nothing to look back on yet.";
  if (/already running/.test(reason)) return "A retro is already running.";
  return "The retro did not run.";
}
