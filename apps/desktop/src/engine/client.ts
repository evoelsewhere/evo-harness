// Bridge to the engine processes managed by the Tauri shell — one per chat.
//
// The shell emits every engine stdout line as `engine://message`, stderr as
// `engine://stderr` and process exit as `engine://exit`, each tagged with the
// chat and the generation of the process that produced it. Restarting a
// chat's engine bumps its generation, so events from an older process are
// dropped here.
//
// The shell decides which engines run and sets each one up (model, tools,
// the hub's mail): a window asks for a chat's engine with `engine_ensure` and
// hears `engine://state` when it is set up
// (docs/features/shell-hosted-engines/spec.md, FR-1). A window is a viewer:
// it attaches to the chat it shows and detaches when it lets go; engines run
// on while their chats work, and idle ones are stopped by the shell (FR-2,
// FR-3). The shell keeps every chat's open questions (FR-9).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isMesh, meshCommand } from "./mesh";
import type { EngineMessage, ResponseMessage } from "./protocol";

export interface EngineInfo {
  binary: string;
  exists: boolean;
  extra_args: string[];
}

type Pending = {
  chat: string;
  resolve: (value: any) => void;
  reject: (error: Error) => void;
};

/**
 * `engine://state`: the shell started a chat's engine, finished setting it
 * up, stopped it as idle, or the last window that showed it closed.
 */
export interface EngineStateEvent {
  chat: string;
  generation: number;
  status: "starting" | "ready" | "stopped";
  /**
   * Why: started for a window, for mail, after a crash with mail waiting,
   * again for a changed sandbox (`settings`), for a scheduled run
   * (`schedule`); stopped with no work and no window beyond the idle limit
   * (`idle`); still running after its last window closed (`detached`).
   */
  reason?: "viewer" | "mail" | "restart" | "idle" | "detached" | "settings" | "schedule" | null;
  /** The engine reopens a saved session (its transcript is there to load). */
  restore?: boolean;
}

/** A question that waits for the person, kept by the shell per chat (FR-9). */
export interface OpenQuestion {
  requestId: string;
  /** The engine's (`ui_dialog_request`) or one of the shell's tools'. */
  source: "engine" | "shell";
  kind: "permission" | "questions" | "choice";
  title: string;
  body?: string;
  options: string[];
  /** An engine question as the engine asked it. */
  ui?: Record<string, unknown>;
  toolCallId?: string;
  /** A shell question: the engine's request id of the tool call that asks. */
  callId?: string;
  at: number;
}

/** What a window draws at once when it shows a chat (`engine_attach`). */
export interface AttachState {
  generation: number | null;
  status: "starting" | "ready" | "stopped";
  /** A run, working agents, reports or mail waiting, or an open question. */
  working: boolean;
  questions: OpenQuestion[];
}

/** What `engine_ensure` did: started a new process, or found one running. */
export interface Ensured {
  generation: number;
  started: boolean;
}

export interface EngineHandlers {
  onMessage: (chat: string, message: EngineMessage) => void;
  onStderr: (chat: string, line: string) => void;
  onExit: (chat: string, code: number | null) => void;
  onState?: (state: EngineStateEvent) => void;
}

/** Every window hears every engine's events; ids carry the window's own prefix. */
const ID_PREFIX = `${Math.random().toString(36).slice(2, 8)}-`;
/** The shell's own requests to an engine (its set-up): their answers are not for windows. */
const SHELL_ID_PREFIX = "shell-";

export class EngineClient {
  private generations = new Map<string, number>();
  private nextId = 1;
  private pending = new Map<string, Pending>();
  private unlisten: UnlistenFn[] = [];

  constructor(private handlers: EngineHandlers) {}

  async attach(): Promise<void> {
    this.unlisten.push(
      await listen<{ chat: string; generation: number; message: EngineMessage }>("engine://message", ({ payload }) => {
        if (!this.accept(payload.chat, payload.generation)) return;
        this.dispatch(payload.chat, payload.message);
      }),
      await listen<{ chat: string; generation: number; line: string }>("engine://stderr", ({ payload }) => {
        if (!this.accept(payload.chat, payload.generation)) return;
        this.handlers.onStderr(payload.chat, payload.line);
      }),
      await listen<{ chat: string; generation: number; code: number | null }>("engine://exit", ({ payload }) => {
        if (payload.generation !== this.generations.get(payload.chat)) return;
        this.failPending(payload.chat, "engine exited");
        this.handlers.onExit(payload.chat, payload.code);
      }),
      await listen<EngineStateEvent>("engine://state", ({ payload }) => {
        // The shell replaced the process (a sandbox change): what was asked
        // of the old one gets no answer.
        const known = this.generations.get(payload.chat);
        if (payload.status === "starting" && known !== undefined && payload.generation > known) {
          this.failPending(payload.chat, "engine restarted");
        }
        if (!this.accept(payload.chat, payload.generation)) return;
        this.handlers.onState?.(payload);
      }),
    );
  }

  detach(): void {
    for (const stop of this.unlisten.splice(0)) stop();
  }

  info(): Promise<EngineInfo> {
    return invoke<EngineInfo>("engine_info");
  }

  /** The chats whose engine runs now. */
  running(): Promise<string[]> {
    return invoke<string[]>("engine_running");
  }

  /**
   * Make sure `chat`'s engine runs (the shell starts it in `cwd` when none
   * does); `restart` replaces a running one. `restore`: the engine reopens the
   * saved session. `options` is what a draft was set to; for a saved chat the
   * shell reads its row.
   */
  async ensure(
    chat: string,
    { cwd, options, restore, restart }: { cwd?: string; options?: { memoryOff?: boolean; incognito?: boolean }; restore: boolean; restart: boolean },
  ): Promise<Ensured> {
    if (restart) this.failPending(chat, "engine restarted");
    // Events of the new process can arrive before `invoke` returns; `accept`
    // adopts the newer generation as soon as one shows up.
    const ensured = await invoke<Ensured>("engine_ensure", {
      chat,
      cwd: cwd ?? null,
      options: options ?? null,
      restore,
      restart,
    });
    this.generations.set(chat, Math.max(this.generations.get(chat) ?? 0, ensured.generation));
    return ensured;
  }

  /** This window shows `chat` now (it shows one chat at a time). */
  attachChat(chat: string): Promise<AttachState> {
    return invoke<AttachState>("engine_attach", { chat });
  }

  /** This window no longer shows `chat`; its engine goes on while it works. */
  async detachChat(chat: string): Promise<void> {
    await invoke("engine_detach", { chat });
  }

  /**
   * `held`: mail must not start `chat`'s engine (it is stopped around a
   * worktree Apply); `false`: done, mail that waited goes out.
   */
  async hold(chat: string, held: boolean): Promise<void> {
    await invoke("engine_hold", { chat, held });
  }

  /**
   * This window's composer holds a draft for `chat` (`composing`), or no
   * longer does; `chat` null and false: none of this window's does. A draft
   * holds the chat's automated wakes (shell-hosted-engines FR-5).
   */
  async setComposing(chat: string | null, composing: boolean): Promise<void> {
    await invoke("engine_set_composing", { chat, composing });
  }

  /** Every question that waits in `chat`. */
  questions(chat: string): Promise<OpenQuestion[]> {
    return invoke<OpenQuestion[]>("engine_questions", { chat });
  }

  async stop(chat: string): Promise<void> {
    this.failPending(chat, "engine stopped");
    this.generations.delete(chat);
    await invoke("engine_stop", { chat });
  }

  /** Send a command to `chat`'s engine and resolve with its `data` when the response arrives. */
  request<T = any>(chat: string, type: string, payload: Record<string, unknown> = {}): Promise<T> {
    const id = ID_PREFIX + String(this.nextId++);
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { chat, resolve, reject });
      invoke("engine_send", { chat, message: { ...payload, id, type } }).catch((error) => {
        this.pending.delete(id);
        reject(error instanceof Error ? error : new Error(String(error)));
      });
    });
  }

  /**
   * Send a session-mesh command (`subscribe`, `list`, `get`, `cancel`) to
   * `chat`'s engine and resolve with the body of its `res` envelope.
   */
  mesh<T = any>(chat: string, op: string, body: Record<string, unknown> = {}): Promise<T> {
    const id = ID_PREFIX + String(this.nextId++);
    const envelope = meshCommand(id, `client:${ID_PREFIX.slice(0, -1)}`, op, body);
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { chat, resolve, reject });
      invoke("engine_send", { chat, message: envelope }).catch((error) => {
        this.pending.delete(id);
        reject(error instanceof Error ? error : new Error(String(error)));
      });
    });
  }

  private accept(chat: string, generation: number): boolean {
    const current = this.generations.get(chat) ?? 0;
    if (generation < current) return false;
    this.generations.set(chat, generation);
    return true;
  }

  private dispatch(chat: string, message: EngineMessage): void {
    // A mesh answer correlates by `corr`; its error is `body.error`.
    if (isMesh(message) && message.kind === "res" && typeof message.corr === "string") {
      const pending = this.pending.get(message.corr);
      if (pending && pending.chat === chat) {
        this.pending.delete(message.corr);
        const error = message.body?.error;
        if (error) pending.reject(new Error(`${error.code}: ${error.message}`));
        else pending.resolve(message.body ?? null);
        return;
      }
    }
    if (message.type === "response" && typeof message.id === "string") {
      if (message.id.startsWith(SHELL_ID_PREFIX)) return;
      const pending = this.pending.get(message.id);
      if (pending && pending.chat === chat) {
        this.pending.delete(message.id);
        const response = message as ResponseMessage;
        if (response.success) pending.resolve(response.data ?? null);
        else pending.reject(new Error(response.error ?? `${response.command} failed`));
        return;
      }
    }
    this.handlers.onMessage(chat, message);
  }

  private failPending(chat: string, reason: string): void {
    for (const [id, pending] of this.pending) {
      if (pending.chat !== chat) continue;
      this.pending.delete(id);
      pending.reject(new Error(reason));
    }
  }
}
