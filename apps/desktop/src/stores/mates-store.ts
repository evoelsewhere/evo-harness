// Evo Mates: the mates, the gallery and which mate the window shows
// (docs/features/evo-mates/spec.md, FR-6 – FR-13; plan 3.2). Mates come from
// the shell (`mates_*`); their conversations are chats the engine store
// shows, so picking a mate selects its chat there.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { MateMood } from "@/components/mate-face/mate-face";
import { log } from "@/lib/db";
import { matesApi, type CreateMate, type Mate, type MatePatch, type MateTemplate } from "@/lib/mates";
import { inTauri } from "@/lib/platform";
import { useEngineStore } from "@/stores/engine-store";
import { useSessionStore, type ChatWork } from "@/stores/session-store";

/** What the conversation area shows. */
export type MatesView = { kind: "mate"; id: string } | { kind: "gallery" } | { kind: "empty" };

interface MatesStore {
  loaded: boolean;
  mates: Mate[];
  templates: MateTemplate[];
  view: MatesView;
  /** The mate panel (ux.md 2) is open. */
  panelOpen: boolean;
  load(): Promise<void>;
  open(id: string): Promise<void>;
  /** Show the mate whose conversation `chat` is (a notice, Harness's Needs you). */
  openChat(chat: string): Promise<void>;
  showGallery(): void;
  setPanelOpen(open: boolean): void;
  create(input: CreateMate): Promise<Mate | null>;
  update(id: string, patch: MatePatch): Promise<void>;
  duplicate(id: string): Promise<void>;
  remove(id: string): Promise<void>;
}

const report = (what: string) => (error: unknown) => log("warn", "mates", `${what}: ${String(error)}`);

let listening = false;

export const useMatesStore = create<MatesStore>()((set, get) => ({
  loaded: false,
  mates: [],
  templates: [],
  view: { kind: "empty" },
  panelOpen: false,

  async load() {
    if (inTauri && !listening) {
      listening = true;
      // Another window (or the shell) changed a mate.
      void listen("mates://changed", () => {
        void matesApi.list().then((mates) => set({ mates }), report("reading the mates"));
      });
      void listen<{ chat: string }>("mates://open-chat", ({ payload }) => void get().openChat(payload.chat));
    }
    const [mates, templates] = await Promise.all([
      matesApi.list().catch((error) => {
        report("reading the mates")(error);
        return [] as Mate[];
      }),
      matesApi.templates().catch(() => [] as MateTemplate[]),
    ]);
    // With no mate yet, the gallery greets the person (spec A-5).
    const view: MatesView = mates.length === 0 ? { kind: "gallery" } : get().view;
    set({ loaded: true, mates, templates, view });
    // Asked to show a mate before this window had loaded.
    const pending = inTauri ? await invoke<string | null>("mates_open_pending").catch(() => null) : null;
    if (pending) await get().openChat(pending);
  },

  async openChat(chat) {
    const mate = get().mates.find((m) => m.chatId === chat);
    if (mate) await get().open(mate.id);
  },

  async open(id) {
    const mate = get().mates.find((m) => m.id === id);
    if (!mate) return;
    set({ view: { kind: "mate", id } });
    await useEngineStore.getState().selectSession(mate.chatId);
  },

  showGallery() {
    set({ view: { kind: "gallery" } });
  },

  setPanelOpen(open) {
    set({ panelOpen: open });
  },

  async create(input) {
    try {
      const mate = await matesApi.create(input);
      set((s) => ({ mates: [...s.mates.filter((m) => m.id !== mate.id), mate] }));
      // Its chat row reached the session store as the shell made it; read it
      // again so the engine store can show it.
      await useSessionStore.getState().load();
      await get().open(mate.id);
      return mate;
    } catch (error) {
      report("creating a mate")(error);
      return null;
    }
  },

  async update(id, patch) {
    try {
      const mate = await matesApi.update(id, patch);
      if (mate) set((s) => ({ mates: s.mates.map((m) => (m.id === id ? mate : m)) }));
    } catch (error) {
      report("changing a mate")(error);
    }
  },

  async duplicate(id) {
    try {
      const copy = await matesApi.duplicate(id);
      set((s) => ({ mates: [...s.mates, copy] }));
    } catch (error) {
      report("duplicating a mate")(error);
    }
  },

  async remove(id) {
    const mate = get().mates.find((m) => m.id === id);
    if (mate && useEngineStore.getState().activeId === mate.chatId) useEngineStore.getState().letGo();
    try {
      await matesApi.remove(id);
    } catch (error) {
      report("deleting a mate")(error);
    }
    set((s) => {
      const mates = s.mates.filter((m) => m.id !== id);
      const view: MatesView =
        s.view.kind === "mate" && s.view.id === id ? (mates.length ? { kind: "empty" } : { kind: "gallery" }) : s.view;
      return { mates, view };
    });
  },
}));

/** A mate's state for its row, header and face (ux.md 4). */
export function mateMood(work: ChatWork | undefined, busy: boolean): MateMood {
  if (work === "waiting") return "waiting";
  if (work === "working" || busy) return "working";
  return "idle";
}

/** The word a state shows in a roster row or the header (ux.md 4). */
export function moodWord(mood: MateMood): string | null {
  switch (mood) {
    case "thinking":
      return "Thinking…";
    case "working":
      return "Working…";
    case "queued":
      return "Queued";
    case "waiting":
      return "Needs you";
    case "error":
      return "Couldn't reply";
    default:
      return null;
  }
}
