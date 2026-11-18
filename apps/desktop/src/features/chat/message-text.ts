// What a chat message's right-click menu copies and quotes.

/**
 * Markdown as the plain text it reads as, line by line (unlike
 * `lib/plain-text.ts`, a one-line preview): no emphasis marks, headings, list
 * bullets kept as "•", links as their label, code blocks as their code.
 */
export function messagePlainText(markdown: string): string {
  const out: string[] = [];
  let fence: string | null = null;
  for (const line of markdown.replace(/\r\n?/g, "\n").split("\n")) {
    const marker = /^\s*(`{3,}|~{3,})/.exec(line)?.[1];
    if (fence) {
      if (marker && marker[0] === fence[0] && marker.length >= fence.length && !line.trim().slice(marker.length)) fence = null;
      else out.push(line);
      continue;
    }
    if (marker) {
      fence = marker;
      continue;
    }
    // A thematic break is not text.
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      out.push("");
      continue;
    }
    // A table's separator row.
    if (/^\s*\|?(\s*:?-{3,}:?\s*\|)+\s*:?-*:?\s*\|?\s*$/.test(line)) continue;
    let text = line
      .replace(/^(\s{0,3})#{1,6}\s+/, "$1")
      .replace(/^(\s*)>\s?/, "$1")
      .replace(/^(\s*)[-*+]\s+\[([ xX])\]\s+/, (_, indent: string, mark: string) => `${indent}${mark === " " ? "☐" : "☑"} `)
      .replace(/^(\s*)[-*+]\s+/, "$1• ");
    // A table row: its cells, apart.
    if (/^\s*\|.*\|\s*$/.test(text)) text = text.trim().slice(1, -1).split("|").map((cell) => cell.trim()).join("\t");
    out.push(inline(text));
  }
  return out.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

function inline(text: string): string {
  // Code spans are kept as they are; the rest loses its marks.
  return text
    .split(/(`+[^`]*`+)/)
    .map((piece, index) => {
      if (index % 2) return piece.replace(/^`+\s?|\s?`+$/g, "");
      return piece
        .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
        .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
        .replace(/<(https?:[^>\s]+)>/g, "$1")
        .replace(/(\*\*|__)(?=\S)(.+?)(?<=\S)\1/g, "$2")
        .replace(/(^|[^\w*])\*(?=\S)(.+?)(?<=\S)\*(?!\*)/g, "$1$2")
        .replace(/(^|[^\w_])_(?=\S)(.+?)(?<=\S)_(?![\w_])/g, "$1$2")
        .replace(/~~(?=\S)(.+?)(?<=\S)~~/g, "$1")
        .replace(/\\([\\`*_{}[\]()#+\-.!|>~])/g, "$1");
    })
    .join("");
}

/** `text` as a Markdown quote for the composer, followed by an empty line to write on. */
export function quoted(text: string): string {
  const body = text
    .trim()
    .split("\n")
    .map((line) => (line ? `> ${line}` : ">"))
    .join("\n");
  return `${body}\n\n`;
}
