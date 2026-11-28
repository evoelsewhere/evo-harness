// A mate's Knowledge (docs/features/evo-mates/spec.md, FR-77): files the
// person adds, copied into the mate's folder, which every turn of the mate
// names and tells it to read when they bear on the request.

import * as React from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileTextIcon, PlusIcon, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { formatBytes } from "@/lib/format";
import { log } from "@/lib/db";
import { matesApi, type KnowledgeFile, type Mate } from "@/lib/mates";

export function Knowledge({ mate }: { mate: Mate }) {
  const [files, setFiles] = React.useState<KnowledgeFile[]>([]);
  const [note, setNote] = React.useState<string | null>(null);
  const read = React.useCallback(() => {
    void matesApi
      .knowledge(mate.id)
      .then(setFiles)
      .catch(() => setFiles([]));
  }, [mate.id]);
  React.useEffect(read, [read]);

  const add = async () => {
    const picked = await open({ multiple: true, title: `Knowledge for ${mate.name}` });
    const paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    if (!paths.length) return;
    try {
      const result = await matesApi.addKnowledge(mate.id, paths);
      setNote(result.refused.length ? `Not added: ${result.refused.map(([name, why]) => `${name} (${why})`).join(", ")}.` : null);
    } catch (error) {
      setNote(String(error).replace(/^Error:\s*/, ""));
    }
    read();
  };

  const remove = async (name: string) => {
    await matesApi.removeKnowledge(mate.id, name).catch((error) => log("warn", "mates", `could not remove ${name}: ${String(error)}`));
    read();
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-[13px] font-medium text-foreground">Knowledge</div>
          <div className="text-xs text-muted-foreground">Files {mate.name} reads when they help. They stay on this computer.</div>
        </div>
        <Button size="sm" variant="secondary" className="rounded-full" onClick={() => void add()}>
          <PlusIcon />
          Add files
        </Button>
      </div>
      {files.length === 0 ? (
        <p className="rounded-2xl bg-muted/40 px-3 py-2 text-xs text-muted-foreground">No files yet: add notes, price lists, guides or anything {mate.name} should know.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {files.map((file) => (
            <li key={file.name} className="flex items-center gap-2 rounded-2xl bg-muted/40 px-3 py-1.5">
              <FileTextIcon className="size-3.5 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate text-xs text-foreground" title={file.name}>
                {file.name}
              </span>
              <span className="shrink-0 text-[11px] text-muted-foreground">{formatBytes(file.bytes)}</span>
              <button
                type="button"
                aria-label={`Remove ${file.name}`}
                onClick={() => void remove(file.name)}
                className="rounded-full p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
              >
                <XIcon className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
      {note && <p className="text-[11px] text-amber-700 dark:text-amber-300">{note}</p>}
    </div>
  );
}
