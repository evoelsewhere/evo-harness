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
  /** A chat's open questions and approvals (`engine_questions`): what Today's Needs you waits on (evo-mates-ui-v2 FR-26). */
  questions: (chat: string) => invoke<{ kind: string; title: string; at: number }[]>("engine_questions", { chat }),
  /** Lines that mentioned the person (`@you`), for Today (evo-mates-messaging FR-18). */
  mentions: () => invoke<Mention[]>("mates_mentions"),
  mentionsRead: (chat: string) => invoke<void>("mates_mentions_read", { chat }),
  /** A link's card, when link previews are on (FR-17). */
  linkPreview: (url: string) => invoke<LinkPreview | null>("link_preview", { url }),
};

/** A channel line that mentioned the person (FR-18). */
export interface Mention {
  chat: string;
  line: string;
  author: string;
  name: string;
  excerpt: string;
  at: number;
}

/** A link's card (FR-17). */
export interface LinkPreview {
  url: string;
  title: string;
  site: string;
  description?: string | null;
  image?: string | null;
}

/** A reaction on a channel line, by the person or a mate (FR-3, FR-4). */
export interface ChannelReaction {
  message: string;
  /** "person" or a mate's id. */
  reactor: string;
  reaction: ReactionKind;
  atMs: number;
}

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
  | {
      kind: "message";
      id: string;
      author: string;
      name?: string;
      text: string;
      at: number;
      status?: string;
      replyTo?: string | null;
      steers?: string;
      /** The line a mate's line answers (evo-mates-messaging FR-5). */
      quote?: string | null;
      /** The thread it is in: its parent line's id (evo-mates-threads-board FR-3). */
      thread?: string | null;
      /** A thread reply also shown in the channel (FR-22). */
      alsoChannel?: boolean;
    }
  | { kind: "route"; message: string; picked: string[]; line: string; at: number; thread?: string | null }
  | {
      kind: "handoff";
      message: string;
      handoff: "asks" | "assigns" | "tells";
      from: string;
      fromName: string;
      to: string;
      toName: string;
      text: string;
      /** Files given with it (FR-6). */
      files?: string[];
      at: number;
      thread?: string | null;
    }
  | { kind: "notice"; notice: string; text: string; at: number; author?: string; thread?: string | null }
  /** A job ended (learning-layers FR-19); not shown. */
  | { kind: "job"; id: string; message: string; status: string; handoffs: number; at: number; thread?: string | null }
  /** A retro's card (learning-layers FR-21). */
  | ({ kind: "retro"; id: string; at: number } & RetroCard)
  /** A mate's `share_to` into this channel (evo-mates-threads-board FR-18): a card, information only. */
  | { kind: "share"; id: string; by: string; name: string; from: string; text: string; files?: string[]; at: number; thread?: string | null }
  /** A card's change on the channel's board (evo-mates-threads-board FR-13); never delivered to mates. */
  | { kind: "board"; text: string; card: string; cardId: string; at: number; thread?: string | null };

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
  /** Reactions on a channel's lines (FR-3). */
  reactions: (channel: string) => invoke<ChannelReaction[]>("channel_reactions", { channel }),
  react: (channel: string, message: string, reaction: ReactionKind | null) => invoke<void>("channel_react", { channel, message, reaction }),
  create: (input: CreateChannel) => invoke<Channel>("channels_create", { input }),
  update: (id: string, patch: ChannelPatch) => invoke<Channel | null>("channels_update", { id, patch }),
  remove: (id: string) => invoke<void>("channels_delete", { id }),
  stopMember: (id: string, mate: string) => invoke<void>("channels_stop_member", { id, mate }),
  /** Every channel reads Settings › Evo Mates again. */
  reload: () => invoke<void>("channels_reload"),
  /** **Run retro**: the retro of the channel's last job, now (learning-layers FR-19). */
  runRetro: (chat: string) => invoke<RetroCard>("channel_run_retro", { chat }),
};

// ── Boards (docs/features/evo-mates-threads-board/spec.md, FR-8 – FR-15) ────

export type CardPriority = "none" | "low" | "medium" | "high" | "urgent";

export interface BoardColumn {
  id: string;
  name: string;
  position: number;
}

export interface Board {
  channelId: string;
  key: string;
  enabled: boolean;
  nextNumber: number;
  columns: BoardColumn[];
}

export interface CheckItem {
  text: string;
  done: boolean;
}

export interface CardLink {
  /** `message`, `file`, `thread` or `chat`. */
  kind: string;
  target: string;
  label: string;
}

export interface Card {
  id: string;
  channelId: string;
  /** `LAUNCH-12`. */
  key: string;
  number: number;
  columnId: string;
  position: number;
  title: string;
  description: string;
  /** `person` or a mate's id. */
  assignee: string | null;
  priority: CardPriority;
  dueMs: number | null;
  labels: string[];
  checklist: CheckItem[];
  links: CardLink[];
  threadRoot: string | null;
  source: Record<string, unknown> | null;
  createdBy: string;
  createdMs: number;
  updatedMs: number;
}

export interface CardEvent {
  id: number;
  cardId: string;
  actor: string;
  kind: "created" | "moved" | "updated" | "comment" | string;
  text: string;
  atMs: number;
}

export interface BoardView {
  board: Board | null;
  cards: Card[];
  proposedKey: string;
}

/** A card with where it lives (FR-14). */
export interface PlacedCard {
  card: Card;
  channelName: string;
  chatId: string;
  column: string;
}

export interface NewCard {
  title: string;
  description?: string;
  assignee?: string | null;
  priority?: CardPriority;
  dueMs?: number | null;
  labels?: string[];
  columnId?: string | null;
  top?: boolean;
  threadRoot?: string | null;
  links?: CardLink[];
}

/** Fields to change; a field set to `null` clears it. */
export type CardPatch = Partial<Pick<Card, "title" | "description" | "assignee" | "priority" | "dueMs" | "labels" | "checklist" | "links" | "threadRoot">>;

export const boardsApi = {
  get: (channel: string) => invoke<BoardView>("board_get", { channel }),
  set: (channel: string, on: boolean, key?: string) => invoke<BoardView>("board_set", { channel, on, key: key ?? null }),
  column: (channel: string, action: "add" | "rename" | "move" | "remove", args: { column?: string; name?: string; index?: number }) =>
    invoke<BoardView>("board_column", { channel, action, column: args.column ?? null, name: args.name ?? null, index: args.index ?? null }),
  keys: () => invoke<string[]>("board_keys"),
  columnCount: (channel: string, column: string) => invoke<number>("board_column_count", { channel, column }),
  /** Labels and checklist change from what the card holds now (board review #6). */
  labels: (id: string, add: string[], remove: string[]) => invoke<Card>("card_labels", { id, add, remove }),
  checklist: (id: string, edit: { add?: string[]; check?: (number | string)[]; uncheck?: (number | string)[]; remove?: (number | string)[] }) =>
    invoke<Card>("card_checklist", { id, edit: { add: [], check: [], uncheck: [], remove: [], ...edit } }),
  create: (channel: string, card: NewCard) => invoke<Card>("card_create", { channel, card }),
  update: (id: string, patch: CardPatch) => invoke<Card>("card_update", { id, patch }),
  move: (id: string, column: string, index: number | null) => invoke<Card>("card_move", { id, column, index }),
  remove: (id: string) => invoke<void>("card_delete", { id }),
  comment: (id: string, text: string) => invoke<CardEvent>("card_comment", { id, text }),
  events: (id: string) => invoke<CardEvent[]>("card_events", { id }),
  byKey: (key: string) => invoke<PlacedCard | null>("card_by_key", { key }),
  assigned: (assignee: string) => invoke<PlacedCard[]>("cards_assigned", { assignee }),
};

// ── Linked conversations and threads (evo-mates-threads-board FR-16 – FR-22) ──

export interface Share {
  id: string;
  fromChat: string;
  fromLines: string[];
  excerpt: string;
  toChat: string;
  note: string;
  /** `person` or a mate's id. */
  by: string;
  atMs: number;
}

export interface ThreadFollow {
  chatId: string;
  root: string;
  rootExcerpt: string;
  following: boolean;
  resolved: boolean;
  replies: number;
  lastMs: number;
  lastAuthor: string;
  lastReadMs: number;
  unread: boolean;
}

export const linksApi = {
  record: (share: { fromChat: string; fromLines: string[]; excerpt: string; toChat: string; note: string }) =>
    invoke<Share>("share_record", { share: { ...share, by: "person" } }),
  linked: (chat: string) => invoke<Share[]>("linked_of", { chat }),
  followed: () => invoke<ThreadFollow[]>("threads_followed"),
  threadsOf: (chat: string) => invoke<ThreadFollow[]>("threads_of", { chat }),
  set: (chat: string, root: string, change: { following?: boolean; resolved?: boolean }) =>
    invoke<void>("thread_set", { chat, root, following: change.following ?? null, resolved: change.resolved ?? null }),
  read: (chat: string, root: string) => invoke<void>("thread_read", { chat, root }),
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
