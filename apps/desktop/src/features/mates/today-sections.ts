// Evo Mates' Today page (docs/features/evo-mates/spec.md, FR-73; plan
// 3.10): what needs the person, what is new, what is coming up and what each
// mate did today, from Evo's own data. No model call.

import type { MateAvatar } from "@/components/mate-face/mate-face";
import type { AgendaItem } from "@/lib/mates";
import type { ChatWork } from "@/stores/session-store";
import { isHeartbeat, isSilent } from "./thread-rows";

/** A mate or a channel, as Today lists it. */
export interface Place {
  kind: "mate" | "channel";
  id: string;
  name: string;
  chatId: string;
  hidden: boolean;
  avatar?: MateAvatar;
  /** A channel's seats, or a mate's seats in channels: they wait and work for it too (evo-mates-ui-v2 FR-25, FR-26). */
  seats?: string[];
}

/** A conversation's row in the session store. */
export interface ChatRow {
  id: string;
  unread?: boolean;
  preview?: string | null;
  updatedAt: number;
}

/** One mate's day so far (`mates_today`). */
export interface MateDay {
  mateId: string;
  replies: number;
  costUsd: number;
}

export interface TodayInput {
  places: Place[];
  rows: ChatRow[];
  work: Record<string, ChatWork | undefined>;
  coming: AgendaItem[];
  team: MateDay[];
  /** When each chat's oldest open question was asked: Needs you lists the oldest first (FR-26). */
  waitingSince?: Record<string, number>;
}

export interface TodaySections {
  needsYou: Place[];
  fresh: { place: Place; line: string; at: number }[];
  coming: { item: AgendaItem; mate: Place }[];
  team: { mate: Place; replies: number; costUsd: number }[];
  totalCost: number;
  /** Mates working now, in their conversation or a channel (FR-25, A-1). */
  working: Place[];
  unreadCount: number;
}

/** What a place waits on, from its open questions' kinds and titles (FR-26). */
export function waitingKind(questions: { kind: string; title: string }[]): string {
  const first = questions[0];
  if (!first) return "Waiting for you";
  if (/secret|key|token|password/i.test(first.title)) return "Secret";
  if (/connect|sign[- ]?in|reconnect/i.test(first.title)) return "Sign-in";
  if (first.kind === "permission") return "Approval";
  return "Question";
}

/** "Good morning" before noon, "Good afternoon" before six, then "Good evening". */
export function greeting(now = new Date()): string {
  const hour = now.getHours();
  return hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
}

/** A preview the person may read: never a heartbeat's prompt or silent answer. */
export function shownLine(preview: string | null | undefined): string {
  if (!preview || isHeartbeat(preview) || isSilent(preview)) return "";
  return preview;
}

export function todaySections({ places, rows, work, coming, team, waitingSince }: TodayInput): TodaySections {
  const shown = places.filter((p) => !p.hidden);
  const byChat = new Map(shown.map((p) => [p.chatId, p]));
  const mates = new Map(shown.filter((p) => p.kind === "mate").map((p) => [p.id, p]));
  const chatsOf = (p: Place) => [p.chatId, ...(p.seats ?? [])];
  const waits = (p: Place) => chatsOf(p).some((chat) => work[chat] === "waiting");
  const since = (p: Place) => Math.min(...chatsOf(p).map((chat) => waitingSince?.[chat] ?? Number.MAX_SAFE_INTEGER));
  const needsYou = shown.filter(waits).sort((a, b) => since(a) - since(b));
  const working = shown.filter((p) => p.kind === "mate" && chatsOf(p).some((chat) => work[chat] === "working"));
  const fresh = rows
    .filter((r) => r.unread && byChat.has(r.id) && !waits(byChat.get(r.id)!))
    .sort((a, b) => b.updatedAt - a.updatedAt)
    .map((r) => ({ place: byChat.get(r.id)!, line: shownLine(r.preview), at: r.updatedAt }));
  const soon = [...coming]
    .sort((a, b) => a.dueAt - b.dueAt)
    .flatMap((item) => {
      const mate = mates.get(item.mateId);
      return mate ? [{ item, mate }] : [];
    });
  const day = team
    .flatMap((t) => {
      const mate = mates.get(t.mateId);
      return mate ? [{ mate, replies: t.replies, costUsd: t.costUsd }] : [];
    })
    .sort((a, b) => b.replies - a.replies);
  return {
    needsYou,
    fresh,
    coming: soon,
    team: day,
    totalCost: day.reduce((sum, t) => sum + t.costUsd, 0),
    working,
    unreadCount: fresh.length,
  };
}
