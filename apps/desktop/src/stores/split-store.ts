// Split view: one to four chats in a grid in the main window
// (docs/features/chat-split-view/spec.md, plan 3.2). The focused pane's chat
// is always the engine store's `activeId`, so everything that follows the
// active chat follows the focused pane; this store keeps which chats the
// panes show, which one has the focus and the divider's place. The rules are
// in `split-rules.ts`.

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { chatWindowId } from "@/lib/windows";
import { useEngineStore } from "./engine-store";
import { useSessionStore } from "./session-store";
import {
  MAX_PANES,
  NO_SPLIT,
  closePane,
  dropMissing,
  followActive,
  openPane,
  replacePane,
  swapPanes,
  type Layout,
  type PaneIndex,
} from "./split-rules";

interface SplitStore extends Layout {
  /** Show `chat` as a new pane at place `at` in reading order, beside the active chat when one pane shows (FR-1, FR-2, FR-11, FR-15). */
  open(chat: string, at: number): Promise<void>;
  /** Show `chat` in pane `index`. */
  replace(index: PaneIndex, chat: string): Promise<void>;
  /** Panes `a` and `b` change places (FR-7). */
  swap(a: PaneIndex, b: PaneIndex): void;
  /** Close pane `index`; its chat goes on in the shell (FR-6). */
  close(index: PaneIndex): void;
  /** Focus pane `index`: its chat becomes the active one (FR-4). */
  focusIndex(index: PaneIndex): void;
  setRatio(ratio: number): void;
  setRowRatio(ratio: number): void;
}

const engine = () => useEngineStore.getState();

export const useSplitStore = create<SplitStore>()(
  persist(
    (set, get) => {
      const apply = (layout: Layout) => set({ panes: layout.panes, focus: layout.focus, ratio: layout.ratio, rowRatio: layout.rowRatio });
      const focused = () => {
        const { panes, focus } = get();
        if (panes) engine().focusPane(panes[focus]);
      };
      return {
        ...NO_SPLIT,

        async open(chat, at) {
          if (chatWindowId) return;
          const next = openPane(get(), engine().activeId, chat, at);
          if (next === get()) return;
          // The new chat's engine is attached before its pane shows.
          if (chat !== engine().activeId) await engine().showChat(chat);
          apply(next);
          focused();
        },

        async replace(index, chat) {
          const next = replacePane(get(), index, chat);
          if (next === get()) return;
          await engine().showChat(chat);
          apply(next);
          focused();
        },

        swap(a, b) {
          apply(swapPanes(get(), a, b));
        },

        close(index) {
          const { layout, remaining } = closePane(get(), index);
          apply({ ...layout, ratio: get().ratio, rowRatio: get().rowRatio });
          if (remaining && remaining !== engine().activeId) engine().focusPane(remaining);
        },

        focusIndex(index) {
          const { panes } = get();
          if (!panes) return;
          set({ focus: index });
          engine().focusPane(panes[index]);
        },

        setRatio(ratio) {
          set({ ratio });
        },

        setRowRatio(rowRatio) {
          set({ rowRatio });
        },
      };
    },
    {
      name: "evo-split",
      // A chat window shows one chat: it never splits, and keeps nothing.
      partialize: (state) => (chatWindowId ? {} : { panes: state.panes, focus: state.focus, ratio: state.ratio, rowRatio: state.rowRatio }),
    },
  ),
);

/**
 * Show `chat` beside the chat on screen: in a new pane after the focused one
 * (a new right pane when one shows), or, with four panes, in the pane after the
 * focused one (Open in › Split view, a session card's Open beside; FR-8, FR-14,
 * FR-15).
 */
export function openBeside(chat: string): Promise<void> {
  const split = useSplitStore.getState();
  if (!split.panes) return split.open(chat, 1);
  const count = split.panes.length;
  return split.open(chat, count >= MAX_PANES ? (split.focus + 1) % count : split.focus + 1);
}

/**
 * The pane that shows `chat` closes (its chat was archived, deleted or moved
 * to a window of its own); the chat the other pane shows, if any.
 */
export function forgetPane(chat: string): string | null {
  const store = useSplitStore.getState();
  const index = store.panes?.indexOf(chat) ?? -1;
  if (index < 0) return null;
  const { layout, remaining } = closePane(store, index as PaneIndex);
  useSplitStore.setState({ panes: layout.panes, focus: layout.focus });
  return remaining;
}

let started = false;

/**
 * Follow the active chat and the chat list: a chat picked elsewhere replaces
 * the focused pane or focuses the pane that shows it (FR-4); a pane whose chat
 * is gone closes (FR-12). The saved pair comes back once the active chat is
 * shown (FR-10).
 */
export function startSplit(): void {
  if (started || chatWindowId) return;
  started = true;
  useEngineStore.subscribe((state, previous) => {
    if (state.activeId === previous.activeId) return;
    const split = useSplitStore.getState();
    const next = followActive(split, state.activeId);
    if (next !== split) useSplitStore.setState({ panes: next.panes, focus: next.focus });
  });
  useSessionStore.subscribe((state, previous) => {
    if (state.sessions === previous.sessions || !state.loaded) return;
    const live = new Set(state.sessions.filter((s) => s.archivedAt == null).map((s) => s.id));
    const split = useSplitStore.getState();
    const { layout, remaining } = dropMissing(split, (chat) => live.has(chat));
    if (layout === split) return;
    useSplitStore.setState({ panes: layout.panes, focus: layout.focus });
    if (remaining && remaining !== engine().activeId) engine().focusPane(remaining);
  });
}

/** Show the saved panes again: the other panes' chats are attached beside the active one (FR-10). */
export async function restoreSplit(): Promise<void> {
  if (chatWindowId) return;
  const split = useSplitStore.getState();
  if (!split.panes) return;
  const sessions = useSessionStore.getState().sessions;
  const alive = (chat: string) => sessions.some((s) => s.id === chat && s.archivedAt == null && !s.draft);
  const { layout } = dropMissing(split, alive);
  if (!layout.panes) {
    useSplitStore.setState({ panes: null, focus: 0 });
    return;
  }
  const active = engine().activeId;
  // The active chat is the focused pane's: put the saved focus on it.
  const at = active ? layout.panes.indexOf(active) : -1;
  if (at < 0) {
    useSplitStore.setState({ panes: null, focus: 0 });
    return;
  }
  for (const chat of layout.panes) if (chat !== active) await engine().showChat(chat);
  useSplitStore.setState({ panes: layout.panes, focus: at, ratio: layout.ratio, rowRatio: layout.rowRatio });
}
