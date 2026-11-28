// A message as one plain line for a roster row or a Today row
// (docs/features/evo-mates-ui-v2/spec.md, FR-4; evo-mates QA G2): Markdown
// marks, code fences and links are removed, so "```js 1,2,3" reads "1,2,3".

export function plainPreview(text: string | null | undefined, max = 160): string {
  if (!text) return "";
  let s = text
    // Fenced code: keep the code, drop the fence and its language.
    .replace(/```[^\n`]*\n?/g, " ")
    .replace(/`([^`]*)`/g, "$1")
    // Pictures and links: keep the words.
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    // Headings, quotes, list marks at a line's start.
    .replace(/^\s{0,3}(#{1,6}|>|[-*+]|\d+[.)])\s+/gm, "")
    // Emphasis.
    .replace(/(\*\*|__)(.+?)\1/g, "$2")
    .replace(/(^|[^\w*])[*_]([^*_\n]+)[*_](?=[^\w*]|$)/g, "$1$2")
    .replace(/~~(.+?)~~/g, "$1")
    // Table rules and pipes.
    .replace(/^\s*\|?\s*:?-{3,}.*$/gm, " ")
    .replace(/\s*\|\s*/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (s.length > max) s = `${s.slice(0, max - 1).trimEnd()}…`;
  return s;
}
