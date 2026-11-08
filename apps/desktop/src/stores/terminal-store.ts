// Side panel terminals: the tabs, which one shows, and the shells to start.
// The xterm instances themselves live in a map beside the store (they are not
// state React renders from); see features/terminal/terminal-instance.ts.

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  attachPty,
  closePty,
  killPty,
  listExtraShells,
  listPtys,
  listShells,
  setDefaultPtyShell,
  setPtyChat,
  spawnPty,
  type CreatedPty,
  type PtyEvent,
  type ShellInfo,
} from "@/features/terminal/pty";
import { inTauri } from "@/lib/platform";
import { shellIntegrationEnabled } from "@/features/terminal/settings";
import { forgetTabChat, recalledTabChat, rememberTabChat } from "@/lib/tab-chats";
import { TerminalInstance } from "@/features/terminal/terminal-instance";
import { useUiStore } from "./ui-store";

export interface TerminalTab {
  /** Stable for the tab's life; a restart binds a new pty to the same tab. The agent's terminal tools use it too. */
  key: string;
  ptyId: number | null;
  shellId: string;
  shell: string;
  cwd: string;
  /** Set by the shell (OSC title), else empty. */
  title: string;
  status: "starting" | "running" | "exited";
  exitCode?: number | null;
  /** Id of the chat whose agent started the tab; `null` for the person's own tabs. */
  owner: string | null;
  /** The shell reports command marks (OSC 133). */
  integration: boolean;
  /**
   * The chat (session) the tab belongs to: it shows only while that chat does.
   * `null` for a tab found after a reload whose chat is unknown.
   */
  chat: string | null;
}

interface TerminalStore {
  /** Every tab of every chat; the panel shows those of `chat` (`chatTabs`). */
  tabs: TerminalTab[];
  /** The visible tab of the showing chat. */
  activeKey: string | null;
  activeByChat: Record<string, string | null>;
  /** The chat whose tabs show. */
  chat: string | null;
  shells: ShellInfo[];
  /** Shell id new terminals start with; `null` = the first one found. */
  defaultShell: string | null;
  /** The find bar is open over the active terminal. */
  findOpen: boolean;
  /** Focus the active terminal when it next shows. */
  focusPending: boolean;
  /** Last start failure, shown in place of the terminal. */
  error: string | null;
  loadShells(): Promise<void>;
  /** Every shell, the slow sources (WSL, profiles) included. */
  allShells(): Promise<ShellInfo[]>;
  /** Re-attach the ptys that outlived a reload; runs once per window. */
  restore(): Promise<void>;
  /** Restore, load the shells, and start a terminal in `cwd` when the showing chat has none; `true` if it started one. */
  ensure(cwd?: string): Promise<boolean>;
  /** Show `chat`'s tabs; tabs whose chat is unknown go to the first chat shown. */
  setChat(chat: string | null): void;
  /**
   * Start a terminal (`shell` id, else the default) in `cwd`; resolves to its tab key.
   * It belongs to `chat`, else to `owner` (the agent's chat), else to the showing chat.
   */
  create(options?: { shell?: string; cwd?: string; owner?: string; chat?: string | null }): Promise<string | null>;
  restart(key: string): Promise<void>;
  /** Stop a tab's shell and what it started; the tab stays, showing it exited. */
  kill(key: string): void;
  close(key: string): void;
  /** Close every tab of `chat` (default: the showing one). */
  closeAll(chat?: string | null): void;
  select(key: string): void;
  /** Show the next (`1`) or previous (`-1`) terminal. */
  cycle(step: 1 | -1): void;
  setDefaultShell(id: string | null): void;
  setFindOpen(open: boolean): void;
  requestFocus(): void;
  consumeFocus(): boolean;
}

const instances = new Map<string, TerminalInstance>();

const keyOf = (chat: string | null) => chat ?? "";

/** The tabs of `chat`. */
export const chatTabs = (tabs: TerminalTab[], chat: string | null) => tabs.filter((tab) => tab.chat === chat);

/** The xterm instance of a tab. */
export const terminalInstance = (key: string) => instances.get(key);

let keySeq = 0;
const newKey = () => `t${Date.now().toString(36)}${(keySeq++).toString(36)}`;

/** PowerShell titles itself with its executable path; show the shell name instead. */
export function cleanTitle(title: string): string {
  const trimmed = title.trim();
  // "C:\WINDOWS\System32\…\powershell.exe": the shell's own name ("Windows PowerShell") says more.
  if (/^(?:[a-z]:)?[\\/].*\.exe$/i.test(trimmed)) return "";
  return trimmed;
}

export const tabLabel = (tab: TerminalTab) => cleanTitle(tab.title) || tab.shell;

/** A shell found in the native layer, as the store adopts it. */
type AdoptedPty = Pick<CreatedPty, "id" | "key" | "shellId" | "shell" | "cwd" | "owner" | "integration">;

/** Set up with the store: adopt / close a tab an agent tool started / closed. */
let onCreated: (pty: CreatedPty) => Promise<void> = async () => undefined;
let onClosed: (key: string) => void = () => undefined;

let restored: Promise<void> | null = null;
/** The background load of WSL distributions and profiles. */
let extraLoad: Promise<void> = Promise.resolve();
let ensuring: Promise<boolean> | null = null;

/** Make `key` the visible tab of `chat`: remembered for the chat, shown when it is the showing one. */
function activate(s: Pick<TerminalStore, "activeByChat" | "chat">, key: string | null, chat: string | null) {
  return {
    activeByChat: { ...s.activeByChat, [keyOf(chat)]: key },
    ...(chat === s.chat ? { activeKey: key } : {}),
  };
}

export const useTerminalStore = create<TerminalStore>()(
  persist(
    (set, get) => {
      const patch = (key: string, change: Partial<TerminalTab>) =>
        set((s) => ({ tabs: s.tabs.map((tab) => (tab.key === key ? { ...tab, ...change } : tab)) }));

      const instanceFor = (key: string) => {
        const instance = new TerminalInstance({
          onTitle: (title) => patch(key, { title }),
          onFindRequest: () => {
            if (get().activeKey === key) set({ findOpen: true });
          },
        });
        instances.set(key, instance);
        return instance;
      };

      /** Output and exit of the pty currently bound to `key`'s tab. */
      const handler = (key: string, ptyId: number) => (event: PtyEvent) => {
        const instance = instances.get(key);
        const tab = get().tabs.find((t) => t.key === key);
        // A restarted or closed tab ignores its old pty.
        if (!instance || !tab || tab.ptyId !== ptyId) return;
        if (event.type === "data") {
          instance.write(event.data);
          return;
        }
        instance.unbind();
        forgetTabChat("terminal", ptyId);
        // `exit` (or Ctrl+D) ends the terminal, as in most editors; a failure stays to be read.
        // An agent's tab stays, so the agent can read how it ended; so does a tab stopped on purpose.
        if (event.code === 0 && !tab.owner && !event.killed) {
          get().close(key);
          return;
        }
        const code = event.code === null ? "" : ` with code ${event.code}`;
        instance.notice(`\r\n\x1b[2m[Process exited${code}]\x1b[0m\r\n`);
        patch(key, { status: "exited", exitCode: event.code });
      };

      const start = async (key: string, shell: string | undefined, cwd: string | undefined, owner?: string, chat?: string | null) => {
        const instance = instances.get(key);
        if (!instance) return;
        // Events can arrive before `spawnPty` resolves; they wait for the pty id here.
        let ready: ((event: PtyEvent) => void) | null = null;
        const early: PtyEvent[] = [];
        const route = (event: PtyEvent) => {
          if (ready) ready(event);
          else early.push(event);
        };
        try {
          const integration = await shellIntegrationEnabled();
          const info = await spawnPty({ shell, cwd, owner, chat, key, integration, cols: instance.term.cols, rows: instance.term.rows }, route);
          if (!get().tabs.some((t) => t.key === key)) {
            // Closed while starting.
            void closePty(info.id).catch(() => undefined);
            return;
          }
          rememberTabChat("terminal", info.id, get().tabs.find((t) => t.key === key)?.chat ?? null);
          patch(key, {
            ptyId: info.id,
            shellId: info.shellId,
            shell: info.shell,
            cwd: info.cwd,
            status: "running",
            owner: info.owner,
            integration: info.integration,
          });
          instance.bind(info.id);
          ready = handler(key, info.id);
          for (const event of early.splice(0)) ready(event);
          set({ error: null });
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          instance.notice(`\x1b[31m${message}\x1b[0m\r\n`);
          patch(key, { status: "exited", exitCode: null });
          set({ error: message });
        }
      };

      /** A shell that already runs in the native layer joins its chat's tabs (after a reload, or when an agent tool started it). */
      const adopt = async (pty: AdoptedPty, chat: string | null, options: { activate: boolean }) => {
        const key = pty.key;
        if (get().tabs.some((t) => t.key === key)) return;
        const instance = instanceFor(key);
        set((s) => ({
          tabs: [
            ...s.tabs,
            {
              key,
              ptyId: pty.id,
              shellId: pty.shellId,
              shell: pty.shell,
              cwd: pty.cwd,
              title: "",
              status: "running",
              owner: pty.owner ?? null,
              integration: pty.integration ?? false,
              chat,
            },
          ],
          ...(options.activate || !s.activeByChat[keyOf(chat)] ? activate(s, key, chat) : {}),
        }));
        rememberTabChat("terminal", pty.id, chat);
        instance.bind(pty.id);
        try {
          const backlog = await attachPty(pty.id, handler(key, pty.id));
          instance.replay(backlog);
        } catch {
          get().close(key);
        }
      };

      // The agent's terminal tools (the shell) start and close tabs; this window shows them.
      onCreated = async (pty: CreatedPty) => {
        if (pty.window !== getCurrentWindow().label) return;
        // The side panel is not opened for it: the tab shows as a card over the chat, or waits as a chip
        // (stores/cards-store.ts, docs/features/floating-cards).
        await adopt(pty, pty.chat, { activate: true });
      };
      onClosed = (key: string) => get().close(key);

      return {
        tabs: [],
        activeKey: null,
        activeByChat: {},
        chat: null,
        shells: [],
        defaultShell: null,
        findOpen: false,
        focusPending: false,
        error: null,

        async loadShells() {
          try {
            set({ shells: await listShells() });
          } catch {
            set({ shells: [] });
          }
          // WSL distributions and profiles load in the background (NFR-2).
          extraLoad = listExtraShells()
            .then((extra) => {
              if (extra.length === 0) return;
              set((s) => ({ shells: [...s.shells, ...extra.filter((e) => !s.shells.some((known) => known.id === e.id))] }));
            })
            .catch(() => undefined);
        },

        async allShells() {
          if (get().shells.length === 0) await get().loadShells();
          await extraLoad;
          return get().shells;
        },

        restore() {
          restored ??= (async () => {
            let ptys: Awaited<ReturnType<typeof listPtys>> = [];
            try {
              ptys = await listPtys();
            } catch {
              return;
            }
            if (ptys.length === 0) return;
            // A chat whose terminal tab was closed: nothing of it should be running.
            const ui = useUiStore.getState();
            const panelOpen = (chat: string | null) =>
              (chat === null || chat === ui.panelChat ? ui.openTabs : (ui.panelsByChat[chat]?.openTabs ?? [])).includes("terminal");
            for (const pty of ptys) {
              // Already a tab (a shell an agent tool started, adopted from its event).
              if (get().tabs.some((t) => t.key === pty.key)) continue;
              const chat = pty.chat ?? recalledTabChat("terminal", pty.id) ?? pty.owner ?? null;
              if (!panelOpen(chat)) {
                void closePty(pty.id).catch(() => undefined);
                continue;
              }
              await adopt(pty, chat, { activate: false });
            }
          })();
          return restored;
        },

        ensure(cwd) {
          ensuring ??= (async () => {
            const store = get();
            await Promise.all([store.shells.length ? null : store.loadShells(), store.restore()]);
            if (chatTabs(get().tabs, get().chat).length > 0) return false;
            await get().create({ cwd });
            return true;
          })().finally(() => {
            ensuring = null;
          });
          return ensuring;
        },

        setChat(chat) {
          if (chat === get().chat) return;
          set((s) => {
            // Tabs found after a reload with no known chat go to the first chat shown.
            const tabs = chat && s.chat === null ? s.tabs.map((t) => (t.chat === null ? { ...t, chat } : t)) : s.tabs;
            // The shell keeps the chat of each tab for the agent's tools.
            if (chat && s.chat === null) {
              for (const t of s.tabs) if (t.chat === null && t.ptyId !== null) void setPtyChat(t.ptyId, chat).catch(() => undefined);
            }
            const mine = chatTabs(tabs, chat);
            const wanted = s.activeByChat[keyOf(chat)];
            const activeKey = wanted && mine.some((t) => t.key === wanted) ? wanted : (mine.at(-1)?.key ?? null);
            return { tabs, chat, activeKey, findOpen: false, focusPending: activeKey !== null };
          });
        },

        async create(options = {}) {
          const key = newKey();
          const chat = options.chat !== undefined ? options.chat : (options.owner ?? get().chat);
          const shell = options.shell ?? get().defaultShell ?? undefined;
          const known = get().shells.find((s) => s.id === shell) ?? get().shells[0];
          instanceFor(key);
          set((s) => ({
            tabs: [
              ...s.tabs,
              {
                key,
                ptyId: null,
                shellId: known?.id ?? shell ?? "",
                shell: known?.name ?? "Terminal",
                cwd: options.cwd ?? "",
                title: "",
                status: "starting",
                owner: options.owner ?? null,
                integration: false,
                chat,
              },
            ],
            ...activate(s, key, chat),
            ...(chat === s.chat ? { findOpen: false, focusPending: true } : {}),
          }));
          await start(key, shell, options.cwd, options.owner, chat);
          return key;
        },

        async restart(key) {
          const tab = get().tabs.find((t) => t.key === key);
          const instance = instances.get(key);
          if (!tab || !instance) return;
          if (tab.ptyId !== null) void closePty(tab.ptyId).catch(() => undefined);
          instance.unbind();
          instance.term.reset();
          patch(key, { ptyId: null, status: "starting", exitCode: undefined, title: "" });
          set({ focusPending: true });
          await start(key, tab.shellId || undefined, tab.cwd || undefined, tab.owner ?? undefined, tab.chat);
        },

        kill(key) {
          const tab = get().tabs.find((t) => t.key === key);
          if (!tab || tab.ptyId === null || tab.status !== "running") return;
          void killPty(tab.ptyId).catch(() => undefined);
        },

        close(key) {
          const { tabs } = get();
          const index = tabs.findIndex((t) => t.key === key);
          if (index < 0) return;
          const tab = tabs[index];
          if (tab.ptyId !== null) void closePty(tab.ptyId).catch(() => undefined);
          instances.get(key)?.dispose();
          instances.delete(key);
          if (tab.ptyId !== null) forgetTabChat("terminal", tab.ptyId);
          const rest = tabs.filter((t) => t.key !== key);
          // The next tab is one of the same chat.
          const mates = chatTabs(rest, tab.chat);
          const chatActive = get().activeByChat[keyOf(tab.chat)];
          const mateIndex = chatTabs(tabs, tab.chat).findIndex((t) => t.key === key);
          const next = chatActive === key || chatActive === undefined ? (mates[Math.min(mateIndex, mates.length - 1)]?.key ?? null) : chatActive;
          set((s) => ({
            tabs: rest,
            ...activate(s, next, tab.chat),
            ...(tab.chat === s.chat ? { findOpen: false, focusPending: next !== null } : {}),
          }));
        },

        closeAll(chat = get().chat) {
          for (const tab of chatTabs(get().tabs, chat)) {
            if (tab.ptyId !== null) void closePty(tab.ptyId).catch(() => undefined);
            if (tab.ptyId !== null) forgetTabChat("terminal", tab.ptyId);
            instances.get(tab.key)?.dispose();
            instances.delete(tab.key);
          }
          set((s) => ({
            tabs: s.tabs.filter((t) => t.chat !== chat),
            ...activate(s, null, chat),
            findOpen: false,
          }));
        },

        select(key) {
          const tab = get().tabs.find((t) => t.key === key);
          if (tab) set((s) => ({ ...activate(s, key, tab.chat), findOpen: false, focusPending: true }));
        },

        cycle(step) {
          const { tabs, activeKey, chat } = get();
          const mine = chatTabs(tabs, chat);
          if (mine.length < 2) return;
          const index = mine.findIndex((t) => t.key === activeKey);
          get().select(mine[(index + step + mine.length) % mine.length].key);
        },

        setDefaultShell: (defaultShell) => set({ defaultShell }),
        setFindOpen: (findOpen) => set({ findOpen }),
        requestFocus: () => set({ focusPending: true }),
        consumeFocus() {
          if (!get().focusPending) return false;
          set({ focusPending: false });
          return true;
        },
      };
    },
    {
      name: "evo.terminal",
      partialize: (state) => ({ defaultShell: state.defaultShell }),
    },
  ),
);

// Closing the Terminal tool tab ends its shells; hiding the side panel or
// switching tools keeps them running.
useUiStore.subscribe((state, previous) => {
  // A chat shows other tabs: the swap itself closes nothing.
  if (state.panelChat === previous.panelChat && previous.openTabs.includes("terminal") && !state.openTabs.includes("terminal")) {
    useTerminalStore.getState().closeAll();
  }
});

// The agent's terminal tools run in the shell: tabs they start or close arrive as events.
if (inTauri) {
  void listen<CreatedPty>("terminal://created", (event) => void onCreated(event.payload));
  void listen<{ key: string }>("terminal://closed", (event) => onClosed(event.payload.key));
  // The shell starts a tool's tab in the person's default shell too.
  const report = (shell: string | null) => void setDefaultPtyShell(shell).catch(() => undefined);
  report(useTerminalStore.getState().defaultShell);
  useTerminalStore.subscribe((state, previous) => {
    if (state.defaultShell !== previous.defaultShell) report(state.defaultShell);
  });
}
