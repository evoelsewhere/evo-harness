// Evo Mates' app settings the window edits (docs/features/evo-mates-messaging/
// spec.md, FR-14, FR-15, FR-17): pinned messages per conversation, mutes and
// notification levels, and the link-previews switch. Kept as app settings
// (`settings_set`), so the shell reads the mutes for its notices (A-1).

import { create } from "zustand";
import { db, log } from "@/lib/db";
import { inTauri } from "@/lib/platform";

export const PINS_KEY = "mates.pins";
export const MUTE_KEY = "mates.mute";
export const LINK_PREVIEWS_KEY = "mates.linkPreviews";
export const COMPUTER_KEY = "mates.computer";
/** Evo's own Computer setting (app control): a mate's switch cannot turn it on. */
export const APP_CONTROL_KEY = "computer_apps.enabled";

/** What a mate may do at the computer (docs/features/evo-mates-computer/spec.md, FR-3); missing means on. */
export interface ComputerSwitches {
  browser: boolean;
  app: boolean;
  chrome: boolean;
}

/** A pinned message: its key (a bubble's or a channel line's id) and a few words. */
export interface Pin {
  key: string;
  excerpt: string;
  at: number;
}

export type Level = "all" | "mentions" | "nothing";

/** A place's mute: until a time (`null`: until turned back on), and its level. */
export interface Mute {
  until?: number | null;
  level?: Level;
}

interface MatesPrefs {
  loaded: boolean;
  pins: Record<string, Pin[]>;
  mute: Record<string, Mute>;
  linkPreviews: boolean;
  computer: Record<string, Partial<ComputerSwitches>>;
  appControl: boolean;
  load(): Promise<void>;
  setComputer(mate: string, key: keyof ComputerSwitches, on: boolean): void;
  togglePin(chat: string, pin: Omit<Pin, "at">): void;
  unpin(chat: string, key: string): void;
  setMute(chat: string, mute: Mute | null): void;
  setLinkPreviews(on: boolean): void;
}

function save(key: string, value: unknown) {
  if (!inTauri) return;
  void db.setSetting(key, value).catch((error) => log("warn", "mates", `could not save ${key}: ${String(error)}`));
}

export const useMatesPrefs = create<MatesPrefs>()((set, get) => ({
  loaded: false,
  pins: {},
  mute: {},
  linkPreviews: false,
  computer: {},
  appControl: false,
  async load() {
    if (!inTauri) return set({ loaded: true });
    const all = await db.settings().catch(() => ({}) as Record<string, unknown>);
    const pins = all[PINS_KEY];
    const mute = all[MUTE_KEY];
    set({
      loaded: true,
      pins: pins && typeof pins === "object" ? (pins as Record<string, Pin[]>) : {},
      mute: mute && typeof mute === "object" ? (mute as Record<string, Mute>) : {},
      linkPreviews: all[LINK_PREVIEWS_KEY] === true,
      computer: all[COMPUTER_KEY] && typeof all[COMPUTER_KEY] === "object" ? (all[COMPUTER_KEY] as Record<string, Partial<ComputerSwitches>>) : {},
      appControl: all[APP_CONTROL_KEY] === true,
    });
  },
  togglePin(chat, pin) {
    const list = get().pins[chat] ?? [];
    const next = list.some((p) => p.key === pin.key) ? list.filter((p) => p.key !== pin.key) : [...list, { ...pin, at: Date.now() }].slice(-20);
    const pins = { ...get().pins, [chat]: next };
    if (!next.length) delete pins[chat];
    set({ pins });
    save(PINS_KEY, pins);
  },
  unpin(chat, key) {
    const next = (get().pins[chat] ?? []).filter((p) => p.key !== key);
    const pins = { ...get().pins, [chat]: next };
    if (!next.length) delete pins[chat];
    set({ pins });
    save(PINS_KEY, pins);
  },
  setMute(chat, mute) {
    const all = { ...get().mute };
    if (mute && (mute.until !== undefined || (mute.level && mute.level !== "all"))) all[chat] = mute;
    else delete all[chat];
    set({ mute: all });
    save(MUTE_KEY, all);
  },
  setComputer(mate, key, on) {
    const computer = { ...get().computer, [mate]: { ...get().computer[mate], [key]: on } };
    set({ computer });
    save(COMPUTER_KEY, computer);
  },
  setLinkPreviews(on) {
    set({ linkPreviews: on });
    save(LINK_PREVIEWS_KEY, on);
  },
}));

/** Whether a place is quiet now: muted, or a level below All (FR-15). */
export function isQuiet(mute: Mute | undefined, now = Date.now()): boolean {
  if (!mute) return false;
  if (mute.until === null) return true;
  if (typeof mute.until === "number" && mute.until > now) return true;
  return Boolean(mute.level && mute.level !== "all");
}

/** When a mute for `choice` ends. */
export function muteUntil(choice: "hour" | "8h" | "tomorrow" | "always", now = new Date()): number | null {
  if (choice === "always") return null;
  if (choice === "hour") return now.getTime() + 60 * 60_000;
  if (choice === "8h") return now.getTime() + 8 * 60 * 60_000;
  const t = new Date(now);
  t.setDate(t.getDate() + 1);
  t.setHours(8, 0, 0, 0);
  return t.getTime();
}
