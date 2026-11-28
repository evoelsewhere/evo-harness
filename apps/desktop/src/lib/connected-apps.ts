// Connected apps: services through Composio with the person's own project key
// (docs/features/connected-apps/spec.md). The shell holds the key, the session
// and the accounts (src-tauri/src/connected_apps); this is its command surface.

import { invoke } from "@tauri-apps/api/core";

export const APPS_CHANGED = "apps://changed";
export const APPS_CONNECTED = "apps://connected";

/** The connector's pool key and name (`mcp__connected_apps__…`). */
export const APP_SERVER_KEY = "app:connected_apps";
export const APP_SERVER_NAME = "connected_apps";

export type KeyState = "none" | "saved" | "invalid";
export type Reach = "off" | "ok" | "unavailable";
export type AccountStatus = "active" | "pending" | "needs-sign-in" | "expired" | "failed";

export interface Account {
  id: string;
  toolkit: string;
  label: string;
  status: AccountStatus;
  connectedAt: string;
}

export interface PendingSignIn {
  id: string;
  toolkit: string;
  label: string;
  account?: string | null;
}

export interface AppsState {
  key: KeyState;
  status: Reach;
  reason?: string | null;
  accounts: Account[];
  pending: PendingSignIn[];
}

export interface Toolkit {
  slug: string;
  name: string;
  logo: string;
  categories: string[];
  managedAuth: boolean;
  noAuth: boolean;
  description: string;
  /** How many tools Composio has for it. */
  toolsCount: number;
  /** The service's own MCP server (an official id), when it has one. */
  official?: string | null;
  /** Composio offers it (false for a card only its official server offers). */
  composio: boolean;
}

/** A service's own MCP server, as the list that ships with Evo gives it (FR-21). */
export interface OfficialServer {
  id: string;
  name: string;
  category: string;
  url: string;
  auth: "oauth" | "token";
  tokenHint?: string | null;
  docs: string;
  preview: boolean;
  composio: string[];
  connected: boolean;
  /** Its pool key (`app:official_<id>`), for its status and tools. */
  key: string;
}

/** Zapier's MCP server (FR-24). */
export interface ZapierServer {
  url: string;
  docs: string;
  /** Where the person chooses the actions their server offers. */
  actions: string;
  tasksPerCall: number;
  connected: boolean;
  key: string;
}

export interface Providers {
  official: OfficialServer[];
  zapier: ZapierServer;
}

/** How a service can be connected, in the order a card offers them (FR-20, FR-27). */
export type Way = { kind: "official"; id: string } | { kind: "composio"; toolkit: string };

export function waysFor(toolkit: Pick<Toolkit, "slug" | "official" | "composio">, composioKey: boolean): Way[] {
  const ways: Way[] = [];
  if (toolkit.official) ways.push({ kind: "official", id: toolkit.official });
  if (toolkit.composio && composioKey) ways.push({ kind: "composio", toolkit: toolkit.slug });
  return ways;
}

/** A mate's `apps` entry name for an app server key: `official:github`, `zapier`. */
export function serviceOfServer(key: string): string | null {
  if (key.startsWith("app:official_")) return `official:${key.slice("app:official_".length)}`;
  if (key === "app:zapier") return "zapier";
  return null;
}

export interface Catalog {
  toolkits: Toolkit[];
  categories: string[];
}

export const appsApi = {
  state: () => invoke<AppsState>("apps_state"),
  setKey: (key: string) => invoke<void>("apps_set_key", { key }),
  removeKey: () => invoke<void>("apps_remove_key"),
  catalog: (query?: string, category?: string) => invoke<Catalog>("apps_catalog", { query: query || null, category: category || null }),
  connect: (toolkit: string, label?: string) => invoke<{ pendingId: string; label: string }>("apps_connect", { toolkit, label: label || null }),
  reopen: (pendingId: string) => invoke<{ pendingId: string; label: string }>("apps_reopen", { pendingId }),
  disconnect: (accountId: string) => invoke<void>("apps_disconnect", { accountId }),
  reconnect: (accountId: string) => invoke<{ pendingId: string; label: string }>("apps_reconnect", { accountId }),
  retry: () => invoke<void>("apps_retry"),
  tools: (toolkit: string) => invoke<AppTool[]>("apps_tools", { toolkit }),
  logos: (slugs: string[]) => invoke<Record<string, string>>("apps_logos", { slugs }),
  providers: () => invoke<Providers>("apps_providers"),
  connectOfficial: (id: string, token?: string) => invoke<void>("apps_connect_official", { id, token: token || null }),
  disconnectOfficial: (id: string) => invoke<void>("apps_disconnect_official", { id }),
  connectZapier: (token?: string) => invoke<void>("apps_connect_zapier", { token: token || null }),
  disconnectZapier: () => invoke<void>("apps_disconnect_zapier"),
};

/** One tool of a service, for a mate's These tools list. */
export interface AppTool {
  slug: string;
  name: string;
  description: string;
  /** Only reads; anything else may send, post or delete. */
  read: boolean;
}

/** How an account's status reads. */
export const statusText: Record<AccountStatus, string> = {
  active: "Active",
  pending: "Signing in",
  "needs-sign-in": "Needs sign-in",
  expired: "Expired",
  failed: "Failed",
};

/** A mate's `apps` entry for a connected service (stored in the mate's `apps`). */
export interface ServiceGrant {
  service: string;
  tools?: "all" | "none" | string[];
  accounts?: string[];
}

export const SERVICE_PREFIX = "composio:";
