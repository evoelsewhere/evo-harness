// Inline Markdown for a callout's text (docs/features/rich-replies/spec.md,
// FR-20): `code`, **bold**, *italic* and [links](…). Text nodes only; no HTML
// and no block elements, so a heading shows as plain text.

import * as React from "react";
import { runBlockAction } from "./block-action";

const INLINE = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(\*[^*\n]+\*|_[^_\n]+_)|(\[[^\]\n]+\]\(([^)\s]+)\))/;

export function InlineText({ text }: { text: string }) {
  const nodes: React.ReactNode[] = [];
  let rest = text;
  let key = 0;
  while (rest) {
    const match = INLINE.exec(rest);
    if (!match) {
      nodes.push(rest);
      break;
    }
    if (match.index > 0) nodes.push(rest.slice(0, match.index));
    const token = match[0];
    if (match[1]) {
      nodes.push(
        <code key={key++} className="rounded-sm bg-muted px-1 py-0.5 font-mono text-[0.9em]">
          {token.slice(1, -1)}
        </code>,
      );
    } else if (match[2]) {
      nodes.push(
        <strong key={key++} className="font-semibold">
          {token.slice(2, -2)}
        </strong>,
      );
    } else if (match[3]) {
      nodes.push(<em key={key++}>{token.slice(1, -1)}</em>);
    } else {
      const label = token.slice(1, token.indexOf("]"));
      const href = match[5];
      nodes.push(
        <button
          key={key++}
          type="button"
          title={href}
          onClick={() => runBlockAction({ open: href })}
          className="cursor-pointer text-link underline-offset-4 hover:underline"
        >
          {label}
        </button>,
      );
    }
    rest = rest.slice(match.index + token.length);
  }
  return <>{nodes}</>;
}
