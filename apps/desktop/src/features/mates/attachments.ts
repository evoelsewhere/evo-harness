// Pictures and files in Evo Mates' messages (docs/features/evo-mates/spec.md,
// FR-81 - FR-83; plan 3.11): what a mate sent (`send_file`), the pictures its
// tools took, and the files a reply names. Pure: tested on its own.

import { FILE_LINK, linkFiles, parseFileRef } from "@/components/file-refs";
import type { Turn } from "@/features/chat";

/** A file a mate sent (`send_file`'s result). */
export interface SentFile {
  path: string;
  name: string;
  size: number;
}

const PICTURE = /\.(png|jpe?g|gif|webp|bmp|svg)$/i;

/** A path to a picture, by its extension. */
export function isPicture(path: string): boolean {
  return PICTURE.test(path.trim());
}

/** The files a turn's `send_file` calls delivered, in order; failed calls left out. */
export function sentFilesOf(turn: Pick<Turn, "parts">): SentFile[] {
  return turn.parts.flatMap((part) => {
    if (part.kind !== "tool" || part.name !== "send_file" || part.phase !== "done") return [];
    const result = part.details?.externalToolResult;
    if (!result || result.isError) return [];
    const files = result.structuredContent?.files;
    return Array.isArray(files) ? (files as SentFile[]).filter((f) => typeof f?.path === "string") : [];
  });
}

/** The pictures a turn's tools took (screenshots), as data URLs. */
export function picturesOf(turn: Pick<Turn, "parts">): string[] {
  return turn.parts.flatMap((part) => (part.kind === "tool" && part.images?.length ? part.images : []));
}

/** The local files a reply names (as links, code spans or bare names), once each, in order. */
export function mentionedFiles(text: string): string[] {
  const out: string[] = [];
  for (const match of linkFiles(text).matchAll(/\]\(([^)\s]+)\)/g)) {
    if (!match[1].startsWith(FILE_LINK)) continue;
    const { path } = parseFileRef(decodeURIComponent(match[1].slice(FILE_LINK.length)));
    if (!out.includes(path)) out.push(path);
  }
  return out;
}

/**
 * A channel message's text with its attachments (FR-83): every member reads
 * the message as text, so pictures without a file of their own are saved
 * first (`save`) and every attachment goes as an `@path` line.
 */
export async function channelMessage(
  text: string,
  attachments: {
    kind?: string;
    path?: string;
    blob?: Blob;
    name: string;
    text?: string;
  }[],
  save: (blob: Blob, name: string) => Promise<string>,
): Promise<string> {
  const pasted: string[] = [];
  const refs: string[] = [];
  for (const file of attachments) {
    if (file.kind === "text" && file.text) {
      pasted.push(`<pasted-text lines="${file.text.split("\n").length}">\n${file.text}\n</pasted-text>`);
    } else if (file.path) refs.push(`@${file.path}`);
    else if (file.blob) refs.push(`@${await save(file.blob, file.name)}`);
  }
  return [text.trim(), ...pasted, refs.join("\n")].filter(Boolean).join("\n");
}
