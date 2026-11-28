// The rows of a mate's thread, worked out from the chat's turns
// (docs/features/evo-mates/spec.md, FR-58, FR-59, FR-31, FR-35): bubbles on
// two sides grouped into runs, time lines, typing, steps in plain words.
// Pure: no React, so it is tested on its own.

import type { Turn } from "@/features/chat";
import { GREETING_PROMPT } from "@/lib/mates";
import { picturesOf, sentFilesOf, splitMessage, type SentFile } from "./attachments";
import { parsePrompt, type ReplyTo } from "./message-markup";

export { GREETING_PROMPT };

/** One bubble of the thread. */
export interface Bubble {
  key: string;
  side: "me" | "mate";
  text: string;
  reply: ReplyTo | null;
  images?: string[];
  /** Files the person attached, by path (FR-81). */
  files?: string[];
  /** Long text the person pasted (FR-81). */
  pasted?: { lines: number; text: string }[];
  streaming: boolean;
  turn: Turn;
  at: number;
}

export type Row =
  /** A day's separator, sticky over its messages (evo-mates-ui-v2 FR-11). */
  | { kind: "day"; key: string; at: number }
  | { kind: "time"; key: string; at: number }
  | { kind: "bubble"; bubble: Bubble; first: boolean; last: boolean }
  | { kind: "typing"; key: string; step: string | null }
  /** A turn's steps as a chip that opens to each step (evo-mates-ui-v2 FR-17). */
  | { kind: "steps"; key: string; text: string; turn: Turn }
  /** The person stopped this reply (evo-mates-ui-v2 FR-18). */
  | { kind: "stopped"; key: string }
  | { kind: "memory"; key: string }
  /** What a mate's turn sent and showed: files and pictures (FR-82). */
  | { kind: "files"; key: string; files: SentFile[]; pictures: string[] }
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

/** The local day of `at`, as a number that changes at midnight. */
function dayOf(at: number): number {
  const d = new Date(at);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** "Today", "Yesterday", or the weekday and date (evo-mates-ui-v2 FR-11, A-5). */
export function dayLabel(at: number, now = Date.now()): string {
  const days = Math.round((dayOf(now) - dayOf(at)) / 86_400_000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  const date = new Date(at);
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return date.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long", ...(sameYear ? {} : { year: "numeric" }) });
}

/** "3:02 PM": a time line inside a day. */
export function clockTime(at: number): string {
  return new Date(at).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

/** A message that folds with Show more (evo-mates-ui-v2 FR-14). */
export const LONG_LINES = 16;
export const LONG_CHARS = 1200;
export function isLong(text: string): boolean {
  return text.length > LONG_CHARS || text.split("\n").length > LONG_LINES;
}

/** One step of a turn, in plain words with what it touched (evo-mates-ui-v2 FR-17). */
export interface StepItem {
  key: string;
  word: string;
  target: string | null;
  status: "done" | "running" | "failed" | "denied";
}

const STEP_WORD: Record<string, string> = {
  read: "Read",
  write: "Wrote",
  edit: "Edited",
  bash: "Ran a command",
  powershell: "Ran a command",
  grep: "Searched files for",
  find: "Looked for files",
  ls: "Listed",
  web_search: "Searched the web for",
  web_fetch: "Read the page",
  send_file: "Sent",
  mate_profile: "Saved your answers",
  connect_app: "Asked to connect",
  request_secret: "Asked for a secret",
  mate_agenda: "Noted to follow up",
};

function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

/** What a step touched, from its arguments: a file's name, a query, a page, a command. */
function stepTarget(name: string, args: unknown): string | null {
  const a = (args && typeof args === "object" ? args : {}) as Record<string, unknown>;
  const str = (v: unknown) => (typeof v === "string" && v.trim() ? v.trim() : null);
  const path = str(a.path) ?? str(a.file_path) ?? str(a.file);
  if (path) return baseName(path);
  const query = str(a.query) ?? str(a.pattern);
  if (query) return `“${query.length > 60 ? `${query.slice(0, 59)}…` : query}”`;
  const url = str(a.url);
  if (url) return url.replace(/^https?:\/\//, "").replace(/\/$/, "");
  const command = str(a.command);
  if (command) return command.length > 60 ? `${command.slice(0, 59)}…` : command;
  const app = str(a.app) ?? str(a.service);
  if (app) return app;
  if (name === "mate_agenda") return str(a.text);
  return null;
}

/** Each tool step of a turn, in order, without raw tool names (FR-17; evo-mates FR-31). */
export function stepItems(turn: Pick<Turn, "parts">): StepItem[] {
  return turn.parts.flatMap((part) => {
    if (part.kind !== "tool") return [];
    // A saved memory has its own chip; questions and to-dos are not steps.
    if (part.name === "memory" || part.name === "ask_user" || part.name === "update_todos") return [];
    const word = STEP_WORD[part.name] ?? (part.name.startsWith("browser_") ? "Used the browser" : part.name.startsWith("terminal_") ? "Ran a command" : "Used a tool");
    const denied = part.phase === "error" && /denied|not allowed|declined|rejected/i.test(part.output);
    const status: StepItem["status"] = part.phase === "error" ? (denied ? "denied" : "failed") : part.phase === "done" ? "done" : "running";
    return [{ key: part.key, word, target: stepTarget(part.name, part.args), status }];
  });
}

/** What a turn's steps did, in plain words (FR-31). */
export function stepsLine(turn: Pick<Turn, "parts">): string | null {
  const counts = new Map<string, number>();
  for (const part of turn.parts) {
    // A denied or failed step did not happen: "Edited 1 file" after Deny
    // misleads the person (qa-2026-11-28, D5).
    if (part.kind !== "tool" || part.phase === "error") continue;
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
  if (name === "mate_agenda") return "Noted to follow up";
  // Its files show under its text (FR-82).
  if (name === "send_file") return "Sent {n} files";
  return "Used {n} tools";
}

/** The step a working turn is on, in plain words. */
export function currentStep(turn: Turn): string | null {
  const running = [...turn.parts].reverse().find((part) => part.kind === "tool" && part.phase !== "done");
  if (!running || running.kind !== "tool") return null;
  const word = toolWord(running.name);
  if (!word) return null;
  // A step with no plain word of its own: the mate is working (FR-59).
  if (word.includes("{n} tools")) return "Working…";
  return `${word.replace(" {n}", "").replace("{n} ", "")}…`;
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

/** A turn that woke a new mate to greet the person (FR-34). */
export function isGreeting(prompt: string): boolean {
  return prompt.trimStart().startsWith("<evo-mates-hello>");
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
    // A new day opens with its separator (evo-mates-ui-v2 FR-11).
    if (lastAt === null || dayOf(bubble.at) !== dayOf(lastAt)) rows.push({ kind: "day", key: `day:${bubble.key}`, at: bubble.at });
    if (lastAt === null || bubble.at - lastAt > GAP_MS || dayOf(bubble.at) !== dayOf(lastAt))
      rows.push({ kind: "time", key: `time:${bubble.key}`, at: bubble.at });
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
    const { text: said0, reply } = parsePrompt(turn.prompt);
    // Attached files and pasted text show as chips, not as markup (FR-81).
    const { text, files, pasted } = splitMessage(said0);
    // A new mate's wake-up is not the person's message (FR-34).
    if (!isGreeting(turn.prompt) && (text.trim() || turn.images?.length || files.length || pasted.length)) {
      push({
        key: `${base}:prompt`,
        side: "me",
        text,
        reply,
        images: turn.images,
        files: files.length ? files : undefined,
        pasted: pasted.length ? pasted : undefined,
        streaming: false,
        turn,
        at: turn.startedAt,
      });
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
    // What the turn sent and showed, under its text (FR-82).
    const sentFiles = sentFilesOf(turn);
    const pictures = picturesOf(turn);
    if (sentFiles.length || pictures.length) rows.push({ kind: "files", key: `files:${base}`, files: sentFiles, pictures });
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
      if (steps) rows.push({ kind: "steps", key: `steps:${base}`, text: steps, turn });
      if (savedMemory(turn)) rows.push({ kind: "memory", key: `memory:${base}` });
      if (turn.entryId) rows.push({ kind: "learned", key: `learned:${base}`, turn });
    }
    if (turn.error && turn.status === "error") rows.push({ kind: "error", key: `error:${base}`, turn });
    // A reply the person stopped says so (evo-mates-ui-v2 FR-18; QA D2).
    if (turn.status === "aborted") rows.push({ kind: "stopped", key: `stopped:${base}` });
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
