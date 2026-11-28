import * as React from "react";
import { GraduationCapIcon, Undo2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { db, type JournalEntry } from "@/lib/db";
import { cn } from "@/lib/utils";
import { useLearnedStore } from "@/stores/learned-store";

// "Learned · <skill>" under a reply after which a learned skill changed;
// selecting it opens the change, with Undo. Changes apply at once; nothing
// waits for approval (docs/features/learning-layers/spec.md, FR-18).

const EMPTY: JournalEntry[] = [];

/** The lines a changed skill's text took out and put in. */
function changedLines(before: string | null, after: string | null) {
  const old = (before ?? "").split("\n").map((line) => line.trim()).filter(Boolean);
  const next = (after ?? "").split("\n").map((line) => line.trim()).filter(Boolean);
  return {
    removed: old.filter((line) => !next.includes(line)),
    added: next.filter((line) => !old.includes(line)),
  };
}

export function LearnedLine({ chat, turnEntry, className }: { chat: string; turnEntry?: string | null; className?: string }) {
  const rows = useLearnedStore((s) => s.byChat[chat] ?? EMPTY);
  const load = useLearnedStore((s) => s.load);
  const [open, setOpen] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!(chat in useLearnedStore.getState().byChat)) void load(chat);
  }, [chat, load]);
  const mine = turnEntry ? rows.filter((row) => row.replyId === turnEntry && row.action !== "undone") : EMPTY;
  if (mine.length === 0) return null;
  const names = [...new Set(mine.map((row) => row.name))];
  const undo = async (row: JournalEntry) => {
    setError(null);
    try {
      await db.learnedSkillUndo(row.owner, { change: row.id });
      await load(chat);
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    }
  };
  return (
    <div className={cn("flex flex-col items-start gap-1", className)}>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground transition-colors hover:text-foreground"
      >
        <GraduationCapIcon className="size-3" />
        Learned · {names.join(", ")}
      </button>
      {open && (
        <div className="flex w-full max-w-xl flex-col gap-2 rounded-xl bg-muted/50 px-3 py-2 text-xs dark:bg-accent/40">
          {mine.map((row) => {
            const { removed, added } = changedLines(row.before, row.after);
            return (
              <div key={row.id} className="flex flex-col gap-1">
                <div className="flex items-center gap-2">
                  <span className={cn("font-mono font-medium", row.undoneAt && "text-muted-foreground line-through")}>{row.name}</span>
                  <span className="text-muted-foreground">{row.action === "added" ? "new skill" : "updated"}</span>
                  {row.diff && <span className="truncate text-muted-foreground">· {row.diff}</span>}
                  <span className="flex-1" />
                  {row.undoneAt ? (
                    <span className="text-muted-foreground">Undone</span>
                  ) : (
                    <Button size="sm" variant="ghost" className="h-6 gap-1 px-2 text-xs" onClick={() => void undo(row)}>
                      <Undo2Icon className="size-3" /> Undo
                    </Button>
                  )}
                </div>
                <div className="max-h-48 overflow-y-auto rounded-md bg-background px-2.5 py-1.5 font-mono ring-1 ring-border/70">
                  {removed.map((line, index) => (
                    <p key={`r${index}`} className="text-destructive">
                      − {line}
                    </p>
                  ))}
                  {added.map((line, index) => (
                    <p key={`a${index}`}>+ {line}</p>
                  ))}
                </div>
              </div>
            );
          })}
          {error && <p className="text-destructive">{error}</p>}
        </div>
      )}
    </div>
  );
}
