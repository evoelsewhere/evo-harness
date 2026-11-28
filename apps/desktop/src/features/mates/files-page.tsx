// Evo Mates' Files page (docs/features/evo-mates/spec.md, FR-84; plan 3.12):
// every file in the mates' own folders, newest first, with its mate, a
// thumbnail for pictures, a search by name and a filter by mate. Evo Mates
// UI v2 (docs/features/evo-mates-ui-v2/spec.md, FR-29, FR-30) groups them by
// day and shows them as a grid or a list, kept as the person left it.

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { FileTextIcon, FolderIcon, LayoutGridIcon, ListIcon, SearchIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useMatesStore } from "@/stores/mates-store";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { isPicture } from "./attachments";
import { FileChip, Thumb } from "./attachment-views";
import { EmptyState } from "./empty-state";
import { previewable, previewOf, type FilePreview } from "./file-preview";

/** What `fs_read_file` answers, as far as a thumbnail needs it. */
type FileContent = { kind: "text"; text: string } | { kind: "image"; dataUrl: string } | { kind: "binary" | "tooLarge" };
import { dayLabel } from "./thread-rows";

/** A file on the page (`mates_files`). */
export interface MateFile {
  mateId: string;
  path: string;
  name: string;
  size: number;
  modified: number;
}

/** The files a search and a mate filter keep. */
export function filterFiles(files: MateFile[], query: string, mate: string | null): MateFile[] {
  const needle = query.trim().toLowerCase();
  return files.filter((f) => (!mate || f.mateId === mate) && (!needle || f.name.toLowerCase().includes(needle)));
}

/** Files by day, newest day first and newest file first (FR-30). */
export function groupByDay(files: MateFile[], now = Date.now()): { label: string; files: MateFile[] }[] {
  const groups = new Map<string, MateFile[]>();
  for (const file of [...files].sort((a, b) => b.modified - a.modified)) {
    const label = dayLabel(file.modified, now);
    groups.set(label, [...(groups.get(label) ?? []), file]);
  }
  return Array.from(groups, ([label, list]) => ({ label, files: list }));
}

function sizeWords(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 102.4) / 10} KB`;
  return `${Math.round(bytes / (1024 * 102.4)) / 10} MB`;
}

type Loaded = { kind: "image"; url: string } | { kind: "preview"; preview: FilePreview } | { kind: "none" };

/** A tile's thumbnail, read once it scrolls into view (`fs_read_file`). */
function useThumbnail(file: MateFile, host: React.RefObject<HTMLElement | null>): Loaded | null {
  const [loaded, setLoaded] = React.useState<Loaded | null>(null);
  const wanted = isPicture(file.path) || previewable(file.name);
  React.useEffect(() => {
    if (!wanted) {
      setLoaded({ kind: "none" });
      return;
    }
    const el = host.current;
    if (!el) return;
    let gone = false;
    const read = () =>
      void invoke<FileContent>("fs_read_file", { path: file.path })
        .then((content) => {
          if (gone) return;
          if (content.kind === "image") setLoaded({ kind: "image", url: content.dataUrl });
          else if (content.kind === "text") setLoaded({ kind: "preview", preview: previewOf(file.name, content.text.slice(0, 4000)) });
          else setLoaded({ kind: "none" });
        })
        .catch(() => !gone && setLoaded({ kind: "none" }));
    // A long list reads only what the person can see.
    const seen = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        seen.disconnect();
        read();
      }
    });
    seen.observe(el);
    return () => {
      gone = true;
      seen.disconnect();
    };
  }, [file.path, file.name, file.modified, wanted, host]);
  return loaded;
}

/** What a file looks like before opening it: its picture, its first lines, or its kind. */
function Thumbnail({ file }: { file: MateFile }) {
  const host = React.useRef<HTMLDivElement>(null);
  const loaded = useThumbnail(file, host);
  const ext = file.name.includes(".") ? file.name.split(".").pop()!.toUpperCase().slice(0, 4) : "FILE";
  const preview = loaded?.kind === "preview" ? loaded.preview : null;
  return (
    <div ref={host} className="relative h-32 overflow-hidden bg-muted/50">
      {loaded?.kind === "image" ? (
        <img src={loaded.url} alt="" draggable={false} className="size-full object-cover" />
      ) : preview && preview.kind !== "none" ? (
        // A small page: the file's first lines, fading out at the bottom.
        <div className="absolute inset-x-3 top-3 bottom-0 overflow-hidden rounded-t-md bg-background px-2.5 pt-2 shadow-[0_0_0_1px_var(--border),0_4px_12px_-6px_rgb(0_0_0/0.25)]">
          {preview.kind === "page" ? (
            preview.lines.map((line, i) => (
              <div
                key={i}
                className={cn(
                  "truncate leading-[11px]",
                  line.heading ? "mb-0.5 text-[8.5px] font-semibold text-foreground" : "text-[7.5px] text-muted-foreground",
                )}
              >
                {line.text}
              </div>
            ))
          ) : (
            <table className="w-full table-fixed border-collapse text-[7.5px] leading-[11px]">
              <tbody>
                {preview.rows.map((row, i) => (
                  <tr key={i} className={cn("border-b border-border/60", i === 0 && "font-semibold text-foreground")}>
                    {row.map((cell, j) => (
                      <td key={j} className={cn("truncate pr-1", i > 0 && "text-muted-foreground")}>
                        {cell}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <div aria-hidden className="pointer-events-none absolute inset-x-0 bottom-0 h-8 bg-linear-to-t from-background to-transparent" />
        </div>
      ) : (
        <div className="flex size-full flex-col items-center justify-center gap-1 text-muted-foreground">
          {loaded === null ? null : <FileTextIcon className="size-8" />}
          {loaded !== null && <span className="text-[10.5px] font-semibold tracking-wide">{ext}</span>}
        </div>
      )}
      {/* The kind, on every thumbnail. */}
      {loaded && loaded.kind !== "none" && (
        <span className="absolute right-2 bottom-2 rounded-md bg-background/90 px-1.5 py-0.5 text-[9.5px] font-semibold tracking-wide text-muted-foreground shadow-[0_0_0_1px_var(--border)]">
          {ext}
        </span>
      )}
    </div>
  );
}

/** A file as a tile (grid view). */
function FileTile({ file, owner }: { file: MateFile; owner?: { name: string; avatar: React.ComponentProps<typeof MateFace>["avatar"] } }) {
  return (
    <div className="flex min-w-0 flex-col overflow-hidden rounded-2xl bg-card shadow-[0_0_0_1px_var(--border)] transition-shadow hover:shadow-[0_0_0_1px_var(--border),0_8px_24px_-12px_rgb(0_0_0/0.25)]">
      <button type="button" onClick={() => void openPath(file.path).catch(() => undefined)} aria-label={`Open ${file.name}`} className="block text-left">
        <Thumbnail file={file} />
      </button>
      <button
        type="button"
        onClick={() => void openPath(file.path).catch(() => undefined)}
        title={file.path}
        className="flex min-w-0 flex-col gap-1 px-3 py-2 text-left transition-colors hover:bg-accent/40"
      >
        <span className="truncate text-[12.5px] font-medium text-foreground">{file.name}</span>
        <span className="flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground">
          {owner && <MateFace avatar={owner.avatar} size={14} plate={false} label={owner.name} />}
          <span className="truncate">
            {owner?.name ?? "A mate"} · {sizeWords(file.size)}
          </span>
        </span>
      </button>
    </div>
  );
}

export function FilesPage() {
  const mates = useMatesStore((s) => s.mates);
  const view = useMatesUiStore((s) => s.filesView);
  const setView = useMatesUiStore((s) => s.setFilesView);
  const [files, setFiles] = React.useState<MateFile[] | null>(null);
  const [query, setQuery] = React.useState("");
  const [mate, setMate] = React.useState<string | null>(null);

  React.useEffect(() => {
    let gone = false;
    void invoke<MateFile[]>("mates_files")
      .then((list) => !gone && setFiles(list))
      .catch(() => !gone && setFiles([]));
    return () => {
      gone = true;
    };
  }, [mates]);

  const shown = filterFiles(files ?? [], query, mate);
  const owners = mates.filter((m) => files?.some((f) => f.mateId === m.id));
  const days = groupByDay(shown);

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-4xl flex-col gap-5 px-6 py-8">
        <header className="flex items-end gap-3 px-1">
          <div className="min-w-0 flex-1">
            <h1 className="text-[28px] leading-9 font-semibold tracking-[-0.02em] text-foreground">Files</h1>
            <p className="text-sm text-muted-foreground">
              What your mates made and keep in their folders{files ? ` · ${files.length} ${files.length === 1 ? "file" : "files"}` : ""}.
            </p>
          </div>
          <div role="radiogroup" aria-label="Show as" className="flex shrink-0 rounded-full bg-muted p-0.5">
            {(
              [
                { id: "grid", label: "Grid", icon: LayoutGridIcon },
                { id: "list", label: "List", icon: ListIcon },
              ] as const
            ).map((option) => (
              <button
                key={option.id}
                type="button"
                role="radio"
                aria-checked={view === option.id}
                aria-label={option.label}
                onClick={() => setView(option.id)}
                className={cn(
                  "flex h-7 items-center gap-1.5 rounded-full px-2.5 text-xs text-muted-foreground transition-colors hover:text-foreground",
                  view === option.id && "bg-background text-foreground shadow-[0_1px_2px_rgb(0_0_0/0.08)]",
                )}
              >
                <option.icon className="size-3.5" />
                {option.label}
              </button>
            ))}
          </div>
        </header>
        <div className="flex flex-wrap items-center gap-2">
          <div role="tablist" aria-label="Mates" className="flex min-w-0 flex-wrap gap-1">
            <button
              type="button"
              role="tab"
              aria-selected={mate === null}
              onClick={() => setMate(null)}
              className={cn("rounded-full px-3 py-1 text-[13px] text-muted-foreground hover:bg-accent", mate === null && "bg-accent text-foreground")}
            >
              All
            </button>
            {owners.map((m) => (
              <button
                key={m.id}
                type="button"
                role="tab"
                aria-selected={mate === m.id}
                onClick={() => setMate(m.id)}
                className={cn(
                  "flex items-center gap-1.5 rounded-full py-1 pr-3 pl-1.5 text-[13px] text-muted-foreground hover:bg-accent",
                  mate === m.id && "bg-accent text-foreground",
                )}
              >
                <MateFace avatar={m.avatar} size={18} plate={false} label={m.name} />
                {m.name}
              </button>
            ))}
          </div>
          <label className="ml-auto flex h-8 w-56 max-w-full items-center gap-2 rounded-lg bg-muted px-2.5 text-sm">
            <SearchIcon className="size-4 shrink-0 text-muted-foreground" />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search files"
              className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-muted-foreground"
            />
          </label>
        </div>
        {files === null ? null : shown.length === 0 ? (
          files.length ? (
            <EmptyState icon={<SearchIcon />} text="No file matches." action="Clear search" onAction={() => (setQuery(""), setMate(null))} />
          ) : (
            <EmptyState icon={<FolderIcon />} title="No files yet" text="What your mates write, draw or download shows here." />
          )
        ) : (
          days.map((day) => (
            <section key={day.label} className="flex flex-col gap-2">
              <h2 className="px-1 text-[13px] font-semibold text-foreground">
                {day.label} <span className="font-normal text-muted-foreground">· {day.files.length}</span>
              </h2>
              {view === "grid" ? (
                <div className="grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-2.5">
                  {day.files.map((f) => (
                    <FileTile key={f.path} file={f} owner={mates.find((m) => m.id === f.mateId)} />
                  ))}
                </div>
              ) : (
                <ul className="flex flex-col overflow-hidden rounded-2xl bg-card shadow-[0_0_0_1px_var(--border)]">
                  {day.files.map((f) => {
                    const owner = mates.find((m) => m.id === f.mateId);
                    return (
                      <li key={f.path} className="flex items-center gap-3 border-b border-border/50 px-3 py-1.5 last:border-b-0">
                        {owner && <MateFace avatar={owner.avatar} size={22} plate={false} label={owner.name} />}
                        <div className="min-w-0 flex-1">
                          {isPicture(f.path) ? <Thumb path={f.path} size={40} /> : <FileChip path={f.path} name={f.name} size={f.size} />}
                        </div>
                        <span className="shrink-0 text-xs text-muted-foreground">{formatRelative(f.modified)}</span>
                      </li>
                    );
                  })}
                </ul>
              )}
            </section>
          ))
        )}
      </div>
    </div>
  );
}
