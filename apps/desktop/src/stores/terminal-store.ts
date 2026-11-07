// Side panel terminals: the tabs, which one shows, and the shells to start.
// The xterm instances themselves live in a map beside the store (they are not
// state React renders from); see features/terminal/terminal-instance.ts.

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { attachPty, killPty, listExtraShells, listPtys, listShells, spawnPty, type PtyEvent, type ShellInfo } from "@/features/terminal/pty";
import { shellIntegrationEnabled } from "@/features/terminal/settings";
import { TerminalInstance } from "@/features/terminal/terminal-instance";
import { useUiStore } from "./ui-store";

export interface TerminalTab {
  /** Stable for the tab's life; a restart binds a new pty to the same tab. */
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
}

interface TerminalStore {
  tabs: TerminalTab[];
  activeKey: string | null;
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
  /** Restore, load the shells, and start a terminal in `cwd` when none is open; `true` if it started one. */
  ensure(cwd?: string): Promise<boolean>;
  /** Start a terminal (`shell` id, else the default) in `cwd`; resolves to its tab key. */
  create(options?: { shell?: string; cwd?: string; owner?: string }): Promise<string | null>;
  restart(key: string): Promise<void>;
  /** Stop a tab's shell and what it started; the tab stays, showing it exited. */
  kill(key: string): void;
  close(key: string): void;
  closeAll(): void;
  select(key: string): void;
  /** Show the next (`1`) or previous (`-1`) terminal. */
  cycle(step: 1 | -1): void;
  setDefaultShell(id: string | null): void;
  setFindOpen(open: boolean): void;
  requestFocus(): void;
  consumeFocus(): boolean;
}

const instances = new Map<string, TerminalInstance>();

/** The xterm instance of a tab. */
export const terminalInstance = (key: string) => instances.get(key);

/** Tabs stopped on purpose: they stay, showing exited, even for exit code 0. */
const killed = new Set<string>();

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

let restored: Promise<void> | null = null;
/** The background load of WSL distributions and profiles. */
let extraLoad: Promise<void> = Promise.resolve();
let ensuring: Promise<boolean> | null = null;

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
        instance.log.close(event.code);
        // `exit` (or Ctrl+D) ends the terminal, as in most editors; a failure stays to be read.
        // An agent's tab stays, so the agent can read how it ended.
        if (event.code === 0 && !tab.owner && !killed.has(key)) {
          get().close(key);
          return;
        }
        const code = event.code === null ? "" : ` with code ${event.code}`;
        instance.notice(`\r\n\x1b[2m[Process exited${code}]\x1b[0m\r\n`);
        patch(key, { status: "exited", exitCode: event.code });
      };

      const start = async (key: string, shell: string | undefined, cwd: string | undefined, owner?: string) => {
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
          const info = await spawnPty({ shell, cwd, owner, integration, cols: instance.term.cols, rows: instance.term.rows }, route);
          if (!get().tabs.some((t) => t.key === key)) {
            // Closed while starting.
            void killPty(info.id).catch(() => undefined);
            return;
          }
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

      return {
        tabs: [],
        activeKey: null,
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
            // The terminal tab was closed: nothing should be running.
            if (!useUiStore.getState().openTabs.includes("terminal")) {
              for (const pty of ptys) void killPty(pty.id).catch(() => undefined);
              return;
            }
            for (const pty of ptys) {
              const key = newKey();
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
                  },
                ],
                activeKey: s.activeKey ?? key,
              }));
              instance.bind(pty.id);
              try {
                const backlog = await attachPty(pty.id, handler(key, pty.id));
                instance.replay(backlog);
              } catch {
                get().close(key);
              }
            }
          })();
          return restored;
        },

        ensure(cwd) {
          ensuring ??= (async () => {
            const store = get();
            await Promise.all([store.shells.length ? null : store.loadShells(), store.restore()]);
            if (get().tabs.length > 0) return false;
            await get().create({ cwd });
            return true;
          })().finally(() => {
            ensuring = null;
          });
          return ensuring;
        },

        async create(options = {}) {
          const key = newKey();
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
              },
            ],
            activeKey: key,
            findOpen: false,
            focusPending: true,
          }));
          await start(key, shell, options.cwd, options.owner);
          return key;
        },

        async restart(key) {
          const tab = get().tabs.find((t) => t.key === key);
          const instance = instances.get(key);
          if (!tab || !instance) return;
          if (tab.ptyId !== null && tab.status === "running") void killPty(tab.ptyId).catch(() => undefined);
          instance.unbind();
          instance.term.reset();
          patch(key, { ptyId: null, status: "starting", exitCode: undefined, title: "" });
          set({ focusPending: true });
          killed.delete(key);
          instance.log.reopen();
          await start(key, tab.shellId || undefined, tab.cwd || undefined, tab.owner ?? undefined);
        },

        kill(key) {
          const tab = get().tabs.find((t) => t.key === key);
          if (!tab || tab.ptyId === null || tab.status !== "running") return;
          killed.add(key);
          void killPty(tab.ptyId).catch(() => undefined);
        },

        close(key) {
          const { tabs, activeKey } = get();
          const index = tabs.findIndex((t) => t.key === key);
          if (index < 0) return;
          const tab = tabs[index];
          if (tab.ptyId !== null && tab.status === "running") void killPty(tab.ptyId).catch(() => undefined);
          instances.get(key)?.dispose();
          instances.delete(key);
          killed.delete(key);
          const rest = tabs.filter((t) => t.key !== key);
          const next = activeKey === key ? (rest[Math.min(index, rest.length - 1)]?.key ?? null) : activeKey;
          set({ tabs: rest, activeKey: next, findOpen: false, focusPending: next !== null });
        },

        closeAll() {
          for (const tab of get().tabs) {
            if (tab.ptyId !== null && tab.status === "running") void killPty(tab.ptyId).catch(() => undefined);
            instances.get(tab.key)?.dispose();
          }
          instances.clear();
          set({ tabs: [], activeKey: null, findOpen: false });
        },

        select(key) {
          if (get().tabs.some((t) => t.key === key)) set({ activeKey: key, findOpen: false, focusPending: true });
        },

        cycle(step) {
          const { tabs, activeKey } = get();
          if (tabs.length < 2) return;
          const index = tabs.findIndex((t) => t.key === activeKey);
          get().select(tabs[(index + step + tabs.length) % tabs.length].key);
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
  if (previous.openTabs.includes("terminal") && !state.openTabs.includes("terminal")) {
    useTerminalStore.getState().closeAll();
  }
});
