// Editing the person's message (docs/features/evo-mates-messaging/spec.md,
// FR-7): only their own words change; the note, the quote, the forward,
// the attached files and the pasted text stay as they were. Pure.

import { splitMessage } from "./attachments";
import { parsePrompt } from "./message-markup";

/** Where pasted blocks lie in `text`, to never edit inside one. */
function pastedRanges(text: string): [number, number][] {
  const out: [number, number][] = [];
  const re = /<pasted-text[^>]*>[\s\S]*?<\/pasted-text>/g;
  for (let m = re.exec(text); m; m = re.exec(text)) out.push([m.index, m.index + m[0].length]);
  return out;
}

/**
 * `prompt` with the person's words `before` replaced by `after`, or `null`
 * when the words cannot be found outside the markup (the edit is then refused,
 * never sent without its quote, files or pasted text).
 */
export function replaceVisibleText(prompt: string, before: string, after: string): string | null {
  if (!before) return null;
  // Past the note and quote prefix: what `parsePrompt` reads as the message.
  const said = parsePrompt(prompt).text;
  const start = prompt.length - said.length;
  if (splitMessage(said).text !== before && !said.includes(before)) return null;
  const blocked = pastedRanges(said);
  for (let at = said.indexOf(before); at >= 0; at = said.indexOf(before, at + 1)) {
    if (blocked.some(([a, b]) => at < b && at + before.length > a)) continue;
    return prompt.slice(0, start + at) + after + prompt.slice(start + at + before.length);
  }
  return null;
}
