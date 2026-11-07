/**
 * Which kind of row a tool gets in the transcript: one map from a tool's name to
 * its kind (docs/features/agent-tools-layers, FR-8). Adding a tool adds a line
 * here; icons, verbs and activity wording are chosen by kind in `tool.tsx`.
 */

export type Kind =
  | "shell"
  | "read"
  | "write"
  | "edit"
  | "search"
  | "list"
  | "web"
  | "browser"
  | "agent"
  | "session"
  | "skill"
  | "memory"
  | "ask"
  | "todo"
  | "code"
  | "user"
  | "mcp"
  | "visual"
  | "other";

export const kinds: Record<string, Kind> = {
  bash: "shell",
  powershell: "shell",
  read: "read",
  write: "write",
  edit: "edit",
  grep: "search",
  find: "search",
  ls: "list",
  web_fetch: "web",
  web_search: "web",
  agent: "agent",
  skill: "skill",
  start_session: "session",
  message_session: "session",
  list_sessions: "session",
  show_file: "read",
  document_preview: "read",
  terminal_run: "shell",
  terminal_read: "shell",
  terminal_list: "shell",
  terminal_send: "shell",
  terminal_kill: "shell",
  browser_navigate: "browser",
  browser_tabs: "browser",
  browser_screenshot: "browser",
  browser_read_page: "browser",
  browser_find: "browser",
  browser_click: "browser",
  browser_type: "browser",
  browser_key: "browser",
  browser_form_input: "browser",
  browser_scroll: "browser",
  browser_eval: "browser",
  browser_console: "browser",
  browser_network: "browser",
  browser_hover: "browser",
  browser_drag: "browser",
  browser_wait: "browser",
  browser_dialog: "browser",
  browser_upload: "browser",
  browser_frames: "browser",
  browser_batch: "browser",
  browser_viewport: "browser",
  browser_storage: "browser",
  browser_storage_read: "browser",
  browser_mock: "browser",
  browser_downloads: "browser",
  browser_download_folder: "browser",
  browser_performance: "browser",
  browser_pdf: "browser",
  browser_status: "browser",
  session_read: "session",
  session_search: "session",
  memory: "memory",
  ask_user: "ask",
  update_todos: "todo",
  lsp: "code",
  send_file: "user",
  notify: "user",
  mcp_search_tools: "mcp",
  mcp_call_tool: "mcp",
  show_widget: "visual",
  visualize_read_me: "visual",
};
