// Opening a place or a message from a link (docs/features/evo-mates-threads-
// board/spec.md, FR-17): `channel/<id>`, `mate/<id>` or
// `message/<chat>/<line>`. A message opens its conversation and scrolls to
// the line; a line in a thread opens the thread.

import { notify } from "@/features/notifications/notify";
import { useBoardStore } from "@/stores/board-store";
import { useChannelStore } from "@/stores/channel-store";
import { useMatesStore } from "@/stores/mates-store";
import { jumpToMessage } from "./pins-bar";
import { messageLink } from "./links-logic";

async function waitFor(line: string, ms = 4000): Promise<boolean> {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    if (document.querySelector(`[data-entry="${CSS.escape(line)}"], [data-key="${CSS.escape(line)}"]`)) return jumpToMessage(line);
    await new Promise((resolve) => setTimeout(resolve, 150));
  }
  return false;
}

export async function openAppLink(path: string): Promise<void> {
  const [kind, a, b] = path.split("/");
  const store = useMatesStore.getState();
  if (kind === "channel" && a) return store.openChannel(a);
  if (kind === "mate" && a) return store.open(a);
  if (kind === "message" && a && b) {
    await store.openChat(a);
    useBoardStore.getState().setTab(a, "chat");
    if (await waitFor(b)) return;
    // A line in a thread: open the thread (FR-1).
    const entry = (useChannelStore.getState().entries[a] ?? []).find((e) => "id" in e && e.id === b);
    const thread = entry && "thread" in entry ? entry.thread : null;
    if (thread) {
      useBoardStore.getState().showThread({ chat: a, root: thread });
      if (await waitFor(b)) return;
    }
    notify({ source: "mates", title: "That message is not here any more", body: "It may have been deleted, or it is too far back.", system: false });
  }
}

/** `openAppLink` for Markdown's `onAppLink`: one function, so its memo holds (board review #7). */
export function openLinkPath(path: string): void {
  void openAppLink(path);
}

/** Copy a line's link (FR-17). */
export function copyMessageLink(chat: string, line: string): void {
  void navigator.clipboard
    .writeText(messageLink(chat, line))
    .then(() => notify({ source: "mates", title: "Link copied", body: "Paste it in any conversation to link back here.", system: false }))
    .catch(() => undefined);
}
