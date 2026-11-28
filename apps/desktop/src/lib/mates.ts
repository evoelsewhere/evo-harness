// The shell's mates commands (src-tauri/src/mates/; docs/features/evo-mates/plan.md 3.3).

import { invoke } from "@tauri-apps/api/core";
import type { MateAvatar } from "@/components/mate-face/mate-face";

export type ApprovalMode = "ask" | "auto-edits" | "auto";

export interface Mate {
  id: string;
  name: string;
  avatar: MateAvatar;
  /** One line: what the mate does. */
  role: string;
  instructions: string;
  provider: string | null;
  model: string | null;
  thinking: string | null;
  tools: string[] | null;
  connectors: string[] | null;
  skills: string[] | null;
  apps: unknown[] | null;
  approvalMode: ApprovalMode;
  sending: "ask" | "send";
  /** How often it may write first (FR-65). */
  initiative: Initiative;
  /** First messages in a row the person has not answered. */
  unanswered: number;
  notifications: boolean;
  paused: boolean;
  folder: string;
  projectFolder: string | null;
  /** The mate's direct conversation. */
  chatId: string;
  template: string | null;
  pinned: boolean;
  hidden: boolean;
  position: number;
  createdAt: number;
  updatedAt: number;
}

/** A gallery role (src-tauri/src/mates/templates/). */
export interface MateTemplate {
  id: string;
  name: string;
  role: string;
  category: string;
  description: string;
  avatar: MateAvatar;
  /** A looser approval mode the role suggests, shown on its card. */
  approval: ApprovalMode | null;
  /** Requests a new conversation offers to send (FR-74). */
  starters: string[];
}

export interface CreateMate {
  template?: string;
  name?: string;
  avatar?: MateAvatar;
  role?: string;
  instructions?: string;
  approvalMode?: ApprovalMode;
}

/** Fields to change; `null` clears an optional one. */
export type MatePatch = Partial<
  Pick<
    Mate,
    | "name"
    | "avatar"
    | "role"
    | "instructions"
    | "approvalMode"
    | "sending"
    | "notifications"
    | "initiative"
    | "paused"
    | "pinned"
    | "hidden"
  >
> & {
  /** The skills the mate may use; `null`: every skill (FR-44). */
  skills?: string[] | null;
  /** The apps the mate may use; `null`: every connected app (FR-39). */
  apps?: unknown[] | null;
  provider?: string | null;
  model?: string | null;
  thinking?: string | null;
  projectFolder?: string | null;
};

/** The reactions a person gives a message (FR-60). */
export type ReactionKind = "heart" | "like" | "dislike" | "laugh" | "emphasis" | "question";

export interface Reaction {
  message: string;
  reaction: ReactionKind;
  excerpt: string;
  atMs: number;
}

export const matesApi = {
  list: () => invoke<Mate[]>("mates_list"),
  templates: () => invoke<MateTemplate[]>("mates_templates"),
  create: (input: CreateMate) => invoke<Mate>("mates_create", { input }),
  update: (id: string, patch: MatePatch) => invoke<Mate | null>("mates_update", { id, patch }),
  duplicate: (id: string) => invoke<Mate>("mates_duplicate", { id }),
  remove: (id: string) => invoke<void>("mates_delete", { id }),
  folder: (id: string) => invoke<string>("mates_folder", { id }),
  reactions: (chat: string) => invoke<Reaction[]>("mates_reactions", { chat }),
  react: (chat: string, message: string, reaction: ReactionKind | null, excerpt: string) =>
    invoke<void>("mates_react", { chat, message, reaction, excerpt }),
  takeReactions: (chat: string) => invoke<Reaction[]>("mates_take_reactions", { chat }),
  pauseAll: (paused: boolean) => invoke<void>("mates_pause_all", { paused }),
  /** Tools a mate runs without asking (FR-41). */
  grants: (id: string) => invoke<string[]>("mates_grants", { id }),
  removeGrant: (id: string, tool: string) => invoke<void>("mates_remove_grant", { id, tool }),
  /** A mate's saved secrets, by name (FR-43). */
  secrets: (id: string) => invoke<string[]>("mates_secrets", { id }),
  deleteSecret: (id: string, name: string) => invoke<void>("mates_delete_secret", { id, name }),
  /** What a mate will come back to (FR-70). */
  agenda: (id: string) => invoke<AgendaItem[]>("mates_agenda", { id }),
  setAgenda: (id: string, item: string, status: AgendaItem["status"]) => invoke<void>("mates_agenda_set", { id, item, status }),
  deleteAgenda: (id: string, item: string) => invoke<void>("mates_agenda_delete", { id, item }),
  repeatWords: (repeat: string) => invoke<string | null>("mates_agenda_repeat_words", { repeat }),
  /** Change a check-in's schedule (`{kind: "daily" | "weekdays" | "weekly", time, days?}`). */
  rescheduleAgenda: (id: string, item: string, repeat: CheckinSchedule) =>
    invoke<AgendaItem>("mates_agenda_reschedule", { id, item, repeat }),
  /** A mate's Knowledge files (FR-77). */
  knowledge: (id: string) => invoke<KnowledgeFile[]>("mates_knowledge", { id }),
  addKnowledge: (id: string, paths: string[]) =>
    invoke<{ added: string[]; refused: [string, string][] }>("mates_knowledge_add", { id, paths }),
  removeKnowledge: (id: string, name: string) => invoke<void>("mates_knowledge_remove", { id, name }),
  /** A check-in the person adds in the panel (FR-79). */
  addCheckin: (id: string, text: string, repeat: CheckinSchedule) =>
    invoke<AgendaItem>("mates_agenda_add_checkin", { id, text, repeat }),
  /** Mates' conversations and channels whose words match (FR-76). */
  search: (query: string) => invoke<MessageHit[]>("mates_search", { query }),
  /** Coming up and each mate's day so far, for Today (FR-73). */
  today: () => invoke<{ coming: AgendaItem[]; team: { mateId: string; replies: number; costUsd: number }[] }>("mates_today"),
};

/** A file in a mate's Knowledge (FR-77). */
export interface KnowledgeFile {
  name: string;
  bytes: number;
}

/** A conversation whose words match a search (FR-76). */
export interface MessageHit {
  chatId: string;
  kind: "mate" | "channel";
  snippet: string;
  updatedAt: number;
}

export type Initiative = "off" | "low" | "medium";

/** A check-in's schedule, in the Scheduler's shape. */
export interface CheckinSchedule {
  kind: "daily" | "weekdays" | "weekly";
  time: string;
  /** Weekly: 0 Sunday - 6 Saturday. */
  days?: number[];
}

/** An item a mate comes back to, or a check-in (FR-63, FR-69). */
export interface AgendaItem {
  id: string;
  mateId: string;
  kind: "promise" | "event" | "question" | "checkin";
  text: string;
  dueAt: number;
  repeat: string | null;
  status: "open" | "done" | "never" | "paused";
  chatId: string | null;
  createdAt: number;
  firedAt: number | null;
}

// ── Channels (docs/features/evo-mates/spec.md, FR-14 – FR-19) ──────────────

/** Who answers a message that mentions nobody. */
export type ReplyRule = "auto" | "lead" | "everyone" | "mentioned";

export interface ChannelMember {
  mateId: string;
  seatChatId: string | null;
  position: number;
}

export interface Channel {
  id: string;
  name: string;
  instructions: string;
  replyRule: ReplyRule;
  leadMateId: string | null;
  folder: string | null;
  /** The channel's root chat: its engine coordinates the channel. */
  chatId: string;
  pinned: boolean;
  hidden: boolean;
  position: number;
  members: ChannelMember[];
  createdAt: number;
  updatedAt: number;
}

export interface CreateChannel {
  name: string;
  instructions?: string;
  replyRule?: ReplyRule;
  leadMateId?: string | null;
  folder?: string | null;
  members: string[];
}

export type ChannelPatch = Partial<Pick<Channel, "name" | "instructions" | "replyRule" | "pinned" | "hidden">> & {
  leadMateId?: string | null;
  folder?: string | null;
  members?: string[];
};

/** One thing a channel shows, as its coordinator wrote it (`channel` entries). */
export type ChannelEntry =
  | { kind: "message"; id: string; author: string; name?: string; text: string; at: number; status?: string; replyTo?: string | null; steers?: string }
  | { kind: "route"; message: string; picked: string[]; line: string; at: number }
  | {
      kind: "handoff";
      message: string;
      handoff: "asks" | "assigns" | "tells";
      from: string;
      fromName: string;
      to: string;
      toName: string;
      text: string;
      at: number;
    }
  | { kind: "notice"; notice: string; text: string; at: number; author?: string }
  /** A job ended (learning-layers FR-19); not shown. */
  | { kind: "job"; id: string; message: string; status: string; handoffs: number; at: number }
  /** A retro's card (learning-layers FR-21). */
  | ({ kind: "retro"; id: string; at: number } & RetroCard);

/** What a channel job's retro found (docs/features/learning-layers/spec.md, FR-20, FR-21). */
export interface RetroCard {
  job: string;
  status: string;
  handoffs: number;
  well: string[];
  wrong: string[];
  lessons: { mate: string; name: string; text: string; skill?: string | null; row?: number | null }[];
}

export const channelsApi = {
  list: () => invoke<Channel[]>("channels_list"),
  create: (input: CreateChannel) => invoke<Channel>("channels_create", { input }),
  update: (id: string, patch: ChannelPatch) => invoke<Channel | null>("channels_update", { id, patch }),
  remove: (id: string) => invoke<void>("channels_delete", { id }),
  stopMember: (id: string, mate: string) => invoke<void>("channels_stop_member", { id, mate }),
  /** Every channel reads Settings › Evo Mates again. */
  reload: () => invoke<void>("channels_reload"),
  /** **Run retro**: the retro of the channel's last job, now (learning-layers FR-19). */
  runRetro: (chat: string) => invoke<RetroCard>("channel_run_retro", { chat }),
};

// ── Team files (FR-20, FR-21, FR-27) ───────────────────────────────────────

export interface TeamPreview {
  channel: { name: string; replyRule?: string; lead?: string; instructions?: string } | null;
  mates: {
    name: string;
    role: string;
    avatar: Record<string, unknown> | null;
    /** The mode it will start with: the app's. */
    approvalMode: string;
    /** What the file asked for. */
    asked: string | null;
    tools: string[] | null;
    connectors: string[] | null;
    missingConnectors: string[];
    skills: string[] | null;
    instructions: string;
  }[];
  unknown: string[];
}

export const teamApi = {
  export: (target: { mate?: string; channel?: string }, path: string) => invoke<void>("team_export", { target, path }),
  preview: (path: string) => invoke<TeamPreview>("team_preview", { path }),
  import: (path: string, skip: string[]) => invoke<{ mates: string[]; channel: string | null }>("team_import", { path, skip }),
  /** The gallery's ready-made teams (FR-75). */
  builtins: () => invoke<TeamSummary[]>("mates_teams"),
  previewBuiltin: (id: string) => invoke<TeamPreview>("team_preview_builtin", { id }),
  importBuiltin: (id: string, skip: string[]) =>
    invoke<{ mates: string[]; channel: string | null }>("team_import_builtin", { id, skip }),
};

/** A ready-made team in the gallery (FR-75). */
export interface TeamSummary {
  id: string;
  name: string;
  about: string;
  mates: { name: string; role: string; avatar: MateAvatar }[];
}

/**
 * What wakes a mate the person just added, so it greets them and asks its
 * setup questions first (FR-34, ux.md 5). It is not the person's message:
 * the thread shows only the mate's answer.
 */
export const GREETING_PROMPT =
  "<evo-mates-hello>\nThe person just added you in Evo Mates and opened your conversation. Greet them and ask your setup questions, as your conduct says.\n</evo-mates-hello>";
