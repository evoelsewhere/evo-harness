// Pure helpers of the terminal tools (no stores, no Tauri), so they can be tested.
// Spec: docs/features/agent-terminal (FR-5, FR-7, FR-14).

import type { ShellInfo } from "./pty";

const KEYS: Record<string, string> = {
  enter: "\r",
  return: "\r",
  tab: "\t",
  esc: "\x1b",
  escape: "\x1b",
  backspace: "\x7f",
  up: "\x1b[A",
  down: "\x1b[B",
  right: "\x1b[C",
  left: "\x1b[D",
  home: "\x1b[H",
  end: "\x1b[F",
  pageup: "\x1b[5~",
  pagedown: "\x1b[6~",
  space: " ",
};

/** The bytes a key name stands for, or `null` when it is not one. */
export function keyBytes(name: string): string | null {
  const key = name.trim().toLowerCase().replace(/\s+/g, "").replace(/^control\+/, "ctrl+").replace(/^ctrl-/, "ctrl+");
  const ctrl = /^ctrl\+([a-z])$/.exec(key);
  if (ctrl) return String.fromCharCode(ctrl[1].charCodeAt(0) - 96);
  return KEYS[key] ?? null;
}

/** "Ctrl+C", "Enter": how a key is shown. */
export const keyLabel = (name: string) =>
  name
    .trim()
    .split("+")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
    .join("+");

/** True when the line looks like a prompt for a password or passphrase (FR-14). */
export const isSecretPrompt = (line: string) =>
  /(password|passphrase|\bpin\b)[^:\n]*:\s*$/i.test(line) || /^\[sudo\]/i.test(line.trim());

/** A shell by id, then name, then program file name (case-insensitive). */
export function matchShell(shells: ShellInfo[], query: string): ShellInfo | undefined {
  const wanted = query.trim().toLowerCase();
  const stem = (program: string) => (program.split(/[\\/]/).pop() ?? "").toLowerCase().replace(/\.exe$/, "");
  return (
    shells.find((shell) => shell.id.toLowerCase() === wanted) ??
    shells.find((shell) => shell.name.toLowerCase() === wanted) ??
    shells.find((shell) => stem(shell.program) === wanted.replace(/\.exe$/, ""))
  );
}

