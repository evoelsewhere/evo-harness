// Evo Mates: the mates, the gallery and which mate the window shows
// (docs/features/evo-mates/spec.md, FR-6 – FR-13; plan 3.2). Mates come from
// the shell (`mates_*`); their conversations are chats the engine store
// shows, so picking a mate selects its chat there.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { MateMood } from "@/components/mate-face/mate-face";
import { log } from "@/lib/db";
import {
  GREETING_PROMPT,
  channelsApi,
  matesApi,
  type Channel,
  type ChannelPatch,
  type CreateChannel,
  type CreateMate,
  type Mate,
  type MatePatch,
  type MateTemplate,
} from "@/lib/mates";
import { entriesFromBranch, useChannelStore } from "@/stores/channel-store";
import { inTauri } from "@/lib/platform";
import { useEngineStore } from "@/stores/engine-store";
import { useSessionStore, type ChatWork } from "@/stores/session-store";

/** What the conversation area shows. */
export type MatesView =
  | { kind: "mate"; id: string }
  | { kind: "channel"; id: string }
  | { kind: "gallery" }
  /** Evo Mates' home (FR-73). */
  | { kind: "today" }
  /** Every file the mates keep (FR-84). */
  | { kind: "files" }
  | { kind: "empty" };

/** The mate panel's tabs. */
export type MatePanelTab = "about" | "settings" | "apps" | "memory" | "files";

interface MatesStore {
  loaded: boolean;
  mates: Mate[];
  channels: Channel[];
  /** Chats waiting for room under the running limit (NFR-6). */
  queued: string[];
  templates: MateTemplate[];
  view: MatesView;
  /** The mate panel (ux.md 2) is open. */
  panelOpen: boolean;
  panelTab: MatePanelTab;
  load(): Promise<void>;
  open(id: string): Promise<void>;
  /** Show the mate whose conversation `chat` is (a notice, Harness's Needs you). */
  openChat(chat: string): Promise<void>;
  showGallery(): void;
  showToday(): void;
  showFiles(): void;
  setPanelOpen(open: boolean): void;
  /** Open the panel at `tab`. */
  openPanel(tab: MatePanelTab): void;
  setPanelTab(tab: MatePanelTab): void;
  /** Make a mate and open it; it greets the person, or answers `first` when given (Describe a mate, FR-78). */
  create(input: CreateMate, first?: string): Promise<Mate | null>;
  update(id: string, patch: MatePatch): Promise<void>;
  duplicate(id: string): Promise<void>;
  remove(id: string): Promise<void>;
  openChannel(id: string): Promise<void>;
  createChannel(input: CreateChannel): Promise<Channel | null>;
  updateChannel(id: string, patch: ChannelPatch): Promise<void>;
  removeChannel(id: string): Promise<void>;
}

const report = (what: string) => (error: unknown) => log("warn", "mates", `${what}: ${String(error)}`);

let listening = false;

export const useMatesStore = create<MatesStore>()((set, get) => ({
  loaded: false,
  mates: [],
  channels: [],
  queued: [],
  templates: [],
  view: { kind: "empty" },
  panelOpen: false,
  panelTab: "about",

  async load() {
    if (inTauri && !listening) {
      listening = true;
      // Another window (or the shell) changed a mate.
      void listen("mates://changed", () => {
        void matesApi.list().then((mates) => set({ mates }), report("reading the mates"));
        void channelsApi.list().then((channels) => set({ channels }), report("reading the channels"));
      });
      void listen<{ chat: string }>("mates://open-chat", ({ payload }) => void get().openChat(payload.chat));
      void listen<{ chats: string[] }>("mates://queued", ({ payload }) => set({ queued: payload?.chats ?? [] }));
      void invoke<string[]>("mates_queued")
        .then((queued) => set({ queued }))
        .catch(() => undefined);
      // A mate's `connect_app` asks the person to switch an app on (FR-40).
      void listen<{ chat: string; tab: MatePanelTab }>("mates://open-panel", ({ payload }) => {
        void get()
          .openChat(payload.chat)
          .then(() => get().openPanel(payload.tab));
      });
    }
    const [mates, templates, channels] = await Promise.all([
      matesApi.list().catch((error) => {
        report("reading the mates")(error);
        return [] as Mate[];
      }),
      matesApi.templates().catch(() => [] as MateTemplate[]),
      channelsApi.list().catch((error) => {
        report("reading the channels")(error);
        return [] as Channel[];
      }),
    ]);
    // With no mate yet, the gallery greets the person (spec A-5).
    const view: MatesView = mates.length === 0 ? { kind: "gallery" } : get().view;
    set({ loaded: true, mates, channels, templates, view });
    // Asked to show a mate before this window had loaded.
    const pending = inTauri ? await invoke<string | null>("mates_open_pending").catch(() => null) : null;
    if (pending) await get().openChat(pending);
  },

  async openChat(chat) {
    const mate = get().mates.find((m) => m.chatId === chat);
    if (mate) return get().open(mate.id);
    const channel = get().channels.find((c) => c.chatId === chat);
    if (channel) return get().openChannel(channel.id);
  },

  async open(id) {
    const mate = get().mates.find((m) => m.id === id);
    if (!mate) return;
    set({ view: { kind: "mate", id } });
    await useEngineStore.getState().selectSession(mate.chatId);
  },

  async openChannel(id) {
    const channel = get().channels.find((c) => c.id === id);
    if (!channel) return;
    set({ view: { kind: "channel", id }, panelOpen: false });
    const engine = useEngineStore.getState();
    await engine.selectSession(channel.chatId);
    // What the channel showed so far, from its engine.
    try {
      const answer = await useEngineStore
        .getState()
        .request("get_session_branch", { customTypes: ["channel"], order: "oldestFirst" });
      useChannelStore.getState().set(channel.chatId, entriesFromBranch(answer));
    } catch (error) {
      report("reading the channel")(error);
    }
  },

  async createChannel(input) {
    try {
      const channel = await channelsApi.create(input);
      set((s) => ({ channels: [...s.channels.filter((c) => c.id !== channel.id), channel] }));
      await useSessionStore.getState().load();
      // Opening waits for the channel's engine: the caller goes on meanwhile.
      void get().openChannel(channel.id);
      return channel;
    } catch (error) {
      report("creating a channel")(error);
      return null;
    }
  },

  async updateChannel(id, patch) {
    try {
      const channel = await channelsApi.update(id, patch);
      if (channel) set((s) => ({ channels: s.channels.map((c) => (c.id === id ? channel : c)) }));
    } catch (error) {
      report("changing a channel")(error);
    }
  },

  async removeChannel(id) {
    const channel = get().channels.find((c) => c.id === id);
    if (channel && useEngineStore.getState().activeId === channel.chatId) useEngineStore.getState().letGo();
    try {
      await channelsApi.remove(id);
    } catch (error) {
      report("deleting a channel")(error);
    }
    set((s) => ({
      channels: s.channels.filter((c) => c.id !== id),
      view: s.view.kind === "channel" && s.view.id === id ? { kind: "empty" } : s.view,
    }));
  },

  showGallery() {
    set({ view: { kind: "gallery" } });
  },

  showToday() {
    set({ view: { kind: "today" }, panelOpen: false });
  },

  showFiles() {
    set({ view: { kind: "files" }, panelOpen: false });
  },

  setPanelOpen(open) {
    set({ panelOpen: open });
  },

  openPanel(tab) {
    set({ panelOpen: true, panelTab: tab });
  },

  setPanelTab(tab) {
    set({ panelTab: tab });
  },

  async create(input, first) {
    try {
      const mate = await matesApi.create(input);
      set((s) => ({ mates: [...s.mates.filter((m) => m.id !== mate.id), mate] }));
      // Its chat row reached the session store as the shell made it; read it
      // again so the engine store can show it.
      await useSessionStore.getState().load();
      await get().open(mate.id);
      // The new mate greets the person and asks its setup questions first
      // (FR-34, ux.md 5): picking it is what asks for that (A-5).
      // Once its engine is up and its (empty) history is shown: a prompt sent
      // while the new conversation still loads is answered but never shown.
      const engine = useEngineStore.getState();
      if (engine.activeId === mate.chatId && (await engine.wakeEngine(mate.chatId)) && useEngineStore.getState().activeId === mate.chatId) {
        // After the shell gave the engine its tools, or the first turn runs without
        // `mate_profile` ("cannot configure external tools while a run is active").
        // A described mate answers the description instead: it is the person's first message (FR-78).
        void useEngineStore
          .getState()
          .whenSetUp(mate.chatId)
          .then(() => useEngineStore.getState().sendPrompt(first ?? GREETING_PROMPT))
          .catch(report("waking the new mate"));
      }
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

/**
 * A mate's state for its row, header and face (ux.md 4); `failed`: its last
 * turn ended in an error, which reads "Couldn't reply" until it works again
 * (FR-32).
 */
export function mateMood(work: ChatWork | undefined, busy: boolean, queued = false, failed = false): MateMood {
  if (work === "waiting") return "waiting";
  if (work === "working" || busy) return "working";
  // Waiting for room under the running limit (NFR-6).
  if (queued) return "queued";
  if (failed) return "error";
  return "idle";
}

/**
 * A reply the person sees as it lands is read: its conversation is shown in a
 * window that has the focus (FR-6). The shell marks it unread when the window
 * was in the background; coming back to it clears that.
 */
export function seenNow(at: { chat: string; shown: string | null; unread: boolean; focused: boolean }): boolean {
  return at.unread && at.focused && at.shown === at.chat;
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
