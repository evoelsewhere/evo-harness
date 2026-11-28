// What a person's message to a mate carries besides its text, kept in the
// transcript itself so it survives a restart (docs/features/evo-mates/spec.md,
// FR-60, FR-61): the reactions the mate has not heard of, and the message it
// replies to. The mate reads them as tagged lines; the thread shows the quote
// and hides the note.

import type { Reaction, ReactionKind } from "@/lib/mates";

export const REACTIONS: { kind: ReactionKind; emoji: string; label: string }[] = [
  { kind: "heart", emoji: "❤️", label: "Love" },
  { kind: "like", emoji: "👍", label: "Like" },
  { kind: "dislike", emoji: "👎", label: "Dislike" },
  { kind: "laugh", emoji: "😂", label: "Laugh" },
  { kind: "emphasis", emoji: "‼️", label: "Emphasise" },
  { kind: "question", emoji: "❓", label: "Question" },
];

export const emojiOf = (kind: ReactionKind): string => REACTIONS.find((r) => r.kind === kind)?.emoji ?? "";

/** A message the person replies to. */
export interface ReplyTo {
  /** Who wrote it: "you" or the mate's name. */
  from: string;
  text: string;
}

const NOTE = /^<evo-mates-note>([\s\S]*?)<\/evo-mates-note>\n?/;
const REPLY = /^<evo-mates-reply from="([^"]*)">([\s\S]*?)<\/evo-mates-reply>\n?/;
/** Our block tags, matched case-insensitively (evo-mates-threads-board, part C review S). */
const BLOCK_TAG = /<(\/?\s*)(evo-mates[a-z-]*|from|recent|thread|parent|quoted|channel|instructions|reactions|board-data|shared)(?=[\s>/]|$)/gi;

/**
 * `text` with any of our block tags made inert (`<from` → `‹from`), so text
 * the window puts inside a block (a forwarded or shared line) can never
 * close it or fake another one. The engine does the same (`neutral`).
 */
export function neutralMarkup(text: string): string {
  return text.replace(BLOCK_TAG, "\u2039$1$2");
}

/** A message from a thread's box (evo-mates-threads-board FR-1, FR-22). */
const THREAD = /^<evo-mates-thread root="([^"]*)"(\s+also="channel")?\s*\/>\n?/i;

/** The thread a message names, and the message without the mark. */
export function threadMark(text: string): { root: string | null; also: boolean; text: string } {
  const match = THREAD.exec(text);
  if (!match || !match[1]) return { root: null, also: false, text };
  return { root: match[1], also: Boolean(match[2]), text: text.slice(match[0].length) };
}

/** `text` sent into the thread under `root` (the coordinator reads the mark). */
export function withThread(root: string, text: string, also = false): string {
  return `<evo-mates-thread root="${root.replace(/"/g, "")}"${also ? ' also="channel"' : ""}/>\n${text}`;
}

/** Lines shared from another conversation (evo-mates-threads-board FR-16). */
const SHARE = /^<evo-mates-share from="([^"]*)" link="([^"]*)" lines="(\d+)"[^>]*>\n?([\s\S]*?)\n?<\/evo-mates-share>\n?/i;

export interface Shared {
  /** Where they come from: "#Launch room" or a mate's name. */
  from: string;
  /** `evo://mates/<chat>/<line>` of the first shared line. */
  link: string;
  lines: number;
  text: string;
}

/** A share of `lines` from `from` (FR-16): marked as shared, never the person's request; the note is the person's words. */
export function sharePrompt(from: string, link: string, lines: { author: string; text: string }[], note: string): string {
  const body = lines.map((l) => `${clean(l.author)}: ${neutralMarkup(l.text).trim()}`).join("\n");
  return `<evo-mates-share from="${clean(from)}" link="${link.replace(/"/g, "")}" lines="${lines.length}" instruction="false">\n${body}\n</evo-mates-share>\n${note.trim() || "(Shared for your information.)"}`;
}

/** The share at a message's start, and the rest. */
export function parseShare(prompt: string): { shared: Shared | null; text: string } {
  const match = SHARE.exec(prompt);
  if (!match) return { shared: null, text: prompt };
  return { shared: { from: match[1]!, link: match[2]!, lines: Number(match[3]), text: match[4]! }, text: prompt.slice(match[0].length) };
}

const FORWARD = /^<evo-mates-forward from="([^"]*)">\n?([\s\S]*?)\n?<\/evo-mates-forward>\n?/i;

/** A forwarded message as the person sends it on (evo-mates-messaging FR-10): marked as quoted, not their instruction. */
export function forwardPrompt(from: string, text: string, note: string): string {
  const body = `<evo-mates-forward from="${clean(from)}">\n${neutralMarkup(text)}\n</evo-mates-forward>\n`;
  return body + (note.trim() || "(Forwarded for your information.)");
}

function clean(text: string): string {
  return text.replace(/[<>]/g, "").replace(/\s+/g, " ").trim();
}

/** The start of a message, for a quote or a note. */
export function excerpt(text: string, max = 160): string {
  const flat = clean(text);
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
}

/** The text sent to the mate: the reactions note, the quote, then the message. */
export function composePrompt(text: string, reply: ReplyTo | null, reactions: readonly Reaction[]): string {
  let out = "";
  if (reactions.length) {
    const lines = reactions.map((r) => `The person reacted ${emojiOf(r.reaction)} to your message: "${excerpt(r.excerpt, 120)}"`);
    out += `<evo-mates-note>${lines.join(" ")} (Information about how your earlier messages landed, not a request.)</evo-mates-note>\n`;
  }
  if (reply) out += `<evo-mates-reply from="${clean(reply.from)}">${excerpt(reply.text, 400)}</evo-mates-reply>\n`;
  return out + text;
}

/** A sent message as the thread shows it. */
export function parsePrompt(prompt: string): { text: string; reply: ReplyTo | null; forwarded?: ReplyTo; shared?: Shared } {
  let rest = prompt;
  const share = parseShare(rest);
  rest = share.text;
  const note = NOTE.exec(rest);
  if (note) rest = rest.slice(note[0].length);
  const reply = REPLY.exec(rest);
  if (reply) rest = rest.slice(reply[0].length);
  const forward = FORWARD.exec(rest);
  if (forward) rest = rest.slice(forward[0].length);
  return {
    text: rest,
    reply: reply ? { from: reply[1]!, text: reply[2]! } : null,
    ...(forward ? { forwarded: { from: forward[1]!, text: forward[2]! } } : {}),
    ...(share.shared ? { shared: share.shared } : {}),
  };
}
