import * as React from "react";
import { GitBranchIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { CodeBlock, CodeBlockContent, CodeBlockLine } from "@/components/ui/code-block";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { notify } from "@/features/notifications/notify";
import { changedSinceReview, patchRows, worktrees, type WorktreeChanges } from "@/lib/worktrees";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";

// Review what a subagent or a nested session changed in its worktree, then
// apply it to the workspace (uncommitted) or discard it. Uses the shell's
// worktree commands (src-tauri/src/worktrees.rs).

export type WorktreeOutcome = "applied" | "discarded";

/** Said when the folder could not be deleted yet (something still works in it). */
const FOLDER_LEFT = "The worktree's folder is still in use and will be removed later.";

function PatchView({ patch }: { patch: string }) {
  const rows = React.useMemo(() => patchRows(patch), [patch]);
  return (
    <CodeBlock size="sm">
      <CodeBlockContent className="max-h-[45vh]" data-selectable>
        {rows.map((row, index) => (
          <CodeBlockLine
            key={index}
            variant={row.kind === "added" || row.kind === "removed" ? row.kind : "default"}
            className={cn(row.kind === "meta" && "text-muted-foreground", row.file && "mt-2 font-medium")}
          >
            {/* The line draws its own +/− marker. */}
            {row.text || " "}
          </CodeBlockLine>
        ))}
      </CodeBlockContent>
    </CodeBlock>
  );
}

export function WorktreeReview({
  open,
  onOpenChange,
  repo,
  path,
  branch,
  onSettled,
  beforeSettle,
  busy = false,
}: {
  open: boolean;
  onOpenChange(open: boolean): void;
  /** The folder the worktree was cut from: its changes are applied there. */
  repo: string;
  path: string;
  branch?: string;
  /** Also called when the changes are applied but the folder stays for now. */
  onSettled?(outcome: WorktreeOutcome): void;
  /** Awaited right before Apply or Discard runs, e.g. to stop what works in the worktree. */
  beforeSettle?: () => Promise<void>;
  /** An agent is still working in the worktree: Apply and Discard wait. */
  busy?: boolean;
}) {
  const [changes, setChanges] = React.useState<WorktreeChanges | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [working, setWorking] = React.useState<WorktreeOutcome | null>(null);
  const [confirmDiscard, setConfirmDiscard] = React.useState(false);
  const settle = useUiStore((s) => s.settleWorktree);

  const load = React.useCallback(() => {
    setChanges(null);
    worktrees
      .changes(repo, path)
      .then(setChanges)
      .catch((e: unknown) => setError(String(e)));
  }, [repo, path]);

  React.useEffect(() => {
    if (!open) return;
    setError(null);
    setConfirmDiscard(false);
    load();
  }, [open, load]);

  async function finish(outcome: WorktreeOutcome) {
    setWorking(outcome);
    setError(null);
    try {
      await beforeSettle?.();
      if (outcome === "applied") {
        // Exactly what is on screen: later edits in the worktree are refused.
        const done = await worktrees.apply(repo, path, changes?.hash);
        const body = `${done.files} ${done.files === 1 ? "file" : "files"} changed in your workspace (not committed).`;
        notify({
          source: "worktrees",
          level: "success",
          title: "Changes applied",
          body: done.cleaned ? body : `${body} ${FOLDER_LEFT}`,
        });
      } else {
        const done = await worktrees.discard(repo, path);
        if (!done.cleaned) notify({ source: "worktrees", title: "Worktree discarded", body: FOLDER_LEFT });
      }
      settle(path, outcome);
      onSettled?.(outcome);
      onOpenChange(false);
    } catch (e) {
      if (outcome === "applied" && changedSinceReview(e)) {
        setError("The worktree changed since you opened this. Its changes were read again: check them, then apply.");
        load();
      } else {
        setError(String(e));
      }
    } finally {
      setWorking(null);
    }
  }

  const blocked = working !== null || busy;
  const empty = changes && changes.files.length === 0;
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* One column no wider than the dialog: a long path or patch line
          scrolls or truncates inside it instead of widening the grid. */}
      <DialogContent className="max-h-[90vh] grid-cols-[minmax(0,1fr)] grid-rows-[auto_1fr_auto] sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <GitBranchIcon className="size-4 text-muted-foreground" />
            Review changes
          </DialogTitle>
          <DialogDescription className="truncate font-mono text-xs" title={path}>
            {branch ? `${branch} · ` : ""}
            {path}
          </DialogDescription>
        </DialogHeader>
        <div className="flex min-h-0 flex-col gap-3 overflow-y-auto">
          {!changes && !error && <p className="text-muted-foreground">Reading the changes…</p>}
          {empty && <p className="text-muted-foreground">Nothing changed in this worktree.</p>}
          {changes && !empty && (
            <>
              <p className="text-[13px] text-muted-foreground">
                {changes.files.length} {changes.files.length === 1 ? "file" : "files"} ·{" "}
                <span className="text-status-ok">+{changes.added}</span>{" "}
                <span className="text-destructive">−{changes.removed}</span> since{" "}
                <code className="font-mono">{changes.base.slice(0, 8)}</code>
              </p>
              <ul className="flex flex-col gap-0.5 font-mono text-xs">
                {changes.files.map((file) => (
                  <li key={file.path} className="flex items-center gap-2">
                    <span
                      className={cn(
                        "w-14 shrink-0 text-[11px] uppercase",
                        file.status === "added" ? "text-status-ok" : file.status === "deleted" ? "text-destructive" : "text-muted-foreground",
                      )}
                    >
                      {file.status}
                    </span>
                    <span className="truncate">{file.path}</span>
                    <span className="ml-auto shrink-0 text-muted-foreground tabular-nums">
                      {file.added === null ? "binary" : `+${file.added} −${file.removed}`}
                    </span>
                  </li>
                ))}
              </ul>
              <PatchView patch={changes.patch} />
              {changes.truncated && (
                <p className="text-xs text-muted-foreground">The patch is long; only its start is shown. Applying uses all of it.</p>
              )}
            </>
          )}
          {error && <p className="text-[13px] whitespace-pre-wrap text-destructive">{error}</p>}
          {busy && (
            <p className="text-[13px] text-muted-foreground">
              The agent is still working in this worktree. Apply or discard its changes when it has finished.
            </p>
          )}
        </div>
        <DialogFooter className="items-center">
          {confirmDiscard ? (
            <>
              <span className="mr-auto text-[13px] text-muted-foreground">Throw these changes and the worktree away?</span>
              <Button variant="outline" onClick={() => setConfirmDiscard(false)}>
                Keep
              </Button>
              <Button variant="destructive" disabled={blocked} onClick={() => void finish("discarded")}>
                {working === "discarded" ? "Discarding…" : "Discard"}
              </Button>
            </>
          ) : (
            <>
              <Button
                variant="ghost"
                className="mr-auto text-destructive hover:text-destructive"
                disabled={blocked}
                onClick={() => setConfirmDiscard(true)}
              >
                Discard…
              </Button>
              <Button variant="outline" onClick={() => onOpenChange(false)}>
                Close
              </Button>
              <Button disabled={blocked || !changes || Boolean(empty)} onClick={() => void finish("applied")}>
                {working === "applied" ? "Applying…" : "Apply to workspace"}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
