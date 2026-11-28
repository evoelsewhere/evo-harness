// The app database on this machine (`~/.evo/agent/evo.db`), through the shell.
//
// Workspaces, chats, shared settings and the run log live there (the shell's
// `db` commands over the `evo-store` crate); each chat's transcript is written
// into the same file by its engine.

import { invoke } from "@tauri-apps/api/core";
import { chatWindowId, matesWindow } from "@/lib/windows";

export interface DbWorkspace {
  id: string;
  path: string;
  name: string;
  collapsed: boolean;
  position: number;
  createdAt: number;
  openedAt: number;
}

export interface DbChat {
  id: string;
  workspaceId: string | null;
  title: string;
  titleSource: "auto" | "user";
  preview: string | null;
  provider: string | null;
  model: string | null;
  pinned: boolean;
  unread: boolean;
  archivedAt: number | null;
  turnCount: number;
  costUsd: number;
  tokens: number;
  createdAt: number;
  updatedAt: number;
  /** The chat that started this one (nested sessions). */
  parentChatId: string | null;
  /** `agent` when another chat's agent started it. */
  origin: "user" | "agent";
  /** A managed worktree the chat works in instead of its workspace folder. */
  worktreePath: string | null;
  /** The chat uses no memory, saves none and cannot search past chats. */
  memoryOff: boolean;
  /** As `memoryOff`, and the chat leaves no history: it is removed when it ends. */
  incognito: boolean;
  /** The Scheduler started it. */
  scheduled: boolean;
  /** The role a nested session runs as (docs/features/session-roles-and-mail/spec.md, FR-1). */
  role?: SessionRole | null;
  /**
   * `chat` (Evo Harness) or an Evo Mates chat: `mate`, `channel` or `seat`
   * (docs/features/evo-mates/spec.md, FR-24). Missing from older rows: `chat`.
   */
  kind?: "chat" | "mate" | "channel" | "seat";
  /** The mate a `mate` or `seat` chat belongs to. */
  mateId?: string | null;
}

/** An agent type, model and thinking level a nested session runs as, resolved when it started. */
export interface SessionRole {
  name?: string | null;
  /** `project`, `projectDir`, `user`, `userDir`, `plugin` or `builtin`. */
  source?: string | null;
  sourcePath?: string | null;
  instructions?: string | null;
  tools?: string[] | null;
  access: "read-only" | "full";
  provider?: string | null;
  model?: string | null;
  thinking?: string | null;
}

/** "reviewer · acme-small · thinking low · read-only" for a card or a header. */
export function roleLabel(role: SessionRole): string {
  return [
    role.name,
    role.model,
    role.thinking ? `thinking ${role.thinking}` : null,
    role.access === "read-only" ? "read-only" : null,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** What a chat is started with, chosen before its first message. */
export interface ChatFlags {
  memoryOff?: boolean;
  incognito?: boolean;
}

/** The memory index worker in the shell (`src-tauri/src/memory_index.rs`). */
export interface MemoryIndexStatus {
  /** `off`: not switched on, or no model. `stopped`: it cannot go on; `message` says why. */
  state: "off" | "idle" | "running" | "stopped";
  chat: string | null;
  message: string | null;
  waiting: number;
}

/** How a pass someone asked for ended (`memory://index-pass`). */
export interface MemoryIndexPassReport {
  /** Chats an index run read. */
  read: number;
  /** Why a run failed; the worker waits a while before the next one. */
  failed: string | null;
}

export interface ChatPatch {
  title?: string;
  autoTitle?: string;
  pinned?: boolean;
  unread?: boolean;
  archived?: boolean;
  provider?: string;
  model?: string;
}

export interface TurnRecord {
  preview?: string;
  costUsd: number;
  tokens: number;
  provider?: string;
  model?: string;
  unread: boolean;
}

/** Which part of a chat made a model call (`crates/evo-store/src/usage.rs`). */
export type UsageSource = "chat" | "subagent" | "plugin" | "side" | "memory" | "mates";

/** Which recorded responses a usage query covers; every field narrows. */
export interface UsageQuery {
  fromMs?: number;
  toMs?: number;
  chatId?: string;
  providerId?: string;
  modelId?: string;
  source?: UsageSource;
  /** Time series bucket (default `day`). */
  bucket?: UsageBucket;
}

export type UsageBucket = "hour" | "day" | "week";

export interface UsageTotals {
  requests: number;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  reasoning: number;
  costInput: number;
  costOutput: number;
  costCacheRead: number;
  costCacheWrite: number;
  costUsd: number;
  /** Responses that failed or were stopped, and what they cost. */
  errors: number;
  costErrors: number;
  /** Responses with a known duration: their count, summed ms and output tokens. */
  timed: number;
  timedMs: number;
  timedOutput: number;
}

export interface UsageSummary {
  totals: UsageTotals;
  bucket: UsageBucket;
  /**
   * Buckets with usage, oldest first, one point per model: local
   * `YYYY-MM-DD HH` (hour) or `YYYY-MM-DD` (day; the Monday of a week).
   */
  series: (UsageTotals & { bucket: string; providerId: string; modelId: string })[];
  byModel: (UsageTotals & { providerId: string; modelId: string; lastMs: number })[];
  byChat: (UsageTotals & { chatId: string | null; chatTitle: string | null; lastMs: number })[];
  bySource: (UsageTotals & { source: UsageSource })[];
  /** Plugins' AI use (plugin-ai FR-15), most expensive first. */
  byPlugin?: (UsageTotals & { pluginId: string })[];
  /** Evo Mates: each mate's use, its direct chat and seats (docs/features/evo-mates/spec.md, FR-22). */
  byMate?: (UsageTotals & { mateId: string; name: string | null })[];
  /** The oldest response in the whole history. */
  firstMs: number | null;
}

export interface UsageEvent {
  id: number;
  atMs: number;
  chatId: string | null;
  chatTitle: string | null;
  source: UsageSource;
  providerId: string;
  modelId: string;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  reasoning: number;
  costUsd: number;
  stopReason: string | null;
  durationMs: number | null;
  /** The plugin of a plugin-AI response. */
  pluginId?: string | null;
}

export type LogLevel = "debug" | "info" | "warn" | "error";

export interface LogEntry {
  id: number;
  atMs: number;
  level: LogLevel;
  source: string;
  chatId: string | null;
  message: string;
  data: unknown;
}

export interface LogQuery {
  minLevel?: LogLevel;
  source?: string;
  chatId?: string;
  search?: string;
  beforeId?: number;
  limit?: number;
}

export interface DbInfo {
  path: string;
  dir: string;
  sizeBytes: number;
  schemaVersion: number;
  logs: { count: number; oldestMs: number | null; newestMs: number | null; sources: string[] };
  /** The folder of the chats' transcript files (one per chat), their total size and count. */
  transcriptsDir: string;
  transcriptsBytes: number;
  transcriptsCount: number;
}

export const db = {
  info: () => invoke<DbInfo>("db_info"),
  reveal: () => invoke<void>("db_reveal"),

  workspaces: () => invoke<DbWorkspace[]>("workspaces_list"),
  ensureWorkspace: (path: string) => invoke<DbWorkspace>("workspace_ensure", { path }),
  setWorkspaceCollapsed: (id: string, collapsed: boolean) => invoke<void>("workspace_set_collapsed", { id, collapsed }),
  removeWorkspace: (id: string) => invoke<string[]>("workspace_remove", { id }),

  // The Evo Mates window lists its mates' chats; a chat window also a mate's
  // seat in a channel, which it may show; every other window Evo Harness's.
  chats: (archived = false) =>
    invoke<DbChat[]>("chats_list", { archived, kind: matesWindow ? "mates" : chatWindowId ? "window" : null }),
  searchChats: (query: string) => invoke<DbChat[]>("chats_search", { query }),
  createChat: (chat: {
    id: string;
    workspaceId: string | null;
    title: string;
    provider?: string;
    model?: string;
    parentChatId?: string | null;
    origin?: "user" | "agent";
    worktreePath?: string | null;
    memoryOff?: boolean;
    incognito?: boolean;
    scheduled?: boolean;
  }) => invoke<DbChat>("chat_create", { chat }),
  /** The worktree a chat works in, or its workspace folder again (`null`). */
  setWorktree: (id: string, worktreePath: string | null) =>
    invoke<DbChat | null>("chat_set_worktree", { id, worktreePath }),
  /** Link a chat under another, or detach it (`null`). */
  setParent: (id: string, parentChatId: string | null) => invoke<DbChat | null>("chat_set_parent", { id, parentChatId }),
  updateChat: (id: string, patch: ChatPatch) => invoke<DbChat | null>("chat_update", { id, patch }),
  recordTurn: (id: string, turn: TurnRecord) => invoke<DbChat | null>("chat_record_turn", { id, turn }),
  deleteChat: (id: string) => invoke<void>("chat_delete", { id }),
  /** Copy `source` (folder, model, whole transcript) as chat `id`. */
  forkChat: (source: string, id: string, title: string) => invoke<DbChat | null>("chat_fork", { source, id, title }),
  /** Move a chat to another workspace (`null`: "This computer"). */
  moveChat: (id: string, workspaceId: string | null) => invoke<DbChat | null>("chat_move", { id, workspaceId }),

  settings: () => invoke<Record<string, unknown>>("settings_all"),
  setSetting: (key: string, value: unknown) => invoke<void>("settings_set", { key, value: value ?? null }),
  /** Hand settings an older build kept in a window's storage to the shell; it never writes over a stored value. */
  importWindowSettings: (values: { autoStartSessions?: boolean; chatConnectorsOff?: Record<string, string[]> }) =>
    invoke<void>("settings_import_window", { values }),

  queryLogs: (query: LogQuery) => invoke<LogEntry[]>("logs_query", { query }),
  clearLogs: () => invoke<number>("logs_clear"),
  exportLogs: (query: LogQuery) => invoke<string>("logs_export", { query }),

  usageSummary: (query: UsageQuery) => invoke<UsageSummary>("usage_summary", { query }),
  usageEvents: (query: UsageQuery, beforeId?: number, limit?: number) =>
    invoke<UsageEvent[]>("usage_events", { query, beforeId: beforeId ?? null, limit: limit ?? null }),
  clearUsage: () => invoke<number>("usage_clear"),

  memoryIndexStatus: () => invoke<MemoryIndexStatus>("memory_index_status"),
  /** Index every chat with new messages now, idle or not. */
  memoryIndexNow: () => invoke<void>("memory_index_now"),
  /** How many chats from before indexing was switched on have not been read. */
  memoryIndexEarlierCount: () => invoke<number>("memory_index_earlier_count"),
  memoryIndexEarlier: () => invoke<void>("memory_index_earlier"),
  /** A memory setting kept by the engine, or the providers, changed. */
  memoryIndexPoke: () => invoke<void>("memory_index_poke"),

  /** Tidy now for memory owners (docs/features/learning-layers/spec.md, FR-6). */
  learningTidyNow: (owners: string[]) =>
    invoke<import("@/stores/learning-store").TidyResult[]>("learning_tidy_now", { owners }),
  /** A learning setting changed: the shell looks again now. */
  learningPoke: () => invoke<void>("learning_poke"),
  /** The dream's ideas still waiting for an answer (FR-14). */
  learningSuggestions: (owners: string[]) => invoke<LearningSuggestion[]>("learning_suggestions", { owners }),
  learningSuggestionAnswer: (id: number, accepted: boolean) => invoke<boolean>("learning_suggestion_answer", { id, accepted }),
  /** Learned skills: list, switch, undo a change, delete (FR-15, FR-18). */
  learnedSkills: (owner: string) => invoke<LearnedSkill[]>("learned_skills", { owner }),
  learnedSkillSetOn: (owner: string, name: string, on: boolean) => invoke<void>("learned_skill_set_on", { owner, name, on }),
  learnedSkillUndo: (owner: string, which: { name?: string; change?: number }) =>
    invoke<void>("learned_skill_undo", { owner, name: which.name ?? null, change: which.change ?? null }),
  learnedSkillDelete: (owner: string, name: string) => invoke<void>("learned_skill_delete", { owner, name }),
  /** The skill changes made after replies of a chat: its "Learned" lines. */
  learnedForChat: (chat: string) => invoke<JournalEntry[]>("learned_for_chat", { chat }),
};

/**
 * Write to the run log on this machine (Settings → Logs). Never throws: the
 * log is for debugging and must not break what it observes.
 */
export function log(level: LogLevel, source: string, message: string, extra?: { chatId?: string | null; data?: unknown }) {
  if (import.meta.env.DEV) {
    const method = level === "error" ? console.error : level === "warn" ? console.warn : console.debug;
    method(`[${source}] ${message}`, extra?.data ?? "");
  }
  void invoke("logs_append", {
    entries: [{ level, source, message, chatId: extra?.chatId ?? null, data: extra?.data ?? null }],
  }).catch(() => undefined);
}

let installed = false;

/** Send uncaught UI errors and rejected promises to the run log. */
export function installErrorLogging() {
  if (installed) return;
  installed = true;
  window.addEventListener("error", (event) => {
    log("error", "ui", event.message || "Uncaught error", {
      data: { file: event.filename, line: event.lineno, column: event.colno, stack: event.error?.stack },
    });
  });
  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    log("error", "ui", reason instanceof Error ? reason.message : String(reason), {
      data: { stack: reason instanceof Error ? reason.stack : undefined },
    });
  });
}

/** An idea the dream offers (docs/features/learning-layers/spec.md, FR-14). */
export interface LearningSuggestion {
  id: number;
  owner: string;
  kind: "schedule" | "skill";
  text: string;
  fromChats: string[];
  status: "pending" | "accepted" | "declined";
  atMs: number;
}

/** A learned skill (docs/features/learning-layers/spec.md, FR-15). */
export interface LearnedSkill {
  name: string;
  description: string;
  text: string;
  on: boolean;
  lastChange: number | null;
  changedAt: number | null;
  changedBy: string | null;
}

/** One journal row (memory or skill), as the shell returns it. */
export interface JournalEntry {
  id: number;
  atMs: number;
  owner: string;
  subject: "memory" | "skill";
  name: string;
  action: string;
  by: string;
  chatId: string | null;
  chatTitle: string | null;
  replyId: string | null;
  diff: string | null;
  before: string | null;
  after: string | null;
  undoable: boolean;
  undoneAt: number | null;
}
