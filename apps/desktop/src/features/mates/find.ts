// Find in a conversation or a channel (docs/features/evo-mates-ui-v2/spec.md,
// FR-15; plan 3.5): where a query occurs in a text, case-insensitive. The
// find bar marks those places with the browser's text highlights, so the
// messages' Markdown is never rewritten (plan 3.7: text stays text).

/** The start of each occurrence of `query` in `text`, case-insensitive, not overlapping. */
export function matchOffsets(text: string, query: string): number[] {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) return [];
  const hay = text.toLowerCase();
  const out: number[] = [];
  for (let at = hay.indexOf(needle); at >= 0; at = hay.indexOf(needle, at + needle.length)) out.push(at);
  return out;
}

/** "2 of 5", "No matches", or nothing before the person types. */
export function findLabel(index: number, total: number, query: string): string {
  if (!query.trim()) return "";
  if (total === 0) return "No matches";
  return `${index + 1} of ${total}`;
}

/** The next match's index, wrapping round (Enter, Shift+Enter). */
export function stepMatch(index: number, total: number, step: 1 | -1): number {
  if (total === 0) return 0;
  return (((index + step) % total) + total) % total;
}
