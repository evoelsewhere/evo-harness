// Slash commands in Evo Mates' message boxes (docs/features/evo-mates-
// messaging/spec.md, FR-13): the list the `/` menu shows and how a typed
// command reads. Pure; the boxes run what it returns.

import type { CheckinSchedule } from "@/lib/mates";

export interface SlashCommand {
  id: "remind" | "checkin" | "pause" | "resume" | "stop" | "memory" | "handoff" | "clear-quote";
  usage: string;
  hint: string;
  /** Where it is offered. */
  where: "mate" | "channel" | "both";
  /** Needs words after it. */
  args: boolean;
}

export const SLASH_COMMANDS: SlashCommand[] = [
  { id: "remind", usage: "/remind <what and when>", hint: "Ask the mate to remind you", where: "mate", args: true },
  { id: "checkin", usage: "/checkin every weekday 9:00 <what>", hint: "Add a regular check-in", where: "mate", args: true },
  { id: "pause", usage: "/pause", hint: "Pause this mate", where: "mate", args: false },
  { id: "resume", usage: "/resume", hint: "Resume this mate", where: "mate", args: false },
  { id: "stop", usage: "/stop", hint: "Stop the work running now", where: "both", args: false },
  { id: "memory", usage: "/memory", hint: "Open the mate's memory", where: "mate", args: false },
  { id: "handoff", usage: "/handoff @mate <what>", hint: "Hand the work to a member", where: "channel", args: true },
  { id: "clear-quote", usage: "/clear-quote", hint: "Drop the quoted reply", where: "both", args: false },
];

/** The commands for a box, filtered by what was typed after `/`. */
export function matchSlash(query: string, where: "mate" | "channel"): SlashCommand[] {
  const q = query.toLowerCase();
  return SLASH_COMMANDS.filter((c) => (c.where === "both" || c.where === where) && c.id.startsWith(q));
}

/** A typed command and its words, when the box holds one. */
export function parseSlash(text: string): { command: SlashCommand; rest: string } | null {
  const match = /^\/([a-z-]+)(?:\s+([\s\S]*))?$/i.exec(text.trim());
  if (!match) return null;
  const command = SLASH_COMMANDS.find((c) => c.id === match[1].toLowerCase());
  return command ? { command, rest: (match[2] ?? "").trim() } : null;
}

const DAYS = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/**
 * "every weekday 9:00 check the inbox", "every day at 17:30 wrap up",
 * "every mon,fri 16:00 weekly review": the schedule and the text.
 */
export function parseCheckin(rest: string): { repeat: CheckinSchedule; text: string } | null {
  const match = /^(?:every\s+)?(day|daily|weekdays?|(?:mon|tue|wed|thu|fri|sat|sun)[a-z]*(?:\s*,\s*(?:mon|tue|wed|thu|fri|sat|sun)[a-z]*)*)\s+(?:at\s+)?(\d{1,2})(?::(\d{2}))?\s*(am|pm)?\s+([\s\S]+)$/i.exec(rest.trim());
  if (!match) return null;
  const [, when, h, m, ampm, text] = match;
  let hour = Number(h);
  if (ampm?.toLowerCase() === "pm" && hour < 12) hour += 12;
  if (ampm?.toLowerCase() === "am" && hour === 12) hour = 0;
  if (hour > 23 || Number(m ?? 0) > 59) return null;
  const time = `${String(hour).padStart(2, "0")}:${m ?? "00"}`;
  const w = when.toLowerCase();
  if (w === "day" || w === "daily") return { repeat: { kind: "daily", time }, text: text.trim() };
  if (w.startsWith("weekday")) return { repeat: { kind: "weekdays", time }, text: text.trim() };
  const days = w.split(/\s*,\s*/).map((d) => DAYS.indexOf(d.slice(0, 3)));
  if (days.some((d) => d < 0)) return null;
  return { repeat: { kind: "weekly", time, days }, text: text.trim() };
}
