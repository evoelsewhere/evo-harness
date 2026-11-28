// Messages under the roster while searching (docs/features/evo-mates/spec.md,
// FR-76): mates' conversations and channels whose words match, each with a
// snippet around the match; choosing one opens it. Evo Harness's chats never
// show (the shell's search keeps mates' and channels' chats only).

import * as React from "react";
import { HashIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { formatRelative } from "@/lib/format";
import { matesApi, type MessageHit } from "@/lib/mates";
import { useMatesStore } from "@/stores/mates-store";

/** Wait this long after the last key before searching. */
const DEBOUNCE_MS = 200;

export function MessageHits({ query }: { query: string }) {
  const mates = useMatesStore((s) => s.mates);
  const channels = useMatesStore((s) => s.channels);
  const [hits, setHits] = React.useState<MessageHit[] | null>(null);
  const text = query.trim();

  React.useEffect(() => {
    if (text.length < 2) {
      setHits(null);
      return;
    }
    let gone = false;
    const timer = window.setTimeout(() => {
      void matesApi
        .search(text)
        .then((found) => !gone && setHits(found))
        .catch(() => !gone && setHits([]));
    }, DEBOUNCE_MS);
    return () => {
      gone = true;
      window.clearTimeout(timer);
    };
  }, [text]);

  if (text.length < 2 || hits === null) return null;
  const rows = hits.flatMap((hit) => {
    const mate = hit.kind === "mate" ? mates.find((m) => m.chatId === hit.chatId) : undefined;
    const channel = hit.kind === "channel" ? channels.find((c) => c.chatId === hit.chatId) : undefined;
    return mate || channel ? [{ hit, mate, channel }] : [];
  });
  return (
    <section className="px-2 pt-3" aria-label="Messages">
      <h2 className="px-2 pb-1 text-xs text-muted-foreground">Messages</h2>
      {rows.length === 0 ? (
        <p className="px-2 py-1 text-[13px] text-muted-foreground/80">No message matches</p>
      ) : (
        <ul className="flex flex-col gap-0.5">
          {rows.map(({ hit, mate, channel }) => (
            <li key={hit.chatId}>
              <button
                type="button"
                onClick={() => {
                  const store = useMatesStore.getState();
                  void (mate ? store.open(mate.id) : store.openChannel(channel!.id));
                }}
                className="no-drag flex w-full items-start gap-2 rounded-xl px-2.5 py-2 text-left text-sidebar-foreground transition-colors hover:bg-sidebar-accent/70"
              >
                {mate ? (
                  <MateFace avatar={mate.avatar} size={24} plate={false} label={mate.name} />
                ) : (
                  <HashIcon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                )}
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="flex items-center gap-2">
                    <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{mate ? mate.name : `#${channel!.name}`}</span>
                    <span className="shrink-0 text-[11px] text-muted-foreground">{formatRelative(hit.updatedAt)}</span>
                  </span>
                  <span className="line-clamp-2 text-xs text-muted-foreground">{hit.snippet}</span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
