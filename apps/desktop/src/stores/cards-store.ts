// Floating cards over the chat for the pages and shells an agent uses
// (docs/features/floating-cards). A card is the view of one tab: it never owns
// the page or the shell, so removing a card changes nothing about the tab.
//
// Cards belong to a chat (`cards[chat]`): the layer draws the showing chat's,
// the others wait. A card is shown (`card`) or minimized to a chip (`chip`);
// at most MAX_VISIBLE are shown per chat. Positions are kept per chat and tab,
// in memory and in the session storage (a window reload keeps them).

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { useUiStore } from "./ui-store";

/** Only a page has a card: an agent's shell runs without one (docs/features/floating-cards FR-18). */
export type CardKind = "browser";
export type CardMode = "card" | "chip";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Card {
  /** `kind:id`, stable for the tab's life. */
  key: string;
  kind: CardKind;
  /** The browser tab's id. */
  id: string;
  mode: CardMode;
  /** Where the person put it (layer pixels); `null` until moved: it sits in its slot. */
  rect: Rect | null;
  /** Its size until the person resizes it. */
  size: { w: number; h: number };
  /** Stacking order among the chat's cards. */
  z: number;
  /** What the agent last did with the tab, in words ("Clicking ref_4"). */
  activity: string | null;
  /** A tool call is working on the tab right now. */
  busy: boolean;
}

/** The most cards a chat shows at once; more tabs are chips (FR-13). */
export const MAX_VISIBLE = 3;
const MARGIN = 16;
/** The card bar's height; the body below it is 16:10 for a page. */
export const BAR_HEIGHT = 32;
export const MIN_WIDTH = 220;

export const cardKey = (kind: CardKind, id: string) => `${kind}:${id}`;

/** A new card's size: a page is 16:10 under the bar. */
export function defaultSize(_kind: CardKind): { w: number; h: number } {
  const w = 360;
  return { w, h: Math.round(w * 0.625) + BAR_HEIGHT };
}

/** Keep `rect` inside a layer of `layer` pixels (it is never larger than the layer less its margin). */
export function clampRect(rect: Rect, layer: { w: number; h: number }): Rect {
  const w = Math.max(Math.min(rect.w, layer.w - MARGIN * 2), Math.min(MIN_WIDTH, layer.w));
  const h = Math.max(Math.min(rect.h, layer.h - MARGIN * 2), Math.min(BAR_HEIGHT * 2, layer.h));
  const x = Math.min(Math.max(rect.x, 0), Math.max(layer.w - w, 0));
  const y = Math.min(Math.max(rect.y, 0), Math.max(layer.h - h, 0));
  return { x, y, w, h };
}

/** Where the `index`-th shown card sits when the person has not moved it: from the top right, cascading. */
export function slotRect(index: number, size: { w: number; h: number }, layer: { w: number; h: number }): Rect {
  return clampRect({ x: layer.w - size.w - MARGIN - index * 24, y: MARGIN + index * 32, w: size.w, h: size.h }, layer);
}

/** The cards a chat shows, in stacking order, and its chips in the order they came. */
export function split(cards: Card[]): { shown: Card[]; chips: Card[] } {
  return {
    shown: cards.filter((c) => c.mode === "card").sort((a, b) => a.z - b.z),
    chips: cards.filter((c) => c.mode === "chip"),
  };
}

/** The visible cards, least recently raised first: the one to demote when another wants a place. */
const oldestShown = (cards: Card[]) => cards.filter((c) => c.mode === "card").sort((a, b) => a.z - b.z)[0];

interface CardsStore {
  cards: Record<string, Card[]>;
  /** A card for the tab, shown unless the setting is off or the chat has MAX_VISIBLE already. */
  ensure(chat: string, kind: CardKind, id: string): void;
  /** Show a card or minimize it to a chip; showing a fourth minimizes the least recently raised. */
  setMode(chat: string, key: string, mode: CardMode): void;
  /** Bring a card to the front. */
  raise(chat: string, key: string): void;
  /** The person moved or resized a card. */
  place(chat: string, key: string, rect: Rect): void;
  setActivity(chat: string, key: string, activity: string | null, busy?: boolean): void;
  /** The tab closed (or the card is not wanted): drop its card and chip. */
  remove(chat: string, key: string): void;
  /** Drop the cards of tabs that closed, wherever they are. */
  removeTab(kind: CardKind, id: string): void;
  /** The chat is gone. */
  forgetChat(chat: string): void;
}

const update = (cards: Record<string, Card[]>, chat: string, change: (list: Card[]) => Card[]) => ({ ...cards, [chat]: change(cards[chat] ?? []) });
const nextZ = (list: Card[]) => list.reduce((top, c) => Math.max(top, c.z), 0) + 1;

export const useCardsStore = create<CardsStore>()(
  persist(
    (set) => ({
      cards: {},
      ensure(chat, kind, id) {
        set((s) => {
          const key = cardKey(kind, id);
          const list = s.cards[chat] ?? [];
          if (list.some((c) => c.key === key)) return s;
          const room = list.filter((c) => c.mode === "card").length < MAX_VISIBLE;
          const mode: CardMode = useUiStore.getState().agentCards && room ? "card" : "chip";
          const card: Card = { key, kind, id, mode, rect: null, size: defaultSize(kind), z: nextZ(list), activity: null, busy: false };
          return { cards: { ...s.cards, [chat]: [...list, card] } };
        });
      },
      setMode(chat, key, mode) {
        set((s) => ({
          cards: update(s.cards, chat, (list) => {
            if (!list.some((c) => c.key === key)) return list;
            let next = list.map((c) => (c.key === key ? { ...c, mode, z: mode === "card" ? nextZ(list) : c.z } : c));
            if (mode === "card" && next.filter((c) => c.mode === "card").length > MAX_VISIBLE) {
              const out = oldestShown(next.filter((c) => c.key !== key));
              if (out) next = next.map((c) => (c.key === out.key ? { ...c, mode: "chip" as const } : c));
            }
            return next;
          }),
        }));
      },
      raise(chat, key) {
        set((s) => ({ cards: update(s.cards, chat, (list) => (list.some((c) => c.key === key && c.z < nextZ(list) - 1) ? list.map((c) => (c.key === key ? { ...c, z: nextZ(list) } : c)) : list)) }));
      },
      place(chat, key, rect) {
        set((s) => ({ cards: update(s.cards, chat, (list) => list.map((c) => (c.key === key ? { ...c, rect, size: { w: rect.w, h: rect.h } } : c))) }));
      },
      setActivity(chat, key, activity, busy = false) {
        set((s) => {
          const card = s.cards[chat]?.find((c) => c.key === key);
          if (!card || (card.activity === activity && card.busy === busy)) return s;
          return { cards: update(s.cards, chat, (list) => list.map((c) => (c.key === key ? { ...c, activity, busy } : c))) };
        });
      },
      remove(chat, key) {
        set((s) => (s.cards[chat]?.some((c) => c.key === key) ? { cards: update(s.cards, chat, (list) => list.filter((c) => c.key !== key)) } : s));
      },
      removeTab(kind, id) {
        const key = cardKey(kind, id);
        set((s) => {
          if (!Object.values(s.cards).some((list) => list.some((c) => c.key === key))) return s;
          return { cards: Object.fromEntries(Object.entries(s.cards).map(([chat, list]) => [chat, list.filter((c) => c.key !== key)])) };
        });
      },
      forgetChat(chat) {
        set((s) => {
          if (!(chat in s.cards)) return s;
          const { [chat]: _gone, ...rest } = s.cards;
          return { cards: rest };
        });
      },
    }),
    {
      name: "evo.cards",
      storage: createJSONStorage(() => sessionStorage),
      partialize: (state) => ({ cards: state.cards }),
      // Cards of shells, which an earlier build kept, are dropped.
      merge: (persisted, current) => {
        const saved = (persisted as { cards?: Record<string, Card[]> } | undefined)?.cards ?? {};
        const cards = Object.fromEntries(Object.entries(saved).map(([chat, list]) => [chat, list.filter((c) => c.kind === "browser")]));
        return { ...current, cards };
      },
    },
  ),
);

