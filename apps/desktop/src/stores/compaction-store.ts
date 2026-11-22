// Auto-compact: when a chat compacts itself (docs/features/auto-compact-threshold/spec.md,
// FR-1, FR-4, FR-5). One app setting, `compaction = { enabled, triggerPercent }`,
// shared by Settings and the context panel; the shell passes it to every engine
// (`src-tauri/src/supervisor/settings.rs`), so saving it is all this store does.

import { create } from "zustand";
import { db } from "@/lib/db";

export const COMPACTION_KEY = "compaction";
/** The slider's range (percent of the model's window). */
export const MIN_TRIGGER_PERCENT = 30;
export const MAX_TRIGGER_PERCENT = 95;
export const DEFAULT_TRIGGER_PERCENT = 90;

export interface CompactionSettings {
  enabled: boolean;
  triggerPercent: number;
}

export const DEFAULT_COMPACTION: CompactionSettings = { enabled: true, triggerPercent: DEFAULT_TRIGGER_PERCENT };

export const clampPercent = (value: number): number =>
  Math.min(MAX_TRIGGER_PERCENT, Math.max(MIN_TRIGGER_PERCENT, Math.round(value)));

/** The saved value as settings: anything unusable falls back to the default. */
export function readCompaction(raw: unknown): CompactionSettings {
  const saved = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  return {
    enabled: typeof saved.enabled === "boolean" ? saved.enabled : DEFAULT_COMPACTION.enabled,
    triggerPercent:
      typeof saved.triggerPercent === "number" && Number.isFinite(saved.triggerPercent)
        ? clampPercent(saved.triggerPercent)
        : DEFAULT_COMPACTION.triggerPercent,
  };
}

interface CompactionStore {
  settings: CompactionSettings;
  /** Read from the app settings once. */
  loaded: boolean;
  load(): Promise<void>;
  /** Show and save a change (the whole object is saved; the shell tells the engines). */
  save(change: Partial<CompactionSettings>): Promise<void>;
}

export const useCompactionStore = create<CompactionStore>((set, get) => ({
  settings: DEFAULT_COMPACTION,
  loaded: false,
  async load() {
    if (get().loaded) return;
    const all = await db.settings().catch(() => ({}) as Record<string, unknown>);
    // A change made while this read was out wins.
    if (!get().loaded) set({ settings: readCompaction(all[COMPACTION_KEY]), loaded: true });
  },
  async save(change) {
    const next = readCompaction({ ...get().settings, ...change });
    set({ settings: next, loaded: true });
    await db.setSetting(COMPACTION_KEY, next);
  },
}));
