import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { db, type JournalEntry } from "@/lib/db";

// The learned-skill changes made after replies, per chat: the "Learned ·
// <skill>" lines under the replies (docs/features/learning-layers/spec.md,
// FR-18). Rows come from the journal; `replyId` is the transcript entry of
// the person's message that started the turn the reply ends.

interface LearnedStore {
  byChat: Record<string, JournalEntry[]>;
  load(chat: string): Promise<void>;
}

let listening = false;

export const useLearnedStore = create<LearnedStore>((set, get) => ({
  byChat: {},
  async load(chat) {
    if (!listening) {
      listening = true;
      void listen("learning://changed", () => {
        for (const known of Object.keys(get().byChat)) void get().load(known);
      });
    }
    const rows = await db.learnedForChat(chat).catch((): JournalEntry[] => []);
    set((state) => ({ byChat: { ...state.byChat, [chat]: rows } }));
  },
}));
