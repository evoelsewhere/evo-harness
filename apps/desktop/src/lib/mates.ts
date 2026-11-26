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
};

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
  | { kind: "notice"; notice: string; text: string; at: number; author?: string };

export const channelsApi = {
  list: () => invoke<Channel[]>("channels_list"),
  create: (input: CreateChannel) => invoke<Channel>("channels_create", { input }),
  update: (id: string, patch: ChannelPatch) => invoke<Channel | null>("channels_update", { id, patch }),
  remove: (id: string) => invoke<void>("channels_delete", { id }),
  stopMember: (id: string, mate: string) => invoke<void>("channels_stop_member", { id, mate }),
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
};
