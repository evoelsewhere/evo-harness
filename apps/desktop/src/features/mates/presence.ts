// A mate's presence (docs/features/evo-mates-ui-v2/spec.md, FR-4, FR-5,
// FR-9, FR-19): one answer, used by the roster, the rail, the headers, the
// profile, the mention list and Today, so a mate's state reads the same
// everywhere (evo-mates FR-32).

import type { MateMood } from "@/components/mate-face/mate-face";
import { moodWord } from "@/stores/mates-store";

/** What the mark on a face says. Idle has no mark. */
export type Presence = "idle" | "working" | "waiting" | "error" | "queued" | "paused";

export interface PresenceInfo {
  presence: Presence;
  /** The state word ("Working…", "Needs you", "Paused"), or null when idle. */
  word: string | null;
}

/** A mate's presence from its mood; a paused mate reads "Paused" unless it still needs the person. */
export function presenceOf(mood: MateMood, paused = false): PresenceInfo {
  if (mood === "waiting") return { presence: "waiting", word: "Needs you" };
  if (paused) return { presence: "paused", word: "Paused" };
  switch (mood) {
    case "working":
    case "thinking":
      return { presence: "working", word: moodWord(mood) };
    case "queued":
      return { presence: "queued", word: "Queued" };
    case "error":
      return { presence: "error", word: "Couldn't reply" };
    default:
      return { presence: "idle", word: null };
  }
}

/** How a presence sorts when a list puts the most urgent first. */
export const PRESENCE_RANK: Record<Presence, number> = { waiting: 0, error: 1, working: 2, queued: 3, paused: 4, idle: 5 };

/** The words a screen reader says for a mark. */
export function presenceLabel(presence: Presence): string | null {
  switch (presence) {
    case "working":
      return "working";
    case "waiting":
      return "needs you";
    case "error":
      return "couldn't reply";
    case "queued":
      return "queued";
    case "paused":
      return "paused";
    default:
      return null;
  }
}

/** A channel's state from its members' seats: who works, who waits (FR-4, FR-19). */
export function channelPresence(
  seats: { mateId: string; seatChatId: string | null }[],
  work: Record<string, "working" | "waiting" | undefined>,
  channelWork: "working" | "waiting" | undefined,
): { working: string[]; waiting: string[]; presence: Presence } {
  const working = seats.filter((s) => s.seatChatId && work[s.seatChatId] === "working").map((s) => s.mateId);
  const waiting = seats.filter((s) => s.seatChatId && work[s.seatChatId] === "waiting").map((s) => s.mateId);
  const presence: Presence =
    waiting.length || channelWork === "waiting" ? "waiting" : working.length || channelWork === "working" ? "working" : "idle";
  return { working, waiting, presence };
}
