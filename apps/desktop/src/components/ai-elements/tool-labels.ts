// The words of a tool call's row in the transcript — its verb, subject and
// state — without React, so the desktop's Tool row (`tool.tsx`) and Evo
// Webbridge's side panel (built from `src/lib/webbridge-chat.ts`) say the same
// thing for the same call (docs/features/browser-bridge/spec.md, FR-10).

import type { ChildRun } from "@/engine/mesh";
import { partialField, type ToolPart } from "@/features/chat/transcript";
import { kinds, type Kind } from "./tool-kinds";

/** The subagent a call started or continued (`details.child`). */
export const childOf = (part: ToolPart): ChildRun | undefined =>
  part.details && typeof part.details === "object" ? (part.details.child as ChildRun | undefined) : undefined;

/** A call's output when it was stopped rather than failed (the run was aborted or cancelled). */
export const STOPPED = /^(?:error:\s*)?(?:operation (?:was )?aborted|aborted|cancelled|canceled|interrupted)\b/i;

export const verbs: Record<Kind, Record<ToolPart["phase"], string>> = {
  shell: { preparing: "Running", running: "Running", done: "Ran", error: "Failed to run" },
  read: { preparing: "Reading", running: "Reading", done: "Read", error: "Failed to read" },
  write: { preparing: "Writing", running: "Writing", done: "Wrote", error: "Failed to write" },
  edit: { preparing: "Editing", running: "Editing", done: "Edited", error: "Failed to edit" },
  search: { preparing: "Searching", running: "Searching", done: "Searched", error: "Failed to search" },
  list: { preparing: "Listing", running: "Listing", done: "Listed", error: "Failed to list" },
  web: { preparing: "Fetching", running: "Fetching", done: "Fetched", error: "Failed to fetch" },
  browser: { preparing: "Using the browser", running: "Using the browser", done: "Used the browser", error: "Browser step failed" },
  computer: { preparing: "Using an app", running: "Using an app", done: "Used an app", error: "App step failed" },
  agent: { preparing: "Starting agent", running: "Agent", done: "Agent", error: "Agent failed" },
  session: { preparing: "Session", running: "Session", done: "Session", error: "Session failed" },
  skill: { preparing: "Loading skill", running: "Loading skill", done: "Skill", error: "Skill failed" },
  memory: { preparing: "Using memory", running: "Using memory", done: "Memory", error: "Memory failed" },
  ask: { preparing: "Asking you", running: "Waiting for your answer", done: "Asked you", error: "Question failed" },
  todo: { preparing: "Updating tasks", running: "Updating tasks", done: "Updated tasks", error: "Task update failed" },
  code: { preparing: "Checking code", running: "Checking code", done: "Checked code", error: "Code check failed" },
  user: { preparing: "Sending", running: "Sending", done: "Sent", error: "Could not send" },
  mcp: { preparing: "Calling", running: "Calling", done: "Called", error: "Failed" },
  visual: { preparing: "Drawing", running: "Drawing", done: "Drew", error: "Failed to draw" },
  other: { preparing: "Calling", running: "Calling", done: "Called", error: "Failed" },
};

/** What a category of tool does, for a group's title ("Ran commands, edited files"). */
export const activity: Record<Kind, { live: string; done: string }> = {
  shell: { live: "Running commands", done: "ran commands" },
  read: { live: "Reading files", done: "read files" },
  write: { live: "Writing files", done: "wrote files" },
  edit: { live: "Editing files", done: "edited files" },
  search: { live: "Searching", done: "searched" },
  list: { live: "Listing files", done: "listed files" },
  web: { live: "Browsing the web", done: "browsed the web" },
  browser: { live: "Using the browser", done: "used the browser" },
  computer: { live: "Working in an app", done: "worked in an app" },
  agent: { live: "Running agents", done: "ran agents" },
  session: { live: "Working with sessions", done: "worked with sessions" },
  skill: { live: "Loading skills", done: "loaded skills" },
  memory: { live: "Using memory", done: "used memory" },
  ask: { live: "Asking you", done: "asked you" },
  todo: { live: "Updating tasks", done: "updated tasks" },
  code: { live: "Checking code", done: "checked code" },
  user: { live: "Reaching you", done: "reached you" },
  mcp: { live: "Using connectors", done: "used connectors" },
  visual: { live: "Drawing widgets", done: "drew widgets" },
  other: { live: "Using tools", done: "used tools" },
};

/**
 * A computer app call in one line: the verb and its steps, e.g. "Used an app ·
 * click e12, type "Dear team…", key ctrl+s" (docs/features/computer-app-control).
 */
export function computerLabel(part: ToolPart, phase: ToolPart["phase"]): string {
  const actions = Array.isArray((part.args as any)?.actions) ? ((part.args as any).actions as Record<string, unknown>[]) : [];
  const short = (value: unknown, max = 24) => {
    const text = String(value ?? "");
    return text.length > max ? `${text.slice(0, max)}…` : text;
  };
  const step = (a: Record<string, unknown>) => {
    const name = String(a.action ?? "");
    if (name === "type") return `type "${short(a.text)}"`;
    if (name === "key") return `key ${short(a.key)}`;
    if (name === "attach") return `attach ${short(a.app ?? a.title ?? a.window_id ?? "")}`.trim();
    if (name === "open_app") return `open ${short(a.app)}`;
    const target = a.ref ? ` ${a.ref}` : a.x !== undefined && a.y !== undefined ? ` (${a.x}, ${a.y})` : "";
    return `${name.replace("_", " ")}${target}`;
  };
  const steps = actions.slice(0, 3).map(step).join(", ") + (actions.length > 3 ? `, +${actions.length - 3}` : "");
  return `${verbs.computer[phase]}${steps ? ` · ${steps}` : ""}`;
}

export const toolKind = (name: string): Kind => kinds[name] ?? (name.startsWith("mcp__") ? "mcp" : "other");

/** "Checked errors · src/main.rs", "Found references · add", … */
export function codeLabel(part: ToolPart, phase: ToolPart["phase"]): string {
  const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, unknown>;
  const live = phase === "preparing" || phase === "running";
  const verbs: Record<string, [string, string]> = {
    diagnostics: ["Checking for errors", "Checked for errors"],
    definition: ["Finding the definition", "Found the definition"],
    type_definition: ["Finding the type", "Found the type"],
    implementation: ["Finding implementations", "Found implementations"],
    references: ["Finding references", "Found references"],
    hover: ["Reading type info", "Read type info"],
    symbols: ["Outlining", "Outlined"],
    workspace_symbols: ["Searching symbols", "Searched symbols"],
  };
  const [doing, did] = verbs[String(args.operation ?? "")] ?? ["Checking code", "Checked code"];
  const what =
    (typeof args.symbol === "string" && args.symbol) ||
    (typeof args.query === "string" && args.query) ||
    (typeof args.path === "string" && String(args.path).split(/[\\/]/).pop()) ||
    "";
  return `${phase === "error" ? "Code check failed" : live ? doing : did}${what ? ` · ${what}` : ""}`;
}

/** "Searched the web · query", "Read · docs.rs/serde". */
export function webLabel(part: ToolPart, phase: ToolPart["phase"]): string {
  const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, unknown>;
  const live = phase === "preparing" || phase === "running";
  if (part.name === "web_search") {
    const query = typeof args.query === "string" ? args.query : partialField(part.argsText, "query");
    return `${phase === "error" ? "Search failed" : live ? "Searching the web" : "Searched the web"}${query ? ` · ${query}` : ""}`;
  }
  const raw = typeof args.url === "string" ? args.url : partialField(part.argsText, "url");
  let where = raw ?? "";
  try {
    const url = new URL(where.includes("://") ? where : `https://${where}`);
    where = `${url.hostname.replace(/^www\./, "")}${url.pathname === "/" ? "" : url.pathname}`;
  } catch {
    // Keep it as typed.
  }
  return `${phase === "error" ? "Could not read" : live ? "Reading" : "Read"}${where ? ` · ${where}` : ""}`;
}

/** The page's text without the `<web_page>` wrapper. */
export function webPageBody(output: string | undefined): string {
  const text = output ?? "";
  const match = /^<web_page [^>]*>\n([\s\S]*?)\n<\/web_page>/.exec(text);
  return match ? match[1] + text.slice(match[0].length) : text;
}

/** "Saved memory · prefers-tabs", "Reading memory", … */
export function memoryLabel(part: ToolPart, phase: ToolPart["phase"]): string {
  const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, unknown>;
  const name = typeof args.name === "string" ? args.name : partialField(part.argsText, "name");
  const live = phase === "preparing" || phase === "running";
  const updated = /^Updated /.test(part.output ?? "");
  const verb: Record<string, [string, string]> = {
    save: ["Saving memory", updated ? "Updated memory" : "Saved memory"],
    update: ["Updating memory", "Updated memory"],
    read: ["Reading memory", "Read memory"],
    list: ["Listing memories", "Listed memories"],
    delete: ["Forgetting", "Forgot"],
  };
  const [doing, did] = verb[String(args.action ?? "")] ?? ["Using memory", "Used memory"];
  // A sensitive subject the person allowed is marked (spec FR-17).
  const sensitive = phase === "done" && part.details?.sensitive === true ? " · sensitive" : "";
  return `${phase === "error" ? "Memory failed" : live ? doing : did}${name ? ` · ${name}` : ""}${sensitive}`;
}

/** The first loading message of a `show_widget` call, from its arguments or the JSON still streaming. */
export function widgetLoading(part: ToolPart): string | undefined {
  const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, unknown>;
  if (Array.isArray(args.loading_messages) && typeof args.loading_messages[0] === "string") return args.loading_messages[0];
  const raw = /"loading_messages"\s*:\s*\[\s*"((?:[^"\\]|\\.)*)"/.exec(part.argsText)?.[1];
  if (raw === undefined) return undefined;
  try {
    return JSON.parse(`"${raw}"`) as string;
  } catch {
    return raw;
  }
}

/** "Widget · Revenue by region", "Read the widget guide · chart, diagram", "Read the block guide · stats, bars". */
export function visualLabel(part: ToolPart, phase: ToolPart["phase"]): string {
  const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, unknown>;
  const live = phase === "preparing" || phase === "running";
  // The UI block guide (docs/features/rich-replies/spec.md, FR-11).
  if (part.name === "ui_guide") {
    const components = Array.isArray(args.components) ? args.components.map(String).join(", ") : "";
    return `${phase === "error" ? "Could not read the block guide" : live ? "Reading the block guide" : "Read the block guide"}${
      components ? ` · ${components}` : ""
    }`;
  }
  if (part.name === "visualize_read_me") {
    const modules = Array.isArray(args.modules) ? args.modules.map(String).join(", ") : "";
    return `${phase === "error" ? "Could not read the widget guide" : live ? "Reading the widget guide" : "Read the widget guide"}${
      modules ? ` · ${modules}` : ""
    }`;
  }
  const title = typeof args.title === "string" ? args.title : partialField(part.argsText, "title");
  return `${phase === "error" ? "Widget failed" : live ? "Building widget" : "Widget"}${title ? ` · ${title}` : ""}`;
}

/** A loaded skill's instructions without the `<skill …>` wrapper. */
export function skillBody(output: string | undefined): string {
  const text = output ?? "";
  const match = /^<skill [^>]*>\n(?:References are relative to [^\n]*\n\n)?([\s\S]*?)\n<\/skill>/.exec(text);
  return match ? match[1] + text.slice(match[0].length) : text;
}

const FIELDS = ["command", "path", "file_path", "pattern", "query", "url"];
/** An agent call is named by its short description, a session by its title. */
const AGENT_FIELDS = ["description"];
const SESSION_FIELDS = ["title"];
const SKILL_FIELDS = ["name"];

/** The call's subject — from the parsed arguments, or from the JSON still streaming. */
export function subject(part: ToolPart): string | undefined {
  const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, unknown>;
  const kind = toolKind(part.name);
  for (const field of kind === "agent" ? AGENT_FIELDS : kind === "session" ? SESSION_FIELDS : kind === "skill" ? SKILL_FIELDS : FIELDS) {
    const value = typeof args[field] === "string" ? (args[field] as string) : partialField(part.argsText, field);
    if (value) {
      const line = value.split("\n")[0];
      return toolKind(part.name) === "shell" ? line : line.replace(/\\/g, "/");
    }
  }
  return undefined;
}

/**
 * "Agent · Review the diff"; a `message_agent` call continues an agent and is
 * named after it: "Agent · Review the diff (continued)".
 */
export function agentLabel(part: ToolPart, phase: ToolPart["phase"], description: string | undefined): string {
  const about = subject(part) ?? description;
  if (part.name !== "message_agent") return `${verbs.agent[phase]}${about ? ` · ${about}` : ""}`;
  const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, unknown>;
  const id = typeof args.agentId === "string" ? args.agentId : partialField(part.argsText, "agentId");
  const name = description || (id ? `agent ${id}` : "");
  if (phase === "preparing") return `Continuing agent${name ? ` · ${name}` : ""}`;
  return `${verbs.agent[phase]}${name ? ` · ${name}` : ""} (continued)`;
}

/** One terminal step in words: "Pressed Ctrl+C in Git Bash tab t3", "Read tab t2", "Stopped tab t3". */
export function terminalLabel(part: ToolPart, phase: ToolPart["phase"]): string {
  const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, any>;
  const live = phase === "preparing" || phase === "running";
  const failed = phase === "error";
  const quote = (value: unknown, max = 40) => {
    const text = String(value ?? "");
    return `"${text.length > max ? `${text.slice(0, max)}…` : text}"`;
  };
  // The result starts with "Tab t3 · Git Bash · …"; before it arrives, the tab id from the arguments.
  const header = /^Tab (\S+) · ([^·]+?) ·/.exec(part.output ?? "");
  const tab = header ? `${header[2]} tab ${header[1]}` : args.tabId ? `tab ${args.tabId}` : "a terminal";
  const key = (name: unknown) =>
    String(name)
      .split("+")
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
      .join("+");
  const keys = Array.isArray(args.keys) ? args.keys.map(key).join(", ") : "";
  const sent = [args.text ? quote(args.text) : "", keys].filter(Boolean).join(" and ");
  const shell = args.shell ? ` in ${args.shell}` : header ? ` in ${header[2]}` : "";
  const words: Record<string, [string, string]> = {
    terminal_list: ["Listing terminal tabs", "Listed terminal tabs"],
    terminal_run: [
      `Running ${quote(args.command, 60)}${args.tabId ? ` in ${tab}` : shell}`,
      `Ran ${quote(args.command, 60)}${args.tabId ? ` in ${tab}` : shell}`,
    ],
    terminal_send: [`Sending ${sent || "keys"} to ${tab}`, `${keys && !args.text ? "Pressed" : "Sent"} ${sent || "keys"} in ${tab}`],
    terminal_read: [`Reading ${tab}`, `Read ${tab}`],
    terminal_kill: [`Stopping ${tab}`, `Stopped ${tab}${args.close ? " and closed it" : ""}`],
  };
  const [doing, did] = words[part.name] ?? ["Using the terminal", "Used the terminal"];
  return failed ? `${did} (failed)` : live ? doing : did;
}

/** One browser step in words: "Opened localhost:3000", "Clicked ref_4", "Took a screenshot". */
export function browserLabel(part: ToolPart, phase: ToolPart["phase"]): string {
  const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, any>;
  const live = phase === "preparing" || phase === "running";
  const failed = phase === "error";
  const quote = (value: unknown, max = 40) => {
    const text = String(value ?? "");
    return `"${text.length > max ? `${text.slice(0, max)}…` : text}"`;
  };
  const target = args.ref ?? (typeof args.x === "number" ? `(${Math.round(args.x)}, ${Math.round(args.y)})` : "");
  const words: Record<string, [string, string]> = {
    browser_navigate: [
      ["back", "forward", "reload"].includes(args.url) ? `Going ${args.url}` : `Opening ${args.url ?? ""}`,
      ["back", "forward", "reload"].includes(args.url) ? `Went ${args.url}` : `Opened ${args.url ?? ""}`,
    ],
    browser_tabs: ["Checking browser tabs", "Checked browser tabs"],
    browser_screenshot: ["Taking a screenshot", "Took a screenshot"],
    browser_read_page: ["Reading the page", "Read the page"],
    browser_find: [`Finding ${quote(args.query)}`, `Found ${quote(args.query)}`],
    browser_click: [`Clicking ${target}`, `Clicked ${target}`],
    browser_type: [`Typing ${quote(args.text)}`, `Typed ${quote(args.text)}`],
    browser_key: [`Pressing ${args.key ?? ""}`, `Pressed ${args.key ?? ""}`],
    browser_form_input: [`Setting ${args.ref ?? "a field"}`, `Set ${args.ref ?? "a field"} to ${quote(args.value)}`],
    browser_scroll: [`Scrolling ${args.direction ?? ""}`, `Scrolled ${args.direction ?? ""}`],
    browser_eval: ["Running script in the page", "Ran script in the page"],
    browser_console: ["Reading the console", "Read the console"],
    browser_network: [args.requestId ? "Reading a network request" : "Reading network requests", args.requestId ? "Read a network request" : "Read network requests"],
    browser_hover: [`Hovering ${target}`, `Hovered ${target}`],
    browser_drag: [`Dragging ${args.ref ?? "an element"}`, `Dragged ${args.ref ?? "an element"}`],
    browser_wait: [
      args.text ? `Waiting for ${quote(args.text)}` : "Waiting on the page",
      args.text ? `Waited for ${quote(args.text)}` : "Waited on the page",
    ],
    browser_dialog: [
      args.action === "status" ? "Checking the page's dialog" : `${args.accept === false ? "Dismissing" : "Answering"} the page's dialog`,
      args.action === "status" ? "Checked the page's dialog" : `${args.accept === false ? "Dismissed" : "Answered"} the page's dialog`,
    ],
    browser_upload: [`Attaching a file to ${args.ref ?? "the page"}`, `Attached a file to ${args.ref ?? "the page"}`],
    browser_frames: ["Listing the page's frames", "Listed the page's frames"],
    browser_batch: [
      `Running ${Array.isArray(args.steps) ? args.steps.length : "several"} browser steps`,
      `Ran ${Array.isArray(args.steps) ? args.steps.length : "several"} browser steps`,
    ],
    browser_viewport: [args.reset ? "Resetting the viewport" : "Changing the viewport", args.reset ? "Reset the viewport" : "Changed the viewport"],
    browser_storage: ["Changing cookies or storage", "Changed cookies or storage"],
    browser_storage_read: ["Reading cookies and storage", "Read cookies and storage"],
    browser_mock: [
      args.action === "clear" ? "Clearing request mocks" : "Mocking requests",
      args.action === "clear" ? "Cleared request mocks" : "Mocked requests",
    ],
    browser_downloads: ["Listing downloads", "Listed downloads"],
    browser_download_folder: ["Setting the download folder", "Set the download folder"],
    browser_performance: ["Measuring page performance", "Measured page performance"],
    browser_pdf: ["Saving the page as a PDF", "Saved the page as a PDF"],
    browser_status: ["Checking the browser", "Checked the browser"],
  };
  const [doing, done] = words[part.name] ?? ["Using the browser", "Used the browser"];
  return failed ? `Browser: ${done.charAt(0).toLowerCase()}${done.slice(1)} failed` : live ? doing : done;
}

/** The first line a read started at or an edit changed (1-based). */
export function touchedLine(part: ToolPart): number | undefined {
  const args = (part.args && typeof part.args === "object" ? part.args : {}) as Record<string, unknown>;
  if (typeof args.offset === "number" && args.offset > 0) return args.offset;
  if (typeof args.line === "number" && args.line > 0) return args.line;
  const diff = typeof part.details?.diff === "string" ? part.details.diff : "";
  const changed = /^[+-]\s*(\d+)/m.exec(diff);
  return changed ? Number(changed[1]) : undefined;
}

export interface DiffLine {
  variant: "default" | "added" | "removed";
  text: string;
}

/** The edit tool's `details.diff`: `+12 new`, `-12 old`, ` 12 same`, ` …`. */
export function parseDiff(diff: string): { lines: DiffLine[]; added: number; removed: number } {
  let added = 0;
  let removed = 0;
  const lines = diff.split("\n").map((raw) => {
    const marker = raw[0];
    const text = raw.slice(1).replace(/^\s*\d+\s?/, "");
    if (marker === "+") {
      added++;
      return { variant: "added" as const, text };
    }
    if (marker === "-") {
      removed++;
      return { variant: "removed" as const, text };
    }
    return { variant: "default" as const, text: raw.trim() === "..." ? "…" : text };
  });
  return { lines, added, removed };
}

/** What a tool row shows on its line: its kind, words and state. */
export interface ToolHeader {
  kind: Kind;
  /** The file, command or query the call is about. */
  about?: string;
  /** The phase shown: a stopped call reads as running, in neutral colours. */
  phase: ToolPart["phase"];
  stopped: boolean;
  live: boolean;
  failed: boolean;
  child?: ChildRun;
  /** The whole line, " · stopped" included. */
  label: string;
  /** For a call on a file: the verb before the file link ("Read", "Edited"). */
  fileVerb?: string;
  /** An edit's diff. */
  diff?: ReturnType<typeof parseDiff>;
}

/** A service's name from its toolkit slug (`gmail` → `Gmail`). */
const serviceName = (toolkit: string) => toolkit.charAt(0).toUpperCase() + toolkit.slice(1);

/**
 * A Connected apps call in one line (docs/features/connected-apps/spec.md,
 * FR-19): "Called Gmail · GMAIL_SEND_EMAIL (work)" for the app tools it runs,
 * the meta-tool's own words otherwise.
 */
export function connectedAppLabel(tool: string, args: unknown, phase: ToolPart["phase"]): string {
  const verb = phase === "error" ? "Failed" : verbs.mcp[phase];
  const a = (args && typeof args === "object" ? args : {}) as Record<string, any>;
  if (tool === "COMPOSIO_MULTI_EXECUTE_TOOL") {
    const tools: Record<string, any>[] = Array.isArray(a.tools) ? a.tools : [];
    const first = tools[0];
    if (!first?.tool_slug) return `${verb} connected apps`;
    const slug = String(first.tool_slug);
    const account = first.account ?? first.connected_account_id ?? first.alias;
    const more = tools.length > 1 ? ` +${tools.length - 1}` : "";
    return `${verb} ${serviceName(slug.split("_")[0].toLowerCase())} · ${slug}${account ? ` (${String(account)})` : ""}${more}`;
  }
  if (tool === "COMPOSIO_SEARCH_TOOLS") {
    const queries: Record<string, any>[] = Array.isArray(a.queries) ? a.queries : [];
    const query = queries[0]?.use_case ?? a.query ?? a.use_case;
    const done = phase === "error" ? "Search failed" : phase === "done" ? "Searched connected apps" : "Searching connected apps";
    return `${done}${query ? ` · ${String(query)}` : ""}`;
  }
  if (tool === "COMPOSIO_MANAGE_CONNECTIONS") {
    const toolkits = Array.isArray(a.toolkits) ? a.toolkits.map(String) : [];
    return `${phase === "done" ? "Asked to connect" : "Asking to connect"}${toolkits.length ? ` · ${toolkits.map(serviceName).join(", ")}` : ""}`;
  }
  return `${verb} connected apps · ${tool.replace(/^COMPOSIO_/, "").toLowerCase().replace(/_/g, " ")}`;
}

/** The line of a tool call; `streaming`: its turn is still running. */
export function toolHeader(part: ToolPart, streaming: boolean): ToolHeader {
  const kind = toolKind(part.name);
  const about = subject(part);
  // A call that never finished while the turn is over was interrupted; one
  // the person stopped (or that ended with the run) reads as stopped, in
  // neutral colours, not as a failure (docs/features/ux-polish/spec.md, FR-4).
  const interrupted = !streaming && (part.phase === "running" || part.phase === "preparing");
  const stopped = interrupted || (part.phase === "error" && STOPPED.test(part.output ?? ""));
  const phase: ToolPart["phase"] = stopped ? "running" : part.phase;
  // A background subagent runs on after its call returned (`agent`, or `message_agent` continuing one).
  const child = kind === "agent" ? childOf(part) : undefined;
  const live =
    !stopped &&
    (phase === "preparing" || phase === "running" || (phase === "done" && Boolean(child?.background) && child?.status === "running"));
  // A background agent's call succeeded when it started; the agent may still have failed.
  const failed = !stopped && (phase === "error" || (Boolean(child?.background) && child?.status === "error"));
  const sessionLabel: Record<string, string> = {
    start_session: phase === "done" ? "Session" : "Starting a session",
    message_session: phase === "done" ? "Messaged a session" : "Messaging a session",
    list_sessions: phase === "done" ? "Listed sessions" : "Listing sessions",
    session_search: phase === "done" ? "Searched past chats" : "Searching past chats",
    session_read: phase === "done" ? "Read a past chat" : "Reading a past chat",
  };
  const searched = part.name === "session_search" ? String((part.args as any)?.query ?? "").trim() : "";
  const label =
    kind === "session"
      ? `${phase === "error" ? "Session tool failed" : (sessionLabel[part.name] ?? part.name)}${
          about && part.name === "start_session" ? ` · ${about}` : ""
        }${searched ? ` · ${searched}` : ""}`
      : kind === "memory"
      ? memoryLabel(part, phase)
      : kind === "code"
      ? codeLabel(part, phase)
      : part.name === "notify"
      ? `${phase === "error" ? "Notification not sent" : phase === "done" ? "Notified you" : "Notifying you"}${
          (part.args as any)?.title ? ` · ${String((part.args as any).title)}` : ""
        }`
      : part.name === "send_file"
      ? `${phase === "error" ? "Could not send files" : phase === "done" ? "Sent you" : "Sending"}${(() => {
          const paths = (part.args as any)?.paths;
          const names = Array.isArray(paths) ? paths.map((p: unknown) => String(p).split(/[\\/]/).pop()) : [];
          return names.length ? ` · ${names.slice(0, 3).join(", ")}${names.length > 3 ? ` +${names.length - 3}` : ""}` : "";
        })()}`
      : kind === "ask"
      ? `${verbs.ask[phase]}${(() => {
          const qs = (part.args as any)?.questions;
          return Array.isArray(qs) && qs[0]?.question ? ` · ${String(qs[0].question)}` : "";
        })()}`
      : kind === "todo"
      ? `${verbs.todo[phase]}${(() => {
          const todos = (part.args as any)?.todos;
          if (!Array.isArray(todos)) return "";
          const done = todos.filter((t: any) => t?.status === "completed").length;
          return ` · ${done}/${todos.length} done`;
        })()}`
      : part.name === "mcp_search_tools"
      ? `${phase === "error" ? "Tool search failed" : phase === "done" ? "Searched connector tools" : "Searching connector tools"}${
          (part.args as any)?.query ? ` · ${String((part.args as any).query)}` : ""
        }`
      : kind === "mcp"
      ? (() => {
          // `mcp_call_tool` runs the tool named in its arguments.
          const name = part.name === "mcp_call_tool" ? String((part.args as any)?.name ?? "") : part.name;
          const args = part.name === "mcp_call_tool" ? (part.args as any)?.arguments : part.args;
          const [, server = "", ...rest] = name.split("__");
          if (server === "connected_apps") return connectedAppLabel(rest.join("__"), args, phase);
          return `${phase === "error" ? "Failed" : verbs.mcp[phase]} ${server} · ${rest.join("__")}`;
        })()
      : part.name.startsWith("terminal_")
      ? terminalLabel(part, phase)
      : kind === "agent"
      ? agentLabel(part, failed ? "error" : phase, child?.description)
      : kind === "skill"
      ? `${verbs[kind][phase]}${about ? ` · ${about}` : ""}`
      : kind === "browser"
      ? browserLabel(part, phase)
      : kind === "computer"
      ? computerLabel(part, phase)
      : kind === "web"
      ? webLabel(part, phase)
      : kind === "visual"
      ? visualLabel(part, phase)
      : `${verbs[kind][phase]} ${about ?? (kind === "other" ? part.name : "")}`.trim();
  const shownLabel = stopped ? `${label} · stopped` : label;
  const fileVerb =
    about && (kind === "read" || kind === "write" || kind === "edit")
      ? part.name === "show_file"
        ? phase === "done"
          ? "Showed"
          : verbs[kind][phase]
        : verbs[kind][phase]
      : undefined;
  const diff = kind === "edit" && typeof part.details?.diff === "string" ? parseDiff(part.details.diff) : undefined;
  return { kind, about, phase, stopped, live, failed, child, label: shownLabel, fileVerb, diff };
}
