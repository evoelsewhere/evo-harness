// A sent message taken apart for display, without the composer's browser and
// file code, so Evo Webbridge's side panel shows a prompt as the app does
// (src/lib/webbridge-chat.ts).

/** Attached paths, one `@path` per line after the text (the agent reads them with its tools). */
const REF_LINE = /^@(.+)$/;
const PASTED = /<pasted-text lines="(\d+)">\n([\s\S]*?)\n<\/pasted-text>/g;

/** A sent message taken apart for display: its text, pasted blocks and attached paths. */
export function splitPrompt(message: string): { text: string; pasted: { lines: number; text: string }[]; files: string[] } {
  const pasted: { lines: number; text: string }[] = [];
  let text = message.replace(PASTED, (_, lines: string, body: string) => {
    pasted.push({ lines: Number(lines), text: body });
    return "";
  });
  const lines = text.replace(/\s+$/, "").split("\n");
  const files: string[] = [];
  // Older messages put several references on one line, space-separated.
  if (lines.length > 0 && /^@\S+(\s+@\S+)+$/.test(lines[lines.length - 1].trim())) {
    files.push(...lines.pop()!.trim().split(/\s+/).map((ref) => ref.slice(1)));
  } else {
    while (lines.length > 0) {
      const match = REF_LINE.exec(lines[lines.length - 1].trim());
      if (!match) break;
      files.unshift(match[1]);
      lines.pop();
    }
  }
  text = lines.join("\n").trim();
  return { text, pasted, files };
}
