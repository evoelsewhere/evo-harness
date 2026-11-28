import * as React from "react";
import { ClipboardCheckIcon, Undo2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { db, log } from "@/lib/db";
import type { RetroCard as Card } from "@/lib/mates";

// A retro of a channel job, as a card in the channel: what went well, what
// went wrong, and each mate's lessons with Undo. It is not a message: no
// mate reads or answers it (docs/features/learning-layers/spec.md, FR-21).

const STATUS: Record<string, string> = {
  completed: "finished",
  stopped: "was stopped",
  limit: "hit the hand-off limit",
  failed: "had a mate fail",
  silent: "got no reply in time",
};

export function RetroCard({ card }: { card: Card }) {
  const [undone, setUndone] = React.useState<Set<number>>(new Set());
  const undo = async (mate: string, row: number) => {
    try {
      await db.learnedSkillUndo(`mate:${mate}`, { change: row });
      setUndone((current) => new Set(current).add(row));
    } catch (error) {
      log("warn", "mates", `could not undo a retro lesson: ${String(error)}`);
    }
  };
  return (
    <div className="flex justify-center px-6 py-2">
      <div className="flex w-full max-w-[85%] flex-col gap-2 rounded-2xl bg-muted/50 px-4 py-3 text-[13px] shadow-[0_0_0_1px_var(--border)]">
        <div className="flex items-center gap-2">
          <ClipboardCheckIcon className="size-4 text-brand" />
          <span className="font-semibold">Retro</span>
          <span className="text-xs text-muted-foreground">
            The job {STATUS[card.status] ?? card.status} after {card.handoffs} {card.handoffs === 1 ? "hand-off" : "hand-offs"}
          </span>
        </div>
        {card.well.length > 0 && (
          <div>
            <p className="text-xs font-medium text-muted-foreground">Went well</p>
            <ul className="list-disc pl-5">
              {card.well.map((point, i) => (
                <li key={i}>{point}</li>
              ))}
            </ul>
          </div>
        )}
        {card.wrong.length > 0 && (
          <div>
            <p className="text-xs font-medium text-muted-foreground">Went wrong</p>
            <ul className="list-disc pl-5">
              {card.wrong.map((point, i) => (
                <li key={i}>{point}</li>
              ))}
            </ul>
          </div>
        )}
        {card.lessons.length > 0 && (
          <div className="flex flex-col gap-1">
            <p className="text-xs font-medium text-muted-foreground">Lessons</p>
            {card.lessons.map((lesson, i) => (
              <div key={i} className="flex items-start gap-2 rounded-xl bg-background/70 px-2.5 py-1.5">
                <div className="min-w-0 flex-1">
                  <span className="font-medium">{lesson.name}</span>: {lesson.text}
                  {lesson.skill && <div className="text-[11px] text-muted-foreground">In its learned skill {lesson.skill}</div>}
                </div>
                {lesson.row != null &&
                  (undone.has(lesson.row) ? (
                    <span className="shrink-0 text-[11px] text-muted-foreground">Undone</span>
                  ) : (
                    <Button size="sm" variant="ghost" className="h-6 shrink-0 gap-1 px-2 text-xs" onClick={() => void undo(lesson.mate, lesson.row!)}>
                      <Undo2Icon className="size-3" /> Undo
                    </Button>
                  ))}
              </div>
            ))}
          </div>
        )}
        {card.well.length + card.wrong.length + card.lessons.length === 0 && (
          <p className="text-xs text-muted-foreground">Nothing to change this time.</p>
        )}
      </div>
    </div>
  );
}
