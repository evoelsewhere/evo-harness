// What the transcript shows for a `run_code` call: its summary and the calls
// its program made, as ordinary tool parts so they draw as their usual cards
// (docs/features/code-mode/spec.md, FR-15). Pure functions; `code-run.tsx`
// draws them.
import { ENGINE_EXITED, type ToolPart } from "@/features/chat/transcript";

export const CODE_TOOL = "run_code";

/** One call while the program runs, from the tool's progress updates. */
export interface LiveCall {
  id: string;
  tool: string;
  phase: "started" | "completed" | "failed";
}

export type CodeRunStatus = "running" | "done" | "failed" | "stopped" | "interrupted";

export interface CodeRunSummary {
  description: string;
  code: string;
  status: CodeRunStatus;
  calls: number;
  failed: number;
  durationMs?: number;
}

const record = (value: unknown): Record<string, any> =>
  value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, any>) : {};

/**
 * `details` after a progress update of a running program: each nested call's
 * latest phase, in the order they started (the engine's `toolProgram`
 * updates carry one call each).
 */
export function withLiveCall(details: ToolPart["details"], update: unknown): ToolPart["details"] {
  const progress = record(record(update).toolProgram);
  const id = typeof progress.requestId === "string" ? progress.requestId : "";
  const phase = progress.phase;
  if (!id || (phase !== "started" && phase !== "completed" && phase !== "failed")) return details;
  const live: LiveCall[] = Array.isArray(details?.live) ? details!.live : [];
  const tool = typeof progress.tool === "string" ? progress.tool : "tool";
  const at = live.findIndex((call) => call.id === id);
  const next = at === -1 ? [...live, { id, tool, phase }] : live.map((call, index) => (index === at ? { ...call, phase } : call));
  return { ...(details ?? {}), live: next };
}

/** The calls a program made, as tool parts: from its result once finished, from its progress while it runs. */
export function codeCalls(part: ToolPart): ToolPart[] {
  const details = record(part.details);
  if (Array.isArray(details.calls)) {
    return details.calls.map((raw: unknown, index: number): ToolPart => {
      const call = record(raw);
      const args = call.args && typeof call.args === "object" ? call.args : {};
      const ms = typeof call.ms === "number" ? call.ms : 0;
      return {
        kind: "tool",
        key: `${part.key}:${index}`,
        callId: `${part.callId}:${index}`,
        name: typeof call.name === "string" ? call.name : "tool",
        args,
        argsText: JSON.stringify(args),
        phase: call.ok === true ? "done" : call.ok === false ? "error" : part.phase === "running" ? "running" : "error",
        output: typeof call.output === "string" ? call.output : "",
        details: call.details && typeof call.details === "object" ? call.details : undefined,
        startedAt: part.startedAt,
        endedAt: part.startedAt + ms,
      };
    });
  }
  const live: LiveCall[] = Array.isArray(details.live) ? details.live : [];
  return live.map((call, index): ToolPart => ({
    kind: "tool",
    key: `${part.key}:live:${index}`,
    callId: `${part.callId}:${call.id}`,
    name: call.tool,
    args: {},
    argsText: "",
    phase: call.phase === "completed" ? "done" : call.phase === "failed" ? "error" : "running",
    output: "",
    startedAt: part.startedAt,
  }));
}

/** Description, code, status, number of calls and duration of a `run_code` call. */
export function codeRunSummary(part: ToolPart): CodeRunSummary {
  const args = record(part.args);
  const details = record(part.details);
  const calls = codeCalls(part);
  const error = typeof details.error === "string" ? details.error : "";
  let status: CodeRunStatus;
  if (part.phase === "preparing" || part.phase === "running") status = "running";
  else if (part.output === ENGINE_EXITED || part.output.startsWith("Stopped:")) status = "interrupted";
  else if (/stopped the code/i.test(error) || details.limit === "cancelled") status = "stopped";
  else if (part.phase === "error" || error) status = "failed";
  else status = "done";
  return {
    description: typeof args.description === "string" ? args.description.trim() : "",
    code: typeof args.code === "string" ? args.code : typeof details.code === "string" ? details.code : "",
    status,
    calls: calls.length,
    failed: calls.filter((call) => call.phase === "error").length,
    durationMs: part.endedAt !== undefined ? Math.max(0, part.endedAt - part.startedAt) : undefined,
  };
}

/** "3 calls · 1.2 s · stopped" */
export function codeRunMeta(summary: CodeRunSummary): string {
  const parts: string[] = [];
  if (summary.calls > 0) parts.push(`${summary.calls} call${summary.calls === 1 ? "" : "s"}`);
  if (summary.failed > 0) parts.push(`${summary.failed} failed`);
  if (summary.durationMs !== undefined && summary.status !== "running") {
    parts.push(summary.durationMs < 1000 ? `${summary.durationMs} ms` : `${(summary.durationMs / 1000).toFixed(1)} s`);
  }
  if (summary.status === "stopped" || summary.status === "interrupted") parts.push(summary.status);
  return parts.join(" · ");
}
