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
import { useComputerStore, type AttachedEvent, type FrontEvent, type ReleasedEvent, type StateEvent } from "./computer-store";
import { activityLabel } from "@/features/cards/activity-label";
import { cardKey, useCardsStore } from "./cards-store";
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

/** `browser://driven`: a tool of `chat` used `tab` for the first time. */
export interface AgentDrivenEvent {
  tab: string;
  chat: string;
}

/** The page an agent uses gets a card (FR-1). */
export function applyDrivenEvent(event: AgentDrivenEvent) {
  if (!event.tab || !event.chat) return;
  useCardsStore.getState().ensure(event.chat, "browser", event.tab);
}

// ── The app a chat's agent controls (docs/features/computer-app-control FR-10, FR-12) ──

/** `computer://attached`: the chat's app gets a card. */
export function applyComputerAttached(event: AttachedEvent) {
  if (!event.chat) return;
  useComputerStore.getState().attached(event);
  useCardsStore.getState().ensure(event.chat, "app", event.chat);
}

/** `computer://released`: the app went back; its card goes too, unless control was stopped (Allow again). */
export function applyComputerReleased(event: ReleasedEvent) {
  const store = useComputerStore.getState();
  const cards = useCardsStore.getState();
  const chats = event.chat === null ? Object.keys(store.apps) : [event.chat];
  store.released(event);
  const after = useComputerStore.getState().apps;
  for (const chat of event.chat === null ? Object.keys(cards.cards) : chats) {
    if (!after[chat]) cards.remove(chat, cardKey("app", chat));
  }
}

/**
 * After a reload the cards come back from the session storage but the apps do
 * not: ask the shell for each app card whether its chat still has an app
 * attached (or stopped), and drop the cards that have none.
 */
export async function restoreAppCards() {
  const cards = useCardsStore.getState().cards;
  for (const [chat, list] of Object.entries(cards)) {
    if (!list.some((c) => c.kind === "app")) continue;
    const status = await invoke<{ attached?: boolean; stopped?: boolean; window?: { app?: string; title?: string; id?: number } } | null>("computer_status", { chat }).catch(() => null);
    if (status?.attached && status.window) {
      useComputerStore.getState().attached({ chat, app: status.window.app, title: status.window.title, windowId: status.window.id ?? null });
      if (status.stopped) useComputerStore.getState().setState({ chat, state: "stopped" });
    } else if (status?.stopped) {
      useComputerStore.getState().attached({ chat, app: "", title: "App" });
      useComputerStore.getState().setState({ chat, state: "stopped" });
    } else {
      useCardsStore.getState().remove(chat, cardKey("app", chat));
    }
  }
}

/** `computer://state`: Stop or Allow again. */
export function applyComputerState(event: StateEvent) {
  useComputerStore.getState().setState(event);
}

/** `computer://front`: the app became, or stopped being, the person's front window (FR-30). */
export function applyComputerFront(event: FrontEvent) {
  useComputerStore.getState().setFront(event);
}

/** `agent://activity`: a browser or terminal tool of `chat` started or ended (only a page has a card). */
export interface AgentActivityEvent {
  chat: string;
  kind: "browser" | "terminal" | "computer";
  tool: string;
  phase: "start" | "end";
  tab: string | null;
  args?: Record<string, unknown>;
}

/** The card a call marked busy when it started, by the call's id, so its end clears that one. */
const busyCalls = new Map<string, { chat: string; key: string; before: string | null }>();

/** How long the words of a finished call stay on a card. */
const WORDS_LINGER_MS = 2500;
const fading = new Map<string, ReturnType<typeof setTimeout>>();

/** Say on the card of the tab what the agent is doing (FR-2); a page a tool works on gets a card too. */
export function applyActivityEvent(event: AgentActivityEvent & { requestId?: string }) {
  if (!event.chat) return;
  const label = activityLabel(event.tool, event.args ?? {});
  if (!label) return;
  const cards = useCardsStore.getState();
  if (event.kind === "computer") {
    // The app card of the chat says what the agent does in the app.
    const key = cardKey("app", event.chat);
    if (!cards.cards[event.chat]?.some((c) => c.key === key)) return;
    clearTimeout(fading.get(key));
    cards.setActivity(event.chat, key, label, event.phase === "start");
    if (event.phase === "end") {
      const { chat } = event;
      fading.set(
        key,
        setTimeout(() => {
          fading.delete(key);
          const now = useCardsStore.getState();
          const card = now.cards[chat]?.find((c) => c.key === key);
          if (card && !card.busy && card.activity === label) now.setActivity(chat, key, null);
        }, WORDS_LINGER_MS),
      );
    }
    return;
  }
  if (event.kind !== "browser") return;
  // The tab a call starts on can differ from the one it ends on (a call that opens a page), so the
  // end first clears what the start marked.
  if (event.phase === "end" && event.requestId) {
    const before = busyCalls.get(event.requestId);
    busyCalls.delete(event.requestId);
    // A call that moved to another page (it opened one): the card it started on gets its old words back.
    const moved = before && event.tab && before.key !== cardKey("browser", event.tab);
    if (before) cards.setActivity(before.chat, before.key, moved ? before.before : (cards.cards[before.chat]?.find((c) => c.key === before.key)?.activity ?? null), false);
  }
  if (!event.tab) return;
  cards.ensure(event.chat, "browser", event.tab);
  const key = cardKey("browser", event.tab);
  if (event.phase === "start" && event.requestId) busyCalls.set(event.requestId, { chat: event.chat, key, before: cards.cards[event.chat]?.find((c) => c.key === key)?.activity ?? null });
  clearTimeout(fading.get(key));
  cards.setActivity(event.chat, key, label, event.phase === "start");
  if (event.phase === "end") {
    // The words of a call that has finished fade away, so a card never keeps saying "Waiting for…".
    const { chat } = event;
    fading.set(
      key,
      setTimeout(() => {
        fading.delete(key);
        const now = useCardsStore.getState();
        const card = now.cards[chat]?.find((c) => c.key === key);
        if (card && !card.busy && card.activity === label) now.setActivity(chat, key, null);
      }, WORDS_LINGER_MS),
    );
  }
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

  void listen<AgentDrivenEvent>("browser://driven", (event) => applyDrivenEvent(event.payload)).catch(() => undefined);
  void listen<AgentActivityEvent>("agent://activity", (event) => applyActivityEvent(event.payload)).catch(() => undefined);
  void listen<AttachedEvent>("computer://attached", (event) => applyComputerAttached(event.payload)).catch(() => undefined);
  void listen<ReleasedEvent>("computer://released", (event) => applyComputerReleased(event.payload)).catch(() => undefined);
  void listen<StateEvent>("computer://state", (event) => applyComputerState(event.payload)).catch(() => undefined);
  void listen<FrontEvent>("computer://front", (event) => applyComputerFront(event.payload)).catch(() => undefined);
  void restoreAppCards();
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
