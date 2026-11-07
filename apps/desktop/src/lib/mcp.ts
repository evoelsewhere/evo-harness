// Connectors: the shell's MCP pool (src-tauri/src/mcp.rs, crates/evo-mcp).
// The connectors' agent tools (names, search, permissions) are the shell's: src-tauri/src/agent_tools/mcp.rs.

import { invoke } from "@tauri-apps/api/core";

export type ConnectorStatus = "off" | "connecting" | "connected" | "needs-auth" | "failed";
export type ToolPermission = "allow" | "ask" | "block";

export interface ConnectorConfig {
  type?: "stdio" | "http" | "sse";
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  url?: string;
  headers?: Record<string, string>;
  disabled?: boolean;
  toolPermissions?: Record<string, ToolPermission>;
  [key: string]: unknown;
}

export interface ConnectorTool {
  name: string;
  title?: string | null;
  description?: string | null;
  readOnly: boolean;
  permission: ToolPermission;
  /** What chats call it: `mcp__<server>__<tool>`. */
  exposedName: string;
}

export interface ConnectorPrompt {
  name: string;
  title?: string;
  description?: string;
  arguments?: { name: string; description?: string; required?: boolean }[];
}

export interface ConnectorResource {
  uri: string;
  name?: string;
  description?: string;
  mimeType?: string;
}

export interface Connector {
  /** The pool's id: the name, or `name#project` for a project's server. */
  key: string;
  name: string;
  /** The project root whose `.mcp.json` it comes from; `null`: your own. */
  project?: string | null;
  /** A URL connector with a saved sign-in. */
  signedIn?: boolean;
  resources: ConnectorResource[];
  kind: "stdio" | "http" | "sse" | "invalid";
  /** The command or URL. */
  summary: string;
  config: ConnectorConfig;
  status: ConnectorStatus;
  error?: string | null;
  server?: { name?: string; title?: string; version?: string } | null;
  instructions?: string | null;
  tools: ConnectorTool[];
  prompts: ConnectorPrompt[];
  log: string[];
}

export interface ProjectServer {
  name: string;
  summary: string;
  kind: "stdio" | "http";
  approval: "approved" | "denied" | "pending";
  key: string;
  /** One of your own connectors has the same name (yours is used). */
  shadowed: boolean;
}

export interface ProjectState {
  root: string | null;
  file?: string;
  error?: string | null;
  servers: ProjectServer[];
}

/** The change event the shell emits. */
export const MCP_CHANGED = "mcp://changed";

export const mcp = {
  state: () => invoke<{ servers: Connector[]; path: string }>("mcp_state"),
  upsert: (name: string, config: ConnectorConfig, previous?: string) =>
    invoke<void>("mcp_upsert", { name, config, previous: previous ?? null }),
  remove: (name: string) => invoke<void>("mcp_remove", { name }),
  setEnabled: (name: string, on: boolean) => invoke<void>("mcp_set_enabled", { name, on }),
  reconnect: (name: string) => invoke<void>("mcp_reconnect", { name }),
  setPermission: (name: string, tool: string, permission: ToolPermission) =>
    invoke<void>("mcp_set_permission", { name, tool, permission }),
  /** Opens the provider's sign-in page; resolves once the browser came back. */
  signIn: (name: string) => invoke<void>("mcp_sign_in", { name }),
  signOut: (name: string) => invoke<void>("mcp_sign_out", { name }),
  projectState: (cwd: string) => invoke<ProjectState>("mcp_project_state", { cwd }),
  openProject: (cwd: string) => invoke<void>("mcp_open_project", { cwd }),
  setProjectApproval: (cwd: string, name: string, approved: boolean) =>
    invoke<void>("mcp_set_project_approval", { cwd, name, approved }),
  readResource: (server: string, uri: string) => invoke<string>("mcp_read_resource", { server, uri }),
  getPrompt: (server: string, prompt: string, args: Record<string, string>) =>
    invoke<string>("mcp_get_prompt", { server, prompt, arguments: args }),
};
