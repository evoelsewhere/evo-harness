// Links between conversations, as pure functions (docs/features/evo-mates-
// threads-board/spec.md, FR-17): a message's link, `#channel` and `@mate`
// written in a message as links to the place, and the person's plain text
// split at card keys, message links and channel names.

import { APP_LINK } from "@/components/file-refs";
import { mapProse } from "./markdown-prose";

/** A message's link, as Copy link gives it. */
export function messageLink(chat: string, line: string): string {
  return `evo://mates/${chat}/${line}`;
}

const MESSAGE_LINK = /evo:\/\/mates\/([A-Za-z0-9_.:-]+)\/([A-Za-z0-9_.:-]+)/g;

/** The first message link in a text. */
export function firstMessageLink(text: string): { chat: string; line: string } | null {
  const match = new RegExp(MESSAGE_LINK.source).exec(text);
  return match ? { chat: match[1]!, line: match[2]! } : null;
}

export interface Place {
  /** As written: `#Launch room` or `@Kai`. */
  word: string;
  /** The app link's path: `channel/<id>` or `mate/<id>`. */
  path: string;
}

function escape(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * `markdown` with message links, `#channel` names and `@mate` names as app
 * links (FR-17), outside code and existing links. Longer names win.
 */
export function linkPlaces(markdown: string, places: readonly Place[], placeOfChat: (chat: string) => string | null): string {
  const known = [...places].filter((p) => p.word.length > 1).sort((a, b) => b.word.length - a.word.length);
  const pattern = known.length ? new RegExp(`(^|[^\\p{L}\\p{N}\\[/.:=?&#@_-])(${known.map((p) => escape(p.word)).join("|")})(?![\\p{L}\\p{N}])`, "giu") : null;
  // A message link is a URL: it is turned into a link before prose is found.
  const linked = markdown.replace(/(^|[^\w/(\[<])(evo:\/\/mates\/[A-Za-z0-9_.:-]+\/[A-Za-z0-9_.:-]+)/g, (_, lead: string, url: string) => {
    const found = firstMessageLink(url)!;
    const place = placeOfChat(found.chat);
    return `${lead}[${place ? `message in ${place}` : "a message"}](${APP_LINK}message/${found.chat}/${found.line})`;
  });
  if (!pattern) return linked;
  return mapProse(linked, (prose) =>
    prose.replace(pattern, (_, lead: string, word: string) => {
      const place = known.find((p) => p.word.toLowerCase() === word.toLowerCase());
      return place ? `${lead}[${word}](${APP_LINK}${place.path})` : `${lead}${word}`;
    }),
  );
}

export interface Piece {
  text: string;
  /** A card's key, or an app link's path. */
  card?: string;
  path?: string;
}

/** The person's plain text in pieces: card keys, message links and `#channel` names apart (FR-14, FR-17). */
export function splitLinks(text: string, keys: readonly string[], channels: readonly Place[]): Piece[] {
  const parts: string[] = [];
  const knownKeys = keys.map((k) => k.replace(/[^A-Z0-9]/g, "")).filter(Boolean);
  if (knownKeys.length) parts.push(`(?<card>(?<![\\w-])(?:${knownKeys.join("|")})-\\d+(?![\\w-]))`);
  parts.push(`(?<link>evo://mates/[A-Za-z0-9_.:-]+/[A-Za-z0-9_.:-]+)`);
  const names = [...channels].sort((a, b) => b.word.length - a.word.length);
  if (names.length) parts.push(`(?<place>(?<![\\p{L}\\p{N}])(?:${names.map((c) => escape(c.word)).join("|")})(?![\\p{L}\\p{N}]))`);
  const pattern = new RegExp(parts.join("|"), "giu");
  const out: Piece[] = [];
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    const at = match.index ?? 0;
    if (at > last) out.push({ text: text.slice(last, at) });
    const groups = match.groups ?? {};
    if (groups.card) out.push({ text: groups.card, card: groups.card.toUpperCase() });
    else if (groups.link) {
      const found = firstMessageLink(groups.link)!;
      out.push({ text: groups.link, path: `message/${found.chat}/${found.line}` });
    } else if (groups.place) {
      const place = names.find((c) => c.word.toLowerCase() === groups.place!.toLowerCase());
      out.push(place ? { text: groups.place, path: place.path } : { text: groups.place });
    }
    last = at + match[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last) });
  return out.length ? out : [{ text }];
}
