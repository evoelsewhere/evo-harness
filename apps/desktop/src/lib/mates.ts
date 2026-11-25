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
};
