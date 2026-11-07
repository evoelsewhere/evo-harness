// Which chat a browser page or terminal belongs to. The pages and ptys live in
// the native layer and outlive a window reload; this window forgets what it
// knew about them, so the chat of each is kept in session storage (it lasts as
// long as the native layer does: the app's own restart ends both).
// Spec: docs/features/session-panels.

type Kind = "browser" | "terminal";

const KEY = "evo.tabChats";

function read(): Record<string, string> {
  try {
    const raw = sessionStorage.getItem(KEY);
    const parsed = raw ? (JSON.parse(raw) as unknown) : null;
    return parsed && typeof parsed === "object" ? (parsed as Record<string, string>) : {};
  } catch {
    return {};
  }
}

function write(map: Record<string, string>) {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(map));
  } catch {
    // Without session storage a reload loses the grouping; the tabs still work.
  }
}

/** Remember that tab `id` (a page label or a pty id) belongs to `chat`. */
export function rememberTabChat(kind: Kind, id: string | number, chat: string | null) {
  const map = read();
  if (chat) map[`${kind}:${id}`] = chat;
  else delete map[`${kind}:${id}`];
  write(map);
}

/** The chat tab `id` belonged to before the window reloaded, if known. */
export function recalledTabChat(kind: Kind, id: string | number): string | null {
  return read()[`${kind}:${id}`] ?? null;
}

export function forgetTabChat(kind: Kind, id: string | number) {
  const map = read();
  if (delete map[`${kind}:${id}`]) write(map);
}
