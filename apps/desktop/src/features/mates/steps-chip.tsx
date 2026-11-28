// A reply's work as a chip (docs/features/evo-mates-ui-v2/spec.md, FR-17;
// evo-mates FR-31, QA B3): "Read 3 files · Searched the web", which opens
// in place to each step in plain words with what it touched, and whether it
// failed or was not allowed. Raw tool names never show.

import * as React from "react";
import { BanIcon, CheckIcon, ChevronRightIcon, LoaderIcon, TriangleAlertIcon, WrenchIcon } from "lucide-react";
import type { Turn } from "@/features/chat";
import { cn } from "@/lib/utils";
import { stepItems } from "./thread-rows";

export function StepsChip({ text, turn }: { text: string; turn: Turn }) {
  const [open, setOpen] = React.useState(false);
  const items = React.useMemo(() => stepItems(turn), [turn]);
  const id = React.useId();
  return (
    <div className="flex flex-col items-start">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 rounded-full bg-muted/60 py-0.5 pr-2.5 pl-2 text-[11.5px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      >
        <WrenchIcon className="size-3" />
        {text}
        <ChevronRightIcon className={cn("size-3 transition-transform", open && "rotate-90")} />
      </button>
      {open && (
        <ol id={id} className="mt-1 ml-1 flex flex-col gap-0.5 border-l border-border py-0.5 pl-3 animate-in fade-in duration-150">
          {items.map((item) => (
            <li key={item.key} className="flex min-w-0 items-center gap-1.5 text-[12px] text-muted-foreground">
              {item.status === "done" ? (
                <CheckIcon className="size-3 shrink-0 text-muted-foreground" />
              ) : item.status === "running" ? (
                <LoaderIcon className="size-3 shrink-0 motion-safe:animate-spin" />
              ) : item.status === "denied" ? (
                <BanIcon className="size-3 shrink-0" />
              ) : (
                <TriangleAlertIcon className="size-3 shrink-0" />
              )}
              <span className="min-w-0 truncate">
                {item.status === "denied" ? "Not allowed: " : item.status === "failed" ? "Didn't work: " : ""}
                {item.word}
                {item.target && <span className="text-foreground/80"> {item.target}</span>}
              </span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
