// Connected apps as the shell reports them (key, status, accounts, pending
// sign-ins), kept fresh by `apps://changed`; and the catalogue, read on demand
// (docs/features/connected-apps/spec.md, FR-1 – FR-6, FR-17).

import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { appsApi, APPS_CHANGED, type AppsState, type Toolkit } from "@/lib/connected-apps";
import { inTauri } from "@/lib/platform";

interface ConnectedAppsStore extends AppsState {
  loaded: boolean;
  toolkits: Toolkit[];
  categories: string[];
  catalogLoading: boolean;
  catalogError: string | null;
  load(): Promise<void>;
  loadCatalog(): Promise<void>;
  setKey(key: string): Promise<void>;
  removeKey(): Promise<void>;
  connect(toolkit: string, label?: string): Promise<void>;
  reopen(pendingId: string): Promise<void>;
  disconnect(accountId: string): Promise<void>;
  reconnect(accountId: string): Promise<void>;
  retry(): Promise<void>;
}

let listening = false;
let timer: ReturnType<typeof setTimeout> | undefined;

export const useConnectedAppsStore = create<ConnectedAppsStore>()((set, get) => ({
  key: "none",
  status: "off",
  reason: null,
  accounts: [],
  pending: [],
  loaded: false,
  toolkits: [],
  categories: [],
  catalogLoading: false,
  catalogError: null,

  async load() {
    if (!inTauri) return;
    if (!listening) {
      listening = true;
      void listen(APPS_CHANGED, () => {
        clearTimeout(timer);
        timer = setTimeout(() => void get().load(), 100);
      }).catch(() => (listening = false));
    }
    try {
      const state = await appsApi.state();
      const keyChanged = state.key !== get().key;
      set({ ...state, loaded: true });
      if (keyChanged && state.key !== "saved") set({ toolkits: [], categories: [] });
    } catch {
      set({ loaded: true });
    }
  },

  async loadCatalog() {
    if (get().catalogLoading || get().key !== "saved") return;
    set({ catalogLoading: true, catalogError: null });
    try {
      const catalog = await appsApi.catalog();
      set({ toolkits: catalog.toolkits, categories: catalog.categories, catalogLoading: false });
    } catch (error) {
      set({ catalogLoading: false, catalogError: String(error) });
    }
  },

  async setKey(key) {
    await appsApi.setKey(key);
    set({ toolkits: [], categories: [] });
    await get().load();
    void get().loadCatalog();
  },

  async removeKey() {
    await appsApi.removeKey();
    set({ toolkits: [], categories: [] });
    await get().load();
  },

  async connect(toolkit, label) {
    try {
      await appsApi.connect(toolkit, label);
    } finally {
      await get().load();
    }
  },

  async reopen(pendingId) {
    await appsApi.reopen(pendingId);
    await get().load();
  },

  async disconnect(accountId) {
    try {
      await appsApi.disconnect(accountId);
    } finally {
      await get().load();
    }
  },

  async reconnect(accountId) {
    try {
      await appsApi.reconnect(accountId);
    } finally {
      await get().load();
    }
  },

  async retry() {
    try {
      await appsApi.retry();
    } finally {
      await get().load();
    }
  },
}));
