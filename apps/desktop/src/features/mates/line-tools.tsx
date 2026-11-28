// What a line offers beyond its text (docs/features/evo-mates-messaging/
// spec.md, FR-3, FR-5, FR-10, FR-14, FR-17): reaction chips with who reacted,
// the hover bar (reactions, Reply, Pin, Forward), and a link's card.

import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { ForwardIcon, MessagesSquareIcon, PinIcon, PinOffIcon, ReplyIcon } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { channelsApi, matesApi, type ChannelReaction, type LinkPreview, type ReactionKind } from "@/lib/mates";
import { inTauri } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useMatesPrefs } from "@/stores/mates-prefs-store";
import { emojiOf, REACTIONS } from "./message-markup";

/** A channel's reactions by line, read again when any changes (FR-3). */
export function useChannelReactions(channel: string): Map<string, ChannelReaction[]> {
  const [list, setList] = React.useState<ChannelReaction[]>([]);
  React.useEffect(() => {
    let gone = false;
    const read = () =>
      void channelsApi
        .reactions(channel)
        .then((next) => !gone && setList(next))
        .catch(() => undefined);
    read();
    const stop = inTauri ? listen<{ channel?: string }>("mates://reactions", ({ payload }) => payload?.channel === channel && read()) : null;
    return () => {
      gone = true;
      void stop?.then((off) => off());
    };
  }, [channel]);
  return React.useMemo(() => {
    const map = new Map<string, ChannelReaction[]>();
    for (const r of list) map.set(r.message, [...(map.get(r.message) ?? []), r]);
    return map;
  }, [list]);
}

/** The chips under a line: one per reaction with its count; hover says who (FR-3). */
export function ReactionChips({
  reactions,
  nameOf,
  onToggle,
  align,
}: {
  reactions: ChannelReaction[];
  nameOf(reactor: string): string;
  onToggle(kind: ReactionKind): void;
  align: "start" | "end";
}) {
  if (!reactions.length) return null;
  const groups = REACTIONS.map((r) => ({ ...r, by: reactions.filter((x) => x.reaction === r.kind) })).filter((g) => g.by.length);
  return (
    <div className={cn("mt-1 flex flex-wrap gap-1", align === "end" ? "justify-end" : "justify-start")}>
      {groups.map((group) => {
        const mine = group.by.some((r) => r.reactor === "person");
        const names = group.by.map((r) => (r.reactor === "person" ? "You" : nameOf(r.reactor)));
        return (
          <Tooltip key={group.kind}>
            <TooltipTrigger
              render={
                <button
                  type="button"
                  aria-pressed={mine}
                  aria-label={`${group.label}: ${names.join(", ")}`}
                  onClick={() => onToggle(group.kind)}
                  className={cn(
                    "flex h-6 items-center gap-1 rounded-full px-2 text-xs shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent",
                    mine ? "bg-brand/10 text-foreground shadow-[0_0_0_1px_var(--brand)]" : "bg-background text-muted-foreground",
                  )}
                />
              }
            >
              <span>{group.emoji}</span>
              <span className="tabular-nums">{group.by.length}</span>
            </TooltipTrigger>
            <TooltipContent>{names.join(", ")}</TooltipContent>
          </Tooltip>
        );
      })}
    </div>
  );
}

/** The hover bar of a channel line or a bubble (FR-3, FR-5, FR-10, FR-14). */
export function LineBar({
  side,
  mine,
  pinned,
  onReact,
  onReply,
  onPin,
  onForward,
  onThread,
}: {
  side: "start" | "end";
  mine?: ReactionKind;
  pinned: boolean;
  onReact?(kind: ReactionKind | null): void;
  onReply?(): void;
  onPin(): void;
  onForward(): void;
  /** Reply in thread (evo-mates-threads-board FR-1). */
  onThread?(): void;
}) {
  const button = "flex size-7 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground";
  return (
    <div
      className={cn(
        "absolute -top-4 z-10 hidden items-center gap-0.5 rounded-full bg-popover p-0.5 shadow-md ring-1 ring-foreground/10 group-focus-within/line:flex group-hover/line:flex",
        side === "end" ? "right-2" : "left-2",
      )}
    >
      {onReact &&
        REACTIONS.map((r) => (
          <button
            key={r.kind}
            type="button"
            aria-label={r.label}
            aria-pressed={mine === r.kind}
            onClick={() => onReact(mine === r.kind ? null : r.kind)}
            className={cn("flex size-7 items-center justify-center rounded-full text-[13px] motion-safe:hover:scale-125", mine === r.kind && "bg-brand/15")}
          >
            {r.emoji}
          </button>
        ))}
      {onReact && <span className="mx-0.5 h-4 w-px bg-border" />}
      {onReply && (
        <button type="button" aria-label="Reply" title="Reply" onClick={onReply} className={button}>
          <ReplyIcon className="size-4" />
        </button>
      )}
      {onThread && (
        <button type="button" aria-label="Reply in thread" title="Reply in thread" onClick={onThread} className={button}>
          <MessagesSquareIcon className="size-3.5" />
        </button>
      )}
      <button type="button" aria-label={pinned ? "Unpin" : "Pin"} title={pinned ? "Unpin" : "Pin"} onClick={onPin} className={button}>
        {pinned ? <PinOffIcon className="size-3.5" /> : <PinIcon className="size-3.5" />}
      </button>
      <button type="button" aria-label="Forward" title="Forward" onClick={onForward} className={button}>
        <ForwardIcon className="size-3.5" />
      </button>
    </div>
  );
}

/** The first web address in a text. */
export function firstLink(text: string): string | null {
  const match = /\bhttps?:\/\/[^\s<>()"'`\]]+[^\s<>()"'`\].,;:!?]/i.exec(text);
  return match ? match[0] : null;
}

/** A link's card under a line, when link previews are on (FR-17). */
export function LinkCard({ text }: { text: string }) {
  const on = useMatesPrefs((s) => s.linkPreviews);
  const url = on ? firstLink(text) : null;
  const [card, setCard] = React.useState<LinkPreview | null>(null);
  React.useEffect(() => {
    setCard(null);
    if (!url || !inTauri) return;
    let gone = false;
    void matesApi
      .linkPreview(url)
      .then((found) => !gone && setCard(found))
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [url]);
  if (!card) return null;
  return (
    <a
      href={card.url}
      target="_blank"
      rel="noreferrer noopener"
      className="mt-1 flex max-w-sm min-w-0 overflow-hidden rounded-2xl bg-card text-left shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent/40"
    >
      {card.image && <img src={card.image} alt="" className="h-20 w-24 shrink-0 object-cover" referrerPolicy="no-referrer" />}
      <span className="flex min-w-0 flex-col gap-0.5 px-3 py-2">
        <span className="truncate text-[11px] text-muted-foreground">{card.site}</span>
        <span className="line-clamp-2 text-[13px] font-medium text-foreground">{card.title}</span>
        {card.description && <span className="line-clamp-1 text-xs text-muted-foreground">{card.description}</span>}
      </span>
    </a>
  );
}

export { emojiOf };
