// The rows of a mate's thread, worked out from the chat's turns
// (docs/features/evo-mates/spec.md, FR-58, FR-59, FR-31, FR-35): bubbles on
// two sides grouped into runs, time lines, typing, steps in plain words.
// Pure: no React, so it is tested on its own.

import type { Turn } from "@/features/chat";
import { parsePrompt, type ReplyTo } from "./message-markup";

/** One bubble of the thread. */
export interface Bubble {
  key: string;
  side: "me" | "mate";
  text: string;
  reply: ReplyTo | null;
  images?: string[];
  streaming: boolean;
  turn: Turn;
  at: number;
}

export type Row =
  | { kind: "time"; key: string; at: number }
  | { kind: "bubble"; bubble: Bubble; first: boolean; last: boolean }
  | { kind: "typing"; key: string; step: string | null }
  | { kind: "steps"; key: string; text: string }
  | { kind: "memory"; key: string }
  /** Where a "Learned · <skill>" line may show (learning-layers FR-18). */
  | { kind: "learned"; key: string; turn: Turn }
  | { kind: "error"; key: string; turn: Turn }
  | { kind: "delivered"; key: string };

const GAP_MS = 15 * 60_000;

/** "Today 3:02 PM", "Yesterday 9:14 AM", "Wed 12 Aug 2:21 PM". */
export function timeLine(at: number, now = Date.now()): string {
  const date = new Date(at);
  const time = date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(new Date(now)) - day(date)) / 86_400_000);
  if (days === 0) return `Today ${time}`;
  if (days === 1) return `Yesterday ${time}`;
  return `${date.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} ${time}`;
}

/** What a turn's steps did, in plain words (FR-31). */
export function stepsLine(turn: Pick<Turn, "parts">): string | null {
  const counts = new Map<string, number>();
  for (const part of turn.parts) {
    if (part.kind !== "tool") continue;
    const word = toolWord(part.name);
    if (word) counts.set(word, (counts.get(word) ?? 0) + 1);
  }
  if (!counts.size) return null;
  return Array.from(counts, ([word, n]) => word.replace("{n}", String(n)).replace(/ 1 (\w+)s\b/, " 1 $1")).join(" · ");
}

function toolWord(name: string): string | null {
  if (name === "read") return "Read {n} files";
  if (name === "write" || name === "edit") return "Edited {n} files";
  if (name === "bash" || name === "powershell" || name.startsWith("terminal_")) return "Ran {n} commands";
  if (name === "grep" || name === "find" || name === "ls") return "Searched files";
  if (name === "web_search") return "Searched the web";
  if (name === "web_fetch") return "Read {n} pages";
  // A saved topic has its own chip (FR-35).
  if (name === "memory") return null;
  if (name.startsWith("browser_")) return "Used the browser";
  if (name === "ask_user" || name === "update_todos") return null;
  // Mates' own tools (FR-34, FR-40).
  if (name === "mate_profile") return "Saved your answers";
  if (name === "connect_app") return "Asked to connect an app";
  if (name === "request_secret") return "Asked for a secret";
  return "Used {n} tools";
}

/** The step a working turn is on, in plain words. */
function currentStep(turn: Turn): string | null {
  const running = [...turn.parts].reverse().find((part) => part.kind === "tool" && part.phase !== "done");
  if (!running || running.kind !== "tool") return null;
  const word = toolWord(running.name);
  return word ? `${word.replace(" {n}", "").replace("{n} ", "")}…` : null;
}

/** The turn saved or changed a memory topic: its "Saved to memory" chip (FR-35). */
export function savedMemory(turn: Pick<Turn, "parts">): boolean {
  return turn.parts.some((part) => {
    if (part.kind !== "tool" || part.name !== "memory" || part.phase !== "done") return false;
    const action = (part.args as { action?: string } | null)?.action;
    return (action === "save" || action === "update") && !part.details?.isError;
  });
}

/** The message keys of a turn (reactions are kept by them, FR-60). */
const baseKey = (turn: Turn) => turn.entryId ?? turn.id;

/** A turn the heartbeat started: the mate decides whether to write first (FR-64). */
export function isHeartbeat(prompt: string): boolean {
  return prompt.trimStart().startsWith("<evo-mates-heartbeat");
}

/** A reply that says nothing is worth saying (`NOTHING_TO_SAY`): never shown. */
export function isSilent(text: string): boolean {
  return text.trim().replace(/^[.`*]+|[.`*]+$/g, "").toUpperCase() === "NOTHING_TO_SAY";
}

export function threadRows(turns: readonly Turn[], busy: boolean, retrying = false): Row[] {
  const rows: Row[] = [];
  const bubbles: { bubble: Bubble }[] = [];
  let lastAt: number | null = null;
  const push = (bubble: Bubble) => {
    if (lastAt === null || bubble.at - lastAt > GAP_MS) rows.push({ kind: "time", key: `time:${bubble.key}`, at: bubble.at });
    lastAt = bubble.at;
    const row = { kind: "bubble" as const, bubble, first: true, last: true };
    rows.push(row);
    bubbles.push(row);
  };
  turns.forEach((turn, index) => {
    const base = baseKey(turn);
    // A heartbeat turn shows only what the mate wrote first, never the
    // prompt, its steps or a silent answer (FR-64, FR-66).
    if (isHeartbeat(turn.prompt)) {
      let said = 0;
      turn.parts.forEach((part) => {
        if (part.kind !== "text" || !part.text.trim() || isSilent(part.text)) return;
        // While it streams it may still turn out silent: wait for the end.
        if (part.streaming && "NOTHING_TO_SAY".startsWith(part.text.trim().toUpperCase())) return;
        push({ key: `${base}:first:${said++}`, side: "mate", text: part.text, reply: null, streaming: part.streaming, turn, at: turn.endedAt ?? turn.startedAt });
      });
      return;
    }
    const { text, reply } = parsePrompt(turn.prompt);
    if (text.trim() || turn.images?.length) {
      push({ key: `${base}:prompt`, side: "me", text, reply, images: turn.images, streaming: false, turn, at: turn.startedAt });
    }
    let said = 0;
    turn.parts.forEach((part) => {
      if (part.kind !== "text" || !part.text.trim()) return;
      push({
        key: `${base}:reply:${said++}`,
        side: "mate",
        text: part.text,
        reply: null,
        streaming: part.streaming,
        turn,
        at: turn.endedAt ?? turn.startedAt,
      });
    });
    const last = index === turns.length - 1;
    const running = turn.status === "running" && last && busy;
    const lastPart = turn.parts[turn.parts.length - 1];
    if (running && !(lastPart?.kind === "text" && lastPart.text.trim())) {
      const step = turn.retry
        ? `Trying again${turn.retry.maxRetries ? ` (${turn.retry.attempt} of ${turn.retry.maxRetries})` : ""}…`
        : currentStep(turn);
      rows.push({ kind: "typing", key: `typing:${base}`, step });
    } else if (!running) {
      const steps = stepsLine(turn);
      if (steps) rows.push({ kind: "steps", key: `steps:${base}`, text: steps });
      if (savedMemory(turn)) rows.push({ kind: "memory", key: `memory:${base}` });
      if (turn.entryId) rows.push({ kind: "learned", key: `learned:${base}`, turn });
    }
    if (turn.error && turn.status === "error") rows.push({ kind: "error", key: `error:${base}`, turn });
    // The harness tries a failed attempt again on its own: say so, as typing.
    if (last && !running && retrying && turn.status === "error") {
      rows.push({ kind: "typing", key: `retry:${base}`, step: "Trying again…" });
    }
    if (!running && said === 0 && !turn.error && index === turns.length - 1 && turn.status !== "aborted") {
      rows.push({ kind: "delivered", key: `delivered:${base}` });
    }
  });
  // Runs: a bubble is first or last of its side's run (tails go on the last).
  rows.forEach((row, i) => {
    if (row.kind !== "bubble") return;
    const prev = rows[i - 1];
    const next = rows[i + 1];
    row.first = !(prev?.kind === "bubble" && prev.bubble.side === row.bubble.side);
    row.last = !(next?.kind === "bubble" && next.bubble.side === row.bubble.side);
  });
  return rows;
}
