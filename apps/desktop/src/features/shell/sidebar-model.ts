// How the sidebar arranges chats: a workspace's initials on the rail, the
// rail's roll-up of chat states, the cross-workspace "Needs you" list, time
// groups and folded scheduled runs. Pure rules, no store access
// (docs/features/workspace-rail/spec.md, FR-2, FR-7, FR-9, FR-10; NFR-2).

/** What a chat's engine is doing: waiting for the person, or working. */
export type ChatState = "waiting" | "running";

export interface ChatLike {
  id: string;
  updatedAt: number;
  unread?: boolean;
  draft?: boolean;
  parentChatId?: string | null;
}

/** "evo-harness" → "EH", "evoWorkspace" → "EW", "harness" → "HA" (spec A-3). */
export function initials(name: string): string {
  const words = name
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .split(/[\s\-_.]+/)
    .filter(Boolean);
  if (words.length === 0) return "";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[1][0]).toUpperCase();
}

export type TimeGroup = "today" | "yesterday" | "week" | "older";

export const TIME_GROUP_LABEL: Record<TimeGroup, string> = {
  today: "Today",
  yesterday: "Yesterday",
  week: "Previous 7 days",
  older: "Older",
};

/** The time group of `at`, by calendar day in the local time zone (spec A-5). */
export function timeGroup(at: number, now: number = Date.now()): TimeGroup {
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);
  const today = midnight.getTime();
  if (at >= today) return "today";
  const yesterday = new Date(midnight);
  yesterday.setDate(yesterday.getDate() - 1);
  if (at >= yesterday.getTime()) return "yesterday";
  const week = new Date(midnight);
  week.setDate(week.getDate() - 7);
  if (at >= week.getTime()) return "week";
  return "older";
}

/** Items in their time groups, in order; empty groups left out. */
export function groupByTime<T>(items: readonly T[], at: (item: T) => number, now: number = Date.now()): { group: TimeGroup; items: T[] }[] {
  const order: TimeGroup[] = ["today", "yesterday", "week", "older"];
  const buckets = new Map<TimeGroup, T[]>(order.map((g) => [g, []]));
  for (const item of items) buckets.get(timeGroup(at(item), now))!.push(item);
  return order.filter((g) => buckets.get(g)!.length > 0).map((group) => ({ group, items: buckets.get(group)! }));
}

export interface RunLike {
  title: string;
}

/**
 * Fold the runs of one scheduled task (same title) under the first one in
 * `chats` — the newest when the list is newest first. Other chats stay as
 * they are, in order.
 */
export function foldRuns<T extends RunLike>(chats: readonly T[], isRun: (chat: T) => boolean): { chat: T; runs: T[] }[] {
  const out: { chat: T; runs: T[] }[] = [];
  const byTask = new Map<string, { chat: T; runs: T[] }>();
  for (const chat of chats) {
    if (!isRun(chat)) {
      out.push({ chat, runs: [] });
      continue;
    }
    const group = byTask.get(chat.title);
    if (group) group.runs.push(chat);
    else {
      const fresh = { chat, runs: [] as T[] };
      byTask.set(chat.title, fresh);
      out.push(fresh);
    }
  }
  return out;
}

/** 3 waiting, 2 unread, 1 running, 0 nothing to show (spec FR-7, A-2). */
function attention(chat: ChatLike, states: Readonly<Record<string, ChatState>>): number {
  if (chat.draft) return 0;
  const state = states[chat.id];
  if (state === "waiting") return 3;
  // A nested session's running and unread state rolls up under its parent.
  if (chat.parentChatId) return 0;
  if (chat.unread) return 2;
  return state === "running" ? 1 : 0;
}

/** The chats that wait, are unread or run, across workspaces, most urgent first. */
export function needsYou<T extends ChatLike>(chats: readonly T[], states: Readonly<Record<string, ChatState>>): T[] {
  return chats
    .map((chat) => ({ chat, rank: attention(chat, states) }))
    .filter((entry) => entry.rank > 0)
    .sort((a, b) => b.rank - a.rank || b.chat.updatedAt - a.chat.updatedAt)
    .map((entry) => entry.chat);
}

export interface RollUp {
  waiting: number;
  running: number;
  unread: number;
}

/** How many of `chats` (nested sessions included) wait, run or are unread (spec FR-2). */
export function rollUp(chats: readonly ChatLike[], states: Readonly<Record<string, ChatState>>): RollUp {
  const total: RollUp = { waiting: 0, running: 0, unread: 0 };
  for (const chat of chats) {
    const state = states[chat.id];
    if (state === "waiting") total.waiting++;
    else if (state === "running") total.running++;
    if (chat.unread) total.unread++;
  }
  return total;
}
