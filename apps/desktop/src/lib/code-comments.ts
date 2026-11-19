// Comments on changed lines (the Source control diff), sent to the agent with
// the next message as plain text naming the file and lines, with the lines
// quoted (docs/features/source-control/spec.md, FR-8). They go in the message
// text before any office annotation block, which must stay at the end of the
// message. No React, no DOM.

export interface CodeComment {
  /** Client id, for removing one from a batch; not sent. */
  id: string;
  /** The file, relative to the repository's top folder (forward slashes). */
  file: string;
  /** 1-based lines of the file as it is now. */
  from: number;
  to: number;
  /** The selected lines. */
  quote: string;
  instruction: string;
}

/** Lines of a quote sent with a comment. */
export const MAX_QUOTE_LINES = 40;

/** "src/a.ts · line 12", "src/a.ts · lines 12–18". */
export function describeCodeComment(comment: Pick<CodeComment, "file" | "from" | "to">): string {
  return comment.from === comment.to ? `${comment.file} · line ${comment.from}` : `${comment.file} · lines ${comment.from}–${comment.to}`;
}

/** The message text with the comments after it. */
export function composeCodeComments(text: string, comments: CodeComment[]): string {
  if (comments.length === 0) return text;
  const items = comments.map((comment, index) => {
    const lines = comment.quote.replace(/\r\n?/g, "\n").replace(/\n$/, "").split("\n");
    const shown = lines.slice(0, MAX_QUOTE_LINES);
    if (lines.length > shown.length) shown.push(`… ${lines.length - shown.length} more lines`);
    const where = comment.from === comment.to ? `line ${comment.from}` : `lines ${comment.from}–${comment.to}`;
    const quote = shown.map((line) => `   > ${line}`).join("\n");
    const instruction = comment.instruction.trim() || "(no instruction: look at these lines)";
    return `${index + 1}. ${comment.file} ${where}\n${quote}\n   ${instruction}`;
  });
  const block = `Comments on the changes:\n${items.join("\n")}`;
  return text.trim() ? `${text.trim()}\n\n${block}` : block;
}
