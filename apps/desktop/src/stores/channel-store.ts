// What each channel shows (docs/features/evo-mates/spec.md, FR-14 – FR-19):
// the `channel` entries its coordinator writes, read from the channel's
// engine when the channel opens and added live as `channel_event` lines
// arrive (engine-store forwards them here).

import { create } from "zustand";
import type { ChannelEntry } from "@/lib/mates";

interface ChannelStore {
  /** Entries per channel root chat, oldest first. */
  entries: Record<string, ChannelEntry[]>;
  set(chat: string, entries: ChannelEntry[]): void;
  push(chat: string, entry: ChannelEntry): void;
}

export const useChannelStore = create<ChannelStore>()((set) => ({
  entries: {},
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
