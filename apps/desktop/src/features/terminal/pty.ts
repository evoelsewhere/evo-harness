// IPC to the shell's pseudo-terminals (src-tauri/src/terminal.rs). No React.

import { Channel, invoke } from "@tauri-apps/api/core";

/** A shell the terminal can start. */
export interface ShellInfo {
  id: string;
  name: string;
  program: string;
  kind: "powershell" | "cmd" | "posix" | "fish" | "nu" | "wsl";
  isDefault: boolean;
}

/** A running pseudo-terminal. */
export interface PtyInfo {
  id: number;
  shellId: string;
  shell: string;
  cwd: string;
  pid: number | null;
  /** The chat that started it (an agent tab), else null. */
  owner: string | null;
  /** The shell reports command marks (OSC 133). */
  integration: boolean;
}

export type PtyEvent = { type: "data"; data: string } | { type: "exit"; code: number | null };

export const listShells = () => invoke<ShellInfo[]>("terminal_shells");

/** The slow sources (WSL distributions, Windows Terminal profiles, Visual Studio shells; 3 s limit). */
export const listExtraShells = () => invoke<ShellInfo[]>("terminal_shells_extra");

/** Start `shell` (default when omitted) in `cwd` (home when omitted). */
export function spawnPty(
  options: { shell?: string; cwd?: string; cols: number; rows: number; owner?: string; integration?: boolean },
  onEvent: (event: PtyEvent) => void,
): Promise<PtyInfo> {
  const channel = new Channel<PtyEvent>();
  channel.onmessage = onEvent;
  return invoke<PtyInfo>("terminal_spawn", {
    shell: options.shell ?? null,
    cwd: options.cwd ?? null,
    cols: options.cols,
    rows: options.rows,
    owner: options.owner ?? null,
    integration: options.integration ?? true,
    onEvent: channel,
  });
}

/** The ptys this window started that still run (after a reload). */
export const listPtys = () => invoke<PtyInfo[]>("terminal_list");

/** Route a running pty's output to `onEvent`; resolves to its output so far. */
export function attachPty(id: number, onEvent: (event: PtyEvent) => void): Promise<string> {
  const channel = new Channel<PtyEvent>();
  channel.onmessage = onEvent;
  return invoke<string>("terminal_attach", { id, onEvent: channel });
}

export const writePty = (id: number, data: string) => invoke<void>("terminal_write", { id, data });
export const resizePty = (id: number, cols: number, rows: number) => invoke<void>("terminal_resize", { id, cols, rows });
export const killPty = (id: number) => invoke<void>("terminal_kill", { id });
