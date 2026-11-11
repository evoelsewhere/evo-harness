// Engines, the model catalog and the live transcripts — one runtime per chat.
//
// Every open chat has its own engine process and runtime (status, state,
// transcript), so several chats run at once and switching chats never stops a
// run. The top-level fields mirror the active chat's runtime, which is what
// the thread, composer and header read; background runtimes keep streaming.
// One store owns the `EngineClient` so components never talk to IPC directly.

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { homeDir } from "@tauri-apps/api/path";
import { invoke } from "@tauri-apps/api/core";
import { EngineClient } from "@/engine/client";
import {
  applyMeshEvent,
  childRunOf,
  emptyView,
  isMesh,
  MESH_VERSION,
  type MeshEnvelope,
  type MeshNode,
  type MeshView,
} from "@/engine/mesh";
import type {
  AgentMessage,
  ApprovalGrant,
  ApprovalMode,
  ApprovalState,
  ContextBreakdown,
  EngineMessage,
  EngineState,
  ModelDescriptor,
} from "@/engine/protocol";
import {
  carryPermissions,
  hasOpenPermission,
  initialTranscript,
  transcriptFromMessages,
  transcriptReducer,
  turnReply,
  typedPrompt,
  userText,
  type TranscriptAction,
  type TranscriptState,
} from "@/features/chat/transcript";

/** A picture for the model, base64 without the `data:` prefix. */
export interface PromptImage {
  data: string;
  mimeType: string;
}

const dataUrl = (image: PromptImage) => `data:${image.mimeType};base64,${image.data}`;

/** The `prompt`/`queue_message` payload: plain text, or text and pictures as content blocks. */
function promptPayload(text: string, images: PromptImage[] = []): Record<string, unknown> {
  if (images.length === 0) return { message: text };
  const blocks: Record<string, unknown>[] = text ? [{ type: "text", text }] : [];
  for (const image of images) blocks.push({ type: "image", data: image.data, mimeType: image.mimeType });
  return { contentBlocks: blocks };
}
import { notify, windowFocused } from "@/features/notifications/notify";
import { db, log } from "@/lib/db";
import { chatWindowId, closeChatWindow, focusChatWindow, openChatWindow } from "@/lib/windows";
import { worktrees } from "@/lib/worktrees";
import { APPROVAL_CHANGED, useUiStore, worktreeKey } from "@/stores/ui-store";
import { emitOfficeFileChanged } from "@/stores/office-events";
import { connectorsChanged, useConnectorsStore } from "@/stores/connectors-store";
import { useCustomizeStore } from "@/stores/customize-store";
import { UNTITLED, useSessionStore } from "@/stores/session-store";

export type EngineStatus = "idle" | "starting" | "ready" | "exited" | "error";

export interface LogLine {
  at: number;
  chat: string;
  line: string;
}

/** One chat's engine and what it shows. */
export interface ChatRuntime {
  chatId: string;
  status: EngineStatus;
  error?: string;
  cwd?: string;
  state: EngineState;
  transcript: TranscriptState;
  contextBreakdown?: ContextBreakdown;
  contextBreakdownAt?: number;
  /** Last time the chat was shown, for closing idle engines. */
  lastActive: number;
  /** Load the saved transcript once the engine is ready. */
  restore: boolean;
  /** The last turn already counted in the chat's totals. */
  recordedTurn?: string;
}

type Mirrored = Pick<ChatRuntime, "status" | "error" | "cwd" | "state" | "transcript" | "contextBreakdown" | "contextBreakdownAt">;

interface EngineStore extends Mirrored {
  /** The chat on screen. */
  activeId: string | null;
  runtimes: Record<string, ChatRuntime>;
  /** Chats running in another window (whose engine this window does not hold). */
  busyElsewhere: Record<string, true>;
  /** Chats of this window with a subagent still working (their engine must stay up). */
  agentsRunning: Record<string, true>;
  logs: LogLine[];
  /** Chats whose `compact` is still running (outlives the context panel that started it). */
  compactingChats: Record<string, boolean>;
  models: ModelDescriptor[];
  thinkingLevels: string[];
  /** `provider/id` of the model new chats start with. */
  defaultModel?: string;
  /** The default was chosen by the person (saved in settings), not picked automatically. */
  defaultSaved: boolean;

  connect(): Promise<void>;
  /** Restart the active chat's engine (optionally in another folder). */
  start(cwd?: string): Promise<void>;
  /** Add `path` as a workspace (if new) and start a chat in it. */
  openWorkspace(path: string): Promise<void>;
  /** Start a chat in a workspace (`null` = this computer), or in the current one. */
  newChat(workspaceId?: string | null): Promise<void>;
  /** Start a chat in the background with `prompt` (a scheduled task's run); its id. */
  startTaskChat(task: { workspaceId: string | null; title: string; prompt: string }): Promise<string | null>;
  selectSession(id: string): Promise<void>;
  /** Archive or delete a chat, stopping its engine; the active one is replaced by a new chat. */
  closeChat(id: string, how: "archive" | "delete"): Promise<void>;
  /** Stop engines of chats removed elsewhere (a removed workspace). */
  forgetChats(ids: string[]): Promise<void>;
  /** Show a saved chat in a window of its own; this window lets go of it. */
  openInNewWindow(id: string): Promise<void>;
  /** Copy a saved chat (whole transcript) into a new chat and show it. */
  forkChat(id: string): Promise<void>;
  /**
   * Move a chat to another workspace (`null`: "This computer"); its engine
   * restarts there. Refused, with a notification, while the chat runs a turn
   * or has subagents working.
   */
  moveChat(id: string, workspaceId: string | null): Promise<void>;
  /**
   * Memory off or incognito for a chat, before its first message only: its
   * engine starts again with the choice (docs/features/memory-topics-and-chat-recall).
   */
  setChatFlags(chat: string, flags: { memoryOff?: boolean; incognito?: boolean }): Promise<void>;
  /**
   * Restart the engines of chats that are not running a turn, so they pick up
   * new startup settings (Settings → Sandbox). Running ones, and ones whose
   * subagents still work, keep going and pick them up when they next start.
   */
  restartIdleEngines(): Promise<void>;
  /**
   * Have the engines of other chats that are not running a turn re-read
   * skills, commands and templates (a skill was switched on or off).
   */
  reloadIdleResources(): Promise<void>;
  /** Switch a connector off (or back on) for one chat; its tools follow. */
  setChatConnector(chat: string, key: string, on: boolean): Promise<void>;
  /** Stop a background subagent of `chat`. */
  cancelBackgroundAgent(chat: string, agentId: string): Promise<void>;
  /**
   * Start a nested session of `parentId`: a new chat (fresh, or a fork of the
   * parent's conversation) that runs `prompt` in the background. Returns its
   * id. With `proposalId` (a proposal card's Start) the hub starts it once:
   * a second Start gets the same session, and a dismissed proposal is refused.
   */
  startChildSession(
    parentId: string,
    options: { title: string; prompt: string; context?: "fresh" | "fork"; worktree?: boolean },
    proposalId?: string,
  ): Promise<string | null>;
  /** A proposal card's Dismiss: the hub starts nothing for it any more. */
  dismissChildProposal(parentId: string, proposalId: string): Promise<void>;
  /** Show the chat that started nested session `id` (an archived one included). */
  openParent(id: string): Promise<void>;
  /** The chat's composer holds a draft: no automated wake of it meanwhile (FR-19). */
  setComposing(chat: string, composing: boolean): void;
  /**
   * Stop `chat`'s engine and wait until its process is gone (its worktree is
   * about to be removed); the chat stays on screen. Rejects while the chat
   * runs a turn, has subagents working, or is held by another window.
   * `resumeEngine` or `leaveWorktree` starts it again.
   */
  stopEngine(chat: string): Promise<void>;
  /** Start the engine `stopEngine` stopped, in the chat's folder. */
  resumeEngine(chat: string): Promise<void>;
  /**
   * A worktree session's changes were applied or discarded: it works in its
   * workspace folder again (its engine restarts there, also after `stopEngine`).
   */
  leaveWorktree(id: string): Promise<void>;
  /** Send a nested session a message: queued if it is running, else a new turn. */
  /** Start a turn with `text` (and pictures). */
  sendPrompt(text: string, images?: PromptImage[]): Promise<void>;
  /**
   * Send a message while the agent works: `steer` reaches it at its next step,
   * `followUp` once it would otherwise stop. Shown as waiting until it does.
   */
  queueMessage(text: string, images: PromptImage[], deliverAs: "steer" | "followUp"): Promise<void>;
  /** Take back a waiting message. */
  cancelQueued(entryId: string): Promise<void>;
  /** Stop the run. Resolves to the waiting messages it never read (to put back in the field). */
  abort(): Promise<string[]>;
  /**
   * Replace the prompt of `turnId` and everything after it: the chat goes back
   * to just before that prompt (the old branch stays in the session) and
   * `text` is sent from there.
   */
  editMessage(turnId: string, text: string): Promise<void>;
  setModel(model: ModelDescriptor): Promise<void>;
  setThinkingLevel(level: string): Promise<void>;
  /** Make `model` (with the current effort) the one new chats start with; `null` goes back to automatic. */
  setDefaultModel(model: ModelDescriptor | null): Promise<void>;
  answerPermission(requestId: string, option: { title: string; permission?: string }, index: number): Promise<void>;
  /** Re-read the approval mode and `chat`'s grants from its engine (`get_approval_state`). */
  refreshApproval(chat?: string): Promise<void>;
  /** Drop what the person allowed for the rest of `chat` ("Allow commands for this chat"); prompts come back. */
  clearApprovalGrants(chat: string): Promise<void>;
  /** Answer the agent's questions (`ask_user`); `null` skips them. */
  answerQuestions(requestId: string, answers: { selected: string[]; other?: string }[] | null): Promise<void>;
  refreshState(chat?: string): Promise<void>;
  loadContextBreakdown(): Promise<void>;
  /** Summarise older turns now to free context (the engine's `compact`). */
  compact(): Promise<void>;
  /** Send any command to the active chat's engine. */
  request<T = unknown>(type: string, payload?: Record<string, unknown>): Promise<T>;
}

const MAX_LOG_LINES = 500;
/** How long `compact` waits for the engine to report the compaction. */
const COMPACT_TIMEOUT_MS = 10 * 60_000;
/** Per chat, who waits for its `compact` run to settle (`agent_settled` with `compact: true`). */
const compactWaiters = new Map<string, (message: EngineMessage | null) => void>();
/** Engines kept alive for chats that are neither shown nor running. */
const MAX_IDLE_ENGINES = 4;
/** Commands that change providers for every engine, not just the one asked. */
const PROVIDER_COMMANDS = new Set([
  "set_api_key",
  "set_provider",
  "remove_provider",
  "refresh_provider_models",
  "refresh_models_catalog",
]);

let client: EngineClient | null = null;

/** Emitted by Settings → Sandbox after a change; every window restarts its idle engines. */
export const SANDBOX_CHANGED = "sandbox://changed";
/** No grants (a stable value for selectors). */
const NO_GRANTS: ApprovalGrant[] = [];
/** The kinds of call the person allowed for the rest of the active chat (chips next to the approval mode). */
export const useApprovalGrants = (): ApprovalGrant[] => useEngineStore((s) => s.state.approval?.grants ?? NO_GRANTS);

/** The saved default, read once from the first engine that is ready. */
let defaultLoaded: Promise<void> | null = null;

/** Working directory for chats that belong to no workspace. */
async function localRoot(): Promise<string> {
  return homeDir().catch(() => "");
}

const newChatId = () =>
  typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `chat-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;

const mirror = (runtime: ChatRuntime | undefined): Mirrored => ({
  status: runtime?.status ?? "idle",
  error: runtime?.error,
  cwd: runtime?.cwd,
  state: runtime?.state ?? {},
  transcript: runtime?.transcript ?? initialTranscript,
  contextBreakdown: runtime?.contextBreakdown,
  contextBreakdownAt: runtime?.contextBreakdownAt,
});

/** The active chat's saved messages are still being loaded (show a spinner, not an empty chat). */
export const useRestoring = () =>
  useEngineStore((s) => {
    const rt = s.activeId ? s.runtimes[s.activeId] : undefined;
    return Boolean(rt?.restore && rt.transcript.turns.length === 0);
  });

/**
 * What a worktree review needs for the worktree `chat` works in: Apply and
 * Discard wait while the chat is working (`busy`); its engine is stopped
 * before the folder goes (`beforeSettle`: an engine keeps its folder open, so
 * on Windows it could not be removed); afterwards the chat goes on in its
 * workspace (`onSettled`); a review closed without either starts the engine
 * again (`onClosed`).
 */
export function useWorktreeSession(chat: string | undefined) {
  const busy = useEngineStore((s) =>
    Boolean(chat && (s.runtimes[chat]?.transcript.busy || s.busyElsewhere[chat] || s.agentsRunning[chat])),
  );
  if (!chat) return { busy: false, beforeSettle: undefined, onSettled: undefined, onClosed: () => undefined };
  const { stopEngine, resumeEngine, leaveWorktree } = useEngineStore.getState();
  return {
    busy,
    beforeSettle: () => stopEngine(chat),
    onSettled: () => void leaveWorktree(chat),
    // After `onSettled`, whichever the dialog calls first.
    onClosed: () => void setTimeout(() => void resumeEngine(chat), 0),
  };
}

const chatTitle = (chat: string) => useSessionStore.getState().sessions.find((s) => s.id === chat)?.title ?? UNTITLED;

export const useEngineStore = create<EngineStore>((set, get) => {
  /** Change one chat's runtime, mirroring it when it is the active one. */
  const update = (chat: string, change: (runtime: ChatRuntime) => Partial<ChatRuntime>) => {
    set((store) => {
      const runtime = store.runtimes[chat];
      if (!runtime) return {};
      const next = { ...runtime, ...change(runtime) };
      const runtimes = { ...store.runtimes, [chat]: next };
      return chat === store.activeId ? { runtimes, ...mirror(next) } : { runtimes };
    });
  };

  const runtime = (chat: string | null | undefined) => (chat ? get().runtimes[chat] : undefined);

  // Streamed deltas publish once per animation frame: a fast stream sends
  // hundreds per second, and re-rendering the thread for each one is what makes
  // long replies stutter. Every other event flushes the queue first, so order
  // is preserved (DeepSeek Harness publishes deltas the same way).
  const pendingDeltas = new Map<string, EngineMessage[]>();
  let frame: number | null = null;
  const flushDeltas = () => {
    if (frame !== null) {
      cancelAnimationFrame(frame);
      frame = null;
    }
    if (pendingDeltas.size === 0) return;
    const batches = [...pendingDeltas];
    pendingDeltas.clear();
    for (const [chat, batch] of batches) {
      update(chat, (rt) => ({
        transcript: batch.reduce((transcript, message) => transcriptReducer(transcript, { type: "engine", message }), rt.transcript),
      }));
    }
  };
  const dispatch = (chat: string, action: TranscriptAction) => {
    flushDeltas();
    const before = runtime(chat);
    const asked = before ? hasOpenPermission(before.transcript) : false;
    update(chat, (rt) => ({ transcript: transcriptReducer(rt.transcript, action) }));
    // An approval request held the chat's wake back (FR-19): look again now that it closed.
    const after = runtime(chat);
    if (asked && after && !hasOpenPermission(after.transcript)) scheduleWake(chat);
  };

  /** Count a finished turn in the chat's totals, and flag it if nobody saw it finish. */
  const recordFinishedTurn = (chat: string) => {
    const rt = runtime(chat);
    const turn = rt?.transcript.turns.at(-1);
    if (!rt || !turn || turn.status === "running" || rt.recordedTurn === turn.id) return;
    update(chat, () => ({ recordedTurn: turn.id }));
    const unseen = chat !== get().activeId || !windowFocused();
    const { meta } = turn;
    void useSessionStore.getState().recordTurn(chat, {
      preview: turnReply(turn).slice(0, 400) || undefined,
      costUsd: meta.cost ?? 0,
      tokens: (meta.inputTokens ?? 0) + (meta.outputTokens ?? 0) + (meta.cacheRead ?? 0) + (meta.cacheWrite ?? 0),
      provider: meta.provider,
      model: meta.model,
      unread: unseen,
    });
    // A nested session's note to its parent goes through the shell's mesh hub
    // (src-tauri/src/mesh.rs): it reaches the parent whichever window shows it.
    if (unseen) {
      notify({
        source: "agent",
        level: turn.status === "error" ? "error" : "success",
        title: turn.status === "error" ? "A run failed" : "Reply ready",
        body: chatTitle(chat),
        key: `agent:${chat}`,
        target: { type: "session", id: chat },
      });
    }
  };

  /** Tell every other engine that provider settings changed. */
  const broadcastProviders = (from: string | null) => {
    for (const rt of Object.values(get().runtimes)) {
      if (rt.chatId === from || rt.status !== "ready" || !client) continue;
      void client
        .request(rt.chatId, "reload_providers")
        .then(() => get().refreshState(rt.chatId))
        .catch((error) => log("warn", "engine", `reloading providers failed: ${String(error)}`, { chatId: rt.chatId }));
    }
  };

  /** An engine told us it does not know the approval commands: said once (NFR-2). */
  let approvalUnsupportedLogged = false;
  const approvalRequestFailed = (chat: string, command: string, error: unknown) => {
    if (/unknown command/i.test(String(error))) {
      if (approvalUnsupportedLogged) return;
      approvalUnsupportedLogged = true;
      log("warn", "engine", "this engine does not know approval modes: the mode is not applied", { chatId: chat });
      return;
    }
    log("warn", "engine", `${command} failed: ${String(error)}`, { chatId: chat });
  };

  /** Tell every ready engine of this window the approval mode changed; it applies from its next tool call. */
  const broadcastApprovalMode = (mode: ApprovalMode) => {
    for (const rt of Object.values(get().runtimes)) {
      if (rt.status !== "ready" || !client) continue;
      void client
        .request<ApprovalState>(rt.chatId, "set_approval_mode", { mode })
        .then(() => get().refreshApproval(rt.chatId))
        .catch((error) => approvalRequestFailed(rt.chatId, "set_approval_mode", error));
    }
  };

  /** Close the engines of idle chats beyond the budget, least recently shown first. */
  const trimIdle = () => {
    const idle = Object.values(get().runtimes)
      .filter(
        (rt) =>
          rt.chatId !== get().activeId &&
          !rt.transcript.busy &&
          rt.status !== "starting" &&
          !pendingPrompts.has(rt.chatId) &&
          // Background agents run in the chat's engine: stopping it would kill them.
          !hasRunningAgents(rt.chatId),
      )
      .sort((a, b) => b.lastActive - a.lastActive);
    for (const rt of idle.slice(MAX_IDLE_ENGINES)) void stopChat(rt.chatId);
  };

  const stopChat = async (chat: string) => {
    pendingDeltas.delete(chat);
    forgetEngine(chat);
    pausedEngines.delete(chat);
    set((store) => {
      const runtimes = { ...store.runtimes };
      delete runtimes[chat];
      return { runtimes };
    });
    await client?.stop(chat).catch(() => undefined);
  };

  /** Forget `chat`'s runtime here but keep its engine running (another window takes it). */
  const release = (chat: string) => {
    pendingDeltas.delete(chat);
    forgetEngine(chat);
    set((store) => {
      const runtimes = { ...store.runtimes };
      delete runtimes[chat];
      return { runtimes };
    });
  };

  /**
   * Start `chat`'s engine in `cwd` with a fresh runtime. With `attach`, an
   * engine that already runs (handed over by another window, or kept across a
   * reload) is picked up as it is instead of restarted.
   */
  const launch = async (chat: string, cwd: string | undefined, restore: boolean, attach = false) => {
    // What this window knew of an earlier process (its agents, its setup) is void.
    forgetEngine(chat);
    pausedEngines.delete(chat);
    set((store) => {
      const rt: ChatRuntime = {
        chatId: chat,
        status: "starting",
        cwd,
        state: {},
        transcript: store.runtimes[chat]?.transcript ?? initialTranscript,
        lastActive: Date.now(),
        restore,
      };
      const runtimes = { ...store.runtimes, [chat]: rt };
      return chat === store.activeId ? { runtimes, ...mirror(rt) } : { runtimes };
    });
    try {
      if (attach && client && (await client.running().catch((): string[] => [])).includes(chat)) {
        update(chat, () => ({ status: "ready", error: undefined }));
        // No `ready` comes from an engine that already runs: subscribe here,
        // before the transcript loads, so its replay can be drawn into it.
        await subscribeMesh(chat);
        if (restore) await restoreTranscript(chat);
        // Its tools were set up by whoever started it: the hub may send now.
        void meshHosted(chat);
        void loadDefault(chat);
        void get().refreshState(chat);
        return;
      }
      // A draft has no row yet: the shell is told what it was set to.
      const row = useSessionStore.getState().sessions.find((s) => s.id === chat);
      await client?.start(chat, cwd, row ? { memoryOff: row.memoryOff, incognito: row.incognito } : undefined);
    } catch (error) {
      update(chat, () => ({ status: "error", error: String(error) }));
      log("error", "engine", `could not start: ${String(error)}`, { chatId: chat });
      notify({
        source: "engine",
        level: "error",
        title: "The engine could not start",
        body: String(error),
        key: "engine:status",
        target: { type: "settings", section: "engine" },
      });
    }
  };

  /**
   * Bumped by every chat selection. A selection awaits (the chat's folder, a
   * chat window); one overtaken by a later click while it waited stops there,
   * so the sidebar and the content always show the last chat picked.
   */
  let selection = 0;

  /** Show `chat`, starting its engine if it has none. */
  const activate = async (chat: string, cwd: string | undefined, restore: boolean) => {
    flushDeltas();
    set((store) => ({ activeId: chat, ...mirror(store.runtimes[chat]) }));
    if (get().runtimes[chat]) update(chat, () => ({ lastActive: Date.now() }));
    else await launch(chat, cwd, restore, true);
    trimIdle();
  };

  /**
   * Restore the saved messages once a reopened chat's engine is ready.
   * `engineStarted`: the engine just sent `ready` (a new process: none of its
   * background agents run); `lostAgents` are the ids it reported lost (FR-12).
   */
  const restoreTranscript = async (chat: string, engineStarted = false, lostAgents?: string[]) => {
    if (!client) return;
    try {
      const [data, state] = await Promise.all([
        client.request<{ messages: AgentMessage[] }>(chat, "get_messages", { limit: 2000 }),
        client.request<{ activeOperation?: unknown }>(chat, "get_state").catch(() => undefined),
      ]);
      // The engine may still be working (the window reloaded mid-run, or
      // another window handed the chat over): its last turn then goes on, live.
      const transcript = transcriptFromMessages((data?.messages ?? []) as (AgentMessage & Record<string, any>)[], {
        running: Boolean(state?.activeOperation),
        engineStarted,
        lostAgents,
      });
      update(chat, (rt) => ({
        // The same engine goes on: a request it still waits on is not sent again.
        transcript: engineStarted ? transcript : carryPermissions(transcript, rt.transcript),
        restore: false,
        error: undefined,
        recordedTurn: state?.activeOperation ? rt.recordedTurn : (transcript.turns.at(-1)?.id ?? rt.recordedTurn),
      }));
      // Mesh events may have arrived before the transcript existed.
      redrawAgents(chat);
      scheduleWake(chat);
    } catch (error) {
      update(chat, () => ({ restore: false, error: `Could not load this chat: ${String(error)}` }));
      log("error", "engine", `could not load the saved chat: ${String(error)}`, { chatId: chat });
    }
  };

  // ── Session mesh ────────────────────────────────────────────────────────
  //
  // The engine pushes its nodes' lifecycle and activity (src/engine/mesh.ts,
  // docs/features/session-mesh-protocol/spec.md). This window subscribes at the
  // summary level and folds subagent events into the `agent` call's nested
  // view, live, for foreground and background agents alike. A subscription
  // replays the retained state and one activity snapshot per node, so a
  // reopened chat or a reloaded window catches up without polling.
  const meshViews = new Map<string, MeshView>();

  const subscribeMesh = async (chat: string) => {
    if (!client) return;
    meshViews.set(chat, emptyView());
    try {
      await client.mesh(chat, "subscribe", { level: "summary" });
    } catch (error) {
      log("warn", "engine", `mesh subscribe failed: ${String(error)}`, { chatId: chat });
    }
  };

  /** Show `address`'s latest agent view in its tool call (if that call is loaded). */
  const drawAgent = (chat: string, view: MeshView, address: string) => {
    const callId = view.nodes.get(address)?.meta.callId;
    const child = childRunOf(view, address);
    if (typeof callId === "string" && child) {
      dispatch(chat, { type: "agent-progress", callId, child: child as Record<string, unknown> });
    }
  };

  /** Redraw every known agent (after the transcript was (re)loaded). */
  const redrawAgents = (chat: string) => {
    const view = meshViews.get(chat);
    if (!view) return;
    for (const node of view.nodes.values()) if (node.kind === "agent") drawAgent(chat, view, node.id);
  };

  /** Whether any subagent of `chat` is still working (its engine must stay up). */
  const hasRunningAgents = (chat: string) =>
    [...(meshViews.get(chat)?.nodes.values() ?? [])].some((node) => node.kind !== "session" && node.state === "running");

  /** Keep `agentsRunning` (what components read) in step with the mesh view. */
  const syncAgentsRunning = (chat: string) => {
    const running = hasRunningAgents(chat);
    set((store) => {
      if (Boolean(store.agentsRunning[chat]) === running) return store;
      const agentsRunning = { ...store.agentsRunning };
      if (running) agentsRunning[chat] = true;
      else delete agentsRunning[chat];
      return { agentsRunning };
    });
  };

  /**
   * Drop what this window keeps about `chat`'s engine process (it stopped,
   * restarts, or went to another window): its mesh view, setup and timers.
   */
  const forgetEngine = (chat: string) => {
    for (const node of meshViews.get(chat)?.nodes.values() ?? []) counted.delete(node.id);
    meshViews.delete(chat);
    openCalls.delete(chat);
    engineSetup.delete(chat);
    toolsStale.delete(chat);
    clearTimeout(wakeTimers.get(chat));
    wakeTimers.delete(chat);
    syncAgentsRunning(chat);
  };

  /** Chats whose engine `stopEngine` stopped; their runtime stays, status `idle`. */
  const pausedEngines = new Set<string>();
  /** Chats `leaveWorktree` is moving back to their workspace folder. */
  const leavingWorktree = new Set<string>();

  const ended = (node: MeshNode | undefined) => node?.state === "settled" || node?.state === "closed";

  const onMesh = (chat: string, envelope: MeshEnvelope) => {
    const view = meshViews.get(chat) ?? emptyView();
    meshViews.set(chat, view);
    const id = envelope.op === "node_state" ? (envelope.body?.node?.id as string | undefined) : undefined;
    const known = id !== undefined && view.nodes.has(id);
    const wasEnded = id !== undefined && ended(view.nodes.get(id));
    const changed = applyMeshEvent(view, envelope);
    if (!changed) return;
    const node = view.nodes.get(changed);
    if (node?.kind === "session") {
      // A run read every queued message and cleared the wake list.
      if (node.state === "running") handledWakes.delete(chat);
      scheduleWake(chat);
      return;
    }
    drawAgent(chat, view, changed);
    if (!node || changed !== id) return;
    syncAgentsRunning(chat);
    if (node.kind !== "agent" || typeof node.meta.callId !== "string") return;
    // First heard of (a replay after subscribing): what it used so far is not this window's to count.
    if (!known) {
      if (!counted.has(node.id)) counted.set(node.id, usageOf(node.usage));
      // A foreground run in progress: its call is open, though this window missed its start.
      if (!ended(node) && node.meta.background !== true) {
        openCalls.set(chat, (openCalls.get(chat) ?? new Set<string>()).add(node.meta.callId));
      }
    } else if (!wasEnded && ended(node) && !openCalls.get(chat)?.has(node.meta.callId)) {
      // A background (or released) run ended: no tool result carries its usage.
      countAgent(chat, node);
    }
  };

  // ── Background subagents ────────────────────────────────────────────────
  //
  // A background `agent` call returns at once; the subagent keeps running in
  // the chat's engine (crates/evo-engine/src/tools/agent.rs). When it finishes
  // while the chat is idle, its report is queued for the chat's next run and
  // the engine lists it in the session node's `meta.wake`; this window then
  // wakes the chat with an automated prompt. The list clears when any run
  // starts. Wakes coalesce for WAKE_DELAY_MS, and at most MAX_AUTO_WAKES
  // happen in a row: then reports wait for the person's next message. No
  // wake while the person has a draft in the chat's composer or an approval
  // request is open there, and none of an archived chat (FR-19).
  const WAKE_DELAY_MS = 2000;
  const MAX_AUTO_WAKES = 5;
  const autoWakes = new Map<string, number>();
  const wakeTimers = new Map<string, ReturnType<typeof setTimeout>>();
  /** Wake requests already acted on (or set aside) since the chat's last run, per chat. */
  const handledWakes = new Map<string, Set<string>>();
  /** Chats whose composer holds a draft (`setComposing`). */
  const composing = new Set<string>();

  // A subagent's usage counts toward a turn of its chat once (FR-6). A
  // foreground run's usage comes on its tool result (the transcript adds it);
  // a background or released run's has no result to ride on, so it is added
  // here when the run ends: to the turn in progress, or kept for the next one.
  type AgentUsage = { input: number; output: number; cacheRead: number; cacheWrite: number; cost: number };
  const usageOf = (u?: Partial<AgentUsage> | null): AgentUsage => ({
    input: u?.input ?? 0,
    output: u?.output ?? 0,
    cacheRead: u?.cacheRead ?? 0,
    cacheWrite: u?.cacheWrite ?? 0,
    cost: typeof u?.cost === "number" ? u.cost : 0,
  });
  /** Usage already counted per agent node (it accumulates over continued runs). */
  const counted = new Map<string, AgentUsage>();
  /** Usage of runs that ended while their chat was idle, for the chat's next turn. */
  const pendingUsage = new Map<string, AgentUsage>();
  /** Tool calls in progress per chat: an agent whose call is open reports its usage on the result. */
  const openCalls = new Map<string, Set<string>>();

  const asMessageUsage = (u: AgentUsage) => ({
    input: u.input,
    output: u.output,
    cacheRead: u.cacheRead,
    cacheWrite: u.cacheWrite,
    cost: { total: u.cost },
  });

  /** Count the part of `agent`'s usage no turn has yet. */
  const countAgent = (chat: string, agent: MeshNode) => {
    const now = usageOf(agent.usage);
    const before = counted.get(agent.id) ?? usageOf();
    counted.set(agent.id, now);
    const delta: AgentUsage = {
      input: Math.max(0, now.input - before.input),
      output: Math.max(0, now.output - before.output),
      cacheRead: Math.max(0, now.cacheRead - before.cacheRead),
      cacheWrite: Math.max(0, now.cacheWrite - before.cacheWrite),
      cost: Math.max(0, now.cost - before.cost),
    };
    if (!(delta.input || delta.output || delta.cacheRead || delta.cacheWrite || delta.cost)) return;
    if (runtime(chat)?.transcript.busy) {
      dispatch(chat, { type: "extra-usage", usage: asMessageUsage(delta) });
      return;
    }
    const kept = pendingUsage.get(chat) ?? usageOf();
    pendingUsage.set(chat, {
      input: kept.input + delta.input,
      output: kept.output + delta.output,
      cacheRead: kept.cacheRead + delta.cacheRead,
      cacheWrite: kept.cacheWrite + delta.cacheWrite,
      cost: kept.cost + delta.cost,
    });
  };

  /** A turn of `chat` just started: it takes the usage kept for it. */
  const takePendingUsage = (chat: string) => {
    const kept = pendingUsage.get(chat);
    if (!kept) return;
    pendingUsage.delete(chat);
    dispatch(chat, { type: "extra-usage", usage: asMessageUsage(kept) });
  };

  /**
   * Follow the chat's tool calls for the usage of `agent` / `message_agent`
   * runs: a result with `usage` was added to the turn by the transcript, so
   * the agent is counted up to its snapshot (cumulative); a call that ended
   * without one (it failed) has its agent counted here once it has ended.
   */
  const trackAgentCall = (chat: string, message: EngineMessage) => {
    const callId = message.toolCallId;
    if (typeof callId !== "string") return;
    if (message.type === "tool_execution_start") {
      const calls = openCalls.get(chat) ?? new Set<string>();
      openCalls.set(chat, calls);
      calls.add(callId);
      return;
    }
    openCalls.get(chat)?.delete(callId);
    const view = meshViews.get(chat);
    if (!view) return;
    const result = message.result as { usage?: unknown; details?: { child?: { agentId?: unknown; usage?: Partial<AgentUsage> } } } | undefined;
    const child = result?.details?.child;
    const agent = [...view.nodes.values()].find(
      (node) =>
        node.kind === "agent" &&
        (typeof child?.agentId === "string" ? node.id.endsWith(`/a_${child.agentId}`) : node.meta.callId === callId),
    );
    if (!agent) return;
    if (result?.usage && child?.usage) counted.set(agent.id, usageOf(child.usage));
    // Still running: it went to the background and is counted when it ends.
    else if (ended(agent) && agent.meta.callId === callId) countAgent(chat, agent);
  };

  /** A run of `chat` ended: no call outlives it. Agents whose calls it cut short are counted. */
  const closeCalls = (chat: string) => {
    const calls = openCalls.get(chat);
    openCalls.delete(chat);
    const view = meshViews.get(chat);
    if (!calls?.size || !view) return;
    for (const node of view.nodes.values()) {
      if (node.kind === "agent" && calls.has(node.meta.callId) && ended(node)) countAgent(chat, node);
    }
  };

  /** Start an automated run of `chat` (not the person typing). */
  const startAutomatedRun = (chat: string, text: string) => {
    dispatch(chat, { type: "prompt", text });
    takePendingUsage(chat);
    void setupDone(chat)
      .then(() => client?.request(chat, "prompt", { message: text }))
      .catch((error) => {
        dispatch(chat, { type: "fail", error: error instanceof Error ? error.message : String(error) });
      });
  };

  const wake = (chat: string, agents: MeshNode[], sessions: string[], mails: string[] = []) => {
    const wakes = autoWakes.get(chat) ?? 0;
    // The reports stay queued (and their usage kept): they reach the model with the next message.
    if (wakes >= MAX_AUTO_WAKES) return;
    autoWakes.set(chat, wakes + 1);
    const quoted = (names: string[]) => names.map((name) => `"${name.replace(/"/g, "'")}"`).join(", ");
    // The chip under the wake shows what woke the chat (chat-turn.tsx).
    const text = !agents.length && !sessions.length
      ? `<mail-arrived>
${quoted(mails)} sent you ${mails.length === 1 ? "a message" : "messages"} above. If one asks a question, answer it with the reply tool (the asker is waiting); then continue the work.
</mail-arrived>`
      : agents.length
      ? `<background-agents-done>
${agents.length === 1 ? "A background agent" : "Background agents"} finished: ${quoted(agents.map((a) => a.title))}. ${
          agents.length === 1 ? "Its report is" : "Their reports are"
        } in the message${agents.length === 1 ? "" : "s"} above.${
          sessions.length ? ` Nested sessions also reported: ${quoted(sessions)}.` : ""
        }${mails.length ? ` Messages also arrived from ${quoted(mails)}; answer questions with the reply tool.` : ""} Continue the work with ${agents.length + sessions.length === 1 ? "it" : "them"}.
</background-agents-done>`
      : `<nested-sessions-done>
${sessions.length === 1 ? "A nested session" : "Nested sessions"} finished a turn: ${quoted(sessions)}. ${
          sessions.length === 1 ? "Its note is" : "Their notes are"
        } in the message${sessions.length === 1 ? "" : "s"} above.${
          mails.length ? ` Messages also arrived from ${quoted(mails)}; answer questions with the reply tool.` : ""
        } Continue the work with ${sessions.length === 1 ? "it" : "them"}.
</nested-sessions-done>`;
    startAutomatedRun(chat, text);
  };

  /** Wake `chat` for the reports its session node says are waiting, if it is idle. */
  const checkWake = (chat: string) => {
    const view = meshViews.get(chat);
    const rt = runtime(chat);
    if (!view || rt?.status !== "ready" || rt.transcript.busy) return;
    // Not while the person writes there or is asked for an approval: their
    // Send would become a steer. Both look again when they end
    // (`setComposing`, `dispatch`).
    if (composing.has(chat) || hasOpenPermission(rt.transcript)) return;
    // An archived (or deleted) chat is never woken.
    if (!useSessionStore.getState().sessions.some((s) => s.id === chat)) return;
    const handled = handledWakes.get(chat) ?? new Set<string>();
    handledWakes.set(chat, handled);
    const agents: MeshNode[] = [];
    const sessions: string[] = [];
    // Messages and questions from this chat's agents (crates/evo-engine/src/tools/mesh.rs).
    const mails: string[] = [];
    for (const session of view.nodes.values()) {
      if (session.kind !== "session" || session.state !== "idle") continue;
      const titles = (session.meta.wakeTitles ?? {}) as Record<string, string>;
      const reasons = (session.meta.wakeReasons ?? {}) as Record<string, string>;
      for (const address of (session.meta.wake ?? []) as string[]) {
        if (handled.has(address)) continue;
        const agent = view.nodes.get(address);
        if (reasons[address] === "mail") mails.push(titles[address] ?? agent?.title ?? "an agent");
        else if (agent) agents.push(agent);
        // Mail from another session (a nested session's note, via the hub).
        else if (!address.includes("/")) sessions.push(titles[address] ?? "a nested session");
        else continue;
        handled.add(address);
      }
    }
    if (agents.length + sessions.length + mails.length > 0) wake(chat, agents, sessions, mails);
  };

  /** Check for wakes once events settle (several agents finishing make one wake). */
  const scheduleWake = (chat: string) => {
    clearTimeout(wakeTimers.get(chat));
    wakeTimers.set(
      chat,
      setTimeout(() => {
        wakeTimers.delete(chat);
        checkWake(chat);
      }, WAKE_DELAY_MS),
    );
  };

  // ── Nested sessions ─────────────────────────────────────────────────────
  //
  // A top-level chat's engine gets three tools from this app (RPC external
  // tools): start_session, message_session, list_sessions. A started session
  // is a full chat linked to its parent (`parentChatId`), run in the
  // background; each time it finishes a turn the parent gets a note — into its
  // run when busy, or as an automated turn when idle (same wake limit as
  // background agents). By default start_session only proposes: the person
  // starts it from the card (Settings → Sandbox can allow starting directly).
  // Nested sessions get none of these tools: one level deep.
  const pendingPrompts = new Map<string, string>();

  const chatRow = (id: string) => {
    const sessions = useSessionStore.getState();
    return sessions.sessions.find((s) => s.id === id) ?? sessions.archived.find((s) => s.id === id);
  };
  /** The folder a chat works in: its worktree, else its workspace, else home. */
  const chatFolder = async (id: string) => {
    const chat = chatRow(id);
    if (chat?.worktreePath) return chat.worktreePath;
    const workspace = useSessionStore.getState().workspaces.find((w) => w.id === chat?.workspaceId);
    return workspace?.path ?? (await localRoot());
  };
  /**
   * A deleted chat's worktree: removed when nothing changed in it. A changed
   * one stays, and the person is told once where to review it.
   */
  const dropWorktree = async (title: string, path: string, workspaceId: string | null) => {
    const repo = useSessionStore.getState().workspaces.find((w) => w.id === workspaceId)?.path;
    if (!repo) return;
    try {
      const key = worktreeKey(path);
      const worktree = (await worktrees.list(repo)).find((w) => worktreeKey(w.path) === key);
      // Not listed: it was applied or discarded already.
      if (!worktree) return;
      if (!worktree.changed) {
        await worktrees.discard(repo, worktree.path);
        return;
      }
      notify({
        source: "worktrees",
        title: "A worktree was kept",
        body: `"${title}" was deleted, but its worktree ${worktree.branch} still has changes. Review or discard them in Settings → Sandbox.`,
        key: `worktree:${key}`,
        target: { type: "settings", section: "sandbox" },
      });
    } catch (error) {
      log("warn", "worktrees", `could not remove a deleted chat's worktree: ${String(error)}`);
    }
  };

  /** Start a turn of any chat (not only the one on screen). */
  const promptChat = async (chat: string, text: string) => {
    dispatch(chat, { type: "prompt", text });
    takePendingUsage(chat);
    try {
      await client?.request(chat, "prompt", { message: text });
    } catch (error) {
      dispatch(chat, { type: "fail", error: error instanceof Error ? error.message : String(error) });
    }
  };

  /**
   * A chat's setup after its engine is `ready` (model, tools). A prompt sent
   * before it ends would start a run, and the engine refuses to change
   * external tools while a run is active: the chat would lack its session,
   * browser and connector tools. Prompts from this window wait for it,
   * at most SETUP_WAIT_MS.
   */
  const engineSetup = new Map<string, Promise<unknown>>();
  const SETUP_WAIT_MS = 10_000;
  const setupDone = (chat: string): Promise<unknown> => {
    const setup = engineSetup.get(chat);
    if (!setup) return Promise.resolve();
    return Promise.race([setup.catch(() => undefined), new Promise((resolve) => setTimeout(resolve, SETUP_WAIT_MS))]);
  };
  /** Chats whose tools could not be set during a run: set again when it settles. */
  const toolsStale = new Set<string>();

  /** Tell the shell which connectors are switched off for a chat (it builds and gates the connector tools). */
  const pushConnectorSwitches = (chat: string) =>
    invoke("agent_mcp_chat_config", { chat, off: useUiStore.getState().chatConnectorsOff[chat] ?? [] }).catch(() => undefined);

  const configureSessionTools = async (chat: string) => {
    if (!client) return;
    // Every chat gets the shell's tools (browser, terminal, Files, scheduler,
    // and the connectors' tools, built by the shell for the chat's folder and
    // the connectors switched off for it); nested sessions get no session tools.
    const folder = await chatFolder(chat).catch(() => "");
    await pushConnectorSwitches(chat);
    // Session tools are for top-level chats; they are defined and answered by
    // the shell's mesh hub (src-tauri/src/mesh.rs). The scheduler tools are the
    // shell's too (agent_tools/scheduler) and refuse nested chats themselves.
    const topLevel = !chatRow(chat)?.parentChatId;
    const sessionTools = topLevel
      ? await invoke<Record<string, unknown>[]>("mesh_session_tools").catch((): Record<string, unknown>[] => [])
      : [];
    const shellTools = await invoke<Record<string, unknown>[]>("agent_tools_definitions", { chat, folder: folder || null }).catch((): Record<string, unknown>[] => []);
    const tools = [...(topLevel ? sessionTools : []), ...shellTools];
    await client
      .request(chat, "configure_external_tools", { tools })
      .then(() => toolsStale.delete(chat))
      .catch((error) => {
        // Refused during a run (a prompt overtook the setup, connectors
        // changed mid-run): try again when the run settles.
        if (/run is active/i.test(String(error))) toolsStale.add(chat);
        log("warn", "engine", `could not add the session tools: ${String(error)}`, { chatId: chat });
      });
  };

  /**
   * Once per folder: when a top-level chat's project uses languages with no
   * language server (and what installing needs is present), say so and point
   * to Customize → Code intelligence.
   */
  const suggestCodeIntelligence = async (chat: string) => {
    if (!client || chatRow(chat)?.parentChatId || chatWindowId) return;
    type Listing = {
      folder: string;
      detected: { server: string; files: number }[];
      servers: { id: string; language: string; installed: unknown; installing: boolean; missing: string[]; install: { kind: string } }[];
    };
    const listing = await client.request<Listing>(chat, "list_language_servers").catch(() => undefined);
    if (!listing?.folder) return;
    const key = `evo.lsp-suggested:${listing.folder.toLowerCase()}`;
    try {
      if (localStorage.getItem(key)) return;
    } catch {
      return;
    }
    const wanted = listing.detected
      .filter((d) => d.files >= 3)
      .map((d) => listing.servers.find((s) => s.id === d.server))
      .filter((s) => s && !s.installed && !s.installing && !s.missing.length && s.install.kind !== "system");
    if (!wanted.length) return;
    try {
      localStorage.setItem(key, String(Date.now()));
    } catch {
      // Ask again next time.
    }
    const languages = wanted.map((s) => s!.language);
    notify({
      title: `Code intelligence for ${languages.slice(0, 3).join(", ")}${languages.length > 3 ? "…" : ""}`,
      body: "Install the language servers so the agent can check types, jump to definitions and find references.",
      source: "code-intelligence",
      target: { type: "customize", section: "languages" },
      system: false,
    });
  };

  /** A chat's engine is ready: its tools, then a waiting first prompt. */
  const onReadyNested = async (chat: string) => {
    await configureSessionTools(chat);
    const first = pendingPrompts.get(chat);
    if (first) {
      pendingPrompts.delete(chat);
      await promptChat(chat, first);
    }
    // Set up: the mesh hub may send what waits for it (a session's brief,
    // a message from its parent, notes).
    await meshHosted(chat);
  };

  const meshHosted = (chat: string) =>
    invoke("mesh_hosted", { chat }).catch((error) =>
      log("warn", "engine", `mesh hub: ${String(error)}`, { chatId: chat }),
    );

  /**
   * Restarts of exited engines for mail, per chat: at most MAIL_RESTARTS in
   * MAIL_RESTART_WINDOW_MS, so an engine that dies on start is not restarted
   * forever (each exit with mail waiting asks for a host again).
   */
  const MAIL_RESTARTS = 3;
  const MAIL_RESTART_WINDOW_MS = 120_000;
  const mailRestarts = new Map<string, number[]>();
  const mayRestartForMail = (chat: string) => {
    const now = Date.now();
    const recent = (mailRestarts.get(chat) ?? []).filter((at) => now - at < MAIL_RESTART_WINDOW_MS);
    if (recent.length >= MAIL_RESTARTS) {
      log("warn", "engine", "the engine keeps exiting: its mail waits until the chat is opened", { chatId: chat });
      mailRestarts.set(chat, recent);
      return false;
    }
    mailRestarts.set(chat, [...recent, now]);
    return true;
  };

  /**
   * The mesh hub needs `chat`'s engine: a session it just started, or a chat
   * with mail (a nested session's note, a parent's message). A window that
   * holds the chat tells the hub it is set up; otherwise the main window
   * starts the engine in the background (`onReadyNested` then tells the hub).
   */
  const onHost = async (chat: string, cwd: string | null, restore: boolean) => {
    if (!client) return;
    const rt = runtime(chat);
    if (rt) {
      if (rt.status === "ready") await meshHosted(chat);
      // Its engine exited or never started (a crash): start it again, or the
      // mail waits until someone opens the chat while its sender is told a
      // note will come. Its `ready` tells the hub (`onReadyNested`).
      else if ((rt.status === "exited" || rt.status === "error") && mayRestartForMail(chat)) {
        log("info", "engine", "restarting the engine to deliver mail", { chatId: chat });
        await launch(chat, rt.cwd ?? (await chatFolder(chat)), true);
      }
      return;
    }
    if (chatWindowId) return;
    let row = chatRow(chat);
    // A chat the shell just created may not have reached this window yet.
    if (!row) {
      await useSessionStore.getState().load().catch(() => undefined);
      row = chatRow(chat);
    }
    if (!row || row.archivedAt) return;
    if ((await client.running().catch((): string[] => [])).includes(chat)) return;
    await launch(chat, cwd ?? (await chatFolder(chat)), restore, true);
  };

  // Questions the shell's tools raise (a connector tool set to "ask", the
  // browser's gate), shown like the engine's permission prompts;
  // `answerPermission` resolves them.
  const appApprovals = new Map<string, { chat: string; externalId: string; resolve(answer: number): void }>();
  /** The engine gave up on a call (the run stopped): its open question is moot. */
  const dropApprovals = (externalId: string) => {
    for (const [requestId, approval] of appApprovals) {
      if (approval.externalId !== externalId) continue;
      appApprovals.delete(requestId);
      dispatch(approval.chat, { type: "engine", message: { type: "ui_dialog_closed", requestId } });
      approval.resolve(-1);
    }
  };

  const isDraft = (chat: string) => Boolean(useSessionStore.getState().sessions.find((s) => s.id === chat)?.draft);

  /** Read the saved default model (`settings.json`) once. */
  const loadDefault = (chat: string) => {
    defaultLoaded ??= (async () => {
      const saved = await client
        ?.request<{ provider?: string | null; modelId?: string | null }>(chat, "get_default_model")
        .catch(() => undefined);
      if (saved?.provider && saved.modelId) set({ defaultModel: `${saved.provider}/${saved.modelId}`, defaultSaved: true });
    })();
    return defaultLoaded;
  };

  /**
   * A new chat starts on the default model. The engine already honours the
   * saved default; this covers providers its automatic pick does not know.
   * A chat whose row already names a model (a nested session: the hub gives
   * it its parent's) starts on that one instead.
   */
  const startOnDefault = async (chat: string) => {
    await loadDefault(chat);
    await get().refreshState(chat);
    const row = chatRow(chat);
    if (client && row && !row.draft && row.provider && row.model) {
      const state = runtime(chat)?.state;
      if (state?.provider === row.provider && (state.modelId ?? state.model) === row.model) return;
      const switched = await client
        .request(chat, "set_model", { provider: row.provider, modelId: row.model })
        .then(() => true)
        .catch((error) => {
          log("warn", "engine", `could not start on the chat's model: ${String(error)}`, { chatId: chat });
          return false;
        });
      if (switched) {
        await get().refreshState(chat);
        return;
      }
      // That model is gone (its provider was removed): the default, as for any new chat.
    }
    const { defaultModel, defaultSaved } = get();
    const state = runtime(chat)?.state;
    if (!client || !defaultSaved || !defaultModel || !state?.provider) return;
    if (`${state.provider}/${state.modelId ?? state.model}` === defaultModel) return;
    const slash = defaultModel.indexOf("/");
    await client
      .request(chat, "set_model", { provider: defaultModel.slice(0, slash), modelId: defaultModel.slice(slash + 1) })
      .then(() => get().refreshState(chat))
      .catch((error) => log("warn", "engine", `could not start on the default model: ${String(error)}`, { chatId: chat }));
  };

  /** Runs of chats this window does not hold, for the sidebar. */
  const trackElsewhere = (chat: string, running: boolean) =>
    set((store) => {
      if (Boolean(store.busyElsewhere[chat]) === running) return store;
      const busyElsewhere = { ...store.busyElsewhere };
      if (running) busyElsewhere[chat] = true;
      else delete busyElsewhere[chat];
      return { busyElsewhere };
    });

  const onMessage = (chat: string, message: EngineMessage) => {
    if (!runtime(chat)) {
      if (message.type === "agent_start") trackElsewhere(chat, true);
      else if (message.type === "agent_end" || message.type === "agent_settled") trackElsewhere(chat, false);
      return;
    }
    if (isMesh(message)) {
      onMesh(chat, message);
      return;
    }
    if (message.type === "ready") {
      update(chat, () => ({ status: "ready", error: undefined }));
      if (message.mesh?.version === MESH_VERSION) void subscribeMesh(chat);
      else log("warn", "engine", "this engine does not speak the session mesh: subagents will not update live", { chatId: chat });
      // The background agents of an earlier engine that never reported (FR-12).
      const lost = Array.isArray(message.lostBackgroundAgents)
        ? message.lostBackgroundAgents.filter((id: unknown): id is string => typeof id === "string")
        : undefined;
      const setup = (async () => {
        if (runtime(chat)?.restore) {
          // `ready` comes from a new process: an earlier engine's background agents are gone.
          await restoreTranscript(chat, true, lost);
          void loadDefault(chat);
          void get().refreshState(chat);
        } else {
          await startOnDefault(chat);
        }
        await onReadyNested(chat);
        void suggestCodeIntelligence(chat);
      })();
      // Prompts from this window wait for it (`setupDone`).
      engineSetup.set(chat, setup);
      void setup.finally(() => {
        if (engineSetup.get(chat) === setup) engineSetup.delete(chat);
      });
      return;
    }
    // The shell answers every external tool request itself (src-tauri/src/agent_tools): nothing to do here.
    if (message.type === "external_tool_request") return;
    if (message.type === "external_tool_request_closed") {
      if (typeof message.requestId === "string") {
        dropApprovals(message.requestId);
      }
      return;
    }
    if (message.type === "office_file_changed") {
      // Office files saved in the chat's folder: the Files tool re-renders and auto-opens (stores/office-events.ts).
      emitOfficeFileChanged(chat, message.changes);
      return;
    }
    if (message.type === "providers_changed") {
      // A sign-in finished or models.dev brought new data: this engine already
      // swapped its provider runtimes; the others reload, and the UI refreshes.
      void get().refreshState(chat);
      broadcastProviders(chat);
      // Memory indexing stopped for a model it could not use may go on now.
      void db.memoryIndexPoke().catch(() => undefined);
      window.dispatchEvent(new CustomEvent("evo:providers-changed", { detail: message }));
      return;
    }
    if (message.type === "message_update") {
      const queue = pendingDeltas.get(chat);
      if (queue) queue.push(message);
      else pendingDeltas.set(chat, [message]);
      frame ??= requestAnimationFrame(() => {
        frame = null;
        flushDeltas();
      });
      return;
    }
    // Before the turn closes: agents whose calls the run's end cut short count toward it.
    if (message.type === "agent_settled" || message.type === "agent_end") closeCalls(chat);
    dispatch(chat, { type: "engine", message });
    if (message.type === "tool_execution_start" || message.type === "tool_execution_end") trackAgentCall(chat, message);
    if (message.type === "agent_settled" && message.compact === true) {
      compactWaiters.get(chat)?.(message);
      compactWaiters.delete(chat);
    }
    if (message.type === "agent_settled" || message.type === "agent_end") {
      recordFinishedTurn(chat);
      scheduleWake(chat);
      if (message.type === "agent_settled" && toolsStale.has(chat)) void configureSessionTools(chat);
      // Context usage changes after every run.
      void get().refreshState(chat);
      if (chat === get().activeId) void get().loadContextBreakdown();
      trimIdle();
    } else if (message.type === "ui_dialog_request" && (chat !== get().activeId || !windowFocused())) {
      const ui = (message.ui ?? {}) as { title?: string; question?: string; message?: string };
      notify({
        source: "agent",
        level: "warning",
        title: "Approval needed",
        // Nothing an incognito chat says is kept, a notification's text included.
        body: useSessionStore.getState().sessions.find((s) => s.id === chat)?.incognito
          ? chatTitle(chat)
          : `${chatTitle(chat)}: ${ui.title ?? ui.question ?? ui.message ?? "the agent needs your input"}`,
        key: `agent:${chat}`,
        target: { type: "session", id: chat },
      });
    }
  };

  const onExit = (chat: string, code: number | null) => {
    // Its subagents went with it: what they used so far still counts.
    const hadAgents = hasRunningAgents(chat);
    const rt = runtime(chat);
    for (const node of (rt && meshViews.get(chat)?.nodes.values()) || []) {
      if (node.kind === "agent" && typeof node.meta.callId === "string" && !ended(node)) countAgent(chat, node);
    }
    forgetEngine(chat);
    pausedEngines.delete(chat);
    if (!rt) {
      trackElsewhere(chat, false);
      return;
    }
    const wasBusy = rt.transcript.busy;
    update(chat, () => ({ status: "exited", error: code ? `Engine exited with code ${code}` : "The engine stopped" }));
    // Also for an idle chat whose subagents were working: the transcript settles them.
    if (wasBusy || hadAgents) dispatch(chat, { type: "fail", error: "The engine stopped" });
    if (chat === get().activeId || wasBusy || hadAgents) {
      notify({
        source: "engine",
        level: "error",
        title: "The engine stopped",
        body: `${chatTitle(chat)} — ${code ? `exited with code ${code}` : "it stopped"}. Restart it from the banner.`,
        key: `engine:${chat}`,
        target: { type: "session", id: chat },
      });
    }
  };

  return {
    activeId: null,
    runtimes: {},
    busyElsewhere: {},
    agentsRunning: {},
    logs: [],
    compactingChats: {},
    defaultSaved: false,
    models: [],
    thinkingLevels: [],
    ...mirror(undefined),

    async connect() {
      if (client) return;
      client = new EngineClient({
        onMessage,
        onStderr: (chat, line) =>
          set((store) => ({ logs: [...store.logs.slice(-MAX_LOG_LINES + 1), { at: Date.now(), chat, line }] })),
        onExit,
      });
      await client.attach();
      // The approval mode is an app setting: read it before any engine starts
      // (an older build's local value moves into it), and follow changes from
      // any window by telling this window's engines.
      await useUiStore.getState().loadApproval();
      void listen<{ mode?: ApprovalMode }>(APPROVAL_CHANGED, ({ payload }) => {
        const mode = payload?.mode;
        if (mode !== "ask" && mode !== "auto-edits" && mode !== "auto") return;
        useUiStore.getState().syncApproval(mode);
        broadcastApprovalMode(mode);
      });
      // Settings → Sandbox changed (in any window): idle engines restart with it.
      void listen(SANDBOX_CHANGED, () => void get().restartIdleEngines());
      // Connectors came, went or changed their tools: every ready chat hears.
      connectorsChanged.add(() => {
        for (const rt of Object.values(get().runtimes)) {
          if (rt.status === "ready") void configureSessionTools(rt.chatId);
        }
      });
      // The shell's own tools changed (computer app control switched on or
      // off in Settings): every ready chat gets the new list.
      void listen("agent-tools://changed", () => {
        for (const rt of Object.values(get().runtimes)) {
          if (rt.status === "ready") void configureSessionTools(rt.chatId);
        }
      });
      // Memory indexing changed topics: every idle chat reads the new lists
      // from its next message.
      void listen("memory://changed", () => {
        for (const rt of Object.values(get().runtimes)) {
          if (rt.status === "ready" && !rt.transcript.busy && client) {
            void client.request(rt.chatId, "reload_resources").catch(() => undefined);
          }
        }
      });
      // A chat was detached from its parent (or its parent was deleted), in
      // any window: it is top-level now and gets the session tools.
      useSessionStore.subscribe((state, before) => {
        if (state.sessions === before.sessions) return;
        for (const rt of Object.values(get().runtimes)) {
          if (rt.status !== "ready") continue;
          const now = state.sessions.find((s) => s.id === rt.chatId);
          const was = before.sessions.find((s) => s.id === rt.chatId);
          if (now && was && Boolean(now.parentChatId) !== Boolean(was.parentChatId)) void configureSessionTools(rt.chatId);
        }
      });
      void useConnectorsStore.getState().load();
      // Plugins were installed, removed or switched: every ready chat re-reads
      // its skills, commands, subagents and hooks.
      // The mesh hub needs a chat's engine (a new nested session, waiting mail).
      void listen<{ chat: string; cwd: string | null; restore: boolean }>("mesh://host", ({ payload }) =>
        void onHost(payload.chat, payload.cwd, payload.restore),
      );
      // Mail left undelivered when the app last closed (a nested session's
      // note, a parent's message): nothing else asks for those engines now.
      if (!chatWindowId) {
        void invoke<string[]>("mesh_pending_chats")
          .then((chats) => Promise.all(chats.map((chat) => onHost(chat, null, true))))
          .catch((error) => log("warn", "engine", `mesh hub: ${String(error)}`));
      }
      // The hub starts a turn of a chat this window may show (a nested
      // session's brief, its parent's message): show it like a typed prompt.
      void listen<{ chat: string; text: string }>("mesh://prompted", ({ payload }) => {
        if (!runtime(payload.chat)) return;
        dispatch(payload.chat, { type: "prompt", text: payload.text });
        takePendingUsage(payload.chat);
      });
      // The hub starts sessions at once or proposes them (Settings → Sandbox).
      const pushMeshSettings = (autoStartSessions: boolean) =>
        void invoke("mesh_configure", { autoStartSessions }).catch(() => undefined);
      pushMeshSettings(useUiStore.getState().autoStartSessions);
      useUiStore.subscribe((state, before) => {
        if (state.autoStartSessions !== before.autoStartSessions) pushMeshSettings(state.autoStartSessions);
      });
      // Questions the shell's tools raise (approvals) show as the chat's approval prompt.
      void listen<{ chat: string; requestId: string; callId?: string; title: string; description: string; options: string[] }>("agent://ask", ({ payload }) => {
        const id = `shell:${payload.requestId}`;
        appApprovals.set(id, {
          chat: payload.chat,
          externalId: payload.callId ?? "",
          resolve: (index) => void invoke("agent_ask_answer", { requestId: payload.requestId, index }).catch(() => undefined),
        });
        dispatch(payload.chat, {
          type: "engine",
          message: { type: "ui_dialog_request", requestId: id, ui: { title: payload.title, description: payload.description, options: payload.options.map((title) => ({ title })) } },
        });
      });
      void listen<{ chat: string; requestId: string }>("agent://ask-closed", ({ payload }) => {
        const id = `shell:${payload.requestId}`;
        if (appApprovals.delete(id)) dispatch(payload.chat, { type: "engine", message: { type: "ui_dialog_closed", requestId: id } });
      });
      void listen("plugins://changed", () => {
        for (const rt of Object.values(get().runtimes)) {
          if (rt.status === "ready" && !rt.transcript.busy && client) {
            void client.request(rt.chatId, "reload_resources").catch(() => undefined);
          }
        }
        setTimeout(() => void useCustomizeStore.getState().load(true), 500);
      });
      const sessions = useSessionStore.getState();
      try {
        await sessions.load();
      } catch (error) {
        log("error", "db", `could not load chats: ${String(error)}`);
      }
      // A chat window shows its one chat.
      if (chatWindowId) {
        await get().selectSession(chatWindowId);
        return;
      }
      const last = useSessionStore.getState().lastChatId;
      if (last) await get().selectSession(last);
      else await get().newChat(null);
    },

    async start(cwd) {
      const chat = get().activeId;
      if (!chat) return;
      const rt = runtime(chat);
      const next = cwd === undefined ? rt?.cwd : cwd || undefined;
      const saved = !useSessionStore.getState().sessions.find((s) => s.id === chat)?.draft;
      // The engine reopens the saved session; reload what it has.
      await launch(chat, next, saved);
      if (!saved) update(chat, () => ({ transcript: initialTranscript }));
    },

    async openWorkspace(path) {
      // A chat window keeps to its chat; new chats start in the main window.
      if (chatWindowId) return;
      const workspace = await useSessionStore.getState().ensureWorkspace(path);
      await get().newChat(workspace.id);
    },

    async newChat(workspaceId) {
      // Picking a chat leaves Customize.
      useUiStore.getState().closeCustomize();
      if (chatWindowId) return;
      const sessions = useSessionStore.getState();
      const target =
        workspaceId === undefined
          ? sessions.workspaceOf(sessions.currentId)
          : (sessions.workspaces.find((w) => w.id === workspaceId) ?? null);
      const targetId = target?.id ?? null;
      const ticket = ++selection;
      // Reuse an untouched draft in that folder instead of stacking empty chats.
      const drafts = sessions.sessions.filter((s) => s.draft);
      const reusable = drafts.find((s) => s.workspaceId === targetId && !runtime(s.id)?.transcript.turns.length);
      for (const draft of drafts) {
        if (draft.id === reusable?.id || runtime(draft.id)?.transcript.turns.length) continue;
        sessions.discardDraft(draft.id);
        void stopChat(draft.id);
      }
      if (reusable) {
        sessions.select(reusable.id);
        // A draft left from an earlier run has no engine yet: start it in its
        // folder, never in the app's own working directory.
        const cwd = runtime(reusable.id)?.cwd ?? target?.path ?? (await localRoot());
        await activate(reusable.id, cwd, false);
        return;
      }
      const cwd = target?.path ?? (await localRoot());
      if (ticket !== selection) return;
      const id = newChatId();
      sessions.addDraft(id, targetId);
      await activate(id, cwd, false);
    },

    async selectSession(id) {
      // Picking a chat leaves Customize.
      useUiStore.getState().closeCustomize();
      // Even a click on the chat already shown overtakes selections still on their way.
      const ticket = ++selection;
      if (id === get().activeId && useSessionStore.getState().currentId === id) return;
      // A chat open in its own window is shown there (also when asked from
      // another chat's window); otherwise a chat window keeps to its chat.
      if (await focusChatWindow(id)) return;
      if (chatWindowId && id !== chatWindowId) return;
      if (ticket !== selection) return;
      const sessions = useSessionStore.getState();
      const chat = sessions.sessions.find((s) => s.id === id) ?? sessions.archived.find((s) => s.id === id);
      if (!chat) return;
      // Leaving an untouched draft discards it.
      const current = sessions.sessions.find((s) => s.id === get().activeId);
      if (current?.draft && !runtime(current.id)?.transcript.turns.length) {
        sessions.discardDraft(current.id);
        void stopChat(current.id);
      }
      sessions.select(id);
      const cwd = await chatFolder(id);
      if (ticket !== selection) return;
      await activate(id, cwd, !chat.draft);
    },

    async closeChat(id, how) {
      const sessions = useSessionStore.getState();
      const chat = sessions.sessions.find((s) => s.id === id) ?? sessions.archived.find((s) => s.id === id);
      const wasActive = id === get().activeId;
      void closeChatWindow(id);
      await stopChat(id);
      if (how === "delete") {
        await sessions.remove(id);
        // Its engine is gone, so its worktree's folder is free. An archived chat keeps its worktree.
        if (chat?.worktreePath) void dropWorktree(chat.title, chat.worktreePath, chat.workspaceId);
      } else await sessions.setArchived(id, true);
      if (wasActive) {
        set({ activeId: null, ...mirror(undefined) });
        await get().newChat(chat?.workspaceId ?? null);
      }
    },

    async forgetChats(ids) {
      const active = get().activeId;
      await Promise.all(ids.map((id) => stopChat(id)));
      if (active && ids.includes(active)) {
        set({ activeId: null, ...mirror(undefined) });
        await get().newChat(null);
      }
    },

    async openInNewWindow(id) {
      const chat = useSessionStore.getState().sessions.find((s) => s.id === id);
      if (!chat || chat.draft || chatWindowId) return;
      if (runtime(id)) release(id);
      if (id === get().activeId) {
        set({ activeId: null, ...mirror(undefined) });
        await get().newChat(chat.workspaceId ?? null);
      }
      await openChatWindow(id, chat.title).catch((error) =>
        notify({ source: "shell", level: "error", title: "Could not open a new window", body: String(error) }),
      );
    },

    async forkChat(id) {
      const copy = await useSessionStore.getState().fork(id, newChatId());
      if (copy) await get().selectSession(copy.id);
    },

    async moveChat(id, workspaceId) {
      const sessions = useSessionStore.getState();
      const rt = runtime(id);
      // Moving restarts the engine in the new folder, which would end the
      // turn and the subagents working in the old one: not while they run.
      if (rt?.transcript.busy || get().busyElsewhere[id] || hasRunningAgents(id)) {
        notify({
          source: "shell",
          level: "warning",
          title: "This chat is still working",
          body: `"${chatTitle(id)}" ${rt?.transcript.busy || get().busyElsewhere[id] ? "is in the middle of a turn" : "has subagents still working"}. Move it when it has finished, or stop it first.`,
          key: `move:${id}`,
          target: { type: "session", id },
        });
        return;
      }
      await sessions.move(id, workspaceId);
      // The engine works in the chat's folder: restart it in the new one.
      if (rt) await launch(id, await chatFolder(id), true);
    },

    async setChatFlags(chat, flags) {
      const rt = runtime(chat);
      const row = useSessionStore.getState().sessions.find((s) => s.id === chat);
      // Only before the first message: afterwards the choice is locked.
      if (!row?.draft || (rt?.transcript.turns.length ?? 0) > 0 || rt?.transcript.busy) return;
      useSessionStore.getState().setDraftFlags(chat, flags);
      // The draft's engine was started without the choice: start it again.
      await launch(chat, rt?.cwd, false);
    },

    async setChatConnector(chat, key, on) {
      useUiStore.getState().setChatConnectorOff(chat, key, !on);
      await pushConnectorSwitches(chat);
      if (runtime(chat)?.status === "ready") await configureSessionTools(chat);
    },

    async reloadIdleResources() {
      const active = get().activeId;
      await Promise.all(
        Object.values(get().runtimes)
          .filter((rt) => rt.chatId !== active && rt.status === "ready" && !rt.transcript.busy && client)
          .map((rt) =>
            client!
              .request(rt.chatId, "reload_resources")
              .catch((error) => log("warn", "engine", `reloading skills failed: ${String(error)}`, { chatId: rt.chatId })),
          ),
      );
    },

    async restartIdleEngines() {
      for (const rt of Object.values(get().runtimes)) {
        if (rt.transcript.busy || rt.status === "starting") continue;
        // Subagents run in the chat's engine: a restart would kill them.
        if (hasRunningAgents(rt.chatId) || pausedEngines.has(rt.chatId)) continue;
        await launch(rt.chatId, rt.cwd, !isDraft(rt.chatId));
      }
    },

    async startTaskChat({ workspaceId, title, prompt }) {
      const id = newChatId();
      const row = await useSessionStore.getState().createTaskChat({ id, workspaceId, title });
      if (!row) return null;
      // Sent once its engine is ready (`onReadyNested`), like a nested session.
      pendingPrompts.set(id, prompt);
      await launch(id, await chatFolder(id), false);
      return id;
    },

    async startChildSession(parentId, { title, prompt, context = "fresh", worktree = false }, proposalId) {
      const parent = chatRow(parentId);
      if (!parent || parent.draft) return null;
      // The shell's mesh hub creates it, has a window host its engine and
      // sends it the brief (src-tauri/src/mesh.rs). A proposal starts once:
      // the hub answers a second Start with the same session.
      return invoke<string>("mesh_start_session", {
        parent: parentId,
        spec: { title, prompt, fork: context === "fork", worktree },
        proposalId: proposalId ?? null,
      });
    },

    async dismissChildProposal(parentId, proposalId) {
      await invoke("mesh_dismiss_proposal", { parent: parentId, proposalId }).catch((error) =>
        log("warn", "engine", `mesh hub: could not dismiss the proposal: ${String(error)}`, { chatId: parentId }),
      );
    },

    async openParent(id) {
      const parentId = chatRow(id)?.parentChatId;
      if (!parentId) return;
      // An archived parent is loaded with the archive.
      if (!chatRow(parentId)) await useSessionStore.getState().loadArchived();
      if (chatRow(parentId)) await get().selectSession(parentId);
      else notify({ source: "shell", title: "The parent chat is gone", body: "It was deleted.", key: `parent:${id}` });
    },

    setComposing(chat, on) {
      if (on) composing.add(chat);
      // A wake held back for the draft may go ahead.
      else if (composing.delete(chat)) scheduleWake(chat);
    },

    async stopEngine(chat) {
      const rt = runtime(chat);
      if (rt?.transcript.busy || get().busyElsewhere[chat]) throw new Error("This chat is in the middle of a turn. Stop it first.");
      if (hasRunningAgents(chat)) throw new Error("This chat has subagents still working. Wait for them, or stop them first.");
      if (!rt) {
        // Not shown here: another window may hold it, or nothing does (kept across a reload).
        if (!client || !(await client.running().catch((): string[] => [])).includes(chat)) return;
        if (chatWindowId || (await focusChatWindow(chat))) {
          throw new Error("This chat is open in another window. Apply or discard its worktree there.");
        }
        await client.stop(chat);
        return;
      }
      pendingDeltas.delete(chat);
      forgetEngine(chat);
      pausedEngines.add(chat);
      update(chat, () => ({ status: "idle", error: undefined }));
      // Resolves once the process is gone: only then is its folder free (Windows).
      await client?.stop(chat);
    },

    async resumeEngine(chat) {
      // `leaveWorktree` starts it itself, in the workspace folder.
      if (!pausedEngines.has(chat) || leavingWorktree.has(chat) || !runtime(chat)) return;
      await launch(chat, await chatFolder(chat), !isDraft(chat));
    },

    async leaveWorktree(id) {
      leavingWorktree.add(id);
      try {
        await useSessionStore.getState().setWorktree(id, null);
        const rt = runtime(id);
        if (!rt) return;
        // The worktree is gone: the engine starts again in the workspace
        // folder, also when `stopEngine` stopped it for the removal.
        if (pausedEngines.has(id) || !(rt.transcript.busy || hasRunningAgents(id))) {
          await launch(id, await chatFolder(id), !isDraft(id));
        } else {
          log("warn", "engine", "the chat left its worktree while working: it moves to its workspace when its engine next starts", { chatId: id });
        }
      } finally {
        leavingWorktree.delete(id);
      }
    },

    async cancelBackgroundAgent(chat, agentId) {
      const node = [...(meshViews.get(chat)?.nodes.values() ?? [])].find((n) => n.id.endsWith(`/a_${agentId}`));
      if (!node || !client) return;
      await client.mesh(chat, "cancel", { node: node.id }).catch((error) => {
        log("warn", "engine", `could not cancel the agent: ${String(error)}`, { chatId: chat });
      });
    },

    async sendPrompt(text, images = []) {
      const message = text.trim();
      const chat = get().activeId;
      if (!client || !chat || (!message && images.length === 0)) return;
      const sessions = useSessionStore.getState();
      if (sessions.sessions.find((s) => s.id === chat)?.draft) {
        const { provider, modelId, model } = get().state;
        void sessions.saveDraft(chat, message, { provider, model: modelId ?? model });
      }
      dispatch(chat, { type: "prompt", text: message, images: images.map(dataUrl) });
      autoWakes.delete(chat);
      takePendingUsage(chat);
      try {
        // A new chat's engine may still be setting up its tools.
        await setupDone(chat);
        await client.request(chat, "prompt", promptPayload(message, images));
      } catch (error) {
        dispatch(chat, { type: "fail", error: error instanceof Error ? error.message : String(error) });
        log("error", "engine", `prompt failed: ${String(error)}`, { chatId: chat });
      }
    },

    async queueMessage(text, images, deliverAs) {
      const message = text.trim();
      const chat = get().activeId;
      if (!client || !chat || (!message && images.length === 0)) return;
      try {
        const queued = await client.request<{ entryId: string }>(chat, "queue_message", {
          ...promptPayload(message, images),
          deliverAs,
        });
        dispatch(chat, {
          type: "queued",
          item: { entryId: queued.entryId, text: message, images: images.length ? images.map(dataUrl) : undefined, deliverAs },
        });
      } catch (error) {
        log("error", "engine", `could not send the message: ${String(error)}`, { chatId: chat });
        throw error;
      }
    },

    async cancelQueued(entryId) {
      const chat = get().activeId;
      if (!client || !chat) return;
      await client.request(chat, "cancel_queued", { entryId }).catch(() => undefined);
      dispatch(chat, { type: "unqueued", entryId });
    },

    async abort() {
      const chat = get().activeId;
      if (!chat) return [];
      const reply = await client
        ?.request<{ steer?: { text?: string }[]; followUp?: { text?: string }[] }>(chat, "abort")
        .catch(() => undefined);
      dispatch(chat, { type: "unqueued", entryId: null });
      return [...(reply?.steer ?? []), ...(reply?.followUp ?? [])]
        .map((item) => item.text ?? "")
        .filter((text) => text.trim().length > 0);
    },

    async editMessage(turnId, text) {
      const chat = get().activeId;
      const rt = chat ? runtime(chat) : undefined;
      if (!client || !chat || !rt || rt.transcript.busy) return;
      const turns = rt.transcript.turns;
      const index = turns.findIndex((turn) => turn.id === turnId);
      const turn = turns[index];
      if (!turn) return;
      let parentId = turn.entryId ? (turn.parentId ?? null) : undefined;
      if (parentId === undefined) {
        // A turn of this window: find its prompt among the saved messages, by
        // its text and how many earlier prompts had the same text.
        const nth = turns.slice(0, index).filter((t) => t.prompt === turn.prompt).length;
        const data = await client.request<{ messages: (AgentMessage & Record<string, any>)[] }>(chat, "get_messages", {
          limit: 2000,
        });
        const prompts = (data?.messages ?? []).filter(
          (message) => message.role === "user" && typedPrompt(userText(message)) === turn.prompt,
        );
        const found = prompts[nth];
        if (!found) throw new Error("This message is not in the saved chat yet.");
        parentId = typeof found.parentId === "string" ? found.parentId : null;
      }
      await client.request(chat, "navigate", parentId ? { targetId: parentId } : {});
      await restoreTranscript(chat);
      // The prompt keeps its pictures.
      const images = (turn.images ?? []).flatMap((url) => {
        const match = /^data:([^;]+);base64,(.*)$/.exec(url);
        return match ? [{ mimeType: match[1], data: match[2] }] : [];
      });
      await get().sendPrompt(text, images);
    },

    async setModel(model) {
      const chat = get().activeId;
      if (!client || !chat) return;
      await client.request(chat, "set_model", { provider: model.provider, modelId: model.id });
      if (!useSessionStore.getState().sessions.find((s) => s.id === chat)?.draft) {
        void db.updateChat(chat, { provider: model.provider, model: model.id }).catch(() => undefined);
      }
      await get().refreshState(chat);
    },

    async setThinkingLevel(level) {
      const chat = get().activeId;
      if (!client || !chat) return;
      await client.request(chat, "set_thinking_level", { level });
      await get().refreshState(chat);
    },

    async setDefaultModel(model) {
      const chat = get().activeId;
      if (!client || !chat) return;
      const saved = await client.request<{ provider?: string | null; modelId?: string | null }>(chat, "set_default_model", {
        provider: model?.provider ?? null,
        modelId: model?.id ?? null,
        thinkingLevel: model ? (get().state.thinkingLevel ?? null) : null,
      });
      if (saved?.provider && saved.modelId) set({ defaultModel: `${saved.provider}/${saved.modelId}`, defaultSaved: true });
      else set({ defaultSaved: false, defaultModel: undefined });
      log("info", "engine", model ? `default model set to ${model.provider}/${model.id}` : "default model reset to automatic");
    },

    async answerQuestions(requestId, answers) {
      const chat = get().activeId;
      if (!client || !chat) return;
      dispatch(chat, { type: "permission-answered", requestId });
      await client
        .request(chat, "ui_dialog_response", answers ? { requestId, answer: { answers } } : { requestId, cancelled: true })
        .catch(() => undefined);
    },

    async answerPermission(requestId, option, index) {
      const approval = appApprovals.get(requestId);
      if (approval) {
        appApprovals.delete(requestId);
        dispatch(approval.chat, { type: "permission-answered", requestId });
        approval.resolve(index);
        return;
      }
      const chat = get().activeId;
      if (!client || !chat) return;
      dispatch(chat, { type: "permission-answered", requestId });
      await client
        .request(chat, "ui_dialog_response", {
          requestId,
          answer: { value: option.title, values: [option.title], index, permission: option.permission },
        })
        .catch(() => undefined);
      // "Allow … for this chat": the engine records the grant as it wakes the
      // call; read it back (twice, in case the first read is ahead of it).
      if (option.permission === "allow-chat") {
        void get().refreshApproval(chat);
        setTimeout(() => void get().refreshApproval(chat), 600);
      }
    },

    async refreshApproval(chat = get().activeId ?? undefined) {
      if (!client || !chat || runtime(chat)?.status !== "ready") return;
      const approval = await client.request<ApprovalState>(chat, "get_approval_state").catch((error) => {
        approvalRequestFailed(chat, "get_approval_state", error);
        return undefined;
      });
      if (approval) update(chat, (rt) => ({ state: { ...rt.state, approval } }));
    },

    async clearApprovalGrants(chat) {
      if (!client || runtime(chat)?.status !== "ready") return;
      await client.request(chat, "clear_approval_grants").catch((error) => approvalRequestFailed(chat, "clear_approval_grants", error));
      await get().refreshApproval(chat);
    },

    async loadContextBreakdown() {
      const chat = get().activeId;
      if (!client || !chat || runtime(chat)?.status !== "ready") return;
      const breakdown = await client.request<ContextBreakdown>(chat, "get_context_breakdown").catch(() => undefined);
      if (breakdown) update(chat, () => ({ contextBreakdown: breakdown, contextBreakdownAt: Date.now() }));
    },

    async compact() {
      const chat = get().activeId;
      if (!client || !chat) return;
      const before = runtime(chat)?.state?.context?.tokens;
      const setCompacting = (on: boolean) =>
        set((s) => ({ compactingChats: { ...s.compactingChats, [chat]: on } }));
      setCompacting(true);
      // The engine accepts `compact` at once and summarises in its run slot,
      // ending with `agent_settled` (`compact: true`). Wait for that, or the
      // numbers read next are still the ones from before.
      const settled = new Promise<EngineMessage | null>((resolve) => {
        compactWaiters.get(chat)?.(null);
        compactWaiters.set(chat, resolve);
        setTimeout(() => {
          if (compactWaiters.get(chat) === resolve) compactWaiters.delete(chat);
          resolve(null);
        }, COMPACT_TIMEOUT_MS);
      });
      try {
        await client.request(chat, "compact");
      } catch (error) {
        compactWaiters.get(chat)?.(null);
        compactWaiters.delete(chat);
        setCompacting(false);
        notify({ source: "engine", level: "error", title: "Could not compact the conversation", body: String(error) });
        return;
      }
      const result = await settled;
      setCompacting(false);
      await Promise.all([get().refreshState(chat), get().loadContextBreakdown()]);
      const after = runtime(chat)?.state?.context?.tokens;
      const tokens = (n: number | undefined) => (n == null ? "?" : `${Math.round(n / 100) / 10}k`);
      switch (result?.outcome) {
        case "compacted":
          notify({ source: "engine", level: "success", title: "Conversation compacted", body: `Context ${tokens(before)} → ${tokens(after)} tokens.` });
          break;
        case "declined":
          notify({ source: "engine", title: "Nothing to compact yet", body: "The conversation was just compacted, or is too short to summarise." });
          break;
        case "aborted":
          notify({ source: "engine", title: "Compaction stopped" });
          break;
        case "failed":
          notify({ source: "engine", level: "error", title: "Compaction failed", body: String(result.error ?? "unknown error") });
          break;
        default:
          if (!result) log("warn", "engine", "compaction did not report its outcome", { chatId: chat });
      }
    },

    async request<T>(type: string, payload?: Record<string, unknown>) {
      const chat = get().activeId;
      if (!client || !chat) throw new Error("The engine is not running");
      const result = await client.request<T>(chat, type, payload);
      if (PROVIDER_COMMANDS.has(type)) broadcastProviders(chat);
      return result;
    },

    async refreshState(chat = get().activeId ?? undefined) {
      if (!client || !chat || !runtime(chat)) return;
      const [state, models, levels] = await Promise.all([
        client.request<EngineState>(chat, "get_state").catch(() => runtime(chat)?.state),
        client.request<{ models: ModelDescriptor[] }>(chat, "get_available_models").catch(() => undefined),
        client.request<{ levels: string[] }>(chat, "get_available_thinking_levels").catch(() => undefined),
      ]);
      const next = state ?? {};
      update(chat, () => ({ state: next }));
      const modelId = next.modelId ?? next.model;
      // Without a saved default, the model a fresh chat starts on is the
      // engine's automatic pick (a reopened chat keeps its own model).
      if (!get().defaultSaved && !get().defaultModel && isDraft(chat) && next.provider && modelId) {
        set({ defaultModel: `${next.provider}/${modelId}` });
      }
      // Every engine reads the same catalog; the active one's answer is shown.
      if (chat === get().activeId) {
        set((store) => ({
          models: models?.models ?? store.models,
          thinkingLevels: levels?.levels ?? store.thinkingLevels,
        }));
      }
    },
  };
});

// Development: a new copy of this module would start with no engine client and
// no chats while the engines keep running under the old one (the window then
// shows an empty chat and clicks do nothing). Reload the window instead; it
// picks the running engines up again.
if (import.meta.hot) import.meta.hot.accept(() => window.location.reload());
