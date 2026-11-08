import * as React from "react";
import { useBrowserStore } from "@/stores/browser-store";
import { split, useCardsStore, type Card } from "@/stores/cards-store";
import { useUiStore } from "@/stores/ui-store";

const NONE: Card[] = [];

/** What a card shows in its bar: the page's title or host. */
export function useCardTitle(card: Card): string {
  const tab = useBrowserStore((s) => s.all.find((t) => t.id === card.id));
  if (!tab) return "Page";
  if (tab.title) return tab.title;
  try {
    return new URL(tab.url).host || tab.url;
  } catch {
    return tab.url || "New tab";
  }
}

/**
 * The chat's cards as the window draws them: its shown cards and its chips,
 * without the tab the side panel is showing at the moment (a tab in the panel
 * has no card, FR-6).
 */
export function useChatCards(chat: string | null): { shown: Card[]; chips: Card[] } {
  const all = useCardsStore((s) => (chat ? (s.cards[chat] ?? NONE) : NONE));
  const panelOpen = useUiStore((s) => s.rightPanelOpen);
  const activePanel = useUiStore((s) => s.activePanel);
  const panelChat = useUiStore((s) => s.panelChat);
  const browserTab = useBrowserStore((s) => s.activeId);
  return React.useMemo(() => {
    const inPanel = (card: Card) =>
      panelOpen &&
      panelChat === chat &&
      activePanel === card.kind &&
      browserTab === card.id;
    return split(all.filter((card) => !inPanel(card)));
  }, [all, panelOpen, activePanel, panelChat, chat, browserTab]);
}
