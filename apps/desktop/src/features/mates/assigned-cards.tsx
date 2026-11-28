// Cards beyond one channel (docs/features/evo-mates-threads-board/spec.md,
// FR-14): the open cards assigned to the person (Today's My cards) or to a
// mate (its profile), across boards that are on, overdue first. A row opens
// the card's sheet.

import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { boardsApi, type PlacedCard } from "@/lib/mates";
import { inTauri } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useBoardStore } from "@/stores/board-store";
import { dueLabel } from "./board-logic";
import { PriorityMark } from "./board-view";

/** Open cards assigned to `assignee`, read again when a board changes. */
export function useAssignedCards(assignee: string): PlacedCard[] {
  const [cards, setCards] = React.useState<PlacedCard[]>([]);
  React.useEffect(() => {
    let gone = false;
    const read = () =>
      void boardsApi
        .assigned(assignee)
        .then((all) => !gone && setCards(all))
        .catch(() => undefined);
    read();
    const stop = inTauri ? listen("mates://board", read) : null;
    return () => {
      gone = true;
      void stop?.then((off) => off());
    };
  }, [assignee]);
  return cards;
}

/** One card in a list beyond its channel: key, title, where, due. */
export function PlacedCardRow({ placed, now = Date.now(), className }: { placed: PlacedCard; now?: number; className?: string }) {
  const { card } = placed;
  const late = card.dueMs !== null && card.dueMs < now;
  return (
    <button
      type="button"
      onClick={() => useBoardStore.getState().showCard(card.key)}
      className={cn("flex w-full items-center gap-2.5 px-3.5 py-2.5 text-left transition-colors hover:bg-accent/60", className)}
    >
      <span className="w-[72px] shrink-0 font-mono text-[11.5px] text-muted-foreground tabular-nums">{card.key}</span>
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[13px] font-medium text-foreground">{card.title}</span>
        <span className="flex items-center gap-1.5 truncate text-xs text-muted-foreground">
          #{placed.channelName} · {placed.column}
          <PriorityMark priority={card.priority} />
        </span>
      </span>
      {card.dueMs !== null && <span className={cn("shrink-0 text-xs", late ? "font-medium text-destructive" : "text-muted-foreground")}>{late ? `Overdue · ${dueLabel(card.dueMs, now)}` : dueLabel(card.dueMs, now)}</span>}
    </button>
  );
}
