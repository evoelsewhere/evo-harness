// What the shell's agent tools ask of the view, and what they need to know back.
//
// `app_panel` (src-tauri/src/agent_tools/app/panel.rs) emits `agent://panel`
// and this applies it to the side panel's tabs of the agent's chat. The shell
// cannot read the view, so each chat's open tabs are reported to it with
// `agent_panels_report` whenever they change; its `status` answers from that.
// Spec: docs/features/agent-browser (FR-18), plan 3.3, 3.5.
//
// The user tools (src-tauri/src/agent_tools/user.rs) ask the same way:
// `agent://notify` is the in-app notification entry of `notify` (the shell sends
// the OS notification itself, and is told with `agent_notify_muted` whether the
// person muted it), `agent://show-file` opens a file in the Files panel, and
// `agent://file-sent` follows a `send_file` so a delivered Office file opens.
// Spec: docs/features/agent-tools-layers (FR-3).

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { notify } from "@/features/notifications/notify";
import { inTauri } from "@/lib/platform";
import { openFile } from "./files-store";
import { useNotificationStore } from "./notification-store";
import { useUiStore, type PanelId } from "./ui-store";

const PANELS: readonly string[] = ["terminal", "browser", "files", "memory", "side-chat"];
const isPanel = (value: unknown): value is PanelId => typeof value === "string" && PANELS.includes(value);

/** `agent://panel`: the agent of `chat` opens or closes `panel`. */
export interface AgentPanelEvent {
  chat: string;
  panel: string;
  action: "open" | "close";
  /** The window that shows the chat (`main` when none does); other windows ignore the event. */
  window?: string;
}

/** Apply the event to the side panel tabs of its chat; `windowLabel` is this window's. */
export function applyPanelEvent(event: AgentPanelEvent, windowLabel?: string) {
  if (!isPanel(event.panel) || !event.chat) return;
  if (event.window && windowLabel && event.window !== windowLabel) return;
  const ui = useUiStore.getState();
  if (event.action === "close") ui.closeChatPanel(event.chat, event.panel);
  else if (event.action === "open") ui.openPanel(event.panel, event.chat);
}

/** An event for one window: the one showing the chat, or any when it names none. */
const forThisWindow = (event: { window?: string }, windowLabel?: string) => !(event.window && windowLabel && event.window !== windowLabel);

/** `agent://notify`: the agent of `chat` called `notify`. */
export interface AgentNotifyEvent {
  chat: string;
  title: string;
  body?: string | null;
  window?: string;
}

/** The bell entry (and toast) of a `notify` call; the OS notification is the shell's, so none here. */
export function applyNotifyEvent(event: AgentNotifyEvent, windowLabel?: string) {
  if (!event.chat || !event.title || !forThisWindow(event, windowLabel)) return;
  notify({
    title: event.title,
    body: event.body ?? undefined,
    source: "agent",
    key: `agent:${event.chat}`,
    target: { type: "session", id: event.chat },
    system: false,
  });
}

/** Whether the shell should keep its OS notification back: Do not disturb, or the agent muted. */
export const notifyMuted = (state: Pick<ReturnType<typeof useNotificationStore.getState>, "doNotDisturb" | "mutedSources">) =>
  state.doNotDisturb || state.mutedSources.includes("agent");

/** `agent://show-file`: the agent of `chat` shows a file. */
export interface AgentShowFileEvent {
  chat: string;
  path: string;
  line?: number | null;
  window?: string;
}

/** Open the file in the Files panel of its chat, scrolled to the line. */
export function applyShowFileEvent(event: AgentShowFileEvent, windowLabel?: string) {
  if (!event.chat || !event.path || !forThisWindow(event, windowLabel)) return;
  void openFile(event.path, typeof event.line === "number" ? event.line : undefined);
  useUiStore.getState().openPanel("files", event.chat);
}

/** `agent://file-sent`: the agent of `chat` delivered files with `send_file`. */
export interface AgentFileSentEvent {
  chat: string;
  paths: string[];
  window?: string;
}

/** A delivered Office file opens in Files for a live look, like one the agent builds in the chat folder. */
export function applyFileSentEvent(event: AgentFileSentEvent, windowLabel?: string) {
  if (!event.chat || !Array.isArray(event.paths) || !forThisWindow(event, windowLabel)) return;
  for (const path of event.paths) {
    void import("@/features/files/office/generated-documents").then((m) => m.openDeliveredDocument(event.chat, path));
  }
}

/** What `agent_panels_report` takes (camelCase, as the command's arguments). */
export interface PanelReport {
  chat: string;
  openTabs: PanelId[];
  activePanel: PanelId | null;
  showing: boolean;
  panelOpen: boolean;
}

type PanelState = Pick<ReturnType<typeof useUiStore.getState>, "panelChat" | "openTabs" | "activePanel" | "rightPanelOpen" | "panelsByChat">;

/** The reports `state` owes the shell after `previous` (all of them without it). */
export function panelReports(state: PanelState, previous?: PanelState): PanelReport[] {
  const reports: PanelReport[] = [];
  const current = state.panelChat;
  if (
    current &&
    (!previous ||
      previous.panelChat !== current ||
      previous.openTabs !== state.openTabs ||
      previous.activePanel !== state.activePanel ||
      previous.rightPanelOpen !== state.rightPanelOpen)
  ) {
    reports.push({ chat: current, openTabs: state.openTabs, activePanel: state.activePanel, showing: true, panelOpen: state.rightPanelOpen });
  }
  for (const [chat, saved] of Object.entries(state.panelsByChat)) {
    if (chat === current || previous?.panelsByChat[chat] === saved) continue;
    reports.push({ chat, openTabs: saved.openTabs, activePanel: saved.activePanel, showing: false, panelOpen: saved.open });
  }
  // A chat that lost its saved tabs (forgotten) has none now.
  for (const chat of Object.keys(previous?.panelsByChat ?? {})) {
    if (chat !== current && !(chat in state.panelsByChat)) {
      reports.push({ chat, openTabs: [], activePanel: null, showing: false, panelOpen: false });
    }
  }
  // The chat that was showing and is no longer reports as not showing, with its saved tabs.
  if (previous?.panelChat && previous.panelChat !== current && !reports.some((r) => r.chat === previous.panelChat)) {
    const saved = state.panelsByChat[previous.panelChat];
    reports.push({ chat: previous.panelChat, openTabs: saved?.openTabs ?? [], activePanel: saved?.activePanel ?? null, showing: false, panelOpen: saved?.open ?? false });
  }
  return reports;
}

function report(state: PanelState, previous?: PanelState) {
  for (const entry of panelReports(state, previous)) void invoke("agent_panels_report", { ...entry }).catch(() => undefined);
}

let started = false;

/** Apply the shell's panel requests and keep it told what the panels show, from now on. */
export function startAgentEvents() {
  if (started || !inTauri) return;
  started = true;
  const label = getCurrentWindow().label;
  void listen<AgentPanelEvent>("agent://panel", (event) => applyPanelEvent(event.payload, label)).catch(() => undefined);
  report(useUiStore.getState());
  useUiStore.subscribe((state, previous) => report(state, previous));

  void listen<AgentNotifyEvent>("agent://notify", (event) => applyNotifyEvent(event.payload, label)).catch(() => undefined);
  void listen<AgentShowFileEvent>("agent://show-file", (event) => applyShowFileEvent(event.payload, label)).catch(() => undefined);
  void listen<AgentFileSentEvent>("agent://file-sent", (event) => applyFileSentEvent(event.payload, label)).catch(() => undefined);
  const reportMuted = (state: ReturnType<typeof useNotificationStore.getState>) =>
    void invoke("agent_notify_muted", { muted: notifyMuted(state) }).catch(() => undefined);
  reportMuted(useNotificationStore.getState());
  useNotificationStore.subscribe((state, previous) => {
    if (notifyMuted(state) !== notifyMuted(previous)) reportMuted(state);
  });
}
