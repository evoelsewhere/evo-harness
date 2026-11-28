// What a file's thumbnail shows on the Files page (docs/features/evo-mates-ui-v2/
// spec.md, FR-30): the first lines of a text file as a small page (headings
// stand out, Markdown marks go), the first rows of a CSV as a small table.

export type FilePreview =
  | { kind: "table"; rows: string[][] }
  | { kind: "page"; lines: { text: string; heading: boolean }[] }
  | { kind: "none" };

const TEXT = /\.(md|markdown|txt|text|log|json|ya?ml|toml|ini|cfg|xml|html?|css|js|jsx|ts|tsx|py|rs|go|java|c|cpp|h|cs|rb|sh|ps1|sql)$/i;
const TABLE = /\.(csv|tsv)$/i;

/** Whether a file's name says its text can preview. */
export function previewable(name: string): boolean {
  return TEXT.test(name) || TABLE.test(name);
}

/** The thumbnail of `text`, a file called `name`: at most `maxLines` lines or rows. */
export function previewOf(name: string, text: string, maxLines = 12): FilePreview {
  if (TABLE.test(name)) {
    const sep = /\.tsv$/i.test(name) ? "\t" : ",";
    const rows = text
      .split(/\r?\n/)
      .filter((line) => line.trim())
      .slice(0, maxLines)
      .map((line) => line.split(sep).slice(0, 4).map((cell) => cell.trim().replace(/^"|"$/g, "")));
    return rows.length ? { kind: "table", rows } : { kind: "none" };
  }
  if (!TEXT.test(name)) return { kind: "none" };
  const markdown = /\.(md|markdown)$/i.test(name);
  const lines = text
    .split(/\r?\n/)
    .filter((line) => line.trim() && !/^\s*(```|---\s*$)/.test(line))
    .slice(0, maxLines)
    .map((line) => {
      const heading = markdown && /^\s{0,3}#{1,6}\s/.test(line);
      const clean = markdown
        ? line
            .replace(/^\s{0,3}#{1,6}\s+/, "")
            .replace(/^\s*[-*+]\s+/, "• ")
            .replace(/(\*\*|__|`)/g, "")
            .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
        : line;
      return { text: clean.length > 80 ? `${clean.slice(0, 79)}…` : clean, heading };
    });
  return lines.length ? { kind: "page", lines } : { kind: "none" };
}
