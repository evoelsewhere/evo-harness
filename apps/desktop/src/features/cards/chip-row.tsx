import { AppWindowIcon, GlobeIcon } from "lucide-react";
import { useEngineStore } from "@/stores/engine-store";
import { useCardsStore, type Card } from "@/stores/cards-store";
import { useCardTitle, useChatCards } from "./use-cards";

function Chip({ chat, card }: { chat: string; card: Card }) {
  const title = useCardTitle(card);
  const Icon = card.kind === "app" ? AppWindowIcon : GlobeIcon;
  return (
    <button
      type="button"
      title={card.activity ? `${title} · ${card.activity}` : title}
      onClick={() => useCardsStore.getState().setMode(chat, card.key, "card")}
      className="flex h-6 max-w-44 min-w-0 shrink-0 items-center gap-1.5 rounded-full border border-border/70 bg-background px-2 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
    >
      <Icon className="size-3 shrink-0" />
      <span className="truncate">{title}</span>
    </button>
  );
}

/**
 * The chat's minimized cards, above the composer: one chip per tab the agent
 * uses whose card is not showing; a click brings the card back (FR-4).
 */
export function ChipRow() {
  const chat = useEngineStore((s) => s.activeId);
  const { chips } = useChatCards(chat);
  if (!chat || chips.length === 0) return null;
  return (
    <div role="group" aria-label="Minimized cards" className="mb-2 flex min-w-0 items-center gap-1.5 overflow-x-auto [scrollbar-width:none]">
      {chips.map((card) => (
        <Chip key={card.key} chat={chat} card={card} />
      ))}
    </div>
  );
}
