import * as React from "react";
import { GlobeIcon, MinusIcon, PanelRightOpenIcon, TerminalIcon } from "lucide-react";
import { BAR_HEIGHT, MIN_WIDTH, clampRect, slotRect, useCardsStore, type Card as CardData, type Rect } from "@/stores/cards-store";
import { useBrowserStore } from "@/stores/browser-store";
import { useTerminalStore } from "@/stores/terminal-store";
import { useUiStore } from "@/stores/ui-store";
import { cn } from "@/lib/utils";
import { useCardTitle } from "./use-cards";

/** Show the card's tab in the side panel (the card, and its chip, go while it shows there). */
export function openInPanel(chat: string, card: CardData) {
  useUiStore.getState().openPanel(card.kind, chat);
  if (card.kind === "browser") useBrowserStore.getState().select(card.id);
  else useTerminalStore.getState().select(card.id);
  useCardsStore.getState().setMode(chat, card.key, "chip");
}

type Layer = { w: number; h: number };

/** Keep a page card 16:10 under its bar. */
const fit = (card: CardData, rect: Rect): Rect => (card.kind === "browser" ? { ...rect, h: Math.round(rect.w * 0.625) + BAR_HEIGHT } : rect);

/**
 * One floating card: a bar (kind, title, what the agent is doing, minimize,
 * open in panel) that drags it, a body, and a corner that resizes it. The body
 * is the tab's own view (browser-body / terminal-body).
 */
export function Card({ chat, card, slot, layer, children }: { chat: string; card: CardData; slot: number; layer: Layer; children: React.ReactNode }) {
  const title = useCardTitle(card);
  const rect = card.rect ? clampRect(card.rect, layer) : slotRect(slot, card.size, layer);
  const { place, raise, setMode } = useCardsStore.getState();
  const drag = React.useRef<{ pointer: number; x: number; y: number; start: Rect; mode: "move" | "resize" } | null>(null);

  const begin = (mode: "move" | "resize") => (event: React.PointerEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    event.stopPropagation();
    raise(chat, card.key);
    drag.current = { pointer: event.pointerId, x: event.clientX, y: event.clientY, start: rect, mode };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const move = (event: React.PointerEvent<HTMLElement>) => {
    const d = drag.current;
    if (!d || d.pointer !== event.pointerId) return;
    const dx = event.clientX - d.x;
    const dy = event.clientY - d.y;
    const next: Rect = d.mode === "move" ? { ...d.start, x: d.start.x + dx, y: d.start.y + dy } : { ...d.start, w: Math.max(MIN_WIDTH, d.start.w + dx), h: Math.max(BAR_HEIGHT * 2, d.start.h + dy) };
    place(chat, card.key, d.mode === "resize" ? fit(card, clampRect(next, layer)) : clampRect(next, layer));
  };
  const end = (event: React.PointerEvent<HTMLElement>) => {
    if (drag.current?.pointer === event.pointerId) drag.current = null;
  };

  const Icon = card.kind === "browser" ? GlobeIcon : TerminalIcon;
  return (
    <section
      aria-label={`${card.kind === "browser" ? "Page" : "Terminal"} card: ${title}`}
      data-slot="agent-card"
      onPointerDown={() => raise(chat, card.key)}
      style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h, zIndex: card.z }}
      className="pointer-events-auto absolute flex flex-col overflow-hidden rounded-xl border border-border/70 bg-popover text-popover-foreground shadow-lg"
    >
      <header
        onPointerDown={begin("move")}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={end}
        style={{ height: BAR_HEIGHT }}
        className="flex shrink-0 cursor-grab touch-none items-center gap-1.5 border-b border-border/60 px-2 text-xs select-none active:cursor-grabbing"
      >
        <Icon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="min-w-0 truncate font-medium">{title}</span>
        {card.activity && <span className="min-w-0 flex-1 truncate text-muted-foreground">· {card.activity}</span>}
        {!card.activity && <span className="flex-1" />}
        <button
          type="button"
          aria-label="Open in side panel"
          title="Open in side panel"
          onPointerDown={(event) => event.stopPropagation()}
          onClick={() => openInPanel(chat, card)}
          className="flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
        >
          <PanelRightOpenIcon className="size-3.5" />
        </button>
        <button
          type="button"
          aria-label="Minimize"
          title="Minimize"
          onPointerDown={(event) => event.stopPropagation()}
          onClick={() => setMode(chat, card.key, "chip")}
          className="flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
        >
          <MinusIcon className="size-3.5" />
        </button>
      </header>
      <div className="relative min-h-0 flex-1">{children}</div>
      <div
        onPointerDown={begin("resize")}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={end}
        aria-hidden
        className={cn("absolute right-0 bottom-0 size-4 cursor-nwse-resize touch-none")}
      />
    </section>
  );
}
