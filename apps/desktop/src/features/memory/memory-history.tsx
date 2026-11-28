import * as React from "react";
import { Undo2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";

// The journal of memory changes: every change to a topic by whoever made it,
// with the chat it came from, each undoable (RPC `list_memory_changes`,
// `undo_memory_change`). Used by the Memory panel and a mate's Memory tab.
// Spec: docs/features/learning-layers/spec.md (FR-1, FR-2).

/** One change to a topic. */
export interface MemoryChange {
  id: number;
  at: number;
  owner?: string;
  scope: "personal" | "project";
  name: string;
  action: "added" | "updated" | "deleted" | "archived" | "undone";
  /** Who made it: agent, person, index, tidy, dream, review, retro or undo. */
  by?: string;
  chatId: string | null;
  chatTitle: string | null;
  removed: string[];
  added: string[];
  /** False when its earlier text held a secret or was too long to keep. */
  undoable?: boolean;
  undone: boolean;
}

const BY: Record<string, string> = {
  agent: "the agent",
  person: "you",
  index: "indexing",
  tidy: "the tidy-up",
  dream: "the dream",
  review: "a skill review",
  retro: "a retro",
  undo: "an undo",
};

const VERB: Record<MemoryChange["action"], string> = {
  added: "Added",
  updated: "Updated",
  deleted: "Deleted",
  archived: "Archived",
  undone: "Undid",
};

/** One change: what it did, who did it, where it came from, and undo. */
function ChangeRow({ change, onUndo }: { change: MemoryChange; onUndo(): void }) {
  const [open, setOpen] = React.useState(false);
  const listed = useSessionStore((s) => (change.chatId ? s.sessions.some((chat) => chat.id === change.chatId) : false));
  const by = change.by ? BY[change.by] : undefined;
  const canUndo = change.undoable !== false;
  return (
    <div className={cn("rounded-lg px-2.5 py-1.5 transition-colors", open ? "bg-muted/60 dark:bg-accent/40" : "hover:bg-accent/50")}>
      <div className="flex min-w-0 items-center gap-2">
        <button type="button" onClick={() => setOpen(!open)} className="flex min-w-0 flex-1 items-baseline gap-1.5 text-left">
          <span className="shrink-0 text-xs text-muted-foreground">{VERB[change.action] ?? "Changed"}</span>
          <span className={cn("truncate font-mono text-[12.5px]", change.undone && "text-muted-foreground line-through")}>
            {change.name}
          </span>
          {by && <span className="shrink-0 truncate text-[11px] text-muted-foreground">by {by}</span>}
          <span className="ml-auto shrink-0 text-[11px] text-muted-foreground tabular-nums">{formatRelative(change.at)}</span>
        </button>
        {change.undone ? (
          <span className="shrink-0 text-[11px] text-muted-foreground">Undone</span>
        ) : canUndo ? (
          <Button size="sm" variant="ghost" className="h-6 shrink-0 gap-1 px-2 text-xs" onClick={onUndo}>
            <Undo2Icon className="size-3" /> Undo
          </Button>
        ) : (
          <span className="shrink-0 text-[11px] text-muted-foreground" title="Its earlier text held a secret or was too long to keep">
            Cannot undo
          </span>
        )}
      </div>
      {open && (
        <div className="mt-1.5 flex flex-col gap-1.5">
          <div className="rounded-md bg-background px-3 py-2 font-mono text-xs ring-1 ring-border/70">
            {change.removed.map((line, index) => (
              <p key={`r${index}`} className="text-destructive">
                − {line}
              </p>
            ))}
            {change.added.map((line, index) => (
              <p key={`a${index}`}>+ {line}</p>
            ))}
            {change.removed.length + change.added.length === 0 && <p className="text-muted-foreground">No line changed.</p>}
          </div>
          {change.chatId &&
            (listed ? (
              <button
                type="button"
                className="self-start truncate text-xs text-link"
                onClick={() => void useEngineStore.getState().selectSession(change.chatId!)}
              >
                From “{change.chatTitle ?? "a chat"}”
              </button>
            ) : (
              <span className="text-xs text-muted-foreground">
                From {change.chatTitle ? `“${change.chatTitle}”` : "a chat that is gone"}
              </span>
            ))}
        </div>
      )}
    </div>
  );
}

/** The journal, newest first. */
export function MemoryHistory({ changes, onUndo }: { changes: MemoryChange[]; onUndo(id: number): void }) {
  if (changes.length === 0) return null;
  return (
    <section className="mb-3 flex flex-col gap-0.5">
      <p className="flex items-baseline gap-1.5 px-2 pb-1 text-[11px] font-medium text-muted-foreground">
        <span className="truncate">History</span>
        <span className="shrink-0 font-normal opacity-70">Every change, newest first</span>
      </p>
      {changes.map((change) => (
        <ChangeRow key={change.id} change={change} onUndo={() => onUndo(change.id)} />
      ))}
    </section>
  );
}
