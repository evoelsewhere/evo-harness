// Followed threads and each channel's thread rows in the Mates window
// (docs/features/evo-mates-threads-board/spec.md, FR-20, FR-21). The
// channel's engine writes a thread's row as lines land; the window reads them
// again on `mates://links`, when a channel's feed changes, and every 30 s.

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { linksApi, type ThreadFollow } from "@/lib/mates";

interface ThreadsState {
  followed: ThreadFollow[];
  /** Per channel chat: its threads' rows, by root. */
  byChat: Record<string, Record<string, ThreadFollow>>;
  load(): Promise<void>;
  loadChat(chat: string): Promise<void>;
  set(chat: string, root: string, change: { following?: boolean; resolved?: boolean }): Promise<void>;
  read(chat: string, root: string): Promise<void>;
}

export const useThreadsStore = create<ThreadsState>()((set, get) => ({
  followed: [],
  byChat: {},
  async load() {
    const followed = await linksApi.followed().catch(() => null);
    if (followed) set({ followed });
  },
  async loadChat(chat) {
    const rows = await linksApi.threadsOf(chat).catch(() => null);
    if (rows) set((s) => ({ byChat: { ...s.byChat, [chat]: Object.fromEntries(rows.map((r) => [r.root, r])) } }));
  },
  async set(chat, root, change) {
    await linksApi.set(chat, root, change).catch(() => undefined);
    await Promise.all([get().load(), get().loadChat(chat)]);
  },
  async read(chat, root) {
    const row = get().byChat[chat]?.[root];
    if (row && !row.unread) return;
    await linksApi.read(chat, root).catch(() => undefined);
    await Promise.all([get().load(), get().loadChat(chat)]);
  },
}));

/** Keep followed threads fresh (once per window). */
export function watchThreads(): () => void {
  const store = useThreadsStore.getState();
  void store.load();
  const timer = window.setInterval(() => {
    void useThreadsStore.getState().load();
    for (const chat of Object.keys(useThreadsStore.getState().byChat)) void useThreadsStore.getState().loadChat(chat);
  }, 30_000);
  let stop: (() => void) | undefined;
  let live = true;
  void listen("mates://links", () => {
    void useThreadsStore.getState().load();
    for (const chat of Object.keys(useThreadsStore.getState().byChat)) void useThreadsStore.getState().loadChat(chat);
  })
    .then((unlisten) => {
      if (live) stop = unlisten;
      else unlisten();
    })
    .catch(() => undefined);
  return () => {
    live = false;
    window.clearInterval(timer);
    stop?.();
  };
}
