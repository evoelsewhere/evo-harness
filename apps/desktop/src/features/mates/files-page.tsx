// Evo Mates' Files page (docs/features/evo-mates/spec.md, FR-84; plan 3.12):
// every file in the mates' own folders, newest first, with its mate, a
// thumbnail for pictures, a search by name and a filter by mate.

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { SearchIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useMatesStore } from "@/stores/mates-store";
import { isPicture } from "./attachments";
import { FileChip, Thumb } from "./attachment-views";

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

export function FilesPage() {
  const mates = useMatesStore((s) => s.mates);
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
  const pictures = shown.filter((f) => isPicture(f.path));
  const others = shown.filter((f) => !isPicture(f.path));

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-3xl flex-col gap-5 px-6 py-8">
        <header className="flex flex-col gap-1 px-1">
          <h1 className="text-2xl font-semibold tracking-tight text-foreground">Files</h1>
          <p className="text-sm text-muted-foreground">What your mates made and keep in their folders.</p>
        </header>
        <div className="flex flex-wrap items-center gap-2">
          <div role="tablist" aria-label="Mates" className="flex flex-wrap gap-1">
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
          <label className="ml-auto flex h-8 w-56 items-center gap-2 rounded-lg bg-muted px-2.5 text-sm">
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
          <p className="px-1 text-sm text-muted-foreground">{files.length ? "No file matches." : "No files yet: what your mates make shows here."}</p>
        ) : (
          <>
            {pictures.length > 0 && (
              <section className="flex flex-col gap-2">
                <h2 className="px-1 text-xs font-medium text-muted-foreground">Pictures</h2>
                <div className="flex flex-wrap gap-2">
                  {pictures.map((f) => (
                    <Thumb key={f.path} path={f.path} size={120} />
                  ))}
                </div>
              </section>
            )}
            {others.length > 0 && (
              <section className="flex flex-col gap-1">
                <h2 className="px-1 pb-1 text-xs font-medium text-muted-foreground">Documents</h2>
                {others.map((f) => {
                  const owner = mates.find((m) => m.id === f.mateId);
                  return (
                    <div key={f.path} className="flex items-center gap-3 px-1 py-1">
                      {owner && <MateFace avatar={owner.avatar} size={22} plate={false} label={owner.name} />}
                      <div className="min-w-0 flex-1">
                        <FileChip path={f.path} name={f.name} size={f.size} />
                      </div>
                      <span className="shrink-0 text-xs text-muted-foreground">{formatRelative(f.modified)}</span>
                    </div>
                  );
                })}
              </section>
            )}
          </>
        )}
      </div>
    </div>
  );
}
