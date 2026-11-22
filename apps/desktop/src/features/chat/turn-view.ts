// How a turn reads, without React: the words and grouping the desktop's
// thread (`chat-turn.tsx`, `reasoning.tsx`) and Evo Webbridge's side panel
// (built from `src/lib/webbridge-chat.ts`) both use, so a turn reads the same
// in both (docs/features/browser-bridge/spec.md, FR-10).

import { activity, toolKind } from "@/components/ai-elements/tool-labels";
import { formatCost, formatDuration, formatTokens } from "@/lib/format";
import type { NoteKind, Part, ReasoningPart, ToolPart, Turn } from "./transcript";

/** What an automated message says in one line, by its kind (`noteKind`). */
export function noteLabel(kind: NoteKind, text: string): string {
  switch (kind) {
    case "mail": {
      // A message or question from an agent.
      const title = /^<mail [^>]*title="([^"]*)"/.exec(text)?.[1];
      const ask = /^<mail [^>]*kind="ask"/.test(text);
      return `${ask ? "Question" : "Message"} from ${title || "an agent"}`;
    }
    case "mail-arrived": {
      // The wake that reads them names the senders.
      const names = /^<mail-arrived>\s*(.*?) sent you/.exec(text)?.[1];
      return `Messages arrived${names ? ` · ${names.replace(/"/g, "")}` : ""}`;
    }
    case "session-note":
    case "sessions-done": {
      // A note carries its title; the wake that reads notes lists them.
      const listed = /finished a turn: (.*?)\. (Its|Their) note/.exec(text)?.[1];
      // The person's answer to a proposed session (crates/evo-mesh/src/hub.rs).
      const answer = /^<nested-session (?:id|proposal)="[^"]*"(?: title="([^"]*)")? status="(started|dismissed)">/.exec(text);
      if (kind === "session-note" && answer) {
        const what = answer[2] === "started" ? "Session started" : "Proposed session dismissed";
        return `${what}${answer[1] ? ` · ${answer[1]}` : ""}`;
      }
      const titles = (
        kind === "sessions-done"
          ? [...(listed ?? "").matchAll(/"([^"]*)"/g)]
          : [...text.matchAll(/(?:^|\n\n)<nested-session id="[^"]*" title="([^"]*)"/g)]
      )
        .map((m) => m[1])
        .filter(Boolean);
      if (titles.length > 1) return `${titles.length} sessions finished a turn · ${titles.join(", ")}`;
      return `Session finished a turn${titles[0] ? ` · ${titles[0]}` : ""}`;
    }
    case "agent-report": {
      const reports = [...text.matchAll(/(?:^|\n\n)<background-agent id="[^"]*" description="([^"]*)" status="([^"]*)">/g)];
      const [, description, status] = reports[0] ?? [];
      if (reports.length > 1) return `${reports.length} background agents reported · ${reports.map((m) => m[1]).join(", ")}`;
      const how = status === "cancelled" ? "was cancelled" : status === "error" ? "stopped" : "reported";
      return `Background agent ${how}${description ? ` · ${description}` : ""}`;
    }
    case "agents-done": {
      const finished = /finished: (.*?)\. (Its|Their) report/.exec(text)?.[1];
      return `Background ${finished?.includes(",") ? "agents" : "agent"} finished${finished ? ` · ${finished.replace(/"/g, "")}` : ""}`;
    }
    case "turn-reverted":
      // docs/features/source-control/spec.md, FR-12: what the agent was told.
      return "You reverted an earlier turn's changes";
  }
}

export type Process = (ReasoningPart | ToolPart)[];
export type Segment =
  | { kind: "process"; key: string; parts: Process }
  | { kind: "text"; key: string; text: string; streaming: boolean }
  | { kind: "widget"; key: string; part: ToolPart };

/** Consecutive reasoning and tool parts form one group; text breaks it. */
export function segments(parts: Part[]): Segment[] {
  const out: Segment[] = [];
  for (const part of parts) {
    if (part.kind === "text") {
      if (part.text) out.push({ kind: "text", key: part.key, text: part.text, streaming: part.streaming });
      continue;
    }
    // A `show_widget` widget is part of the answer: it stands on its own and
    // stays in sight when the turn folds (docs/features/office-v3-parity/spec.md).
    // So does a plugin's card (docs/features/plugin-ui/spec.md, FR-7).
    if (part.kind === "tool" && (part.name === "show_widget" || part.name === "plugin_show_card")) {
      out.push({ kind: "widget", key: part.key, part });
      continue;
    }
    const last = out.at(-1);
    if (last?.kind === "process") last.parts.push(part);
    else out.push({ kind: "process", key: part.key, parts: [part] });
  }
  return out;
}

export function sentence(items: string[]): string {
  if (items.length <= 1) return items[0] ?? "";
  return `${items.slice(0, -1).join(", ")} and ${items.at(-1)}`;
}

/** "Running commands" while live; "Read files, ran commands and edited files" after. */
export function groupTitle(parts: Process, live: boolean): string {
  const current = parts.at(-1)!;
  if (live) return current.kind === "reasoning" ? "Thinking" : activity[toolKind(current.name)].live;
  const counts = new Map<string, number>();
  for (const part of parts) {
    const label = part.kind === "reasoning" ? "thought" : activity[toolKind(part.name)].done;
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  const top = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 3).map(([label]) => label);
  const text = sentence(top);
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/**
 * A subagent still working after its call returned (an `agent` call or a
 * `message_agent` that continued one). A foreground agent ends with its call.
 */
export function runningInBackground(part: Part): boolean {
  if (part.kind !== "tool") return false;
  const child = part.details?.child;
  return Boolean(child?.background) && child.status === "running";
}

/** The line a reader sees: the latest finished paragraph while streaming, else the first. */
export function preview(text: string, streaming: boolean): string {
  const paragraphs = text
    .split(/\n\s*\n/)
    .map((p) => p.trim())
    .filter(Boolean);
  const pick = streaming ? (paragraphs.length > 1 ? paragraphs.at(-2) : paragraphs.at(-1)) : paragraphs[0];
  const line = (pick ?? "").split("\n")[0].replace(/^[#*_>\-\s]+/, "");
  return line.length > 120 ? `${line.slice(0, 119)}…` : line;
}

export function seconds(ms: number): string {
  const s = Math.max(1, Math.round(ms / 1000));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
}

/** "Thinking" while it streams, "Thought for 4s" after. */
export function reasoningTitle(part: Pick<ReasoningPart, "streaming" | "startedAt" | "endedAt">): string {
  return part.streaming ? "Thinking" : part.endedAt ? `Thought for ${seconds(part.endedAt - part.startedAt)}` : "Thought";
}

/** The step a running turn is on, for "Working for 12s · running commands". */
export function runningStep(parts: Part[]): string | null {
  for (let i = parts.length - 1; i >= 0; i--) {
    const part = parts[i];
    if (part.kind === "tool" && (part.phase === "running" || part.phase === "preparing")) {
      return activity[toolKind(part.name)].live.toLowerCase();
    }
  }
  return null;
}

/** A finished turn's footer: model · duration · tokens · cached · cost. */
export function turnMetaLine(meta: Turn["meta"]): string {
  // Usage reports uncached input, cache reads and cache writes separately;
  // the prompt the model saw is their sum.
  const prompt = (meta.inputTokens ?? 0) + (meta.cacheRead ?? 0) + (meta.cacheWrite ?? 0);
  const total = prompt + (meta.outputTokens ?? 0);
  return [
    meta.model,
    meta.durationMs !== undefined ? formatDuration(meta.durationMs) : undefined,
    total ? `${formatTokens(total)} tokens` : undefined,
    meta.cacheRead && prompt ? `${Math.round((meta.cacheRead / prompt) * 100)}% cached` : undefined,
    meta.cost ? formatCost(meta.cost) : undefined,
  ]
    .filter(Boolean)
    .join(" · ");
}
