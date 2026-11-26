// What each channel shows (docs/features/evo-mates/spec.md, FR-14 – FR-19):
// the `channel` entries its coordinator writes, read from the channel's
// engine when the channel opens and added live as `channel_event` lines
// arrive (engine-store forwards them here).

import { create } from "zustand";
import type { ChannelEntry } from "@/lib/mates";

interface ChannelStore {
  /** Entries per channel root chat, oldest first. */
  entries: Record<string, ChannelEntry[]>;
  /**
   * Messages written while the channel works, per root chat, oldest first:
   * each goes out once the one before has its answers (FR-56).
   */
  queued: Record<string, string[]>;
  set(chat: string, entries: ChannelEntry[]): void;
  push(chat: string, entry: ChannelEntry): void;
  enqueue(chat: string, text: string): void;
  /** Take the next queued message of `chat`. */
  dequeue(chat: string): string | undefined;
  unqueue(chat: string, index: number): void;
}

export const useChannelStore = create<ChannelStore>()((set, get) => ({
  entries: {},
  queued: {},
  enqueue(chat, text) {
    set((s) => ({ queued: { ...s.queued, [chat]: [...(s.queued[chat] ?? []), text] } }));
  },
  dequeue(chat) {
    const [next, ...rest] = get().queued[chat] ?? [];
    if (next !== undefined) set((s) => ({ queued: { ...s.queued, [chat]: rest } }));
    return next;
  },
  unqueue(chat, index) {
    set((s) => ({ queued: { ...s.queued, [chat]: (s.queued[chat] ?? []).filter((_, i) => i !== index) } }));
  },
  set(chat, entries) {
    set((s) => ({ entries: { ...s.entries, [chat]: entries } }));
  },
  push(chat, entry) {
    set((s) => ({ entries: { ...s.entries, [chat]: [...(s.entries[chat] ?? []), entry] } }));
  },
}));

/** The `channel` entries of a `get_session_branch` answer, oldest first. */
export function entriesFromBranch(answer: unknown): ChannelEntry[] {
  const list = (answer as { entries?: unknown[] } | null)?.entries ?? [];
  return list
    .map((entry) => {
      const e = entry as { customType?: string; custom_type?: string; data?: unknown };
      return (e.customType ?? e.custom_type) === "channel" ? (e.data as ChannelEntry) : null;
    })
    .filter((e): e is ChannelEntry => Boolean(e && typeof e === "object" && "kind" in e));
}
