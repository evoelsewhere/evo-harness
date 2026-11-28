// What the Evo Mates message boxes do with keys (docs/features/evo-mates-
// messaging/spec.md, FR-12, FR-13): Markdown wrapping, the `:shortcode` and
// `/command` being typed, and the typing line's words (FR-1). Pure.

/** `text` with the selection wrapped in `mark` (or the marks removed when they are already there). */
export function wrapSelection(text: string, start: number, end: number, mark: string): { text: string; start: number; end: number } {
  const before = text.slice(0, start);
  const inner = text.slice(start, end);
  const after = text.slice(end);
  if (before.endsWith(mark) && after.startsWith(mark)) {
    return { text: before.slice(0, -mark.length) + inner + after.slice(mark.length), start: start - mark.length, end: end - mark.length };
  }
  return { text: before + mark + inner + mark + after, start: start + mark.length, end: end + mark.length };
}

/** The `:code` being typed at `caret` (two letters or more), if any. */
export function shortcodeAt(text: string, caret: number): { start: number; query: string } | null {
  const match = /(^|\s):([a-z0-9_+-]{2,})$/i.exec(text.slice(0, caret));
  if (!match) return null;
  return { start: caret - match[2].length - 1, query: match[2] };
}

/** The command word being typed, when the box starts with `/` and has no space yet. */
export function slashQuery(text: string): string | null {
  const match = /^\/(\S*)$/.exec(text);
  return match ? match[1] : null;
}

/** "Writer is typing…", "Writer and Analyst are typing…", "Planner and 2 others are typing…" (FR-1). */
export function typingWords(names: string[]): string {
  if (names.length === 0) return "";
  if (names.length === 1) return `${names[0]} is typing…`;
  if (names.length === 2) return `${names[0]} and ${names[1]} are typing…`;
  return `${names[0]} and ${names.length - 1} others are typing…`;
}
