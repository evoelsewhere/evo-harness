// Plugins and marketplaces (src-tauri/src/plugins.rs, crates/evo-plugins).

import { invoke } from "@tauri-apps/api/core";

export interface UserConfigOption {
  type?: "string" | "number" | "boolean" | "directory" | "file";
  title?: string;
  description?: string;
  required?: boolean;
  default?: unknown;
  sensitive?: boolean;
}

/** One view a plugin adds (`ui` in plugin.json; docs/features/plugin-ui/spec.md, FR-1). */
export interface PluginView {
  id: string;
  title: string;
  entry: string;
  icon?: string | null;
  defaultShown: boolean;
  shortcut?: string | null;
}

export interface PluginUi {
  sidebar: PluginView[];
  panels: PluginView[];
  cards: PluginView[];
  composer: PluginView[];
}

/** A permission, with the words the person reads when approving it (FR-3). */
export interface PermissionRow {
  id: string;
  label: string;
  /** Why the plugin needs it, in its own words. */
  reason?: string | null;
  /** The plugin works without it and asks when it needs it (plugin-settings FR-9). */
  optional?: boolean;
  /** Not declared: its parts need it (hooks, servers, secrets). */
  implied?: boolean;
  /** Its parts a network or file permission does not bind (FR-15). */
  notEnforcedFor?: string[];
}

/** One setting a plugin asks for (docs/features/plugin-settings/spec.md, FR-2, FR-3). */
export interface PluginSetting {
  key: string;
  kind: "text" | "multiline" | "number" | "boolean" | "select" | "folder" | "file" | "secret";
  title: string;
  description?: string | null;
  default?: unknown;
  required: boolean;
  group?: string | null;
  min?: number | null;
  max?: number | null;
  pattern?: string | null;
  options: string[];
  scope: "personal" | "workspace";
}

/** An account a plugin signs in to (FR-20). */
export interface PluginAccount {
  id: string;
  service: string;
  method: { kind: "oauth" | "device" | "token" | "web" } & Record<string, unknown>;
  reason?: string | null;
  optional: boolean;
}

/** One sign-in to an account (FR-20, FR-29, FR-31); no secret. */
export interface AccountSignIn {
  id: string;
  label: string;
  state: "signedIn" | "needsSignIn" | "offline";
  default: boolean;
  workspaces: string[];
  scopes: string[];
  /** Access the plugin now asks for that this sign-in was not granted. */
  missingScopes: string[];
}

/** A device code to show while a sign-in waits (FR-22). */
export interface DeviceCode {
  code: string;
  url: string;
  complete?: string | null;
  expiresAt: number;
}

/** An account and its sign-ins as the page shows them. */
export interface AccountStatus {
  id: string;
  service: string;
  kind: "oauth" | "device" | "token" | "web";
  reason?: string | null;
  optional: boolean;
  state: "signedIn" | "needsSignIn" | "signedOut";
  signIns: AccountSignIn[];
  /** A sign-in under way: how it goes on. */
  signingIn?: { flow: "browser" | "device" | "window"; device?: DeviceCode } | null;
}

/** A model by provider and id. */
export interface AiModelRef {
  provider: string;
  model: string;
}

/** What a plugin's AI may use and spend (plugin-ai FR-14). */
export interface AiSettings {
  providers?: string[] | null;
  models?: AiModelRef[] | null;
  defaultModel?: AiModelRef | null;
  tiers?: Partial<Record<"fast" | "balanced" | "best", AiModelRef>>;
  capDayUsd?: number | null;
  capMonthUsd?: number | null;
}

/** A connected model as plugin AI sees it. */
export interface AiModelInfo extends AiModelRef {
  name: string;
  vision: boolean;
  tools: boolean;
  reasoning: boolean;
  context: number;
  maxOutput: number;
  cost?: { input: number; output: number } | null;
  tiers: ("fast" | "balanced" | "best")[];
}

/** A plugin's AI use and runs (plugin-ai FR-14, FR-21). */
export interface AiState {
  settings: AiSettings;
  spentToday: number;
  spentMonth: number;
  monthCalls: number;
  monthInput: number;
  monthOutput: number;
  runs: {
    going: { id: string; model?: string | null; costUsd: number; seconds: number }[];
    recent: { id: string; threadId?: string | null; status: string; providerId?: string | null; modelId?: string | null; startedAtMs: number; endedAtMs?: number | null; stopped?: string | null; costUsd: number }[];
  };
}

/** A plugin's AI thread (plugin-ai FR-18, FR-19). */
export interface AiThread {
  id: string;
  title?: string | null;
  createdAt: number;
  updatedAt: number;
  messages: number;
  /** A run continues it now. */
  going: boolean;
}

/** A plugin's run asks before one of evo's tools (plugin-ai FR-16). */
export interface AiQuestion {
  id: string;
  runId: string;
  plugin: string;
  title: string;
  detail: string;
  options: string[];
}

/** One message of a thread as the person reads it. */
export interface AiThreadMessage {
  role: "user" | "assistant";
  text: string;
  at: number;
  provider?: string | null;
  model?: string | null;
  tools?: { id: string; name: string; input: unknown; result: string | null; isError: boolean }[];
}

/** A plugin's backend (FR-10, FR-11). */
export interface PluginRuntime {
  language: "typescript" | "javascript" | "python";
  entry: string;
  startup: "onDemand" | "withApp" | "off";
  idleStopMinutes: number;
}

/** A part that does not run yet, and what it waits for (FR-4, FR-8). */
export interface PluginWaiting {
  part: "setup" | "hooks" | "runtime";
  needs: string[];
}

/** What a plugin keeps on this computer (FR-19). */
export interface PluginData {
  path: string;
  bytes: number;
  storageBytes: number;
  secrets: number;
  accounts: number;
}

/** What an install, update or approval leaves for the person to approve. */
export interface ConsentState {
  id: string;
  version: string;
  enabled: boolean;
  needsConsent: PermissionRow[];
  pendingVersion?: string | null;
}

export interface InstalledPlugin {
  id: string;
  name: string;
  marketplace: string;
  version: string;
  enabled: boolean;
  path: string;
  installedAt: number;
  config: Record<string, unknown>;
  description?: string | null;
  author?: string | null;
  homepage?: string | null;
  repository?: string | null;
  license?: string | null;
  keywords: string[];
  userConfig: Record<string, UserConfigOption>;
  skills: string[];
  commands: number;
  agents: string[];
  hooks: { event: string; count: number }[];
  connectors: string[];
  bin: boolean;
  problems: string[];
  ui: PluginUi;
  /** The views' icons as data URLs, by path. */
  icons: Record<string, string>;
  /** What its views ask for. */
  permissions: PermissionRow[];
  approvedPermissions: string[];
  /** Asked for and not approved (the installed version's, then a pending update's). */
  unapproved: PermissionRow[];
  /** An update waiting for consent to new permissions. */
  pending?: { version: string; permissions: PermissionRow[] } | null;
  developerMode: boolean;
  settings: PluginSetting[];
  /** Workspace values of per-workspace settings, by workspace id. */
  workspaceConfig: Record<string, Record<string, unknown>>;
  /** Secret settings that have a value: `key`, or `key@<workspace id>`. */
  secretsSet: string[];
  accounts: PluginAccount[];
  accountStatus: AccountStatus[];
  runtime?: PluginRuntime | null;
  optionalAnswers: Record<string, boolean>;
  waiting: PluginWaiting[];
  /** The person's choice; unset follows the manifest. */
  startMode?: PluginRuntime["startup"] | null;
  idleStopMinutes?: number | null;
}

/** A plugin's code as it runs now (plugin-settings FR-11 – FR-13, FR-18). */
export interface RuntimeState {
  preparation?: { phase: "downloading" | "installing" | "ready" | "failed"; message?: string | null } | null;
  servers: { key: string; name: string; status: string; error?: string | null; tools: number; log: string[] }[];
}

export interface MarketplaceInfo {
  name: string;
  /** `owner/repo#ref`, a git URL, or a folder. */
  source: string;
  kind: "git" | "folder";
  location: string;
  description?: string | null;
  owner?: string | null;
  updatedAt: number;
  plugins: number;
  error?: string | null;
}

export interface CatalogEntry {
  id: string;
  name: string;
  marketplace: string;
  description?: string;
  version?: string;
  category?: string;
  keywords?: string[];
  author?: string | { name?: string };
  homepage?: string;
  /** The installed version, if installed. */
  installed?: string | null;
  error?: string;
}

export interface PluginsState {
  installed: InstalledPlugin[];
  marketplaces: MarketplaceInfo[];
  catalog: CatalogEntry[];
  root: string;
}

export const PLUGINS_CHANGED = "plugins://changed";

export const plugins = {
  state: () => invoke<PluginsState>("plugins_state"),
  addMarketplace: (source: string) => invoke<{ name: string }>("plugin_marketplace_add", { source }),
  updateMarketplace: (name: string) => invoke<void>("plugin_marketplace_update", { name }),
  removeMarketplace: (name: string) => invoke<void>("plugin_marketplace_remove", { name }),
  install: (marketplace: string, name: string) => invoke<ConsentState>("plugin_install", { marketplace, name }),
  installFolder: (path: string) => invoke<ConsentState>("plugin_install_folder", { path }),
  uninstall: (id: string) => invoke<void>("plugin_uninstall", { id }),
  update: (id: string) => invoke<ConsentState>("plugin_update", { id }),
  /** Approve permissions; once all are approved the plugin is on (FR-3). */
  approve: (id: string, permissions: string[]) => invoke<ConsentState>("plugin_approve", { id, permissions }),
  revoke: (id: string, permission: string) => invoke<void>("plugin_revoke", { id, permission }),
  setDeveloperMode: (id: string, on: boolean) => invoke<void>("plugin_set_developer_mode", { id, on }),
  setEnabled: (id: string, on: boolean) => invoke<void>("plugin_set_enabled", { id, on }),
  setConfig: (id: string, config: Record<string, unknown>) => invoke<void>("plugin_set_config", { id, config }),
  /** Save plain settings, personal or for a workspace; nothing is saved when a value does not fit. */
  saveSettings: (id: string, values: Record<string, unknown>, workspace?: string | null) =>
    invoke<{ problems: { key: string; message: string }[] }>("plugin_settings_save", { id, values, workspace: workspace ?? null }),
  setSecret: (id: string, key: string, value: string, workspace?: string | null) =>
    invoke<void>("plugin_secret_set", { id, key, value, workspace: workspace ?? null }),
  clearSecret: (id: string, key: string, workspace?: string | null) => invoke<void>("plugin_secret_clear", { id, key, workspace: workspace ?? null }),
  data: (id: string) => invoke<PluginData>("plugin_data", { id }),
  clearData: (id: string) => invoke<void>("plugin_data_clear", { id }),
  runtimeState: (id: string, name: string) => invoke<RuntimeState>("plugin_runtime_state", { id, name }),
  runtimeControl: (id: string, name: string, action: "start" | "stop" | "restart") => invoke<void>("plugin_runtime_control", { id, name, action }),
  runtimeMode: (id: string, mode: PluginRuntime["startup"], idleMinutes?: number | null) =>
    invoke<void>("plugin_runtime_mode", { id, mode, idleMinutes: idleMinutes ?? null }),
  /** Start a sign-in (or join the running one): how it goes on (FR-21 – FR-24). */
  signIn: (id: string, account: string) => invoke<NonNullable<AccountStatus["signingIn"]>>("plugin_account_sign_in", { id, account }),
  cancelSignIn: (id: string, account: string) => invoke<void>("plugin_account_cancel", { id, account }),
  signOut: (id: string, record: string) => invoke<void>("plugin_account_sign_out", { id, record }),
  /** The default sign-in, or (with a workspace) that workspace's choice (FR-29). */
  setDefaultAccount: (id: string, record: string, workspace?: string | null) =>
    invoke<void>("plugin_account_default", { id, record, workspace: workspace ?? null }),
  /** Run a key's check in the plugin's backend (FR-23). */
  testAccount: (id: string, account: string) => invoke<{ ok: boolean; message: string }>("plugin_account_test", { id, account }),
  aiState: (id: string) => invoke<AiState>("plugin_ai_state", { id }),
  aiSaveSettings: (id: string, settings: AiSettings) => invoke<void>("plugin_ai_settings_save", { id, settings }),
  /** Every connected model, for the person choosing (starts the AI engine). */
  aiModels: (id: string) => invoke<AiModelInfo[]>("plugin_ai_models", { id }),
  aiCancel: (runId: string) => invoke<void>("plugin_ai_cancel", { runId }),
  aiThreads: (id: string) => invoke<AiThread[]>("plugin_ai_threads", { id }),
  /** Questions about evo's tools in plugins' runs, open now (FR-16). */
  aiQuestions: () => invoke<AiQuestion[]>("plugin_ai_questions"),
  aiAnswer: (id: string, index: number) => invoke<void>("plugin_ai_answer", { id, index }),
  aiThread: (id: string, thread: string) => invoke<AiThread & { messages: AiThreadMessage[] }>("plugin_ai_thread", { id, thread }),
  aiDeleteThread: (id: string, thread: string) => invoke<void>("plugin_ai_thread_delete", { id, thread }),
  /** Answer an optional permission; the answer is kept (FR-9). */
  answerOptional: (id: string, permission: string, allow: boolean) => invoke<void>("plugin_optional_answer", { id, permission, allow }),
};

/** A slot of the window: where a plugin view shows (docs/features/plugin-ui/spec.md, FR-4 – FR-7). */
export type PluginSlotKind = keyof PluginUi;

/** Bounds in window logical pixels (CSS pixels × the app zoom). */
export interface ViewBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Plugin views: native webviews the shell lays over a slot (src-tauri/src/plugin_ui/views.rs). */
export const pluginViews = {
  open: (plugin: string, slot: PluginSlotKind, view: string, bounds: ViewBounds, visible: boolean, data?: unknown) =>
    invoke<string>("plugin_view_open", { plugin, slot, view, bounds, visible, data: data ?? null }),
  bounds: (label: string, bounds: ViewBounds) => invoke<void>("plugin_view_bounds", { label, bounds }),
  visible: (label: string, visible: boolean) => invoke<void>("plugin_view_visible", { label, visible }),
  reload: (label: string) => invoke<void>("plugin_view_reload", { label }),
  close: (label: string) => invoke<void>("plugin_view_close", { label }),
  theme: (tokens: Record<string, string>, dark: boolean) => invoke<void>("plugin_view_theme", { tokens, dark }),
  /** Close the views an earlier page of this window left (a reload keeps native views). */
  reset: () => invoke<void>("plugin_view_reset"),
};

/** Events the shell sends the window about its views (`plugin-ui://…`). */
export const PLUGIN_VIEW_EVENTS = {
  loaded: "plugin-ui://loaded",
  closed: "plugin-ui://closed",
  resize: "plugin-ui://resize",
  open: "plugin-ui://open",
  notify: "plugin-ui://notify",
  composerInsert: "plugin-ui://composer-insert",
} as const;
