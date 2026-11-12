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
import { EngineClient, type EngineStateEvent, type OpenQuestion } from "@/engine/client";
import {
  applyMeshEvent,
  childRunOf,
  emptyView,
  isMesh,
  MESH_VERSION,
  type MeshEnvelope,
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
  TurnTotals,
} from "@/engine/protocol";
import {
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
import { db, log, type SessionRole } from "@/lib/db";
import { chatWindowId, closeChatWindow, focusChatWindow, openChatWindow } from "@/lib/windows";
import { worktrees } from "@/lib/worktrees";
import { APPROVAL_CHANGED, useUiStore, worktreeKey } from "@/stores/ui-store";
import { emitOfficeFileChanged } from "@/stores/office-events";
import { useConnectorsStore } from "@/stores/connectors-store";
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
  /** Last time the chat was shown. */
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
    options: {
      title: string;
      prompt: string;
      context?: "fresh" | "fork";
      worktree?: boolean;
      /** The resolved role and branch the proposal carries (FR-1, FR-9). */
      role?: SessionRole | null;
      branch?: string | null;
    },
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
let client: EngineClient | null = null;

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
    update(chat, (rt) => ({ transcript: transcriptReducer(rt.transcript, action) }));
  };

  /**
   * Chats whose engine said `ready` without `turnTotals` (a stale sidecar):
   * the shell cannot record their turns, so this window does
   * (docs/features/shell-hosted-engines/spec.md, FR-6; plan 3.3).
   */
  const noTurnTotals = new Set<string>();

  /**
   * Count a finished turn in the chat's totals, and flag it if nobody saw it
   * finish. The shell records every turn of an engine that gives turn
   * totals (src-tauri/src/supervisor/turns.rs, FR-6); this is the fallback
   * for one that does not.
   */
  const recordFinishedTurn = (chat: string) => {
    if (!noTurnTotals.has(chat)) return;
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
    // "Reply ready" and "A run failed" are the shell's to announce, once
    // (src-tauri/src/supervisor/notices.rs; shell-hosted-engines FR-10).
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

  // Settings reach every engine from the shell, viewed or not: the approval
  // mode, providers, resources (skills, plugins, memory) and the sandbox, with
  // the hub's and the engines' limits (src-tauri/src/supervisor/settings.rs,
  // docs/features/shell-hosted-engines/spec.md, FR-8). It says
  // `engine://reloaded` when an engine follows a change; a sandbox restart
  // comes as `engine://state` reason `settings`.

  // Idle engines are stopped by the shell, over the whole app: those with no
  // work and no window beyond its limit (src-tauri/src/supervisor.rs,
  // docs/features/shell-hosted-engines/spec.md, FR-3). It says so with
  // `engine://state` `stopped` (`onState`).

  /** Forget what this window keeps of `chat`'s engine. */
  const dropRuntime = (chat: string) => {
    pendingDeltas.delete(chat);
    forgetEngine(chat);
    pausedEngines.delete(chat);
    set((store) => {
      const runtimes = { ...store.runtimes };
      delete runtimes[chat];
      return { runtimes };
    });
  };

  const stopChat = async (chat: string) => {
    dropRuntime(chat);
    void client?.detachChat(chat).catch(() => undefined);
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
    void client?.detachChat(chat).catch(() => undefined);
  };

  /**
   * Show `chat`'s engine in this window with a fresh runtime: the shell starts
   * it in `cwd` when none runs, and sets it up (docs/features/shell-hosted-engines/spec.md,
   * FR-1). With `attach`, an engine that already runs (handed over by another
   * window, kept across a reload, or started by the shell for its mail) is
   * picked up as it is; without, it is restarted.
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
      if (!client) return;
      // A draft has no row yet: the shell is told what it was set to.
      const row = useSessionStore.getState().sessions.find((s) => s.id === chat);
      // The shell keeps every chat's connector switches (`connectors.chatOff`).
      const ensured = await client.ensure(chat, {
        cwd,
        options: row ? { memoryOff: row.memoryOff, incognito: row.incognito } : undefined,
        restore,
        restart: !attach,
      });
      if (ensured.started) return;
      update(chat, () => ({ status: "ready", error: undefined }));
      // The shell may still be setting it up: prompts wait for it.
      trackSetup(chat);
      // No `ready` comes from an engine that already runs: subscribe here,
      // before the transcript loads, so its replay can be drawn into it.
      await subscribeMesh(chat);
      if (restore) await restoreTranscript(chat);
      void loadDefault(chat);
      void get().refreshState(chat);
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

  /**
   * Show `chat`, starting its engine if it has none. This window attaches to
   * it: the shell keeps a shown chat's engine (FR-2), and hands over its open
   * questions (FR-9).
   */
  const activate = async (chat: string, cwd: string | undefined, restore: boolean) => {
    flushDeltas();
    set((store) => ({ activeId: chat, ...mirror(store.runtimes[chat]) }));
    const attached = await client?.attachChat(chat).catch(() => undefined);
    if (get().runtimes[chat]) {
      update(chat, () => ({ lastActive: Date.now() }));
      for (const question of attached?.questions ?? []) showQuestion(chat, question);
    } else await launch(chat, cwd, restore, true);
  };

  /**
   * Restore the saved messages once a reopened chat's engine is ready.
   * `engineStarted`: the engine just sent `ready` (a new process: none of its
   * background agents run); `lostAgents` are the ids it reported lost (FR-12).
   */
  const restoreTranscript = async (chat: string, engineStarted = false, lostAgents?: string[]) => {
    if (!client) return;
    try {
      const [data, state, questions] = await Promise.all([
        client.request<{ messages: AgentMessage[] }>(chat, "get_messages", { limit: 2000 }),
        client.request<{ activeOperation?: unknown }>(chat, "get_state").catch(() => undefined),
        // The engine does not ask again: the shell keeps what waits (FR-9).
        client.questions(chat).catch((): OpenQuestion[] => []),
      ]);
      // The engine may still be working (the window reloaded mid-run, or
      // another window handed the chat over): its last turn then goes on, live.
      const transcript = transcriptFromMessages((data?.messages ?? []) as (AgentMessage & Record<string, any>)[], {
        running: Boolean(state?.activeOperation),
        engineStarted,
        lostAgents,
      });
      update(chat, (rt) => ({
        transcript,
        restore: false,
        error: undefined,
        recordedTurn: state?.activeOperation ? rt.recordedTurn : (transcript.turns.at(-1)?.id ?? rt.recordedTurn),
      }));
      // The questions it waits on, after a reload or from another window too.
      for (const question of questions) showQuestion(chat, question);
      // Mesh events may have arrived before the transcript existed.
      redrawAgents(chat);
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
    meshViews.delete(chat);
    engineSetup.delete(chat);
    syncAgentsRunning(chat);
  };

  /** Chats whose engine `stopEngine` stopped; their runtime stays, status `idle`. */
  const pausedEngines = new Set<string>();
  /** Chats `leaveWorktree` is moving back to their workspace folder. */
  const leavingWorktree = new Set<string>();

  const onMesh = (chat: string, envelope: MeshEnvelope) => {
    const view = meshViews.get(chat) ?? emptyView();
    meshViews.set(chat, view);
    const id = envelope.op === "node_state" ? (envelope.body?.node?.id as string | undefined) : undefined;
    const changed = applyMeshEvent(view, envelope);
    if (!changed) return;
    const node = view.nodes.get(changed);
    // The session node lists what waits to be read: the shell wakes the chat
    // for it (src-tauri/src/supervisor/wakes.rs, shell-hosted-engines FR-5).
    if (node?.kind === "session") return;
    drawAgent(chat, view, changed);
    if (!node || changed !== id) return;
    syncAgentsRunning(chat);
  };

  // ── Background subagents ────────────────────────────────────────────────
  //
  // A background `agent` call returns at once; the subagent keeps running in
  // the chat's engine (crates/evo-engine/src/tools/agent.rs). When it finishes
  // while the chat is idle, its report is queued for the chat's next run and
  // the engine lists it in the session node's `meta.wake`. The shell wakes
  // the chat then, within the guard rails (FR-19; src-tauri/src/supervisor/wakes.rs,
  // shell-hosted-engines FR-5): this window only tells it when the composer
  // holds a draft (`setComposing`). The wake's turn comes as `mesh://prompted`.
  //
  // A subagent's usage counts toward a turn of its chat once (FR-6): the
  // engine adds it to the turn's totals (`agent_settled.turn`), and the shell
  // records them. A foreground run's usage also rides on its tool result,
  // which the transcript shows; a background run's is shown from the totals
  // when the turn settles (`showChildUsage`).

  /** Show the part of a settled turn's totals the transcript did not count (background children). */
  const showChildUsage = (chat: string, message: EngineMessage) => {
    const turn = message.turn as TurnTotals | undefined;
    const shown = runtime(chat)?.transcript.turns.at(-1)?.meta;
    if (!turn || !shown) return;
    const tokens = (m: { inputTokens?: number; outputTokens?: number; cacheRead?: number; cacheWrite?: number }) =>
      (m.inputTokens ?? 0) + (m.outputTokens ?? 0) + (m.cacheRead ?? 0) + (m.cacheWrite ?? 0);
    const cost = turn.costUsd + turn.childCostUsd - (shown.cost ?? 0);
    const more = turn.input + turn.output + turn.cacheRead + turn.cacheWrite + turn.childTokens - tokens(shown);
    if (cost <= 1e-12 && more <= 0) return;
    dispatch(chat, { type: "extra-usage", usage: { input: Math.max(0, more), cost: { total: Math.max(0, cost) } } });
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

  /**
   * A chat's setup after its engine is `ready`. The shell sets the engine up
   * (its model, its tools, the hub's mail; src-tauri/src/supervisor.rs) and
   * says so with `engine://state` `ready`. A prompt sent before that would
   * start a run, and the engine refuses to change external tools while a run
   * is active: the chat would lack its session, browser and connector tools.
   * Prompts and model picks from this window wait for it, at most SETUP_WAIT_MS.
   */
  const engineSetup = new Map<string, Promise<unknown>>();
  const SETUP_WAIT_MS = 10_000;
  const setupDone = (chat: string): Promise<unknown> => {
    const setup = engineSetup.get(chat);
    if (!setup) return Promise.resolve();
    return Promise.race([setup.catch(() => undefined), new Promise((resolve) => setTimeout(resolve, SETUP_WAIT_MS))]);
  };
  /** What the shell last said of each chat's engine (`engine://state`). */
  const shellStatus = new Map<string, EngineStateEvent["status"]>();
  /** Who waits for the shell to finish setting a chat's engine up. */
  const setupWaiters = new Map<string, (() => void)[]>();
  /** Resolves once the shell has set `chat`'s engine up (or at once when it has). */
  const shellSetUp = (chat: string): Promise<void> =>
    shellStatus.get(chat) === "ready"
      ? Promise.resolve()
      : new Promise((resolve) => setupWaiters.set(chat, [...(setupWaiters.get(chat) ?? []), resolve]));
  /** Prompts wait while the shell sets up an engine this window picked up running. */
  const trackSetup = (chat: string) => {
    if (shellStatus.get(chat) !== "starting") return;
    const setup = shellSetUp(chat);
    engineSetup.set(chat, setup);
    void setup.finally(() => {
      if (engineSetup.get(chat) === setup) engineSetup.delete(chat);
    });
  };

  /** Tell the shell which connectors are switched off for a chat (it builds and gates the connector tools). */
  const pushConnectorSwitches = (chat: string) =>
    invoke("agent_mcp_chat_config", { chat, off: useUiStore.getState().chatConnectorsOff[chat] ?? [] }).catch(() => undefined);

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

  /**
   * The main window keeps a runtime for an engine the shell started for
   * mail or a scheduled run, as it did when it started that engine itself.
   * The shell wakes it, records its turns and sends a scheduled task's first
   * prompt as hub mail (docs/features/shell-hosted-engines, FR-5 – FR-7); the
   * runtime is for recording the turns of an engine without turn totals. It
   * only watches: its `ready` is handled like any other.
   */
  const adopt = (chat: string, restore: boolean) =>
    set((store) => {
      if (store.runtimes[chat]) return store;
      const rt: ChatRuntime = { chatId: chat, status: "starting", cwd: undefined, state: {}, transcript: initialTranscript, lastActive: Date.now(), restore };
      return { runtimes: { ...store.runtimes, [chat]: rt } };
    });

  /** The engines that run with no runtime here (the shell hosted them for mail before this window loaded). */
  const adoptRunning = async () => {
    if (!client || chatWindowId) return;
    const running = await client.running().catch((): string[] => []);
    const saved = new Set(useSessionStore.getState().sessions.filter((s) => !s.draft).map((s) => s.id));
    for (const chat of running) {
      if (!runtime(chat) && saved.has(chat)) await launch(chat, undefined, true, true);
    }
  };

  /**
   * The shell started `chat`'s engine or finished setting it up. It starts
   * engines on its own too: a session it created, mail for a chat, a crashed
   * engine with mail waiting (FR-4). A window that shows such a chat picks
   * the new process up and reloads its transcript when it is ready.
   */
  const onState = ({ chat, status, reason, restore }: EngineStateEvent) => {
    shellStatus.set(chat, status);
    if (status === "ready") {
      for (const resolve of setupWaiters.get(chat) ?? []) resolve();
      setupWaiters.delete(chat);
    }
    const rt = runtime(chat);
    if (reason === "detached") {
      // The window that showed it closed while it runs (FR-2): the main
      // window follows its runs, as it does any chat's engine the shell runs.
      const saved = useSessionStore.getState().sessions.some((s) => s.id === chat && !s.draft);
      if (!rt && !chatWindowId && saved) void launch(chat, undefined, true, true);
      return;
    }
    if (status === "ready") return;
    if (status === "stopped") {
      // The shell stopped it: no work and no window, beyond the idle limit (FR-3).
      if (!rt || pausedEngines.has(chat)) return;
      // Shown here after all (the stop crossed this window's attach): start it again.
      if (chat === get().activeId) void launch(chat, rt.cwd, !isDraft(chat), true);
      else dropRuntime(chat);
      return;
    }
    if (!rt) {
      if (!chatWindowId && (reason === "mail" || reason === "restart" || reason === "schedule")) adopt(chat, restore ?? true);
      return;
    }
    if (reason === "settings") {
      // The shell started it again with a changed sandbox (FR-8): what this
      // window knew of the old process is void; the new one's `ready` reloads it.
      forgetEngine(chat);
      pausedEngines.delete(chat);
      update(chat, () => ({ status: "starting", restore: !isDraft(chat), error: undefined }));
      return;
    }
    if (rt.status === "starting" || rt.status === "ready") return;
    forgetEngine(chat);
    pausedEngines.delete(chat);
    update(chat, () => ({ status: "starting", restore: !isDraft(chat), error: undefined }));
  };

  // Questions the shell's tools raise (a connector tool set to "ask", the
  // browser's gate), shown like the engine's permission prompts;
  // `answerPermission` resolves them.
  const appApprovals = new Map<string, { chat: string; externalId: string; resolve(answer: number): void }>();

  /**
   * Draw a question the shell keeps for `chat` (docs/features/shell-hosted-engines,
   * FR-9): one of its tools' (as an approval prompt), or the engine's, as the
   * engine asked it. Drawing one again replaces it.
   */
  const showQuestion = (chat: string, question: OpenQuestion) => {
    if (question.source === "shell") {
      const id = `shell:${question.requestId}`;
      appApprovals.set(id, {
        chat,
        externalId: question.callId ?? "",
        resolve: (index) => void invoke("agent_ask_answer", { requestId: question.requestId, index }).catch(() => undefined),
      });
      dispatch(chat, {
        type: "engine",
        message: {
          type: "ui_dialog_request",
          requestId: id,
          ui: { title: question.title, description: question.body ?? "", options: question.options.map((title) => ({ title })) },
        },
      });
      return;
    }
    dispatch(chat, {
      type: "engine",
      message: {
        type: "ui_dialog_request",
        requestId: question.requestId,
        toolCallId: question.toolCallId,
        ui: question.ui ?? { title: question.title, description: question.body, options: question.options.map((title) => ({ title })) },
      },
    });
  };
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
      // Without turn totals the shell records no turn of this engine: this window does (FR-6).
      if (message.turnTotals === true) noTurnTotals.delete(chat);
      else noTurnTotals.add(chat);
      if (message.mesh?.version === MESH_VERSION) void subscribeMesh(chat);
      else log("warn", "engine", "this engine does not speak the session mesh: subagents will not update live", { chatId: chat });
      // The background agents of an earlier engine that never reported (FR-12).
      const lost = Array.isArray(message.lostBackgroundAgents)
        ? message.lostBackgroundAgents.filter((id: unknown): id is string => typeof id === "string")
        : undefined;
      const setup = (async () => {
        // `ready` comes from a new process: an earlier engine's background agents are gone.
        if (runtime(chat)?.restore) await restoreTranscript(chat, true, lost);
        // The shell sets the engine up: the chat's model (or the default),
        // its tools, then the hub's mail (FR-1).
        await Promise.race([shellSetUp(chat), new Promise((resolve) => setTimeout(resolve, SETUP_WAIT_MS))]);
        void loadDefault(chat);
        await get().refreshState(chat);
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
      // swapped its provider runtimes; the shell has the others reload
      // (`engine://reloaded`), and the UI refreshes.
      void get().refreshState(chat);
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
    dispatch(chat, { type: "engine", message });
    if (message.type === "agent_settled" && message.compact !== true) showChildUsage(chat, message);
    if (message.type === "agent_settled" && message.compact === true) {
      compactWaiters.get(chat)?.(message);
      compactWaiters.delete(chat);
    }
    if (message.type === "agent_settled" || message.type === "agent_end") {
      recordFinishedTurn(chat);
      // Context usage changes after every run.
      void get().refreshState(chat);
      if (chat === get().activeId) void get().loadContextBreakdown();
    }
    // An approval or a question in a chat nobody looks at is the shell's to
    // announce, once (FR-9).
  };

  const onExit = (chat: string, code: number | null) => {
    shellStatus.delete(chat);
    const hadAgents = hasRunningAgents(chat);
    const rt = runtime(chat);
    forgetEngine(chat);
    pausedEngines.delete(chat);
    if (!rt) {
      trackElsewhere(chat, false);
      return;
    }
    const wasBusy = rt.transcript.busy;
    update(chat, () => ({ status: "exited", error: code ? `Engine exited with code ${code}` : "The engine stopped" }));
    // Also for an idle chat whose subagents were working: the transcript settles them.
    // The shell announces it when no focused window shows the chat (FR-10);
    // a window that shows it has the banner.
    if (wasBusy || hadAgents) dispatch(chat, { type: "fail", error: "The engine stopped" });
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
        onState,
      });
      await client.attach();
      // A reloaded window's composers hold no draft: wakes it held may go (FR-5).
      void client.setComposing(null, false).catch(() => undefined);
      // Start sessions without asking and the connector switches are app
      // settings the shell follows; an older build's local values move into
      // them once (shell-hosted-engines FR-8, NFR-4).
      await useUiStore.getState().loadShellSettings();
      // The approval mode is an app setting: read it before any engine starts
      // (an older build's local value moves into it), and show changes from
      // any window. The shell tells every engine (FR-8).
      await useUiStore.getState().loadApproval();
      void listen<{ mode?: ApprovalMode }>(APPROVAL_CHANGED, ({ payload }) => {
        const mode = payload?.mode;
        if (mode !== "ask" && mode !== "auto-edits" && mode !== "auto") return;
        useUiStore.getState().syncApproval(mode);
      });
      // An engine this window holds followed a changed setting: show its state.
      void listen<{ chat: string; what: string }>("engine://reloaded", ({ payload }) => {
        if (!runtime(payload.chat)) return;
        if (payload.what === "approval") void get().refreshApproval(payload.chat);
        else if (payload.what === "providers") void get().refreshState(payload.chat);
      });
      void useConnectorsStore.getState().load();
      // Plugins were installed, removed or switched: every ready chat re-reads
      // its skills, commands, subagents and hooks.
      // The shell's supervisor hosts what the mesh hub needs (a new nested
      // session, waiting mail, at app start too) and sets every engine's
      // tools up again when they change: no window does (FR-1, FR-4).
      // The hub starts a turn of a chat this window may show (a nested
      // session's brief, its parent's message): show it like a typed prompt.
      void listen<{ chat: string; text: string }>("mesh://prompted", ({ payload }) => {
        if (!runtime(payload.chat)) return;
        dispatch(payload.chat, { type: "prompt", text: payload.text });
      });
      // Open questions the shell keeps (FR-9). Its tools' questions (approvals)
      // show as the chat's approval prompt; the engine's come with its own line.
      // An answer from any window closes them in every window.
      void listen<{ chat: string; question: OpenQuestion }>("engine://question", ({ payload }) => {
        if (payload.question.source === "shell") showQuestion(payload.chat, payload.question);
      });
      void listen<{ chat: string; requestId: string }>("engine://question-closed", ({ payload }) => {
        const shell = `shell:${payload.requestId}`;
        const id = appApprovals.delete(shell) ? shell : payload.requestId;
        dispatch(payload.chat, { type: "engine", message: { type: "ui_dialog_closed", requestId: id } });
      });
      // A notification or the tray asks to show a chat (the shell showed this window).
      if (!chatWindowId) {
        void listen<{ chat: string }>("app://open-chat", ({ payload }) => {
          const ui = useUiStore.getState();
          if (ui.settingsOpen) ui.closeSettings();
          void get().selectSession(payload.chat);
        });
      }
      // Plugins changed: the shell has every engine re-read them (FR-8);
      // Customize shows the new lists.
      void listen("plugins://changed", () => {
        setTimeout(() => void useCustomizeStore.getState().load(true), 500);
      });
      const sessions = useSessionStore.getState();
      try {
        await sessions.load();
      } catch (error) {
        log("error", "db", `could not load chats: ${String(error)}`);
      }
      await adoptRunning();
      // A chat window shows its one chat.
      if (chatWindowId) {
        await get().selectSession(chatWindowId);
        return;
      }
      // A notification or the tray asked for a chat while there was no main
      // window: the shell built this one to show it (shell-hosted-engines FR-11).
      const pending = await invoke<string | null>("app_open_pending").catch(() => null);
      const known = (id: string) => useSessionStore.getState().sessions.some((s) => s.id === id && !s.draft);
      const last = pending && known(pending) ? pending : useSessionStore.getState().lastChatId;
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
      // The shell gives the chat's engine its new tools.
      await pushConnectorSwitches(chat);
    },

    async reloadIdleResources() {
      // The active chat's engine already reloaded; the shell has every other
      // engine re-read them, viewed or not (FR-8).
      await invoke("engine_reload_resources", { except: get().activeId }).catch((error) =>
        log("warn", "engine", `reloading skills failed: ${String(error)}`),
      );
    },

    async startChildSession(parentId, { title, prompt, context = "fresh", worktree = false, role, branch }, proposalId) {
      const parent = chatRow(parentId);
      if (!parent || parent.draft) return null;
      // The shell's mesh hub creates it, has a window host its engine and
      // sends it the brief (src-tauri/src/mesh.rs). A proposal starts once:
      // the hub answers a second Start with the same session.
      return invoke<string>("mesh_start_session", {
        parent: parentId,
        spec: { title, prompt, fork: context === "fork", worktree, role: role ?? undefined, branch: branch ?? undefined },
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
      // The shell holds the chat's wakes while a draft sits here (FR-5).
      void client?.setComposing(chat, on).catch(() => undefined);
    },

    async stopEngine(chat) {
      const rt = runtime(chat);
      if (rt?.transcript.busy || get().busyElsewhere[chat]) throw new Error("This chat is in the middle of a turn. Stop it first.");
      if (hasRunningAgents(chat)) throw new Error("This chat has subagents still working. Wait for them, or stop them first.");
      if (!client) return;
      if (!rt) {
        // Not shown here: another window may hold it, or nothing does (kept across a reload).
        const running = (await client.running().catch((): string[] => [])).includes(chat);
        if (running && (chatWindowId || (await focusChatWindow(chat)))) {
          throw new Error("This chat is open in another window. Apply or discard its worktree there.");
        }
        // Mail must not start it while its folder goes: the shell holds it
        // until `resumeEngine` or `leaveWorktree` (shell-hosted-engines plan 5).
        await client.hold(chat, true);
        if (running) await client.stop(chat);
        return;
      }
      pendingDeltas.delete(chat);
      forgetEngine(chat);
      pausedEngines.add(chat);
      update(chat, () => ({ status: "idle", error: undefined }));
      await client.hold(chat, true);
      // Resolves once the process is gone: only then is its folder free (Windows).
      await client.stop(chat);
    },

    async resumeEngine(chat) {
      // `leaveWorktree` starts it itself, in the workspace folder, and ends the hold.
      if (leavingWorktree.has(chat)) return;
      try {
        // Starting it ends the shell's hold.
        if (pausedEngines.has(chat) && runtime(chat)) await launch(chat, await chatFolder(chat), !isDraft(chat));
      } finally {
        // Nothing here starts it: mail may again, and what waited goes out.
        await client?.hold(chat, false).catch(() => undefined);
      }
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
        // Mail may start it again (in its workspace folder now); what waited goes out.
        await client?.hold(id, false).catch(() => undefined);
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
      // After the shell's set-up, which sets the chat's own model: the person's pick wins.
      if (runtime(chat)?.status === "starting") {
        await Promise.race([shellSetUp(chat), new Promise((resolve) => setTimeout(resolve, SETUP_WAIT_MS))]);
      }
      await setupDone(chat);
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
      // A provider command reaches the other engines through the shell (FR-8).
      return client.request<T>(chat, type, payload);
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
