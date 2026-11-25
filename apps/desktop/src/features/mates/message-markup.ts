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
export function parsePrompt(prompt: string): { text: string; reply: ReplyTo | null } {
  let rest = prompt;
  const note = NOTE.exec(rest);
  if (note) rest = rest.slice(note[0].length);
  const reply = REPLY.exec(rest);
  if (reply) rest = rest.slice(reply[0].length);
  return { text: rest, reply: reply ? { from: reply[1]!, text: reply[2]! } : null };
}
