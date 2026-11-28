import * as React from "react";
import { Trash2Icon, Undo2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { db } from "@/lib/db";

// **Undo last change** and **Delete** for a learned skill, in Customize and
// a mate's Skills tab (docs/features/learning-layers/spec.md, FR-15, FR-18).

export function LearnedSkillActions({
  owner,
  name,
  canUndo = true,
  onDone,
}: {
  owner: string;
  name: string;
  canUndo?: boolean;
  onDone?(): void;
}) {
  const [confirming, setConfirming] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const run = async (action: () => Promise<unknown>) => {
    setError(null);
    try {
      await action();
      onDone?.();
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    }
  };
  return (
    <div className="flex flex-col items-end gap-1">
      <div className="flex items-center gap-1">
        {confirming ? (
          <>
            <span className="text-xs text-muted-foreground">Delete this learned skill?</span>
            <Button size="sm" variant="ghost" className="h-6 px-2 text-xs" onClick={() => setConfirming(false)}>
              Keep
            </Button>
            <Button size="sm" variant="destructive" className="h-6 px-2 text-xs" onClick={() => void run(() => db.learnedSkillDelete(owner, name))}>
              Delete
            </Button>
          </>
        ) : (
          <>
            <Button
              size="sm"
              variant="ghost"
              className="h-6 gap-1 px-2 text-xs"
              disabled={!canUndo}
              onClick={() => void run(() => db.learnedSkillUndo(owner, { name }))}
            >
              <Undo2Icon className="size-3" /> Undo last change
            </Button>
            <Button size="sm" variant="ghost" className="h-6 gap-1 px-2 text-xs" onClick={() => setConfirming(true)}>
              <Trash2Icon className="size-3" /> Delete
            </Button>
          </>
        )}
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}
