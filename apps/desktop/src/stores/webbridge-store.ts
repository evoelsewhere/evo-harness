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

interface WebbridgeState {
  status: WebbridgeStatus | null;
  busy: string | null;
  error: string | null;
  load(): Promise<void>;
  setUp(browser: string): Promise<void>;
  remove(browser: string): Promise<void>;
  rename(profile: string, label: string): Promise<void>;
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
  return {
    status: null,
    busy: null,
    error: null,
    async load() {
      if (!inTauri) return;
      if (!listening) {
        listening = true;
        void listen("webbridge://changed", () => void get().load());
      }
      try {
        set({ status: await invoke<WebbridgeStatus>("webbridge_status") });
      } catch (error) {
        set({ error: String(error) });
      }
    },
    setUp: (browser) => run(`setup:${browser}`, () => invoke<WebbridgeStatus>("webbridge_setup", { browser })),
    remove: (browser) => run(`remove:${browser}`, () => invoke<WebbridgeStatus>("webbridge_remove", { browser })),
    rename: (profile, label) => run(`rename:${profile}`, () => invoke<WebbridgeStatus>("webbridge_rename", { profile, label })),
  };
});
