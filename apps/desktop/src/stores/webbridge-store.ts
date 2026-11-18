// Evo Webbridge as the shell reports it (src-tauri/src/webbridge/): which
// browsers are set up, which profiles are connected, and why a connection was
// refused. See /docs/features/browser-bridge/spec.md (FR-2, FR-4, FR-23).

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { inTauri } from "@/lib/platform";

export interface WebbridgeBrowser {
  id: "chrome" | "edge";
  label: string;
  registered: boolean;
  extensionsPage: string;
}

export interface WebbridgeProfile {
  key: string;
  browser: "chrome" | "edge";
  profile: string;
  label: string;
  version: string;
  debugger: boolean;
}

export interface WebbridgeStatus {
  browsers: WebbridgeBrowser[];
  profiles: WebbridgeProfile[];
  running: boolean;
  refused: string | null;
  extensionFolder: string | null;
  hostMissing: string | null;
}

export type SitePermission = "ask" | "allow" | "block";

export interface WebbridgeSite {
  site: string;
  permission: SitePermission;
}

/** A tab of the person's own browser that a chat works in. */
export interface WebbridgeChatTab {
  id: string;
  browser: string;
  title: string;
  url: string;
  loading: boolean;
  taken: boolean;
}

interface WebbridgeState {
  status: WebbridgeStatus | null;
  sites: WebbridgeSite[];
  chatTabs: Record<string, WebbridgeChatTab[]>;
  busy: string | null;
  error: string | null;
  load(): Promise<void>;
  setUp(browser: string): Promise<void>;
  remove(browser: string): Promise<void>;
  rename(profile: string, label: string): Promise<void>;
  setSite(site: string, permission: SitePermission): Promise<void>;
  stopAll(profile?: string): Promise<void>;
  loadChatTabs(chat: string): Promise<void>;
  release(chat: string, tab: string): Promise<void>;
}

let listening = false;

export const useWebbridgeStore = create<WebbridgeState>((set, get) => {
  async function run(busy: string, call: () => Promise<WebbridgeStatus>) {
    set({ busy, error: null });
    try {
      set({ status: await call() });
    } catch (error) {
      set({ error: String(error) });
    } finally {
      set({ busy: null });
    }
  }
  const watched = new Set<string>();
  return {
    status: null,
    sites: [],
    chatTabs: {},
    busy: null,
    error: null,
    async load() {
      if (!inTauri) return;
      if (!listening) {
        listening = true;
        const refresh = () => {
          void get().load();
          for (const chat of watched) void get().loadChatTabs(chat);
        };
        void listen("webbridge://changed", refresh);
        void listen("browser://external", refresh);
        void listen("browser://closed", refresh);
      }
      try {
        const [status, sites] = await Promise.all([invoke<WebbridgeStatus>("webbridge_status"), invoke<WebbridgeSite[]>("webbridge_sites")]);
        set({ status, sites });
      } catch (error) {
        set({ error: String(error) });
      }
    },
    setUp: (browser) => run(`setup:${browser}`, () => invoke<WebbridgeStatus>("webbridge_setup", { browser })),
    remove: (browser) => run(`remove:${browser}`, () => invoke<WebbridgeStatus>("webbridge_remove", { browser })),
    rename: (profile, label) => run(`rename:${profile}`, () => invoke<WebbridgeStatus>("webbridge_rename", { profile, label })),
    async setSite(site, permission) {
      try {
        set({ sites: await invoke<WebbridgeSite[]>("webbridge_set_site", { site, permission }), error: null });
      } catch (error) {
        set({ error: String(error) });
      }
    },
    stopAll: (profile) => run("stop-all", () => invoke<WebbridgeStatus>("webbridge_stop_all", { profile: profile ?? null })),
    async loadChatTabs(chat) {
      if (!inTauri || !chat) return;
      watched.add(chat);
      if (!listening) void get().load();
      try {
        const tabs = await invoke<WebbridgeChatTab[]>("webbridge_chat_tabs", { chat });
        set({ chatTabs: { ...get().chatTabs, [chat]: tabs } });
      } catch {
        // The shell has no Webbridge (an older build): nothing to show.
      }
    },
    async release(chat, tab) {
      try {
        await invoke("webbridge_release", { tab });
      } catch (error) {
        set({ error: String(error) });
      }
      await get().loadChatTabs(chat);
    },
  };
});
