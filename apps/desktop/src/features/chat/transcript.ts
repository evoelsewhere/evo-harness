// Folds the engine's event stream into the turns the thread renders.
//
// A turn is one person message plus everything the agent did in response, as
// an ordered list of parts — reasoning, text and tool calls — in exactly the
// order the model produced them across every step of the run (the model may
// think, write, call a tool, think again, write more). Streamed blocks are
// keyed by `messageId:contentIndex`, so deltas land on the block they belong
// to; tool calls are also keyed by their call id, which is how the execution
// events find them. (The same model as DeepSeek Harness's per-step `blocks[]`.)

import { type AgentMessage, type ApprovalPrompt, type EngineMessage } from "@/engine/protocol";

export interface TextPart {
  kind: "text";
  key: string;
  text: string;
  streaming: boolean;
}

export interface ReasoningPart {
  kind: "reasoning";
  key: string;
  text: string;
  streaming: boolean;
  startedAt: number;
  endedAt?: number;
}

/** `preparing`: the model is still writing the arguments. */
export type ToolPhase = "preparing" | "running" | "done" | "error";

export interface ToolPart {
  kind: "tool";
  key: string;
  callId: string;
  name: string;
  /** Parsed arguments once complete. */
  args: unknown;
  /** Raw argument JSON while it streams. */
  argsText: string;
  phase: ToolPhase;
  output: string;
  /** Tool-specific result data (an edit's diff, …). */
  details?: Record<string, any>;
  /** Images the result carried (a browser screenshot), as data URLs. */
  images?: string[];
  startedAt: number;
  endedAt?: number;
}

export type Part = TextPart | ReasoningPart | ToolPart;

export interface TurnMeta {
  model?: string;
  provider?: string;
  durationMs?: number;
  inputTokens?: number;
  outputTokens?: number;
  cacheRead?: number;
  cacheWrite?: number;
  cost?: number;
}

/** One of the agent's questions (`ask_user`). */
export interface Question {
  question: string;
  header?: string;
  options: { label: string; description?: string }[];
  multiSelect: boolean;
}

export interface PermissionRequest {
  requestId: string;
  toolCallId?: string;
  title: string;
  description?: string;
  options: { title: string; permission?: string }[];
  kind: string;
  /** `kind: "questions"`: the agent's questions, answered in the questions panel. */
  questions?: Question[];
  /** An engine approval (a command or an edit about to run); absent for older prompts. */
  approval?: ApprovalPrompt;
}

export interface Turn {
  id: string;
  prompt: string;
  /** Pictures sent with the prompt, as data URLs. */
  images?: string[];
  /** The prompt's session entry and its parent (saved chats), for editing it. */
  entryId?: string;
  parentId?: string | null;
  parts: Part[];
  status: "running" | "done" | "error" | "aborted";
  error?: string;
  startedAt: number;
  endedAt?: number;
  meta: TurnMeta;
  /**
   * The request the chat shows now, mirrored onto the last turn for readers of
   * a turn (the sidebar). The requests themselves are `TranscriptState.permissions`.
   */
  permission?: PermissionRequest;
}

/** A message sent while the agent works, waiting for it to be read. */
export interface QueuedMessage {
  entryId: string;
  text: string;
  images?: string[];
  /** `steer`: read at the agent's next step; `followUp`: after it would stop. */
  deliverAs: "steer" | "followUp";
}

export interface TranscriptState {
  turns: Turn[];
  busy: boolean;
  /** Messages sent during the run that the agent has not read yet. */
  queued?: QueuedMessage[];
  /** Assistant message currently streaming, so deltas land on the right blocks. */
  streamingMessageId?: string;
  /**
   * Requests waiting for the person's answer, oldest first. The engine sends
   * every pending one at once (two subagents each asking to run a command) and
   * they outlive a turn (a background agent's), so they are kept here, by
   * `requestId`, not on a turn. The chat shows the oldest.
   */
  permissions?: PermissionRequest[];
  /**
   * How background subagents ended, from their reports to the chat, keyed
   * `id:<agent id>` and `description:<…>` (the latest report of each). The
   * report is also applied to the call that started the run (`applyAgentReport`).
   */
  agentReports?: Record<string, AgentReport>;
}

export interface AgentReport {
  status: "done" | "error" | "cancelled";
  at: number;
  /** The report itself: the body of the `<background-agent>` message. */
  text?: string;
}

/** True while any approval request or question of the chat is unanswered. */
export const hasOpenPermission = (transcript: TranscriptState): boolean => (transcript.permissions?.length ?? 0) > 0;

/** The oldest pending request is mirrored on the last turn, and on no other. */
function mirrorPermission(state: TranscriptState): TranscriptState {
  const shown = state.permissions?.[0];
  const last = state.turns.length - 1;
  if (state.turns.every((turn, index) => turn.permission === (index === last ? shown : undefined))) return state;
  return {
    ...state,
    turns: state.turns.map((turn, index) => {
      const want = index === last ? shown : undefined;
      return turn.permission === want ? turn : { ...turn, permission: want };
    }),
  };
}

interface FoundReport extends AgentReport {
  id: string;
  description: string;
}

/**
 * The reports in a background agent message, oldest first: one or more
 * `<background-agent id description status>` blocks, each with its report as
 * the body (crates/evo-engine/src/tools/agent.rs `tell_parent`, `lost_agent_note`).
 */
function reportsIn(text: string, at: number): FoundReport[] {
  const found: FoundReport[] = [];
  const block =
    /<background-agent id="([^"]*)" description="([^"]*)" status="([^"]*)">\n([\s\S]*?)\n<\/background-agent>(?=\s*$|\n\n<background-agent )/g;
  for (const match of text.matchAll(block)) {
    const status = match[3] === "error" || match[3] === "cancelled" ? match[3] : "done";
    found.push({ id: match[1], description: match[2], status, at, text: match[4] });
  }
  return found;
}

/** The reports of a background agent message, keyed `id:<agent id>` and `description:<…>`. */
export function agentReportsIn(text: string, at: number): Record<string, AgentReport> {
  const reports: Record<string, AgentReport> = {};
  for (const { id, description, ...report } of reportsIn(text, at)) {
    reports[`id:${id}`] = report;
    reports[`description:${description}`] = report;
  }
  return reports;
}

/** Why a background agent with no report shows as stopped after a restart. */
export const LOST_AGENT =
  "Stopped: the engine running it exited (the app closed or crashed) before it finished. Start it again if its work is still needed.";
/** What a call or an agent still open says when its engine exits under it. */
export const ENGINE_EXITED = "Stopped: the engine exited.";
/** The error the store reports an engine exit with (`onExit` in stores/engine-store.ts). */
const ENGINE_STOPPED = "The engine stopped";

/** A child snapshot that is waiting for its report: running, or settled without one after a restart. */
const awaitsReport = (child: Record<string, any>) =>
  child.status === "running" || child.error === LOST_AGENT || child.reportPending === true;

/**
 * Apply a background agent's report to the call whose run it ends: the latest
 * call of that agent that returned before the report (a continued agent has a
 * call per run). The call keeps the snapshot from when it returned
 * (`status: "running"`); the report says how the run ended and what it found.
 */
function applyAgentReport(turns: Turn[], report: FoundReport): Turn[] {
  for (let t = turns.length - 1; t >= 0; t--) {
    const turn = turns[t];
    for (let p = turn.parts.length - 1; p >= 0; p--) {
      const part = turn.parts[p];
      const child = part.kind === "tool" ? (part.details?.child as Record<string, any> | undefined) : undefined;
      if (part.kind !== "tool" || !child?.background || child.agentId !== report.id) continue;
      if ((part.endedAt ?? part.startedAt ?? 0) > report.at) continue;
      let settled: Record<string, any>;
      if (awaitsReport(child)) {
        settled = {
          ...child,
          status: report.status,
          endedAt: report.at,
          reportPending: false,
          report: report.text,
          error: report.status === "error" ? report.text || "The agent stopped." : null,
        };
      } else if (child.report == null && report.text) {
        // Already settled (live, from the mesh): it only lacks the report.
        settled = { ...child, report: report.text };
      } else return turns;
      const parts = turn.parts.slice();
      parts[p] = { ...part, details: { ...part.details, child: settled } };
      const next = turns.slice();
      next[t] = { ...turn, parts };
      return next;
    }
  }
  return turns;
}

/** Record the reports of a background agent message and settle the calls they end. */
const withReports = (state: TranscriptState, text: string, at: number): TranscriptState => {
  const found = reportsIn(text, at);
  if (!found.length) return state;
  return {
    ...state,
    turns: found.reduce(applyAgentReport, state.turns),
    agentReports: { ...state.agentReports, ...agentReportsIn(text, at) },
  };
};

export const initialTranscript: TranscriptState = { turns: [], busy: false };

let counter = 0;
const localId = (prefix: string) => `${prefix}-${Date.now().toString(36)}-${++counter}`;

/** The reply text of a turn (every text block, in order). */
export function turnReply(turn: Turn): string {
  return turn.parts
    .filter((part): part is TextPart => part.kind === "text")
    .map((part) => part.text)
    .filter(Boolean)
    .join("\n\n");
}

function updateCurrent(state: TranscriptState, update: (turn: Turn) => Turn): TranscriptState {
  const last = state.turns.at(-1);
  if (!last) return state;
  return { ...state, turns: [...state.turns.slice(0, -1), update(last)] };
}

function toolOutput(result: any): string {
  if (result == null) return "";
  if (typeof result === "string") return result;
  const content = Array.isArray(result.content) ? result.content : [];
  const text = content
    .filter((part: any) => part?.type === "text")
    .map((part: any) => part.text)
    .join("\n");
  return text || (result.content === undefined ? JSON.stringify(result, null, 2) : "");
}

/** Image blocks of a tool result, as data URLs. */
function toolImages(result: any): string[] | undefined {
  const content = Array.isArray(result?.content) ? result.content : [];
  const images = content
    .filter((part: any) => part?.type === "image" && typeof part.data === "string")
    .map((part: any) => `data:${part.mimeType || part.mime_type || "image/png"};base64,${part.data}`);
  return images.length ? images : undefined;
}

function upsert(turn: Turn, key: string, create: () => Part, change: (part: Part) => Part): Turn {
  const index = turn.parts.findIndex((part) => part.key === key);
  if (index === -1) return { ...turn, parts: [...turn.parts, change(create())] };
  const parts = turn.parts.slice();
  parts[index] = change(parts[index]);
  return { ...turn, parts };
}

function findTool(turn: Turn, callId: string): number {
  return turn.parts.findIndex((part) => part.kind === "tool" && part.callId === callId);
}

/**
 * Change the tool part of `callId`, in whichever turn holds it (the last one
 * first: a message steered into the run may have started a turn since).
 */
function updateTool(state: TranscriptState, callId: string, change: (turn: Turn, index: number) => Turn): TranscriptState {
  for (let t = state.turns.length - 1; t >= 0; t--) {
    const index = findTool(state.turns[t], callId);
    if (index === -1) continue;
    const turns = state.turns.slice();
    turns[t] = change(state.turns[t], index);
    return { ...state, turns };
  }
  return state;
}

/** A subagent snapshot whose run is over without the engine saying how: stopped, with its open steps. */
function stopChild(child: Record<string, any>, now: number, reason: string): Record<string, any> {
  return {
    ...child,
    status: "error",
    endedAt: now,
    error: reason,
    tools: Array.isArray(child.tools)
      ? child.tools.map((row: any) => (row?.phase === "running" || row?.phase === "preparing" ? { ...row, phase: "error" } : row))
      : child.tools,
  };
}

/**
 * Close a tool call that can no longer finish, and its subagent's snapshot.
 * `background`: also a background agent's (its engine is gone); otherwise only
 * a foreground agent's, which ends with its call.
 */
function closeTool(part: ToolPart, now: number, reason: string, background: boolean): ToolPart {
  const open = part.phase === "running" || part.phase === "preparing";
  const child = part.details?.child as Record<string, any> | undefined;
  const childOpen = Boolean(child) && child!.status === "running" && (background || !child!.background);
  if (!open && !childOpen) return part;
  return {
    ...part,
    ...(open ? { phase: "error" as const, endedAt: now, output: part.output || reason } : {}),
    details: childOpen ? { ...part.details, child: stopChild(child!, now, reason) } : part.details,
  };
}

/**
 * A result's `details`, keeping the subagent snapshot the call already has
 * when the result brings none (an error result of an older engine).
 */
function resultDetails(result: any, before: ToolPart["details"]): ToolPart["details"] {
  const details = result && typeof result === "object" && result.details && typeof result.details === "object" ? result.details : undefined;
  if (!details) return before;
  return before?.child && details.child === undefined ? { ...details, child: before.child } : details;
}

/** The engine exited: nothing of this chat runs any more. */
function engineExited(state: TranscriptState, now: number, error: string): TranscriptState {
  const last = state.turns.length - 1;
  const turns = state.turns.map((turn, index) => {
    const running = index === last && turn.status === "running";
    const base = running ? settleBlocks(turn, now) : turn;
    let changed = running;
    const parts = base.parts.map((part) => {
      const closed = part.kind === "tool" ? closeTool(part, now, ENGINE_EXITED, true) : part;
      if (closed !== part) changed = true;
      return closed;
    });
    if (!changed) return turn;
    return running
      ? { ...base, parts, status: "error" as const, error, endedAt: now, meta: { ...base.meta, durationMs: now - base.startedAt } }
      : { ...turn, parts };
  });
  // Its requests died with it.
  return { ...state, turns, busy: false, streamingMessageId: undefined, permissions: undefined };
}

/** Close every still-streaming text or reasoning block (a new channel began, or the step ended). */
function settleBlocks(turn: Turn, now: number, except?: string): Turn {
  let changed = false;
  const parts = turn.parts.map((part) => {
    if (part.key === except) return part;
    if (part.kind === "reasoning" && part.streaming) {
      changed = true;
      return { ...part, streaming: false, endedAt: now };
    }
    if (part.kind === "text" && part.streaming) {
      changed = true;
      return { ...part, streaming: false };
    }
    return part;
  });
  return changed ? { ...turn, parts } : turn;
}

/** Add a tool's own usage (a subagent's) to the turn, keeping its model. */
function applyToolUsage(turn: Turn, usage: any): Turn {
  if (!usage || typeof usage !== "object") return turn;
  return applyUsage(turn, { usage } as AgentMessage & Record<string, any>);
}

function applyUsage(turn: Turn, message: AgentMessage & Record<string, any>): Turn {
  const usage = message.usage as any;
  return {
    ...turn,
    meta: {
      ...turn.meta,
      model: message.model ?? turn.meta.model,
      provider: message.provider ?? turn.meta.provider,
      inputTokens: (turn.meta.inputTokens ?? 0) + (usage?.input ?? 0),
      outputTokens: (turn.meta.outputTokens ?? 0) + (usage?.output ?? 0),
      cacheRead: (turn.meta.cacheRead ?? 0) + (usage?.cacheRead ?? 0),
      cacheWrite: (turn.meta.cacheWrite ?? 0) + (usage?.cacheWrite ?? 0),
      cost: (turn.meta.cost ?? 0) + (usage?.cost?.total ?? 0),
    },
  };
}

function permissionFrom(message: EngineMessage): PermissionRequest {
  const ui = (message.ui ?? {}) as Record<string, any>;
  const options = Array.isArray(ui.options)
    ? ui.options.map((o: any) => ({ title: String(o?.title ?? o), permission: o?.permission }))
    : [];
  const questions: Question[] | undefined =
    ui.kind === "questions" && Array.isArray(ui.questions)
      ? ui.questions.map((q: any) => ({
          question: String(q?.question ?? ""),
          header: q?.header ? String(q.header) : undefined,
          options: Array.isArray(q?.options)
            ? q.options.map((o: any) => ({ label: String(o?.label ?? o), description: o?.description ? String(o.description) : undefined }))
            : [],
          multiSelect: Boolean(q?.multi_select ?? q?.multiSelect),
        }))
      : undefined;
  return {
    requestId: message.requestId,
    toolCallId: message.toolCallId ?? undefined,
    title: ui.title ?? ui.question ?? ui.message ?? "The agent needs your input",
    description: ui.description ?? ui.detail ?? undefined,
    options: options.length || questions ? options : [{ title: "Allow" }, { title: "Deny" }],
    kind: ui.kind ?? "confirm",
    questions,
    approval: approvalFrom(ui.approval),
  };
}

/** Take one request off the pending list; without an id, the one shown (the oldest). */
function dropPermission(state: TranscriptState, requestId?: string): TranscriptState {
  const pending = state.permissions ?? [];
  const rest = requestId === undefined ? pending.slice(1) : pending.filter((item) => item.requestId !== requestId);
  return rest.length === pending.length ? state : { ...state, permissions: rest };
}

/** The engine's approval details of a prompt; anything unexpected reads as a plain prompt. */
function approvalFrom(value: unknown): ApprovalPrompt | undefined {
  const approval = value as Record<string, unknown> | null | undefined;
  if (!approval || (approval.kind !== "command" && approval.kind !== "edit")) return undefined;
  return {
    kind: approval.kind,
    tool: typeof approval.tool === "string" ? approval.tool : "",
    agent: typeof approval.agent === "string" && approval.agent ? approval.agent : undefined,
  };
}

const newText = (key: string): TextPart => ({ kind: "text", key, text: "", streaming: true });
const newReasoning = (key: string, now: number): ReasoningPart => ({
  kind: "reasoning",
  key,
  text: "",
  streaming: true,
  startedAt: now,
});
const newTool = (key: string, callId: string, name: string, now: number): ToolPart => ({
  kind: "tool",
  key,
  callId,
  name,
  args: undefined,
  argsText: "",
  phase: "preparing",
  output: "",
  startedAt: now,
});

/** Reconcile a finished assistant message's content with the streamed blocks. */
function finishMessage(turn: Turn, messageId: string, message: AgentMessage & Record<string, any>, now: number): Turn {
  const content = Array.isArray(message.content) ? message.content : [];
  let next = turn;
  content.forEach((block: any, index: number) => {
    const key = `${messageId}:${index}`;
    if (block?.type === "text" && typeof block.text === "string" && block.text) {
      next = upsert(next, key, () => newText(key), (part) => ({ ...(part as TextPart), text: block.text, streaming: false }));
    } else if (block?.type === "thinking" && typeof block.thinking === "string" && block.thinking) {
      next = upsert(
        next,
        key,
        () => newReasoning(key, now),
        (part) => ({ ...(part as ReasoningPart), text: block.thinking, streaming: false, endedAt: (part as ReasoningPart).endedAt ?? now }),
      );
    } else if (block?.type === "toolCall" && block.id) {
      const existing = findTool(next, block.id);
      const toolKey = existing === -1 ? key : next.parts[existing].key;
      next = upsert(
        next,
        toolKey,
        () => newTool(toolKey, block.id, block.name ?? "tool", now),
        (part) => ({ ...(part as ToolPart), name: block.name ?? (part as ToolPart).name, args: block.arguments ?? (part as ToolPart).args }),
      );
    }
  });
  return settleBlocks(next, now);
}

export type TranscriptAction =
  | { type: "reset" }
  | { type: "prompt"; text: string; images?: string[] }
  | { type: "queued"; item: QueuedMessage }
  /** Drop a waiting message (`null`: all of them). */
  | { type: "unqueued"; entryId: string | null }
  /** The run could not start or broke off; the engine is still there. */
  | { type: "fail"; error: string }
  /** The chat's engine exited: every open call, agent and request ends. */
  | { type: "engine-exited"; error?: string }
  /** A request was answered; without `requestId`, the one shown. */
  | { type: "permission-answered"; requestId?: string }
  | { type: "engine"; message: EngineMessage }
  /** A background subagent's latest snapshot, for the call that started it (any turn). */
  | { type: "agent-progress"; callId: string; child: Record<string, unknown> }
  /** Usage of background subagents whose reports this turn delivers. */
  | { type: "extra-usage"; usage: unknown };

/**
 * How a run ended. `agent_settled` says so (`outcome`, `aborted`, `error`);
 * `agent_end`, which usually comes first, carries only the run's messages, so
 * its last assistant message's `stopReason` tells: a run stopped while a tool
 * ran ends with an `aborted` assistant message.
 */
export function runEnd(message: EngineMessage): { aborted: boolean; failed: boolean; error?: string } {
  const messages: AgentMessage[] = Array.isArray(message.messages) ? message.messages : [];
  const last = [...messages].reverse().find((m) => m?.role === "assistant") as (AgentMessage & Record<string, any>) | undefined;
  const aborted = message.aborted === true || message.outcome === "aborted" || last?.stopReason === "aborted";
  const error =
    message.error ?? (last?.stopReason === "error" ? (last.errorMessage ?? "The model returned an error") : undefined);
  const failed = message.outcome === "failed" || (!!error && !aborted);
  return { aborted, failed, error: error == null ? undefined : String(error) };
}

function applyEngine(state: TranscriptState, message: EngineMessage, now: number): TranscriptState {
  switch (message.type) {
    case "agent_start":
      return { ...state, busy: true };
    case "agent_end":
    case "agent_settled": {
      const { aborted, failed, error } = runEnd(message);
      const next = updateCurrent(state, (turn) => {
        if (turn.status !== "running") {
          // `agent_end` closed the turn as done; the settlement knows better.
          // Not a `compact` run's: it has no turn, and its outcome is not the last turn's.
          if (message.type === "agent_settled" && message.compact !== true && turn.status === "done" && (aborted || failed)) {
            return {
              ...turn,
              status: aborted ? "aborted" : "error",
              error: failed ? (error ?? turn.error ?? "The run failed") : turn.error,
            };
          }
          return turn;
        }
        const settled = settleBlocks(turn, now);
        return {
          ...settled,
          status: aborted ? "aborted" : failed ? "error" : "done",
          error: failed ? (error ?? turn.error ?? "The run failed") : turn.error,
          endedAt: now,
          meta: { ...turn.meta, durationMs: now - turn.startedAt },
          // A call still open when the run ends was interrupted (and its foreground agent with it).
          parts: settled.parts.map((part) => (part.kind === "tool" ? closeTool(part, now, "Stopped before it finished.", false) : part)),
        };
      });
      return { ...next, busy: false, streamingMessageId: undefined };
    }
    case "message_start": {
      const msg = message.message as AgentMessage | undefined;
      if (msg?.role === "user") return userMessageStarted(state, msg, now);
      if (msg?.role !== "assistant") return state;
      return { ...state, streamingMessageId: message.messageId ?? localId("message") };
    }
    case "message_update": {
      const event = message.assistantMessageEvent ?? {};
      const type: string = event.type ?? message.eventType ?? "";
      const index: number = event.contentIndex ?? message.contentIndex ?? 0;
      const messageId = message.messageId ?? state.streamingMessageId ?? "message";
      const key = `${messageId}:${index}`;
      const delta: string = event.delta ?? message.delta ?? "";
      return updateCurrent(state, (turn) => {
        switch (type) {
          case "text_start":
          case "text_delta":
            return upsert(settleBlocks(turn, now, key), key, () => newText(key), (part) => ({
              ...(part as TextPart),
              text: (part as TextPart).text + (type === "text_delta" ? delta : ""),
              streaming: true,
            }));
          case "thinking_start":
          case "thinking_delta":
            return upsert(settleBlocks(turn, now, key), key, () => newReasoning(key, now), (part) => ({
              ...(part as ReasoningPart),
              text: (part as ReasoningPart).text + (type === "thinking_delta" ? delta : ""),
              streaming: true,
            }));
          case "text_end":
          case "thinking_end":
            return settleBlocks(turn, now);
          case "toolcall_start":
          case "toolcall_delta": {
            const partial = event.partial?.content?.[index] ?? message.message?.content?.[index];
            const callId: string = event.id ?? partial?.id ?? key;
            const name: string = event.toolName ?? partial?.name ?? "tool";
            const existing = callId ? findTool(turn, callId) : -1;
            const toolKey = existing === -1 ? key : turn.parts[existing].key;
            return upsert(settleBlocks(turn, now), toolKey, () => newTool(toolKey, callId, name, now), (part) => ({
              ...(part as ToolPart),
              callId: (part as ToolPart).callId === toolKey && callId !== key ? callId : (part as ToolPart).callId,
              name: name !== "tool" ? name : (part as ToolPart).name,
              argsText: (part as ToolPart).argsText + (type === "toolcall_delta" ? delta : ""),
            }));
          }
          case "toolcall_end": {
            const call = event.toolCall;
            if (!call?.id) return turn;
            const existing = findTool(turn, call.id);
            const toolKey = existing === -1 ? key : turn.parts[existing].key;
            return upsert(turn, toolKey, () => newTool(toolKey, call.id, call.name, now), (part) => ({
              ...(part as ToolPart),
              callId: call.id,
              name: call.name ?? (part as ToolPart).name,
              args: call.arguments,
            }));
          }
          default:
            return turn;
        }
      });
    }
    case "message_end": {
      const msg = message.message as (AgentMessage & Record<string, any>) | undefined;
      if (msg?.role !== "assistant") return state;
      const messageId = message.messageId ?? state.streamingMessageId ?? localId("message");
      const next = updateCurrent(state, (turn) => {
        let updated = applyUsage(finishMessage(turn, messageId, msg, now), msg);
        if (msg.stopReason === "error") updated = { ...updated, error: msg.errorMessage ?? "The model returned an error" };
        return updated;
      });
      return { ...next, streamingMessageId: undefined };
    }
    case "tool_execution_start":
      return updateCurrent(state, (turn) => {
        const settled = settleBlocks(turn, now);
        const index = findTool(settled, message.toolCallId);
        const key = index === -1 ? `call:${message.toolCallId}` : settled.parts[index].key;
        return upsert(settled, key, () => newTool(key, message.toolCallId, message.toolName, now), (part) => ({
          ...(part as ToolPart),
          name: message.toolName ?? (part as ToolPart).name,
          args: message.args ?? (part as ToolPart).args,
          phase: "running",
        }));
      });
    case "tool_execution_update":
    case "tool_execution_end": {
      const done = message.type === "tool_execution_end";
      const result = done ? message.result : message.partialResult;
      return updateTool(state, message.toolCallId, (turn, index) => {
        const parts = turn.parts.slice();
        const part = parts[index] as ToolPart;
        parts[index] = {
          ...part,
          phase: done ? (message.isError ? "error" : "done") : part.phase,
          output: toolOutput(result) || part.output,
          images: toolImages(result) ?? part.images,
          // A failed call's result carries its details (a subagent's last snapshot) like any other.
          details: resultDetails(result, part.details),
          endedAt: done ? now : part.endedAt,
        };
        // A subagent's tokens and cost count toward the turn, also when it failed.
        return done ? applyToolUsage({ ...turn, parts }, result?.usage) : { ...turn, parts };
      });
    }
    case "ui_dialog_request": {
      // Every pending request is kept: a second one waits behind the first.
      const request = permissionFrom(message);
      const pending = state.permissions ?? [];
      const at = pending.findIndex((item) => item.requestId === request.requestId);
      return {
        ...state,
        permissions: at === -1 ? [...pending, request] : pending.map((item, index) => (index === at ? request : item)),
      };
    }
    case "ui_dialog_closed":
      return dropPermission(state, message.requestId);
    default:
      return state;
  }
}

export function transcriptReducer(state: TranscriptState, action: TranscriptAction): TranscriptState {
  return mirrorPermission(reduce(state, action));
}

function reduce(state: TranscriptState, action: TranscriptAction): TranscriptState {
  const now = Date.now();
  switch (action.type) {
    case "reset":
      return initialTranscript;
    case "prompt":
      return {
        ...state,
        busy: true,
        turns: [
          ...state.turns,
          {
            id: localId("turn"),
            prompt: action.text,
            images: action.images?.length ? action.images : undefined,
            parts: [],
            status: "running",
            startedAt: now,
            meta: {},
          },
        ],
      };
    case "queued":
      return { ...state, queued: [...(state.queued ?? []), action.item] };
    case "unqueued":
      return {
        ...state,
        queued: action.entryId === null ? [] : (state.queued ?? []).filter((item) => item.entryId !== action.entryId),
      };
    case "fail":
      // The store reports an engine exit this way too.
      if (action.error === ENGINE_STOPPED) return engineExited(state, now, action.error);
      return {
        ...updateCurrent(state, (turn) => {
          const settled = settleBlocks(turn, now);
          return {
            ...settled,
            status: "error",
            error: action.error,
            endedAt: now,
            // The run is over: its open calls and their foreground agents with it.
            parts: settled.parts.map((part) => (part.kind === "tool" ? closeTool(part, now, "Stopped before it finished.", false) : part)),
          };
        }),
        busy: false,
        streamingMessageId: undefined,
      };
    case "engine-exited":
      return engineExited(state, now, action.error ?? ENGINE_STOPPED);
    case "permission-answered":
      return dropPermission(state, action.requestId);
    case "engine":
      return applyEngine(state, action.message, now);
    case "agent-progress":
      // The call that started the agent or last continued it (`agent`, `message_agent`), in any turn.
      return updateTool(state, action.callId, (turn, at) => {
        const parts = turn.parts.slice();
        const part = parts[at] as ToolPart;
        const before = part.details?.child as Record<string, any> | undefined;
        // The mesh view has the report only while its engine remembers it;
        // keep the one this call already shows.
        const child =
          action.child.report == null && before?.report != null && before.agentId === action.child.agentId && action.child.status !== "running"
            ? { ...action.child, report: before.report }
            : action.child;
        parts[at] = { ...part, details: { ...(part.details ?? {}), child } };
        return { ...turn, parts };
      });
    case "extra-usage":
      return updateCurrent(state, (turn) => applyToolUsage(turn, action.usage));
  }
}

/** Messages about background subagents (crates/evo-engine/src/tools/agent.rs `NOTE_TAG`). */
export const BACKGROUND_NOTE = "<background-agent";
/** A note from a nested session to the chat that started it. */
export const SESSION_NOTE = "<nested-session";
/** A message or question from another agent (crates/evo-engine/src/tools/mesh.rs), or the wake that reads it. */
export const MAIL_NOTE = "<mail";

/**
 * The automated messages a chat receives: a background agent's report, a
 * nested session's note, mail from another agent, and the three prompts this
 * app wakes an idle chat with (`wake` in stores/engine-store.ts).
 */
export type NoteKind = "agent-report" | "session-note" | "mail" | "agents-done" | "sessions-done" | "mail-arrived";

/** One or more blocks `<tag attrs>\n…\n</tag>`, blank lines between, and nothing else. */
const blocks = (open: string, tag: string) => {
  const one = `${open}\\n[\\s\\S]*?\\n</${tag}>`;
  return new RegExp(`^${one}(?:\\n\\n${one})*$`);
};
const NOTES: [NoteKind, RegExp][] = [
  // crates/evo-engine/src/tools/agent.rs (`tell_parent`, `lost_agent_note`)
  ["agent-report", blocks('<background-agent id="[^"]*" description="[^"]*" status="[^"]*">', "background-agent")],
  // crates/evo-mesh/src/hub.rs (`session_note`, and the notes that a
  // proposed session was started or dismissed: the latter names its proposal).
  [
    "session-note",
    blocks('<nested-session (?:id|proposal)="[^"]*"(?: title="[^"]*")? status="[^"]*">', "nested-session"),
  ],
  // crates/evo-engine/src/tools/mesh.rs (`render`)
  ["mail", blocks('<mail id="[^"]*" from="[^"]*" title="[^"]*" kind="(?:ask|message)">', "mail")],
  ["agents-done", blocks("<background-agents-done>", "background-agents-done")],
  ["sessions-done", blocks("<nested-sessions-done>", "nested-sessions-done")],
  ["mail-arrived", blocks("<mail-arrived>", "mail-arrived")],
];

/**
 * Which automated message `text` is, if any. The whole message must have the
 * structure the engine, the hub or this app writes, from its opening tag with
 * every attribute to the closing tag at the end: text that merely starts like
 * one (pasted XML) is the person's.
 */
export function noteKind(text: string): NoteKind | undefined {
  const body = text.trim();
  if (!body.startsWith("<")) return undefined;
  return NOTES.find(([, pattern]) => pattern.test(body))?.[0];
}

export const isBackgroundNote = (text: string) => noteKind(text) !== undefined;
/** A report, note or mail (not a wake prompt): it carries what another agent said. */
const isDelivery = (text: string) => {
  const kind = noteKind(text);
  return kind === "agent-report" || kind === "session-note" || kind === "mail";
};

// ── Restoring a saved chat ─────────────────────────────────────────────────────

/**
 * A prompt as the person typed it: a `/name args` the engine expanded into a
 * `<skill name="…">…</skill>` block is shown as `/name args` again.
 */
export function typedPrompt(text: string): string {
  // Context hooks added (`<hook-context>`, crates/evo-engine/src/hooks.rs) is not what was typed.
  text = text.replace(/\n\n<hook-context>\n[\s\S]*?\n<\/hook-context>\s*$/, "");
  const match = /^<skill name="([^"]+)" location="[^"]*">[\s\S]*?\n<\/skill>(?:\n\n([\s\S]*))?$/.exec(text);
  if (!match) return text;
  return `/${match[1]}${match[2] ? ` ${match[2]}` : ""}`;
}

/**
 * A user message reached the model during a run. The prompt that started the
 * run echoes back and changes nothing; a message sent meanwhile (steered or
 * queued) ends the turn so far and starts one of its own, as in a saved chat.
 *
 * A delivery (an agent's report, a session's note, mail) follows the same
 * rule as `transcriptFromMessages`, so a chat reads the same live and
 * reopened: next to a prompt (queued for the run it starts, or read before the
 * run's first step) it makes no turn; in the middle of a run it starts a turn
 * shown as an event line; and one that the next message follows at once gives
 * way to it.
 */
function userMessageStarted(state: TranscriptState, message: any, now: number): TranscriptState {
  const text = typedPrompt(userText(message));
  const queued = state.queued ?? [];
  // A message the person sent is theirs, whatever it looks like.
  const at = queued.findIndex((item) => item.text === text);
  const delivery = at === -1 && isDelivery(text);
  if (delivery) state = withReports(state, text, now);
  const current = state.turns.at(-1);
  // Nothing of the run shows under the last prompt yet.
  const fresh = current?.status === "running" && current.parts.length === 0;
  const echo = at === -1 && fresh && current.prompt === text;
  if (echo || (at === -1 && !current && !delivery)) return state;
  const superseded = fresh && isDelivery(current.prompt);
  if (delivery && fresh && !superseded) return state;
  const item = at === -1 ? undefined : queued[at];
  const rest = at === -1 ? queued : queued.filter((_, index) => index !== at);
  const kept = superseded ? state.turns.slice(0, -1) : state.turns;
  const turns = kept.map((turn, index) =>
    index === kept.length - 1 && turn.status === "running"
      ? { ...settleBlocks(turn, now), status: "done" as const, endedAt: now, meta: { ...turn.meta, durationMs: now - turn.startedAt } }
      : turn,
  );
  const images = item?.images ?? userImages(message);
  return {
    ...state,
    queued: rest,
    busy: true,
    streamingMessageId: undefined,
    turns: [
      ...turns,
      { id: localId("turn"), prompt: text, images, parts: [], status: "running", startedAt: now, meta: {} },
    ],
  };
}

/** The pictures of a user message, as data URLs. */
export function userImages(message: any): string[] | undefined {
  if (!Array.isArray(message?.content)) return undefined;
  const images = message.content
    .filter((part: any) => part?.type === "image" && typeof part.data === "string")
    .map((part: any) => `data:${part.mimeType || part.mime_type || "image/png"};base64,${part.data}`);
  return images.length ? images : undefined;
}

export function userText(message: any): string {
  if (typeof message?.content === "string") return message.content;
  if (!Array.isArray(message?.content)) return "";
  return message.content
    .filter((part: any) => part?.type === "text" && typeof part.text === "string")
    .map((part: any) => part.text)
    .join("\n");
}

/**
 * A saved background agent's call keeps the snapshot from when it returned
 * (`status: "running"`); its reports were applied already (`applyAgentReport`).
 * One still running belongs to an earlier engine when this chat's engine was
 * just started:
 * - lost (`lost(agentId)`: the engine exited under it; the engine also queues
 *   such a report for the chat's next run): shown as stopped;
 * - otherwise it finished and its report waits for the chat's next run:
 *   shown as finished, the report to come.
 * Without an engine start it may well be running: left as it is.
 */
function settleBackgroundAgents(turns: Turn[], lost: (agentId: string) => boolean, engineStarted: boolean): Turn[] {
  return turns.map((turn) => {
    let changed = false;
    const parts = turn.parts.map((part) => {
      const child = part.kind === "tool" ? (part.details?.child as Record<string, any> | undefined) : undefined;
      if (part.kind !== "tool" || !child?.background || child.status !== "running") return part;
      const gone = lost(typeof child.agentId === "string" ? child.agentId : "");
      if (!gone && !engineStarted) return part;
      changed = true;
      const settled = gone
        ? { ...child, status: "error", endedAt: child.startedAt ?? part.endedAt, error: LOST_AGENT }
        : { ...child, status: "done", endedAt: null, reportPending: true };
      return { ...part, details: { ...part.details, child: settled } };
    });
    return changed ? { ...turn, parts } : turn;
  });
}

/**
 * Rebuild the turns of a saved chat from its messages (`get_messages`), in the
 * same shape the live stream produces, so a reopened chat looks the way it did.
 */
export function transcriptFromMessages(
  messages: (AgentMessage & Record<string, any>)[],
  {
    running = false,
    engineStarted = false,
    lostAgents,
  }: {
    running?: boolean;
    /**
     * The chat's engine was just started: no background agent of an earlier
     * engine runs any more, so one that never reported was lost.
     */
    engineStarted?: boolean;
    /**
     * The agents the engine says it lost (`lostBackgroundAgents` of its
     * `ready`): only these show as stopped. An agent that finished with its
     * report still queued for the next run is not among them. Absent, every
     * unreported agent of a started engine counts as lost.
     */
    lostAgents?: string[];
  } = {},
): TranscriptState {
  let turns: Turn[] = [];
  let agentReports: Record<string, AgentReport> = {};
  // The last turn is a prompt nothing has answered yet.
  let fresh = false;
  const replace = (turn: Turn) => {
    turns[turns.length - 1] = turn;
  };
  messages.forEach((message, index) => {
    const at = typeof message.timestamp === "number" ? message.timestamp : 0;
    if (message.role === "user") {
      const prompt = typedPrompt(userText(message));
      if (isDelivery(prompt)) {
        // The calls these reports end are earlier messages: settle them now.
        const found = reportsIn(prompt, at);
        turns = found.reduce(applyAgentReport, turns);
        agentReports = { ...agentReports, ...agentReportsIn(prompt, at) };
        // The rule of `userMessageStarted`: a delivery the next message
        // follows at once, or one next to a prompt, makes no turn.
        if (messages[index + 1]?.role === "user" || fresh) return;
      }
      fresh = true;
      turns.push({
        id: `saved-${index}`,
        prompt,
        images: userImages(message),
        entryId: typeof message.entryId === "string" ? message.entryId : undefined,
        parentId: typeof message.parentId === "string" ? message.parentId : null,
        parts: [],
        status: "done",
        startedAt: at,
        endedAt: at,
        meta: {},
      });
      return;
    }
    if (turns.length === 0) {
      // Messages before the first prompt (a fork, a summary): an unprompted turn.
      turns.push({ id: `saved-${index}`, prompt: "", parts: [], status: "done", startedAt: at, endedAt: at, meta: {} });
    }
    const turn = turns[turns.length - 1];
    if (message.role === "assistant") {
      fresh = false;
      let next = applyUsage(finishMessage(turn, `saved-${index}`, message, at), message);
      if (message.stopReason === "error") next = { ...next, status: "error", error: message.errorMessage ?? "The model returned an error" };
      if (message.stopReason === "aborted") next = { ...next, status: "aborted" };
      replace({ ...next, endedAt: at || next.endedAt });
    } else if (message.role === "toolResult") {
      // The call's turn: the last one, unless a message steered in since started another.
      let t = turns.length - 1;
      while (t >= 0 && findTool(turns[t], message.toolCallId) === -1) t--;
      if (t < 0) return;
      const owner = turns[t];
      const found = findTool(owner, message.toolCallId);
      const parts = owner.parts.slice();
      const part = parts[found] as ToolPart;
      parts[found] = {
        ...part,
        phase: message.isError ? "error" : "done",
        output: toolOutput(message),
        images: toolImages(message) ?? part.images,
        // A failed call's result carries its details (a subagent's last snapshot) and usage like any other.
        details: resultDetails(message, part.details),
        endedAt: at || part.endedAt,
      };
      turns[t] = applyToolUsage({ ...owner, parts, endedAt: at || owner.endedAt }, (message as any).usage);
    }
  });
  for (let i = 0; i < turns.length; i++) {
    const turn = turns[i];
    // The engine still runs the last turn: its open calls are running, not stopped.
    if (running && i === turns.length - 1) {
      turns[i] = {
        ...turn,
        status: "running",
        endedAt: undefined,
        parts: turn.parts.map((part) =>
          part.kind === "tool" && part.phase === "preparing" ? { ...part, phase: "running" as const } : part,
        ),
      };
      continue;
    }
    turns[i] = {
      ...turn,
      meta: {
        ...turn.meta,
        durationMs: turn.endedAt && turn.startedAt ? Math.max(0, turn.endedAt - turn.startedAt) : undefined,
      },
      // A call with no saved result never finished; it ended with its turn at the latest.
      parts: turn.parts.map((part) =>
        part.kind === "tool" && (part.phase === "preparing" || part.phase === "running")
          ? {
              ...part,
              phase: "error" as const,
              endedAt: part.endedAt ?? (turn.endedAt || part.startedAt),
              output: part.output || "Stopped before it finished.",
            }
          : part,
      ),
    };
  }
  const lost = (agentId: string) => (lostAgents ? lostAgents.includes(agentId) : engineStarted);
  return { turns: settleBackgroundAgents(turns, lost, engineStarted), busy: running, agentReports };
}

// ── Partial tool arguments ────────────────────────────────────────────────────

/**
 * Read a string field out of tool arguments that may still be streaming, so a
 * row can show the command or path before the call is dispatched.
 */
export function partialField(argsText: string, field: string): string | undefined {
  const match = new RegExp(`"${field}"\\s*:\\s*"((?:[^"\\\\]|\\\\.)*)`).exec(argsText);
  if (!match) return undefined;
  try {
    return JSON.parse(`"${match[1]}"`);
  } catch {
    // An escape cut in half: drop the dangling backslash.
    return JSON.parse(`"${match[1].replace(/\\+$/, "")}"`);
  }
}
