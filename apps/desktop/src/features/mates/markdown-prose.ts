// The prose of a Markdown text, apart from its code and links, so words in
// it can be turned into links without touching code, URLs or links that are
// already there (docs/features/evo-mates-threads-board/spec.md, FR-14,
// FR-17; board review #8).

/**
 * `markdown` with `fn` applied to its prose only. Left as written: fenced
 * code (``` or ~~~, also a fence still open while a reply streams),
 * indented code, inline code of any number of backticks, Markdown links and
 * images, `<…>` autolinks and bare URLs (`https://…`, `www.…`).
 */
export function mapProse(markdown: string, fn: (prose: string) => string): string {
  const out: string[] = [];
  let prose: string[] = [];
  let fence: string | null = null;
  let previousBlank = true;
  let inCode = false;
  const flush = () => {
    if (prose.length) out.push(mapInline(prose.join("\n"), fn));
    prose = [];
  };
  for (const line of markdown.split("\n")) {
    const marker = /^ {0,3}(`{3,}|~{3,})/.exec(line)?.[1];
    if (fence) {
      out.push(line);
      if (marker && marker[0] === fence[0] && marker.length >= fence.length && line.trim() === marker) fence = null;
      previousBlank = false;
      continue;
    }
    if (marker) {
      flush();
      fence = marker;
      out.push(line);
      previousBlank = false;
      continue;
    }
    // Indented code: four spaces or a tab after a blank line, and the lines of it that follow.
    const indented = /^( {4}|\t)/.test(line) && line.trim() !== "";
    if (indented && (previousBlank || inCode)) {
      flush();
      out.push(line);
      inCode = true;
      previousBlank = false;
      continue;
    }
    inCode = false;
    prose.push(line);
    previousBlank = line.trim() === "";
  }
  flush();
  return out.join("\n");
}

/** Inline code, links, images, autolinks and bare URLs, kept whole. */
const KEEP = /(`+)[\s\S]*?\1|!?\[[^\]]*\]\([^)]*\)|<[a-z][a-z0-9+.-]*:[^>\s]*>|\b[a-z][a-z0-9+.-]*:\/\/[^\s<>()]*[^\s<>().,;:!?'"]|\bwww\.[^\s<>()]*[^\s<>().,;:!?'"]/gi;

function mapInline(text: string, fn: (prose: string) => string): string {
  let out = "";
  let last = 0;
  for (const match of text.matchAll(KEEP)) {
    const at = match.index ?? 0;
    out += fn(text.slice(last, at)) + match[0];
    last = at + match[0].length;
  }
  return out + fn(text.slice(last));
}
