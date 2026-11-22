// Under a finished reply: the files the turn changed, each opening its diff
// for that turn in Source control, and Revert this turn (with Undo)
// (docs/features/source-control/spec.md, FR-10 – FR-13).

import * as React from "react";
import { ChevronRightIcon, RotateCcwIcon, Undo2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ConfirmDialog, type Confirm } from "@/features/source-control";
import { notify } from "@/features/notifications/notify";
import { scm } from "@/lib/scm";
import { cn } from "@/lib/utils";
import { useEngineStore, usePaneChat, usePaneEngine } from "@/stores/engine-store";
import { useSourceControlStore } from "@/stores/source-control-store";
import { useUiStore } from "@/stores/ui-store";
import type { Turn, TurnCheckpoint, TurnCheckpointFile } from "./transcript";

const baseName = (path: string) => path.split("/").pop() ?? path;
const dirName = (path: string) => path.split("/").slice(0, -1).join("/");

// A status is a dot, never a fill (DESIGN.md): added, deleted and renamed get
// a colour; a modified file, the common case, stays quiet.
const STATUS: Record<TurnCheckpointFile["status"], { label: string; dot: string }> = {
  added: { label: "Added", dot: "bg-status-ok" },
  modified: { label: "Modified", dot: "bg-muted-foreground/40" },
  deleted: { label: "Deleted", dot: "bg-status-error" },
  renamed: { label: "Renamed", dot: "bg-status-info" },
};
const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

function Counts({ added, removed }: { added?: number | null; removed?: number | null }) {
  return (
    <span className="flex shrink-0 items-center gap-1.5 font-mono text-[11px] tabular-nums">
      {added ? <span className="text-status-ok">+{added}</span> : null}
      {removed ? <span className="text-status-error">−{removed}</span> : null}
    </span>
  );
}

export function TurnChanges({ turn }: { turn: Turn & { checkpoint: TurnCheckpoint } }) {
  const chat = usePaneChat();
  const busy = usePaneEngine((s) => s.transcript.busy);
  const { checkpoint } = turn;
  const [open, setOpen] = React.useState(false);
  const [working, setWorking] = React.useState(false);
  const [confirm, setConfirm] = React.useState<Confirm | null>(null);
  const [overlap, setOverlap] = React.useState<string[]>([]);
  const files = checkpoint.files;

  // Another chat in the same folder at the same time (this app run only).
  React.useEffect(() => {
    if (!chat || !checkpoint.runId) return;
    let live = true;
    void scm
      .turnOverlap(chat, checkpoint.runId)
      .then((chats) => live && setOverlap(chats))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [chat, checkpoint.runId]);

  if (files.length === 0 || !chat) return null;
  const added = files.reduce((sum, file) => sum + (file.added ?? 0), 0);
  const removed = files.reduce((sum, file) => sum + (file.removed ?? 0), 0);

  const show = (path: string) => {
    if (!checkpoint.start || !checkpoint.end) return;
    void useSourceControlStore.getState().showTurn(
      { chat, checkpoint: checkpoint.id, start: checkpoint.start, end: checkpoint.end, prompt: turn.prompt, files },
      path,
    );
    useUiStore.getState().openPanel("source-control");
  };

  const revert = async (id: string, force: boolean, undoing: boolean) => {
    setWorking(true);
    try {
      const result = await useEngineStore.getState().revertTurn(chat, id, force);
      void useSourceControlStore.getState().refresh(useSourceControlStore.getState().cwd);
      if (result.skipped.length > 0 && !force) {
        setConfirm({
          title: `${plural(result.skipped.length, "file")} changed since this turn`,
          body: `${result.reverted.length ? `${plural(result.reverted.length, "file")} went back. ` : ""}Left as they are, because they changed after the turn:\n${result.skipped.map((s) => s.path).join("\n")}`,
          action: "Keep them",
          alternative: { label: "Revert anyway", run: () => void revert(id, true, undoing) },
          run: () => undefined,
        });
      }
    } catch (error) {
      notify({
        source: "source-control",
        level: "error",
        title: undoing ? "Could not undo the revert" : "Could not revert the turn",
        body: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setWorking(false);
    }
  };

  const askRevert = () =>
    setConfirm({
      title: "Revert this turn?",
      body: `${plural(files.length, "file")} go back to how they were before this turn:\n${files.map((f) => f.path).join("\n")}\n\nFiles changed after the turn are left alone. The chat stays as it is; the agent is told on your next message.`,
      action: "Revert",
      destructive: true,
      run: () => void revert(checkpoint.id, false, false),
    });

  const notes = [
    !checkpoint.complete && "Changes made before the checkpoint was taken may be missing from this list.",
    overlap.length > 0 && "Another chat worked in this folder at the same time; its changes may be listed here too.",
  ].filter((note): note is string => Boolean(note));

  return (
    <div className="flex flex-col rounded-xl bg-card text-[13px] shadow-border-small" data-testid="turn-changes">
      <div className="flex items-center justify-between gap-2 py-1 pr-1.5 pl-1">
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          aria-expanded={open}
          className="flex min-w-0 items-center gap-2 rounded-lg px-2 py-1 text-left transition-colors hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring/30 focus-visible:outline-none"
        >
          <ChevronRightIcon className={cn("size-3.5 shrink-0 text-muted-foreground transition-transform", open && "rotate-90")} />
          <span className="font-medium">Changed {plural(files.length, "file")}</span>
          <Counts added={added} removed={removed} />
        </button>
        {checkpoint.reverted ? (
          <span className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
            Reverted
            <Button
              variant="ghost"
              size="xs"
              aria-label="Undo the revert of this turn"
              disabled={working || busy}
              onClick={() => void revert(checkpoint.reverted!.undo, false, true)}
            >
              <Undo2Icon />
              Undo
            </Button>
          </span>
        ) : (
          <Button
            variant="ghost"
            size="xs"
            className="shrink-0 text-muted-foreground"
            disabled={working || busy || !checkpoint.start}
            title={busy ? "Wait for the turn to finish" : "Put these files back as they were before this turn"}
            onClick={askRevert}
          >
            <RotateCcwIcon />
            Revert this turn
          </Button>
        )}
      </div>
      {notes.length > 0 && (
        <div className="flex flex-col gap-0.5 px-3.5 pb-2">
          {notes.map((note) => (
            <p key={note} className="text-xs text-muted-foreground">
              {note}
            </p>
          ))}
        </div>
      )}
      {open && (
        <div className="px-1.5 pb-1.5">
          <ul className="flex flex-col">
            {files.map((file) => {
              const status = STATUS[file.status] ?? STATUS.modified;
              const dir = dirName(file.path);
              return (
                <li key={file.path}>
                  <button
                    type="button"
                    onClick={() => show(file.path)}
                    className="flex w-full min-w-0 items-center gap-2.5 rounded-lg px-2 py-1 text-left transition-colors hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring/30 focus-visible:outline-none"
                    title={file.from ? `${file.from} → ${file.path}` : file.path}
                  >
                    <span className={cn("size-1.5 shrink-0 rounded-full", status.dot)} role="img" aria-label={status.label} />
                    <span className="flex min-w-0 flex-1 items-baseline gap-2">
                      <span className="shrink-0">{baseName(file.path)}</span>
                      {dir && <span className="min-w-0 truncate text-xs text-muted-foreground">{dir}</span>}
                    </span>
                    <Counts added={file.added} removed={file.removed} />
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </div>
  );
}
