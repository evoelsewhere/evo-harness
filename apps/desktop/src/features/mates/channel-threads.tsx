// A channel's **Threads** tab (docs/features/evo-mates-threads-board/spec.md,
// FR-23): every thread of the channel, open ones first by their last reply,
// with the line it started from, its replies and repliers, whether it is
// resolved, and its card. A row opens the thread beside the conversation.

import * as React from "react";
import { CheckCircle2Icon, ChevronRightIcon, Columns3Icon, MessagesSquareIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { formatRelative } from "@/lib/format";
import type { Channel } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useBoardStore } from "@/stores/board-store";
import { useChannelStore } from "@/stores/channel-store";
import { useThreadsStore } from "@/stores/threads-store";
import { channelThreads } from "./channel-rows";
import { useMembers } from "./channel-header";
import { EmptyState } from "./empty-state";
import { excerpt, parsePrompt, threadMark } from "./message-markup";

const EMPTY: never[] = [];

export function ChannelThreads({ channel }: { channel: Channel }) {
  const members = useMembers(channel);
  const entries = useChannelStore((s) => s.entries[channel.chatId] ?? EMPTY);
  const states = useThreadsStore((s) => s.byChat[channel.chatId]);
  const cards = useBoardStore((s) => s.views[channel.id]?.cards);
  const [showResolved, setShowResolved] = React.useState(false);
  React.useEffect(() => {
    void useThreadsStore.getState().loadChat(channel.chatId);
  }, [channel.chatId]);
  const threads = React.useMemo(() => channelThreads(entries), [entries]);
  const open = (root: string) => {
    useBoardStore.getState().showThread({ chat: channel.chatId, root });
    useBoardStore.getState().setTab(channel.chatId, "chat");
  };
  const nameOf = (id: string) => (id === "person" ? "You" : (members.find((m) => m.id === id)?.name ?? id));
  const shown = threads.filter((t) => !states?.[t.root]?.resolved);
  const resolved = threads.filter((t) => states?.[t.root]?.resolved);
  const row = (t: (typeof threads)[number]) => {
    const state = states?.[t.root];
    const card = cards?.find((c) => c.threadRoot === t.root);
    const words = excerpt(parsePrompt(threadMark(t.parent.text).text).text, 140);
    return (
      <li key={t.root} className="border-b border-border/50 last:border-b-0">
        <button type="button" onClick={() => open(t.root)} className="flex w-full items-start gap-3 px-3.5 py-3 text-left transition-colors hover:bg-accent/60">
          <span className="mt-0.5 flex shrink-0 -space-x-1.5">
            {t.repliers.filter((id) => id !== "person").slice(0, 3).map((id) => {
              const mate = members.find((m) => m.id === id);
              return mate ? <MateFace key={id} avatar={mate.avatar} size={24} plate={false} label={mate.name} className="rounded-full ring-2 ring-card" /> : null;
            })}
            {t.repliers.every((id) => id === "person") && (
              <span className="flex size-6 items-center justify-center rounded-full bg-muted text-muted-foreground">
                <MessagesSquareIcon className="size-3.5" />
              </span>
            )}
          </span>
          <span className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className={cn("line-clamp-2 text-[13px] text-foreground", state?.unread && "font-semibold")}>
              <span className="font-medium">{nameOf(t.parent.author)}:</span> {words}
            </span>
            <span className="flex flex-wrap items-center gap-x-1.5 text-xs text-muted-foreground">
              <span className="font-medium text-brand">
                {t.count} {t.count === 1 ? "reply" : "replies"}
              </span>
              · last by {nameOf(t.repliers[0] ?? "")} · {formatRelative(t.last)}
              {card && (
                <span className="flex items-center gap-1 rounded-full bg-muted px-1.5 py-px">
                  <Columns3Icon className="size-3" />
                  <span className="font-mono">{card.key}</span>
                </span>
              )}
              {state?.resolved && (
                <span className="flex items-center gap-0.5 text-emerald-700 dark:text-emerald-300">
                  <CheckCircle2Icon className="size-3" />
                  Resolved
                </span>
              )}
            </span>
          </span>
          {state?.unread && <span className="mt-1.5 size-2 shrink-0 rounded-full bg-brand" aria-label="Unread" />}
        </button>
      </li>
    );
  };
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-3xl flex-col gap-4 px-4 py-5">
        {threads.length === 0 ? (
          <EmptyState text="No threads in this channel yet. Reply in thread on any line to start one." />
        ) : (
          <>
            {shown.length > 0 && <ul className="flex flex-col overflow-hidden rounded-2xl bg-card shadow-[0_0_0_1px_var(--border)]">{shown.map(row)}</ul>}
            {resolved.length > 0 && (
              <section className="flex flex-col gap-2">
                <button type="button" onClick={() => setShowResolved(!showResolved)} aria-expanded={showResolved} className="flex items-center gap-1 px-1 text-[13px] font-semibold text-foreground">
                  <ChevronRightIcon className={cn("size-4 transition-transform", showResolved && "rotate-90")} />
                  Resolved <span className="rounded-full bg-muted px-1.5 text-[11px] font-medium text-muted-foreground">{resolved.length}</span>
                </button>
                {showResolved && <ul className="flex flex-col overflow-hidden rounded-2xl bg-card shadow-[0_0_0_1px_var(--border)]">{resolved.map(row)}</ul>}
              </section>
            )}
          </>
        )}
      </div>
    </div>
  );
}
