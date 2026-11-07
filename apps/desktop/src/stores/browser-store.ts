// In-app browser: tabs (each a native child webview, src-tauri/src/browser.rs),
// which one shows, and where. Tabs belong to a chat: `tabs` and `activeId` are
// those of the chat showing (`chat`), the other chats' pages keep running,
// hidden, until their chat shows again. The side panel's browser view reports
// the area the page should cover; the active tab is shown there only while the
// Browser tool is visible and nothing (a menu, a dialog) lies over it.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { forgetTabChat, recalledTabChat, rememberTabChat } from "@/lib/tab-chats";
import { inTauri } from "@/lib/platform";
import { useUiStore } from "./ui-store";

export interface BrowserTab {
  /** The child webview's label. */
  id: string;
  url: string;
  title: string;
  loading: boolean;
  /** The chat the page belongs to; `null` for a page found after a reload whose chat is unknown. */
  chat: string | null;
}

export interface Bounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface BrowserStore {
  /** The showing chat's tabs. */
  tabs: BrowserTab[];
  /** The showing chat's visible tab. */
  activeId: string | null;
  /** Every tab of every chat. */
  all: BrowserTab[];
  activeByChat: Record<string, string | null>;
  /** The chat whose tabs show. */
  chat: string | null;
  /** Where pages show, in window logical pixels; `null` while the view is not mounted. */
  bounds: Bounds | null;
  /** A menu or dialog covers the page area: hide the native view meanwhile. */
  covered: boolean;
  /** Recently visited URLs, newest first (for the start page). */
  history: string[];
  error: string | null;
  /** Show `chat`'s tabs; pages whose chat is unknown go to the first chat shown. */
  setChat(chat: string | null): void;
  /** Open `url` in a new tab (shown); resolves to its id. `chat`: the agent's chat when not the showing one. */
  newTab(url: string, options?: { background?: boolean; chat?: string | null }): Promise<string | null>;
  closeTab(id: string): void;
  select(id: string): void;
  navigate(id: string, input: string): Promise<void>;
  go(id: string, action: "back" | "forward" | "reload" | "stop"): void;
  patchTab(id: string, change: Partial<BrowserTab>): void;
  setBounds(bounds: Bounds | null): void;
  setCovered(covered: boolean): void;
  /** Resolve once `id` finishes loading (or after `timeoutMs`). */
  waitForLoad(id: string, timeoutMs?: number): Promise<void>;
}

const HISTORY_MAX = 12;
const loadWaiters = new Map<string, Array<() => void>>();

const keyOf = (chat: string | null) => chat ?? "";

/** Whether the Browser tool is the visible side panel tool. */
const browserShown = () => {
  const ui = useUiStore.getState();
  return ui.rightPanelOpen && ui.activePanel === "browser";
};

type Source = Pick<BrowserStore, "all" | "activeByChat" | "chat">;

/** What the showing chat sees. */
function view({ all, activeByChat, chat }: Source): Pick<BrowserStore, "tabs" | "activeId"> {
  const tabs = all.filter((t) => t.chat === chat);
  const wanted = activeByChat[keyOf(chat)];
  return { tabs, activeId: wanted && tabs.some((t) => t.id === wanted) ? wanted : (tabs.at(-1)?.id ?? null) };
}

export const useBrowserStore = create<BrowserStore>()(
  persist(
    (set, get) => {
      /** Change the source of truth and refresh what the showing chat sees. */
      const commit = (change: (s: BrowserStore) => Partial<Source>) =>
        set((s) => {
          const next: Source = { all: s.all, activeByChat: s.activeByChat, chat: s.chat, ...change(s) };
          return { ...next, ...view(next) };
        });

      const patch = (id: string, change: Partial<BrowserTab>) =>
        commit((s) => ({ all: s.all.map((t) => (t.id === id ? { ...t, ...change } : t)) }));

      return {
        tabs: [],
        activeId: null,
        all: [],
        activeByChat: {},
        chat: null,
        bounds: null,
        covered: false,
        history: [],
        error: null,

        setChat(chat) {
          if (chat === get().chat) return;
          commit((s) => ({
            chat,
            // Pages found after a reload with no known chat go to the first chat shown.
            all: chat && s.chat === null ? s.all.map((t) => (t.chat === null ? { ...t, chat } : t)) : s.all,
          }));
          sync();
        },

        async newTab(url, options = {}) {
          if (!inTauri) return null;
          const chat = options.chat !== undefined ? options.chat : get().chat;
          const bounds = get().bounds ?? { x: 0, y: 0, width: 800, height: 600 };
          try {
            const id = await invoke<string>("browser_create", { url, ...bounds, visible: false });
            rememberTabChat("browser", id, chat);
            commit((s) => {
              const current = view(s);
              const key = keyOf(chat);
              const showing = chat === s.chat ? current.activeId : (s.activeByChat[key] ?? null);
              return {
                all: [...s.all, { id, url, title: "", loading: true, chat }],
                activeByChat: { ...s.activeByChat, [key]: options.background && showing ? showing : id },
              };
            });
            set({ error: null });
            sync();
            return id;
          } catch (error) {
            set({ error: String(error) });
            return null;
          }
        },

        closeTab(id) {
          void invoke("browser_close", { tab: id }).catch(() => undefined);
          forgetTabChat("browser", id);
          commit((s) => {
            const closing = s.all.find((t) => t.id === id);
            if (!closing) return {};
            const mates = s.all.filter((t) => t.chat === closing.chat);
            const index = mates.findIndex((t) => t.id === id);
            const rest = mates.filter((t) => t.id !== id);
            const key = keyOf(closing.chat);
            const active = s.activeByChat[key] === id || s.activeByChat[key] === undefined ? (rest[Math.min(index, rest.length - 1)]?.id ?? null) : s.activeByChat[key];
            return { all: s.all.filter((t) => t.id !== id), activeByChat: { ...s.activeByChat, [key]: active } };
          });
          sync();
        },

        select(id) {
          const tab = get().all.find((t) => t.id === id);
          if (!tab) return;
          commit((s) => ({ activeByChat: { ...s.activeByChat, [keyOf(tab.chat)]: id } }));
          sync(true);
        },

        async navigate(id, input) {
          try {
            const url = await invoke<string>("browser_navigate", { tab: id, url: input });
            patch(id, { url, loading: true });
            set({ error: null });
          } catch (error) {
            set({ error: String(error) });
          }
        },

        go(id, action) {
          void invoke("browser_history", { tab: id, action }).catch(() => undefined);
        },

        patchTab: patch,

        setBounds(bounds) {
          const current = get().bounds;
          if (
            bounds &&
            current &&
            Math.abs(bounds.x - current.x) < 0.5 &&
            Math.abs(bounds.y - current.y) < 0.5 &&
            Math.abs(bounds.width - current.width) < 0.5 &&
            Math.abs(bounds.height - current.height) < 0.5
          )
            return;
          set({ bounds });
          sync();
        },

        setCovered(covered) {
          if (covered === get().covered) return;
          set({ covered });
          sync();
        },

        waitForLoad(id, timeoutMs = 15000) {
          const tab = get().all.find((t) => t.id === id);
          if (!tab?.loading) return Promise.resolve();
          return new Promise((resolve) => {
            const done = () => {
              clearTimeout(timer);
              resolve();
            };
            const timer = setTimeout(done, timeoutMs);
            loadWaiters.set(id, [...(loadWaiters.get(id) ?? []), done]);
          });
        },
      };
    },
    {
      name: "evo.browser",
      partialize: (state) => ({ history: state.history }),
    },
  ),
);

// ── Native views follow the store ────────────────────────────────────────────

let shownTab: string | null = null;

/** Show the showing chat's active tab at the current bounds (or hide every tab). */
function sync(focus = false) {
  if (!inTauri) return;
  const { tabs, activeId, bounds, covered } = useBrowserStore.getState();
  const visible = browserShown() && !!bounds && !covered && bounds.width > 4 && bounds.height > 4;
  const target = visible && activeId && tabs.some((t) => t.id === activeId) ? activeId : null;
  if (shownTab && shownTab !== target) void invoke("browser_visible", { tab: shownTab, visible: false }).catch(() => undefined);
  if (target && bounds) {
    void invoke("browser_bounds", { tab: target, ...bounds })
      .then(() => invoke("browser_visible", { tab: target, visible: true, focus }))
      .catch(() => undefined);
  }
  shownTab = target;
}

useUiStore.subscribe((state, previous) => {
  if (state.rightPanelOpen !== previous.rightPanelOpen || state.activePanel !== previous.activePanel) sync();
  // Closing the Browser tool tab closes its pages: the showing chat's, not those of
  // a chat whose tabs were swapped in.
  if (state.panelChat === previous.panelChat && previous.openTabs.includes("browser") && !state.openTabs.includes("browser")) {
    for (const tab of useBrowserStore.getState().tabs) useBrowserStore.getState().closeTab(tab.id);
  }
});

/** Re-read a tab's title from the page (title events skip pages without one). */
async function refreshTitle(id: string) {
  const title = await invoke<unknown>("browser_eval", { tab: id, script: "document.title" }).catch(() => null);
  if (typeof title !== "string") return;
  useBrowserStore.getState().patchTab(id, { title });
}

/**
 * Take over this window's pages the store lost track of (the window reloaded,
 * or this module was hot-reloaded): they keep running in the native layer.
 * Each goes back to the chat it belonged to when that is remembered.
 * Resolves to the number of tabs adopted.
 */
export async function adoptTabs(): Promise<number> {
  if (!inTauri) return 0;
  const native = await invoke<Array<[string, string]>>("browser_list").catch(() => []);
  const known = new Set(useBrowserStore.getState().all.map((t) => t.id));
  const fresh = native.filter(([id]) => !known.has(id));
  if (!fresh.length) return 0;
  useBrowserStore.setState((s) => {
    const adopted: BrowserTab[] = fresh.map(([id, url]) => ({ id, url, title: "", loading: false, chat: recalledTabChat("browser", id) ?? s.chat }));
    const all = [...s.all, ...adopted];
    const activeByChat = { ...s.activeByChat };
    for (const tab of adopted) if (activeByChat[keyOf(tab.chat)] == null) activeByChat[keyOf(tab.chat)] = tab.id;
    const next: Source = { all, activeByChat, chat: s.chat };
    return { ...next, ...view(next) };
  });
  for (const [id] of fresh) void refreshTitle(id);
  sync();
  return fresh.length;
}

if (inTauri) {
  void adoptTabs();
  const unlisten = listen<{ tab: string; kind: string; url?: string; title?: string }>("browser://event", ({ payload }) => {
    const store = useBrowserStore.getState();
    const { tab, kind, url, title } = payload;
    if (!store.all.some((t) => t.id === tab)) return;
    const set = (change: Partial<BrowserTab>) => store.patchTab(tab, change);
    if (kind === "loadStart") set({ loading: true, ...(url ? { url } : {}) });
    else if (kind === "loadEnd") {
      set({ loading: false, ...(url ? { url } : {}) });
      void refreshTitle(tab);
      for (const done of loadWaiters.get(tab) ?? []) done();
      loadWaiters.delete(tab);
      if (url && /^https?:/.test(url)) {
        useBrowserStore.setState((s) => ({
          history: [url, ...s.history.filter((h) => h !== url)].slice(0, HISTORY_MAX),
        }));
      }
    } else if (kind === "title") set({ title: title ?? "" });
    else if (kind === "newWindow" && url) {
      // A page opened from a page belongs to the same chat.
      const opener = store.all.find((t) => t.id === tab);
      void store.newTab(url, { chat: opener?.chat ?? store.chat });
    }
  });
  // A hot-reloaded copy of this module must not leave the old listener behind.
  import.meta.hot?.dispose(() => {
    void unlisten.then((stop) => stop());
  });
}

/**
 * The tab the agent acts on: the given one (by id, in any chat), else the
 * visible one of `chat` (default: the chat showing).
 */
export function agentTab(id?: string, chat?: string | null): BrowserTab | undefined {
  const { all, activeByChat, chat: showing } = useBrowserStore.getState();
  if (id) return all.find((t) => t.id === id);
  const owner = chat === undefined ? showing : chat;
  const mine = all.filter((t) => t.chat === owner);
  const wanted = activeByChat[keyOf(owner)];
  return mine.find((t) => t.id === wanted) ?? mine.at(-1);
}
