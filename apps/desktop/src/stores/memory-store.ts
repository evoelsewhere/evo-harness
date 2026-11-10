import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { db, log, type MemoryIndexStatus } from "@/lib/db";

// The memory settings the app keeps (Settings → Memory) and what the shell's
// index worker is doing. The settings the engine keeps — pause, sensitive
// subjects, a project's personal switch — are read and set through the active
// chat's engine instead (RPC `list_memories`, `set_memory_enabled`,
// `set_memory_settings`).
//
// Spec: docs/features/memory-topics-and-chat-recall/spec.md (FR-10, FR-15,
// FR-24 – FR-26, FR-33); keys in src-tauri/src/memory_index.rs and
// src-tauri/src/chat_flags.rs.

export const SEARCH_PAST_CHATS_KEY = "memory.searchPastChats";
export const AUTO_INDEX_KEY = "memory.autoIndex";
export const INDEX_MODEL_KEY = "memory.indexModel";
export const IDLE_MINUTES_KEY = "memory.indexIdleMinutes";

export const DEFAULT_IDLE_MINUTES = 5;

/** The model that reads idle chats: a provider and one of its models. */
export interface IndexModel {
  provider: string;
  modelId: string;
}

interface MemoryStore {
  loaded: boolean;
  /** The agent may look up earlier chats of the same project. */
  searchPastChats: boolean;
  autoIndex: boolean;
  indexModel: IndexModel | null;
  indexIdleMinutes: number;
  /** What the index worker is doing; `null` until it was asked. */
  status: MemoryIndexStatus | null;

  load(): Promise<void>;
  setSearchPastChats(on: boolean): Promise<void>;
  /** Switch automatic indexing; it stays off without an index model. */
  setAutoIndex(on: boolean): Promise<void>;
  /** Choose the index model; `null` clears it and switches indexing off. */
  setIndexModel(model: IndexModel | null): Promise<void>;
  setIndexIdleMinutes(minutes: number): Promise<void>;
  /** Index every chat with new messages now. */
  indexNow(): Promise<void>;
  /** How many chats from before indexing was switched on have not been read. */
  earlierCount(): Promise<number>;
  indexEarlier(): Promise<void>;
}

const asModel = (value: unknown): IndexModel | null => {
  const model = value as Partial<IndexModel> | null | undefined;
  return model && typeof model.provider === "string" && typeof model.modelId === "string" && model.provider && model.modelId
    ? { provider: model.provider, modelId: model.modelId }
    : null;
};

let listening = false;

export const useMemoryStore = create<MemoryStore>((set, get) => {
  const save = (key: string, value: unknown) =>
    db.setSetting(key, value).catch((error) => log("error", "db", `saving the memory setting ${key} failed: ${String(error)}`));

  return {
    loaded: false,
    searchPastChats: true,
    autoIndex: false,
    indexModel: null,
    indexIdleMinutes: DEFAULT_IDLE_MINUTES,
    status: null,

    async load() {
      if (!listening) {
        listening = true;
        void listen<MemoryIndexStatus>("memory://index-status", ({ payload }) => set({ status: payload }));
      }
      const [settings, status] = await Promise.all([
        db.settings().catch((): Record<string, unknown> => ({})),
        db.memoryIndexStatus().catch(() => null),
      ]);
      const minutes = settings[IDLE_MINUTES_KEY];
      set({
        loaded: true,
        searchPastChats: settings[SEARCH_PAST_CHATS_KEY] !== false,
        autoIndex: settings[AUTO_INDEX_KEY] === true,
        indexModel: asModel(settings[INDEX_MODEL_KEY]),
        indexIdleMinutes: typeof minutes === "number" ? Math.min(120, Math.max(1, Math.round(minutes))) : DEFAULT_IDLE_MINUTES,
        status,
      });
    },

    async setSearchPastChats(on) {
      set({ searchPastChats: on });
      await save(SEARCH_PAST_CHATS_KEY, on);
    },

    async setAutoIndex(on) {
      // Indexing needs a model: without one the switch stays off.
      const next = on && get().indexModel != null;
      set({ autoIndex: next });
      await save(AUTO_INDEX_KEY, next);
    },

    async setIndexModel(model) {
      set({ indexModel: model });
      await save(INDEX_MODEL_KEY, model);
      if (!model && get().autoIndex) await get().setAutoIndex(false);
    },

    async setIndexIdleMinutes(minutes) {
      const next = Math.min(120, Math.max(1, Math.round(minutes)));
      set({ indexIdleMinutes: next });
      await save(IDLE_MINUTES_KEY, next);
    },

    async indexNow() {
      await db.memoryIndexNow().catch((error) => log("warn", "db", `index now failed: ${String(error)}`));
    },

    earlierCount: () => db.memoryIndexEarlierCount().catch(() => 0),

    async indexEarlier() {
      await db.memoryIndexEarlier().catch((error) => log("warn", "db", `indexing earlier chats failed: ${String(error)}`));
    },
  };
});
