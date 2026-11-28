// Pictures and files in Evo Mates' messages (docs/features/evo-mates/spec.md,
// FR-81 - FR-83; plan 3.11): thumbnails that open full size, file chips that
// open with the default app or show in their folder, pasted-text chips, and
// the strip of what a mate's turn sent and showed.

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { ClipboardIcon, ExternalLinkIcon, FileIcon, FolderOpenIcon } from "lucide-react";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { baseName, formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { isPicture, mentionedFiles, type SentFile } from "./attachments";

type Read = { kind: "image"; dataUrl: string } | { kind: string };

/** A local picture as a data URL (`fs_read_file`), or `null` while it loads or when it cannot be read. */
function usePicture(path: string | null): string | null {
  const [url, setUrl] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!path) return;
    let gone = false;
    void invoke<Read>("fs_read_file", { path })
      .then((read) => !gone && setUrl(read.kind === "image" && "dataUrl" in read ? read.dataUrl : null))
      .catch(() => !gone && setUrl(null));
    return () => {
      gone = true;
    };
  }, [path]);
  return url;
}

/** A file's size (`fs_stat`), or `null` when it is gone. */
function useSize(path: string, known?: number): number | null {
  const [size, setSize] = React.useState<number | null>(known ?? null);
  React.useEffect(() => {
    if (known !== undefined) return;
    let gone = false;
    void invoke<{ exists: boolean; size: number }>("fs_stat", { path })
      .then((stat) => !gone && setSize(stat.exists ? stat.size : null))
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [path, known]);
  return size;
}

/** A picture full size, over the window. */
export function PictureViewer({ src, name, onClose }: { src: string | null; name: string; onClose(): void }) {
  return (
    <Dialog open={src !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex max-h-[90vh] w-auto max-w-[90vw] items-center justify-center p-2 sm:max-w-[90vw]">
        <DialogTitle className="sr-only">{name}</DialogTitle>
        {src && <img src={src} alt={name} className="max-h-[84vh] max-w-full rounded-lg object-contain" />}
      </DialogContent>
    </Dialog>
  );
}

/** A thumbnail of a picture given as a data URL or a local path; a click shows it full size. */
export function Thumb({ src, path, size = 160 }: { src?: string; path?: string; size?: number }) {
  const loaded = usePicture(src ? null : (path ?? null));
  const url = src ?? loaded;
  const [open, setOpen] = React.useState(false);
  const name = path ? baseName(path) : "Picture";
  if (!url) return path ? <FileChip path={path} /> : null;
  return (
    <>
      <button type="button" onClick={() => setOpen(true)} aria-label={`Open ${name}`} className="overflow-hidden rounded-xl">
        <img src={url} alt={name} style={{ maxHeight: size, maxWidth: size * 1.5 }} className="block object-cover" />
      </button>
      <PictureViewer src={open ? url : null} name={name} onClose={() => setOpen(false)} />
    </>
  );
}

/** A file: name and size; opens with the default app; Show in folder beside it. */
export function FileChip({
  path,
  name,
  size: known,
  tone = "plain",
}: {
  path: string;
  name?: string;
  size?: number;
  tone?: "plain" | "me";
}) {
  const size = useSize(path, known);
  const label = name ?? baseName(path);
  return (
    <span
      className={cn(
        "flex w-fit max-w-full min-w-0 items-center gap-2 rounded-xl px-2.5 py-1.5 text-left",
        tone === "me" ? "bg-white/15" : "bg-background/70 ring-1 ring-border",
      )}
    >
      <FileIcon className="size-4 shrink-0 opacity-80" />
      <button
        type="button"
        onClick={() => void openPath(path).catch(() => undefined)}
        title={path}
        className="flex min-w-0 flex-1 flex-col items-start text-left"
      >
        <span className="truncate text-[13px] font-medium">{label}</span>
        {size !== null && <span className="text-[11px] opacity-70">{formatBytes(size)}</span>}
      </button>
      <button
        type="button"
        aria-label={`Open ${label}`}
        onClick={() => void openPath(path).catch(() => undefined)}
        className="rounded-md p-1 opacity-70 hover:opacity-100"
      >
        <ExternalLinkIcon className="size-3.5" />
      </button>
      <button
        type="button"
        aria-label="Show in folder"
        onClick={() => void revealItemInDir(path).catch(() => undefined)}
        className="rounded-md p-1 opacity-70 hover:opacity-100"
      >
        <FolderOpenIcon className="size-3.5" />
      </button>
    </span>
  );
}

/** Long text the person pasted: a chip that shows the text when opened. */
export function PastedChip({ lines, text, tone = "plain" }: { lines: number; text: string; tone?: "plain" | "me" }) {
  const [open, setOpen] = React.useState(false);
  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className={cn(
          "flex items-center gap-2 rounded-xl px-2.5 py-1.5 text-[13px]",
          tone === "me" ? "bg-white/15" : "bg-background/70 ring-1 ring-border",
        )}
      >
        <ClipboardIcon className="size-4 opacity-80" />
        Pasted text · {lines} {lines === 1 ? "line" : "lines"}
      </button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-2xl">
          <DialogTitle>Pasted text</DialogTitle>
          <pre className="max-h-[60vh] overflow-auto rounded-lg bg-muted p-3 text-xs whitespace-pre-wrap">{text}</pre>
        </DialogContent>
      </Dialog>
    </>
  );
}

/** The person's attachments in their bubble: pictures, then files and pasted text. */
export function SentByMe({ images, files, pasted }: { images?: string[]; files?: string[]; pasted?: { lines: number; text: string }[] }) {
  const pictures = (files ?? []).filter(isPicture);
  const others = (files ?? []).filter((f) => !isPicture(f));
  if (!images?.length && !files?.length && !pasted?.length) return null;
  return (
    <div className="mb-1 flex flex-col gap-1">
      {(images?.length || pictures.length) > 0 && (
        <div className="flex flex-wrap gap-1">
          {images?.map((src, i) => (
            <Thumb key={`i${i}`} src={src} />
          ))}
          {pictures.map((path) => (
            <Thumb key={path} path={path} />
          ))}
        </div>
      )}
      {others.map((path) => (
        <FileChip key={path} path={path} tone="me" />
      ))}
      {pasted?.map((p, i) => (
        <PastedChip key={i} lines={p.lines} text={p.text} tone="me" />
      ))}
    </div>
  );
}

/** What a mate's turn sent (files, pictures among them as previews) and the pictures its tools took (FR-82). */
export function SentByMate({ files, pictures, max = 4 }: { files: SentFile[]; pictures: string[]; max?: number }) {
  const sentPictures = files.filter((f) => isPicture(f.path));
  const others = files.filter((f) => !isPicture(f.path));
  const shown = pictures.slice(0, max);
  return (
    <div className="flex max-w-[72%] flex-col items-start gap-1.5">
      {(sentPictures.length > 0 || shown.length > 0) && (
        <div className="flex flex-wrap items-end gap-1">
          {sentPictures.map((f) => (
            <Thumb key={f.path} path={f.path} size={200} />
          ))}
          {shown.map((src, i) => (
            <Thumb key={`p${i}`} src={src} size={96} />
          ))}
          {pictures.length > max && <span className="px-1 text-xs text-muted-foreground">+{pictures.length - max}</span>}
        </div>
      )}
      {others.map((f) => (
        <FileChip key={f.path} path={f.path} name={f.name} size={f.size} />
      ))}
    </div>
  );
}

/** `path` as written, or under `root` when it is relative. */
export function resolvePath(path: string, root: string | null): string {
  if (/^(?:[A-Za-z]:[\/]|[\/]|~[\/])/.test(path) || !root) return path;
  return `${root.replace(/[\/]+$/, "")}/${path.replace(/^\.[\/]/, "")}`;
}

/** The files a mate's line names that exist, under its text (FR-83): pictures as previews, others as cards. */
export function NamedFiles({ text, root, paths: given }: { text: string; root: string | null; paths?: string[] }) {
  const [found, setFound] = React.useState<string[]>([]);
  const key = given?.join("|") ?? "";
  React.useEffect(() => {
    let gone = false;
    // Files given by path (a hand-off's, evo-mates-messaging FR-6), else the ones the text names.
    const paths = (given ?? mentionedFiles(text)).map((p) => resolvePath(p, root));
    void Promise.all(
      paths.map((path) =>
        invoke<{ exists: boolean; dir: boolean }>("fs_stat", { path })
          .then((stat) => (stat.exists && !stat.dir ? path : null))
          .catch(() => null),
      ),
    ).then((list) => !gone && setFound(list.filter((p): p is string => p !== null)));
    return () => {
      gone = true;
    };
    // `key` stands for the given paths.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text, root, key]);
  if (!found.length) return null;
  return (
    <div className="mt-1 flex flex-col items-start gap-1">
      {found.some(isPicture) && (
        <div className="flex flex-wrap gap-1">
          {found.filter(isPicture).map((path) => (
            <Thumb key={path} path={path} />
          ))}
        </div>
      )}
      {found
        .filter((p) => !isPicture(p))
        .map((path) => (
          <FileChip key={path} path={path} />
        ))}
    </div>
  );
}
