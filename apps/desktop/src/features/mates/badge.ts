// The unread count on Evo Mates' taskbar or dock icon
// (docs/features/evo-mates-messaging/spec.md, FR-16): mates and channels
// that are unread or wait on the person. On Windows the shell shows an
// overlay picture; this draws it (a red circle with the number).

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { inTauri } from "@/lib/platform";
import { useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";

/** The overlay's side, as the shell expects it (`mates/badge.rs`). */
const SIDE = 32;

/** How many places to count: unread or waiting, hidden ones left out. */
export function badgeCount(
  places: { chat: string; seats: (string | null)[]; hidden: boolean }[],
  unread: Set<string>,
  work: Record<string, string | undefined>,
): number {
  return places.filter((p) => !p.hidden && (unread.has(p.chat) || [p.chat, ...p.seats].some((c) => c && work[c] === "waiting"))).length;
}

/** "9+" past nine. */
export function badgeText(count: number): string {
  return count > 9 ? "9+" : String(count);
}

function overlay(count: number): number[] | null {
  try {
    const canvas = document.createElement("canvas");
    canvas.width = SIDE;
    canvas.height = SIDE;
    const g = canvas.getContext("2d");
    if (!g) return null;
    g.fillStyle = "#e5484d";
    g.beginPath();
    g.arc(SIDE / 2, SIDE / 2, SIDE / 2, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = "#ffffff";
    g.font = `600 ${count > 9 ? 15 : 19}px Geist, system-ui, sans-serif`;
    g.textAlign = "center";
    g.textBaseline = "middle";
    g.fillText(badgeText(count), SIDE / 2, SIDE / 2 + 1);
    return Array.from(g.getImageData(0, 0, SIDE, SIDE).data);
  } catch {
    return null;
  }
}

/** Keep the window's icon badge current (FR-16). */
export function useIconBadge() {
  const mates = useMatesStore((s) => s.mates);
  const channels = useMatesStore((s) => s.channels);
  const sessions = useSessionStore((s) => s.sessions);
  const work = useSessionStore((s) => s.work);
  const count = React.useMemo(() => {
    const unread = new Set(sessions.filter((c) => c.unread).map((c) => c.id));
    const places = [
      ...mates.map((m) => ({ chat: m.chatId, seats: [] as (string | null)[], hidden: m.hidden })),
      ...channels.map((c) => ({ chat: c.chatId, seats: c.members.map((m) => m.seatChatId), hidden: c.hidden })),
    ];
    return badgeCount(places, unread, work);
  }, [mates, channels, sessions, work]);
  React.useEffect(() => {
    if (!inTauri) return;
    let label = "mates";
    try {
      label = getCurrentWindow().label;
    } catch {
      return;
    }
    void invoke("mates_set_badge", { window: label, count, rgba: count > 0 ? overlay(count) : null }).catch(() => undefined);
  }, [count]);
  return count;
}
