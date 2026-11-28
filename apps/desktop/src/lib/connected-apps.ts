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
