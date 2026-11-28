// Channels' boards in the Mates window (docs/features/evo-mates-threads-board/
// spec.md, FR-8 – FR-15): each channel's board and cards as the shell last
// gave them, every board's key (for `KEY-n` links), which tab a channel
// shows, and the card or thread asked to open. A change anywhere emits
// `mates://board`; `watchBoards` reads the board again.

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { boardsApi, type BoardView, type CardPatch, type NewCard } from "@/lib/mates";
import { log } from "@/lib/db";

interface BoardState {
  views: Record<string, BoardView>;
  keys: string[];
  /** Per channel chat: the conversation or the board (FR-8). */
  tab: Record<string, "chat" | "board">;
  /** The card whose sheet is open, by key. */
  openCard: string | null;
  /** A thread asked to open from a card (FR-15). */
  openThread: { chat: string; root: string } | null;
  load(channel: string): Promise<BoardView | null>;
  loadKeys(): Promise<void>;
  setTab(chat: string, tab: "chat" | "board"): void;
  showCard(key: string | null): void;
  showThread(target: { chat: string; root: string } | null): void;
  turn(channel: string, on: boolean, key?: string): Promise<void>;
  create(channel: string, card: NewCard): Promise<void>;
  update(channel: string, id: string, patch: CardPatch): Promise<void>;
  move(channel: string, id: string, column: string, index: number | null): Promise<void>;
}

function warn(what: string) {
  return (error: unknown) => {
    log("warn", "mates", `${what}: ${String(error)}`);
    throw error;
  };
}

export const useBoardStore = create<BoardState>()((set, get) => ({
  views: {},
  keys: [],
  tab: {},
  openCard: null,
  openThread: null,
  async load(channel) {
    try {
      const view = await boardsApi.get(channel);
      set((s) => ({ views: { ...s.views, [channel]: view } }));
      return view;
    } catch (error) {
      log("warn", "mates", `could not read the board: ${String(error)}`);
      return null;
    }
  },
  async loadKeys() {
    const keys = await boardsApi.keys().catch(() => null);
    if (keys) set({ keys });
  },
  setTab: (chat, tab) => set((s) => ({ tab: { ...s.tab, [chat]: tab } })),
  showCard: (key) => set({ openCard: key }),
  showThread: (target) => set({ openThread: target }),
  async turn(channel, on, key) {
    const view = await boardsApi.set(channel, on, key).catch(warn("could not change the board"));
    set((s) => ({ views: { ...s.views, [channel]: view } }));
    void get().loadKeys();
  },
  async create(channel, card) {
    await boardsApi.create(channel, card).catch(warn("could not add the card"));
    await get().load(channel);
  },
  async update(channel, id, patch) {
    await boardsApi.update(id, patch).catch(warn("could not change the card"));
    await get().load(channel);
  },
  async move(channel, id, column, index) {
    // Shown at once (FR-10): the card moves here before the shell answers.
    const view = get().views[channel];
    if (view) {
      const rest = view.cards.filter((c) => c.columnId === column && c.id !== id).sort((a, b) => a.position - b.position);
      const at = index === null ? rest.length : Math.min(index, rest.length);
      const prev = rest[at - 1]?.position;
      const next = rest[at]?.position;
      const position = prev === undefined ? (next === undefined ? 0 : next - 1) : next === undefined ? prev + 1 : (prev + next) / 2;
      const cards = view.cards.map((c) => (c.id === id ? { ...c, columnId: column, position } : c));
      set((s) => ({ views: { ...s.views, [channel]: { ...view, cards } } }));
    }
    await boardsApi.move(id, column, index).catch(warn("could not move the card"));
    await get().load(channel);
  },
}));

/** Read boards again when they change (once per window). */
export function watchBoards(): () => void {
  let stop: (() => void) | undefined;
  let live = true;
  void useBoardStore.getState().loadKeys();
  void listen<{ channel: string }>("mates://board", (event) => {
    const channel = event.payload?.channel;
    if (channel && useBoardStore.getState().views[channel]) void useBoardStore.getState().load(channel);
    void useBoardStore.getState().loadKeys();
  })
    .then((unlisten) => {
      if (live) stop = unlisten;
      else unlisten();
    })
    .catch(() => undefined);
  return () => {
    live = false;
    stop?.();
  };
}
