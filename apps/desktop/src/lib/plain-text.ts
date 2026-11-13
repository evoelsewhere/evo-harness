// A short preview of chat text without markdown marks: a sidebar tooltip, a
// Scheduler past run (docs/features/ux-polish/spec.md, FR-9). Not a
// markdown parser: it removes the marks people see in a one-line preview.

export function plainText(markdown: string): string {
  return (
    markdown
      // Code fences: keep what is inside.
      .replace(/```[^\n]*\n?([\s\S]*?)```/g, "$1")
      // Images and links: their text.
      .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      // Headings, quotes, list bullets and numbers at line starts.
      .replace(/^\s{0,3}(?:#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+)/gm, "")
      // Bold, italic, strike, inline code.
      .replace(/(\*\*|__)(.+?)\1/g, "$2")
      .replace(/(^|[^\w*])\*(?!\s)([^*\n]+?)\*(?!\w)/g, "$1$2")
      .replace(/(^|[^\w_])_(?!\s)([^_\n]+?)_(?!\w)/g, "$1$2")
      .replace(/~~(.+?)~~/g, "$1")
      .replace(/`([^`]+)`/g, "$1")
      // One line.
      .replace(/\s+/g, " ")
      .trim()
  );
}
