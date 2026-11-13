// Split view: two chats side by side in the main window
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
  NO_SPLIT,
  closePane,
  dropMissing,
  followActive,
  openPane,
  replacePane,
  swapPanes,
  type Layout,
  type PaneIndex,
  type Side,
} from "./split-rules";

interface SplitStore extends Layout {
  /** Show `chat` beside the active chat on `side`, or in the pane on that side when two show (FR-1, FR-2, FR-11). */
  open(chat: string, side: Side): Promise<void>;
  /** Show `chat` in pane `index`. */
  replace(index: PaneIndex, chat: string): Promise<void>;
  /** The panes change sides (FR-7). */
  swap(): void;
  /** Close pane `index`; its chat goes on in the shell (FR-6). */
  close(index: PaneIndex): void;
  /** Focus pane `index`: its chat becomes the active one (FR-4). */
  focusIndex(index: PaneIndex): void;
  setRatio(ratio: number): void;
}

const engine = () => useEngineStore.getState();

export const useSplitStore = create<SplitStore>()(
  persist(
    (set, get) => {
      const apply = (layout: Layout) => set({ panes: layout.panes, focus: layout.focus, ratio: layout.ratio });
      const focused = () => {
        const { panes, focus } = get();
        if (panes) engine().focusPane(panes[focus]);
      };
      return {
        ...NO_SPLIT,

        async open(chat, side) {
          if (chatWindowId) return;
          const next = openPane(get(), engine().activeId, chat, side);
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

        swap() {
          apply(swapPanes(get()));
        },

        close(index) {
          const { layout, remaining } = closePane(get(), index);
          apply({ ...layout, ratio: get().ratio });
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
      };
    },
    {
      name: "evo-split",
      // A chat window shows one chat: it never splits, and keeps nothing.
      partialize: (state) => (chatWindowId ? {} : { panes: state.panes, focus: state.focus, ratio: state.ratio }),
    },
  ),
);

/**
 * Show `chat` beside the chat on screen: in a new right pane, or in the pane
 * that does not have the focus (Open in › Split view, a session card's Open
 * beside; FR-8, FR-14).
 */
export function openBeside(chat: string): Promise<void> {
  const split = useSplitStore.getState();
  return split.panes ? split.replace(split.focus === 0 ? 1 : 0, chat) : split.open(chat, "right");
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

/** Show the saved pair again: the other pane's chat is attached beside the active one (FR-10). */
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
  const other = layout.panes[at === 0 ? 1 : 0];
  await engine().showChat(other);
  useSplitStore.setState({ panes: layout.panes, focus: at as PaneIndex, ratio: layout.ratio });
}
