// **Threads** (docs/features/evo-mates-threads-board/spec.md, FR-20, FR-21):
// the threads the person follows, unread first, each with its channel, the
// line it started from, how many replies and who wrote last. Resolved
// threads fold into their own list. A row opens the thread in its channel.

import * as React from "react";
import { CheckCircle2Icon, ChevronRightIcon, MessagesSquareIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { formatRelative } from "@/lib/format";
import type { ThreadFollow } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useBoardStore } from "@/stores/board-store";
import { useMatesStore } from "@/stores/mates-store";
import { useThreadsStore } from "@/stores/threads-store";
import { EmptyState } from "./empty-state";

function open(thread: ThreadFollow) {
  useBoardStore.getState().showThread({ chat: thread.chatId, root: thread.root });
  useBoardStore.getState().setTab(thread.chatId, "chat");
  void useMatesStore.getState().openChat(thread.chatId);
}

function ThreadRow({ thread }: { thread: ThreadFollow }) {
  const channels = useMatesStore((s) => s.channels);
  const mates = useMatesStore((s) => s.mates);
  const channel = channels.find((c) => c.chatId === thread.chatId);
  const last = thread.lastAuthor === "person" ? null : mates.find((m) => m.id === thread.lastAuthor);
  return (
    <li className="border-b border-border/50 last:border-b-0">
      <button type="button" onClick={() => open(thread)} className="flex w-full items-center gap-3 px-3.5 py-2.5 text-left transition-colors hover:bg-accent/60">
        {last ? (
          <MateFace avatar={last.avatar} size={30} plate={false} label={last.name} />
        ) : (
          <span className="flex size-[30px] shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground">
            <MessagesSquareIcon className="size-4" />
          </span>
        )}
        <span className="flex min-w-0 flex-1 flex-col">
          <span className={cn("truncate text-[13px] text-foreground", thread.unread && "font-semibold")}>{thread.rootExcerpt || "A thread"}</span>
          <span className="truncate text-xs text-muted-foreground">
            #{channel?.name ?? "a channel"} · {thread.replies} {thread.replies === 1 ? "reply" : "replies"}
            {thread.lastAuthor && ` · last by ${thread.lastAuthor === "person" ? "you" : (last?.name ?? thread.lastAuthor)}`}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
          {thread.resolved && <CheckCircle2Icon className="size-3.5 text-emerald-600 dark:text-emerald-400" aria-label="Resolved" />}
          {thread.lastMs > 0 && formatRelative(thread.lastMs)}
          {thread.unread && <span className="size-2 rounded-full bg-brand" aria-label="Unread" />}
        </span>
      </button>
    </li>
  );
}

export function ThreadsPage() {
  const followed = useThreadsStore((s) => s.followed);
  const [showResolved, setShowResolved] = React.useState(false);
  React.useEffect(() => {
    void useThreadsStore.getState().load();
  }, []);
  const open = followed.filter((t) => !t.resolved);
  const resolved = followed.filter((t) => t.resolved);
  const unread = open.filter((t) => t.unread).length;
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-3xl flex-col gap-4 px-6 py-8">
        <div>
          <h1 className="text-[22px] font-semibold tracking-tight text-foreground">Threads</h1>
          <p className="text-[13px] text-muted-foreground">
            {followed.length === 0 ? "The threads you start, reply in or are mentioned in show here." : `${open.length} open${unread ? ` · ${unread} unread` : ""}${resolved.length ? ` · ${resolved.length} resolved` : ""}`}
          </p>
        </div>
        {followed.length === 0 ? (
          <EmptyState text="No threads yet. Reply in thread on any channel line to start one." />
        ) : (
          <>
            {open.length > 0 && <ul className="flex flex-col overflow-hidden rounded-2xl bg-card shadow-[0_0_0_1px_var(--border)]">{open.map((t) => <ThreadRow key={`${t.chatId}:${t.root}`} thread={t} />)}</ul>}
            {resolved.length > 0 && (
              <section className="flex flex-col gap-2">
                <button type="button" onClick={() => setShowResolved(!showResolved)} aria-expanded={showResolved} className="flex items-center gap-1 px-1 text-[13px] font-semibold text-foreground">
                  <ChevronRightIcon className={cn("size-4 transition-transform", showResolved && "rotate-90")} />
                  Resolved <span className="rounded-full bg-muted px-1.5 text-[11px] font-medium text-muted-foreground">{resolved.length}</span>
                </button>
                {showResolved && <ul className="flex flex-col overflow-hidden rounded-2xl bg-card shadow-[0_0_0_1px_var(--border)]">{resolved.map((t) => <ThreadRow key={`${t.chatId}:${t.root}`} thread={t} />)}</ul>}
              </section>
            )}
          </>
        )}
      </div>
    </div>
  );
}
