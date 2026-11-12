// View state that is not owned by the engine.

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { emit } from "@tauri-apps/api/event";
import type { ApprovalMode } from "@/engine/protocol";
import { db, log } from "@/lib/db";

export type PanelId = "terminal" | "browser" | "files" | "side-chat" | "memory";
/** Panels that exist; a saved tab for any other (Plugins moved to Customize, Scheduler to its page) is dropped. */
const PANEL_IDS: readonly string[] = ["terminal", "browser", "files", "side-chat", "memory"];
export type { ApprovalMode };
/** The app setting that holds the approval mode (read by the shell for every engine it starts). */
export const APPROVAL_MODE_KEY = "approvalMode";
/** Emitted after the mode changed (payload `{ mode }`): every window tells its engines. */
export const APPROVAL_CHANGED = "approval://changed";
const APPROVAL_MODES: readonly string[] = ["ask", "auto-edits", "auto"];
const isApprovalMode = (value: unknown): value is ApprovalMode => typeof value === "string" && APPROVAL_MODES.includes(value);

const STORAGE_KEY = "evo.ui";
/** Earlier builds kept the mode in this store's local storage; read it once, before the store hydrates. */
const legacy = (() => {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === null) return { existing: false, approval: undefined };
    const approval = (JSON.parse(raw) as { state?: { approval?: unknown } } | null)?.state?.approval;
    return { existing: true, approval: isApprovalMode(approval) ? approval : undefined };
  } catch {
    return { existing: false, approval: undefined };
  }
})();
/** App settings the shell follows (docs/features/shell-hosted-engines/spec.md, FR-8). */
export const AUTO_START_SESSIONS_KEY = "mesh.autoStartSessions";
export const CHAT_CONNECTORS_OFF_KEY = "connectors.chatOff";
/** Earlier builds kept these two in this store's local storage; read once, before the store hydrates (NFR-4). */
const legacyShell = (() => {
  try {
    const state = (JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null") as { state?: Record<string, unknown> } | null)?.state;
    const off = state?.chatConnectorsOff;
    return {
      autoStartSessions: typeof state?.autoStartSessions === "boolean" ? state.autoStartSessions : undefined,
      chatConnectorsOff: off && typeof off === "object" && !Array.isArray(off) ? (off as Record<string, string[]>) : undefined,
    };
  } catch {
    return { autoStartSessions: undefined, chatConnectorsOff: undefined };
  }
})();
export type SettingsSection = "general" | "appearance" | "providers" | "memory" | "computer-apps" | "engine" | "sandbox" | "logs" | "about";
export type CustomizeSection = "instructions" | "skills" | "agents" | "connectors" | "plugins" | "hooks" | "languages";

/** The side panel's tabs of one chat. */
export interface ChatPanels {
  openTabs: PanelId[];
  activePanel: PanelId | null;
  /** The side panel is open for the chat (a new chat starts with it closed). */
  open: boolean;
}

/**
 * One spelling per worktree folder: the engine reports backslashes on
 * Windows, the worktree list forward slashes. `settledWorktrees` is keyed by it.
 */
export const worktreeKey = (path: string): string => path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

export const SIDEBAR_WIDTH ={ min: 220, default: 272, max: 420 } as const;
export const SIDE_PANEL_WIDTH = { min: 300, default: 440 } as const;

interface UiStore {
  sidebarOpen: boolean;
  sidebarWidth: number;
  sidePanelWidth: number;
  rightPanelOpen: boolean;
  /**
   * The chat summary card (changes, task, progress) pinned beside the thread,
   * where it fits next to the messages (`summaryFits`). Where it does not, the
   * toolbar opens it as an overlay instead (`summaryPeek`, not saved).
   */
  summaryOpen: boolean;
  summaryFits: boolean;
  summaryPeek: boolean;
  toggleSummary(open?: boolean): void;
  setSummaryFits(fits: boolean): void;
  setSummaryPeek(peek: boolean): void;
  /** Tools open as tabs in the side panel, in tab order. */
  openTabs: PanelId[];
  /** The visible tab; `null` shows the tool launcher. */
  activePanel: PanelId | null;
  /**
   * Tabs belong to a chat: `openTabs` and `activePanel` are those of
   * `panelChat`; the other chats' are kept here until their chat shows again.
   */
  panelChat: string | null;
  panelsByChat: Record<string, ChatPanels>;
  /** Show `chat`'s tabs (a chat that never opened any shows none). */
  enterChat(chat: string | null): void;
  /** `chat` is gone (deleted, archived or an untouched draft discarded): drop its saved tabs. */
  forgetChat(chat: string): void;
  commandOpen: boolean;
  /** The settings page replaces the sidebar and main column while open. */
  settingsOpen: boolean;
  settingsSection: SettingsSection;
  /** Connectors (pool keys) switched off per chat, from the composer's + menu. */
  chatConnectorsOff: Record<string, string[]>;
  setChatConnectorOff(chat: string, key: string, off: boolean): void;
  /** Customize (skills, agents, …) replaces the main column while open. */
  customizeOpen: boolean;
  /** The Scheduler page (sidebar → Scheduler) shows instead of the chat. */
  schedulerOpen: boolean;
  openScheduler(): void;
  /** The Usage page (sidebar → Usage) shows instead of the chat. */
  usageOpen: boolean;
  openUsage(): void;
  customizeSection: CustomizeSection;
  openCustomize(section?: CustomizeSection): void;
  closeCustomize(): void;
  /**
   * The approval mode: the app setting `approvalMode`, which every engine
   * starts with and live-changes follow (docs/features/approval-modes/spec.md).
   */
  approval: ApprovalMode;
  /** Read the setting (and move the mode an older build kept locally into it); once per window. */
  loadApproval(): Promise<void>;
  /** The mode changed in another window: show it (nothing is written). */
  syncApproval(mode: ApprovalMode): void;
  /** The one-time "approvals now apply" note was dismissed or acted on. */
  approvalNoteSeen: boolean;
  dismissApprovalNote(): void;
  /** `provider/id` keys shown at the top of the model menu, in a stable order. */
  pinnedModels: string[];
  /** Page zoom (1 = 100%). */
  zoom: number;
  /** The app "Open in" used last; the toolbar's one-click target. */
  openInApp: string | null;
  setOpenInApp(id: string): void;
  /** The app "Open in" always uses (Settings → General); null follows the one used last. */
  openInDefault: string | null;
  setOpenInDefault(id: string | null): void;
  /** Floating cards of the agent's pages and shells appear by themselves (otherwise only chips). */
  agentCards: boolean;
  setAgentCards(on: boolean): void;
  /** Agents may start nested sessions without asking (otherwise they propose). */
  autoStartSessions: boolean;
  setAutoStartSessions(on: boolean): void;
  /** Read `autoStartSessions` and `chatConnectorsOff` from the app settings, handing an older build's local values over once. */
  loadShellSettings(): Promise<void>;
  /** What became of each proposed session: started (with its id) or dismissed. */
  proposals: Record<string, { status: "started" | "dismissed"; sessionId?: string }>;
  settleProposal(id: string, outcome: { status: "started" | "dismissed"; sessionId?: string }): void;
  /** Worktrees whose changes were applied or discarded, by `worktreeKey(path)`. */
  settledWorktrees: Record<string, "applied" | "discarded">;
  settleWorktree(path: string, outcome: "applied" | "discarded"): void;
  setZoom(zoom: number): void;
  setSidebarOpen(open: boolean): void;
  setSidebarWidth(width: number): void;
  setSidePanelWidth(width: number): void;
  toggleRightPanel(open?: boolean): void;
  /**
   * Open `panel` as a tab (or focus it); `null` shows the launcher. With
   * `chat` (an agent working in a chat that is not showing) the tab opens in
   * that chat's tabs and the visible ones stay as they are.
   */
  openPanel(panel: PanelId | null, chat?: string | null): void;
  closeTab(panel: PanelId): void;
  /** Close `panel`'s tab of `chat` (an agent closing a tool): the showing chat's own tabs, or another chat's saved ones. */
  closeChatPanel(chat: string, panel: PanelId): void;
  /** Close every tab but `panel`, which becomes the visible one. */
  closeOtherTabs(panel: PanelId): void;
  closeAllTabs(): void;
  /** Show the next (`1`) or previous (`-1`) tab, wrapping around. */
  cycleTab(step: 1 | -1): void;
  setCommandOpen(open: boolean): void;
  /** Open the settings page, on `section` when given. */
  openSettings(section?: SettingsSection): void;
  closeSettings(): void;
  setApproval(mode: ApprovalMode): void;
  setPinnedModels(keys: string[]): void;
}

export const PINNED_MODELS_MAX = 4;

const clampWidth = (width: number) => Math.round(Math.min(SIDEBAR_WIDTH.max, Math.max(SIDEBAR_WIDTH.min, width)));

export const useUiStore = create<UiStore>()(
  persist(
    (set, get) => ({
      sidebarOpen: true,
      sidebarWidth: SIDEBAR_WIDTH.default,
      sidePanelWidth: SIDE_PANEL_WIDTH.default,
      rightPanelOpen: false,
      summaryOpen: true,
      summaryFits: true,
      summaryPeek: false,
      toggleSummary: (open) =>
        get().summaryFits
          ? set({ summaryOpen: open ?? !get().summaryOpen })
          : set({ summaryPeek: open ?? !get().summaryPeek }),
      setSummaryFits: (summaryFits) => set({ summaryFits, summaryPeek: false }),
      setSummaryPeek: (summaryPeek) => set({ summaryPeek }),
      openTabs: [],
      activePanel: null,
      panelChat: null,
      panelsByChat: {},
      commandOpen: false,
      settingsOpen: false,
      customizeOpen: false,
      schedulerOpen: false,
      openScheduler: () =>
        set({ schedulerOpen: true, usageOpen: false, customizeOpen: false, settingsOpen: false, commandOpen: false }),
      usageOpen: false,
      openUsage: () =>
        set({ usageOpen: true, schedulerOpen: false, customizeOpen: false, settingsOpen: false, commandOpen: false }),
      customizeSection: "skills",
      chatConnectorsOff: {},
      setChatConnectorOff: (chat, key, off) =>
        set((s) => {
          const current = s.chatConnectorsOff[chat] ?? [];
          const next = off ? [...new Set([...current, key])] : current.filter((k) => k !== key);
          const all = { ...s.chatConnectorsOff };
          if (next.length) all[chat] = next;
          else delete all[chat];
          return { chatConnectorsOff: all };
        }),
      settingsSection: "general",
      approval: "ask",
      loadApproval: async () => {
        try {
          const saved = (await db.settings())[APPROVAL_MODE_KEY];
          if (isApprovalMode(saved)) {
            set({ approval: saved });
            return;
          }
          // First start of this version: keep what was last picked in the menu.
          const mode = legacy.approval ?? "ask";
          set({ approval: mode });
          await db.setSetting(APPROVAL_MODE_KEY, mode);
        } catch (error) {
          log("warn", "ui", `could not read the approval mode: ${String(error)}`);
        }
      },
      syncApproval: (approval) => set({ approval }),
      // New installs have nothing to be told about.
      approvalNoteSeen: !legacy.existing,
      dismissApprovalNote: () => set({ approvalNoteSeen: true }),
      pinnedModels: [],
      zoom: 1,
      openInApp: null,
      setOpenInApp: (openInApp) => set({ openInApp }),
      openInDefault: null,
      setOpenInDefault: (openInDefault) => set({ openInDefault }),
      agentCards: true,
      setAgentCards: (agentCards) => set({ agentCards }),
      autoStartSessions: false,
      setAutoStartSessions: (autoStartSessions) => {
        set({ autoStartSessions });
        // The shell's hub follows it (FR-8).
        void db
          .setSetting(AUTO_START_SESSIONS_KEY, autoStartSessions)
          .catch((error) => log("error", "ui", `could not save start sessions without asking: ${String(error)}`));
      },
      loadShellSettings: async () => {
        try {
          const { autoStartSessions, chatConnectorsOff } = legacyShell;
          // The shell never writes over a value it already has.
          if (autoStartSessions !== undefined || chatConnectorsOff) await db.importWindowSettings({ autoStartSessions, chatConnectorsOff });
          const saved = await db.settings();
          const off = saved[CHAT_CONNECTORS_OFF_KEY];
          set({
            autoStartSessions: saved[AUTO_START_SESSIONS_KEY] === true,
            chatConnectorsOff: off && typeof off === "object" && !Array.isArray(off) ? (off as Record<string, string[]>) : {},
          });
        } catch (error) {
          log("warn", "ui", `could not read the shell's settings: ${String(error)}`);
        }
      },
      proposals: {},
      settleProposal: (id, outcome) => set((s) => ({ proposals: { ...s.proposals, [id]: outcome } })),
      settledWorktrees: {},
      settleWorktree: (path, outcome) =>
        set((s) => ({ settledWorktrees: { ...s.settledWorktrees, [worktreeKey(path)]: outcome } })),
      setZoom: (zoom) => set({ zoom }),
      setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
      setSidebarWidth: (width) => set({ sidebarWidth: clampWidth(width) }),
      setSidePanelWidth: (width) => set({ sidePanelWidth: Math.max(SIDE_PANEL_WIDTH.min, Math.round(width)) }),
      toggleRightPanel: (open) => set({ rightPanelOpen: open ?? !get().rightPanelOpen }),
      openPanel: (panel, chat) =>
        set((s) => {
          if (chat && s.panelChat && chat !== s.panelChat) {
            // Another chat's tabs: it sees the tab when it shows again.
            const saved = s.panelsByChat[chat] ?? { openTabs: [], activePanel: null, open: false };
            const openTabs = panel && !saved.openTabs.includes(panel) ? [...saved.openTabs, panel] : saved.openTabs;
            return { panelsByChat: { ...s.panelsByChat, [chat]: { openTabs, activePanel: panel, open: true } } };
          }
          return {
            activePanel: panel,
            rightPanelOpen: true,
            openTabs: panel && !s.openTabs.includes(panel) ? [...s.openTabs, panel] : s.openTabs,
          };
        }),
      forgetChat: (chat) =>
        set((s) => {
          if (!(chat in s.panelsByChat)) return s;
          const { [chat]: _gone, ...panelsByChat } = s.panelsByChat;
          return { panelsByChat };
        }),
      enterChat: (chat) =>
        set((s) => {
          if (!chat || chat === s.panelChat) return s;
          // Tabs saved before they belonged to chats go to the first chat shown.
          if (s.panelChat === null) return { panelChat: chat };
          const panelsByChat = { ...s.panelsByChat, [s.panelChat]: { openTabs: s.openTabs, activePanel: s.activePanel, open: s.rightPanelOpen } };
          const next = panelsByChat[chat] ?? { openTabs: [], activePanel: null, open: false };
          return { panelChat: chat, panelsByChat, openTabs: next.openTabs, activePanel: next.activePanel, rightPanelOpen: next.open };
        }),
      closeTab: (panel) =>
        set((s) => {
          const index = s.openTabs.indexOf(panel);
          const openTabs = s.openTabs.filter((t) => t !== panel);
          // Closing the visible tab shows its neighbour, or the launcher when none is left.
          const activePanel =
            s.activePanel === panel ? (openTabs[Math.min(index, openTabs.length - 1)] ?? null) : s.activePanel;
          return { openTabs, activePanel };
        }),
      closeChatPanel: (chat, panel) => {
        if (!get().panelChat || chat === get().panelChat) {
          get().closeTab(panel);
          return;
        }
        set((s) => {
          const saved = s.panelsByChat[chat];
          if (!saved || !saved.openTabs.includes(panel)) return s;
          const index = saved.openTabs.indexOf(panel);
          const openTabs = saved.openTabs.filter((t) => t !== panel);
          const activePanel = saved.activePanel === panel ? (openTabs[Math.min(index, openTabs.length - 1)] ?? null) : saved.activePanel;
          return { panelsByChat: { ...s.panelsByChat, [chat]: { ...saved, openTabs, activePanel } } };
        });
      },
      closeOtherTabs: (panel) => set({ openTabs: [panel], activePanel: panel }),
      closeAllTabs: () => set({ openTabs: [], activePanel: null }),
      cycleTab: (step) =>
        set((s) => {
          if (s.openTabs.length === 0) return s;
          const index = s.activePanel ? s.openTabs.indexOf(s.activePanel) : step > 0 ? -1 : 0;
          const next = (index + step + s.openTabs.length) % s.openTabs.length;
          return { activePanel: s.openTabs[next], rightPanelOpen: true };
        }),
      setCommandOpen: (commandOpen) => set({ commandOpen }),
      openSettings: (section) =>
        set((s) => ({ settingsOpen: true, settingsSection: section ?? s.settingsSection, commandOpen: false })),
      closeSettings: () => set({ settingsOpen: false }),
      openCustomize: (section) =>
        set((s) => ({
          customizeOpen: true,
          schedulerOpen: false,
          usageOpen: false,
          customizeSection: section ?? s.customizeSection,
          settingsOpen: false,
          commandOpen: false,
        })),
      // Leaves any full page (Customize, Scheduler or Usage) for the chat.
      closeCustomize: () => set({ customizeOpen: false, schedulerOpen: false, usageOpen: false }),
      setApproval: (approval) => {
        if (approval === get().approval) return;
        set({ approval });
        // Saved first: an engine that starts now reads the new mode; then
        // every window tells its running engines.
        void db
          .setSetting(APPROVAL_MODE_KEY, approval)
          .then(() => emit(APPROVAL_CHANGED, { mode: approval }))
          .catch((error) => log("error", "ui", `could not save the approval mode: ${String(error)}`));
      },
      setPinnedModels: (keys) => set({ pinnedModels: keys.slice(0, PINNED_MODELS_MAX) }),
    }),
    {
      name: STORAGE_KEY,
      merge: (persisted, current) => {
        // `approval`, `autoStartSessions` and `chatConnectorsOff` lived here in earlier builds: they are app settings now.
        const { approval: _moved, autoStartSessions: _auto, chatConnectorsOff: _off, ...saved } = (persisted ?? {}) as Partial<UiStore>;
        const openTabs = (saved.openTabs ?? current.openTabs).filter((id) => PANEL_IDS.includes(id));
        const activePanel = saved.activePanel && PANEL_IDS.includes(saved.activePanel) ? saved.activePanel : (openTabs[0] ?? null);
        const panelsByChat: Record<string, ChatPanels> = {};
        for (const [chat, panels] of Object.entries(saved.panelsByChat ?? {})) {
          const tabs = (panels?.openTabs ?? []).filter((id) => PANEL_IDS.includes(id));
          panelsByChat[chat] = {
            openTabs: tabs,
            activePanel: panels?.activePanel && tabs.includes(panels.activePanel) ? panels.activePanel : (tabs[0] ?? null),
            open: panels?.open ?? tabs.length > 0,
          };
        }
        // Earlier builds keyed settled worktrees by the path as it was reported.
        const settledWorktrees: UiStore["settledWorktrees"] = {};
        for (const [path, outcome] of Object.entries(saved.settledWorktrees ?? {})) settledWorktrees[worktreeKey(path)] = outcome;
        return { ...current, ...saved, openTabs, activePanel, panelsByChat, settledWorktrees };
      },
      partialize: (state) => ({
        approvalNoteSeen: state.approvalNoteSeen,
        pinnedModels: state.pinnedModels,
        zoom: state.zoom,
        openInApp: state.openInApp,
        openInDefault: state.openInDefault,
        agentCards: state.agentCards,
        proposals: state.proposals,
        settledWorktrees: state.settledWorktrees,
        rightPanelOpen: state.rightPanelOpen,
        summaryOpen: state.summaryOpen,
        sidebarOpen: state.sidebarOpen,
        sidebarWidth: state.sidebarWidth,
        sidePanelWidth: state.sidePanelWidth,
        openTabs: state.openTabs,
        activePanel: state.activePanel,
        panelChat: state.panelChat,
        panelsByChat: state.panelsByChat,
      }),
    },
  ),
);
