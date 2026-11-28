// What a channel's board shows, as pure functions (docs/features/evo-mates-
// threads-board/spec.md, FR-8 – FR-15): filters and search, cards per
// column, where a drop lands, overdue dates, and `KEY-n` written in a
// message as a link to its card.

import { CARD_LINK } from "@/components/file-refs";
import type { Card, CardPriority } from "@/lib/mates";

/** How each priority reads and looks (FR-9). */
export const PRIORITIES: { id: CardPriority; label: string; tone: string }[] = [
  { id: "none", label: "No priority", tone: "text-muted-foreground" },
  { id: "low", label: "Low", tone: "text-muted-foreground" },
  { id: "medium", label: "Medium", tone: "text-amber-600 dark:text-amber-400" },
  { id: "high", label: "High", tone: "text-orange-600 dark:text-orange-400" },
  { id: "urgent", label: "Urgent", tone: "text-destructive" },
];

export interface CardFilter {
  query: string;
  /** `person`, a mate's id, `none` (unassigned) or empty for all. */
  assignee: string;
  label: string;
  priority: CardPriority | "";
}

export const NO_FILTER: CardFilter = { query: "", assignee: "", label: "", priority: "" };

/** The cards a filter leaves (FR-10): search by key, title, description and labels. */
export function filterCards(cards: readonly Card[], filter: CardFilter): Card[] {
  const words = filter.query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return cards.filter((card) => {
    if (filter.assignee === "none" ? card.assignee : filter.assignee && card.assignee !== filter.assignee) return false;
    if (filter.label && !card.labels.includes(filter.label)) return false;
    if (filter.priority && card.priority !== filter.priority) return false;
    const hay = `${card.key} ${card.title} ${card.description} ${card.labels.join(" ")}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** Whether any filter is set. */
export function filtering(filter: CardFilter): boolean {
  return Boolean(filter.query.trim() || filter.assignee || filter.label || filter.priority);
}

/** A column's cards in their order. */
export function cardsIn(cards: readonly Card[], column: string): Card[] {
  return cards.filter((c) => c.columnId === column).sort((a, b) => a.position - b.position || a.number - b.number);
}

/** Every label in use, sorted. */
export function labelsOf(cards: readonly Card[]): string[] {
  return [...new Set(cards.flatMap((c) => c.labels))].sort((a, b) => a.localeCompare(b));
}

/**
 * Where a card dropped before `before` (a card id, or `null` for the end)
 * lands in a column, as the index the shell takes: counted without the
 * dragged card itself.
 */
export function dropIndex(columnCards: readonly Card[], dragged: string, before: string | null): number {
  const rest = columnCards.filter((c) => c.id !== dragged);
  if (before === null) return rest.length;
  const at = rest.findIndex((c) => c.id === before);
  return at < 0 ? rest.length : at;
}

/** A card past its due date that is not in the board's last column. */
export function overdue(card: Card, lastColumn: string | undefined, now = Date.now()): boolean {
  return card.dueMs !== null && card.dueMs < now && card.columnId !== lastColumn;
}

/** "Fri 4 Dec", or "Today" / "Tomorrow" / "Yesterday". */
export function dueLabel(dueMs: number, now = Date.now()): string {
  const day = (ms: number) => {
    const d = new Date(ms);
    return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  };
  const days = Math.round((day(dueMs) - day(now)) / 86_400_000);
  if (days === 0) return "Today";
  if (days === 1) return "Tomorrow";
  if (days === -1) return "Yesterday";
  return new Date(dueMs).toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
}

/** `<input type="date">`'s value for a due date, and back (end of that local day). */
export function dateValue(dueMs: number | null): string {
  if (dueMs === null) return "";
  const d = new Date(dueMs);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function dueFromDate(value: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  return new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]), 23, 59).getTime();
}

/**
 * `markdown` with every `KEY-n` of a known board as a link to its card
 * (FR-14), outside code. Keys are matched as whole words.
 */
export function linkCards(markdown: string, keys: readonly string[]): string {
  if (keys.length === 0) return markdown;
  const known = keys.map((k) => k.replace(/[^A-Z0-9]/g, "")).filter(Boolean);
  if (known.length === 0) return markdown;
  const pattern = new RegExp(`(^|[^\\w/\\[-])((?:${known.join("|")})-\\d+)(?![\\w-])`, "g");
  // Leave code spans, fences and existing links alone.
  return markdown
    .split(/(```[\s\S]*?```|`[^`\n]*`|\[[^\]]*\]\([^)]*\))/g)
    .map((part, i) => (i % 2 === 1 ? part : part.replace(pattern, (_, lead: string, key: string) => `${lead}[${key}](${CARD_LINK}${key})`)))
    .join("");
}

/** `text` in pieces, each `KEY-n` of a known board apart with its key (FR-14). */
export function splitCardKeys(text: string, keys: readonly string[]): { text: string; key?: string }[] {
  const known = keys.map((k) => k.replace(/[^A-Z0-9]/g, "")).filter(Boolean);
  if (known.length === 0) return [{ text }];
  const pattern = new RegExp(`(?<![\\w-])((?:${known.join("|")})-\\d+)(?![\\w-])`, "g");
  const out: { text: string; key?: string }[] = [];
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    const at = match.index ?? 0;
    if (at > last) out.push({ text: text.slice(last, at) });
    out.push({ text: match[1], key: match[1] });
    last = at + match[1].length;
  }
  if (last < text.length) out.push({ text: text.slice(last) });
  return out.length ? out : [{ text }];
}
