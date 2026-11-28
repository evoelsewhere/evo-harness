// A mate's conversation as a phone messaging app shows it
// (docs/features/evo-mates/spec.md, FR-58 – FR-61; ux.md 3): the person's
// bubbles on the right in the accent colour, the mate's on the left; runs
// grouped with a tail on the last; a time line after a gap; a typing bubble
// while the mate works; reactions and Reply on every bubble. Turns come from
// the engine store like any chat's; replies render with Harness's Markdown.

import * as React from "react";
import { LearnedLine } from "@/components/learned-line";
import { CopyIcon, InfoIcon, ReplyIcon } from "lucide-react";
import { Markdown } from "@/components/markdown";
import { openFileRef } from "@/components/open-file-ref";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { DropdownMenuItem, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { TurnError, type Turn } from "@/features/chat";
import { formatCost, formatDuration } from "@/lib/format";
import type { Mate, ReactionKind } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";
import { emojiOf, parsePrompt, REACTIONS, type ReplyTo } from "./message-markup";
import { threadRows, timeLine, type Bubble, type Row } from "./thread-rows";

function MetaInfo({ turn }: { turn: Turn }) {
  const { meta } = turn;
  const parts = [meta.model, meta.durationMs ? formatDuration(meta.durationMs) : null, meta.cost ? formatCost(meta.cost) : null].filter(
    Boolean,
  );
  if (!parts.length) return null;
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            aria-label="Details"
            className="rounded-full p-1 text-muted-foreground opacity-0 transition-opacity group-hover/bubble:opacity-100 hover:text-foreground"
          />
        }
      >
        <InfoIcon className="size-3.5" />
      </TooltipTrigger>
      <TooltipContent>{parts.join(" · ")}</TooltipContent>
    </Tooltip>
  );
}

function BubbleActions({
  bubble,
  reaction,
  onReact,
  onReply,
}: {
  bubble: Bubble;
  reaction?: ReactionKind;
  onReact(kind: ReactionKind | null): void;
  onReply(): void;
}) {
  return (
    <div
      className={cn(
        "absolute -top-9 z-10 hidden items-center gap-0.5 rounded-full bg-popover p-1 shadow-lg ring-1 ring-foreground/10 group-hover/bubble:flex",
        bubble.side === "me" ? "right-0" : "left-0",
      )}
    >
      {REACTIONS.map((r) => (
        <button
          key={r.kind}
          type="button"
          aria-label={r.label}
          aria-pressed={reaction === r.kind}
          onClick={() => onReact(reaction === r.kind ? null : r.kind)}
          className={cn(
            "flex size-7 items-center justify-center rounded-full text-sm transition-transform hover:scale-125",
            reaction === r.kind && "bg-brand/15",
          )}
        >
          {r.emoji}
        </button>
      ))}
      <span className="mx-0.5 h-4 w-px bg-border" />
      <button
        type="button"
        aria-label="Reply"
        onClick={onReply}
        className="flex size-7 items-center justify-center rounded-full text-muted-foreground hover:text-foreground"
      >
        <ReplyIcon className="size-4" />
      </button>
      <button
        type="button"
        aria-label="Copy"
        onClick={() => void navigator.clipboard.writeText(bubble.text)}
        className="flex size-7 items-center justify-center rounded-full text-muted-foreground hover:text-foreground"
      >
        <CopyIcon className="size-3.5" />
      </button>
    </div>
  );
}

function BubbleView({
  row,
  mate,
  reaction,
  onReact,
  onReply,
}: {
  row: Extract<Row, { kind: "bubble" }>;
  mate: Mate;
  reaction?: ReactionKind;
  onReact(kind: ReactionKind | null): void;
  onReply(): void;
}) {
  const { bubble, first, last } = row;
  const me = bubble.side === "me";
  // Files the mate names open from its working folder (rich-replies FR-37).
  const root = mate.projectFolder ?? mate.folder;
  const openRef = React.useCallback((reference: string) => void openFileRef(reference, root, "mates"), [root]);
  return (
    <div className={cn("flex w-full px-4", me ? "justify-end" : "justify-start", first ? "mt-2" : "mt-0.5")}>
      <div className={cn("group/bubble relative flex max-w-[72%] flex-col", me ? "items-end" : "items-start")}>
        {bubble.reply && (
          <div
            className={cn(
              "mb-0.5 max-w-full truncate rounded-2xl border border-border px-3 py-1 text-xs text-muted-foreground",
              me ? "mr-2" : "ml-2",
            )}
          >
            <span className="font-medium">{bubble.reply.from === "you" ? "You" : bubble.reply.from}:</span> {bubble.reply.text}
          </div>
        )}
        <div className={cn("flex items-end gap-1", me && "flex-row-reverse")}>
          <ContextMenu>
            <ContextMenuTrigger
              render={
                <div
                  className={cn(
                    "relative min-w-0 rounded-[20px] px-3.5 py-2 text-[14px] leading-[20px]",
                    me ? "bg-brand text-brand-foreground" : "bg-muted text-foreground",
                    last && (me ? "rounded-br-[6px]" : "rounded-bl-[6px]"),
                    // Replies are markdown: tighter blocks in a bubble.
                    "[&_p]:my-0 [&_p+p]:mt-2 [&_ul]:my-1 [&_ol]:my-1 [&_li]:my-0 [&_li]:leading-5 [&_li_p]:my-0 [&_ul]:pl-4 [&_ol]:pl-4",
                    me && "[&_a]:text-brand-foreground [&_a]:underline [&_code]:bg-white/15",
                  )}
                />
              }
            >
              {bubble.images?.length ? (
                <div className="mb-1 flex flex-wrap gap-1">
                  {bubble.images.map((src, i) => (
                    <img key={i} src={src} alt="" className="max-h-40 rounded-xl" />
                  ))}
                </div>
              ) : null}
              {me ? (
                <span className="whitespace-pre-wrap break-words">{bubble.text}</span>
              ) : (
                <Markdown streaming={bubble.streaming} blocks onLocalLink={openRef}>
                  {bubble.text}
                </Markdown>
              )}
              {reaction && (
                <span
                  className={cn(
                    "absolute -top-3 flex size-6 items-center justify-center rounded-full bg-background text-xs shadow ring-1 ring-border",
                    me ? "-left-3" : "-right-3",
                  )}
                  aria-label={`Reacted ${reaction}`}
                >
                  {emojiOf(reaction)}
                </span>
              )}
            </ContextMenuTrigger>
            <ContextMenuContent className="w-48">
              <div className="flex gap-0.5 px-1 pb-1">
                {REACTIONS.map((r) => (
                  <button
                    key={r.kind}
                    type="button"
                    aria-label={r.label}
                    onClick={() => onReact(reaction === r.kind ? null : r.kind)}
                    className="flex size-7 items-center justify-center rounded-full text-sm hover:bg-accent"
                  >
                    {r.emoji}
                  </button>
                ))}
              </div>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={onReply}>
                <ReplyIcon />
                Reply
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => void navigator.clipboard.writeText(bubble.text)}>
                <CopyIcon />
                Copy
              </DropdownMenuItem>
            </ContextMenuContent>
          </ContextMenu>
          {!me && last && !bubble.streaming && <MetaInfo turn={bubble.turn} />}
        </div>
        <BubbleActions bubble={bubble} reaction={reaction} onReact={onReact} onReply={onReply} />
        <span className="sr-only">{me ? "You" : mate.name}</span>
      </div>
    </div>
  );
}

function Typing({ step }: { step: string | null }) {
  return (
    <div className="mt-2 flex flex-col items-start gap-1 px-4" aria-live="polite" aria-label="Working">
      <div className="flex items-center gap-1 rounded-[20px] rounded-bl-[6px] bg-muted px-3.5 py-3">
        {[0, 150, 300].map((delay) => (
          <span
            key={delay}
            className="size-2 rounded-full bg-muted-foreground/70 motion-safe:animate-bounce"
            style={{ animationDelay: `${delay}ms`, animationDuration: "1s" }}
          />
        ))}
      </div>
      {step && <span className="px-1 text-xs text-muted-foreground">{step}</span>}
    </div>
  );
}

/** The thread of `mate`'s conversation, shown when the engine store shows its chat. */
export function MateThread({
  mate,
  reactions,
  onReact,
  onReply,
  empty,
}: {
  mate: Mate;
  reactions: Record<string, ReactionKind>;
  onReact(bubble: Bubble, kind: ReactionKind | null): void;
  onReply(reply: ReplyTo): void;
  empty: React.ReactNode;
}) {
  const turns = useEngineStore((s) => s.transcript.turns);
  const busy = useEngineStore((s) => s.transcript.busy);
  const queued = useEngineStore((s) => s.transcript.queued);
  const shellWorking = useSessionStore((s) => s.work[mate.chatId] === "working");
  const rows = React.useMemo(() => threadRows(turns, busy, shellWorking && !busy), [turns, busy, shellWorking]);
  const scroller = React.useRef<HTMLDivElement>(null);
  const pinned = React.useRef(true);

  // Follow new messages while the person is at the bottom.
  React.useLayoutEffect(() => {
    const el = scroller.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;
  }, [rows]);

  if (rows.length === 0) return <div className="flex h-full items-center justify-center">{empty}</div>;
  return (
    <div
      ref={scroller}
      onScroll={(event) => {
        const el = event.currentTarget;
        pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
      }}
      className="h-full overflow-y-auto pt-4 pb-6"
    >
      <div className="mx-auto flex max-w-3xl flex-col">
        {rows.map((row) => {
          switch (row.kind) {
            case "time":
              return (
                <div key={row.key} className="mt-4 mb-1 text-center text-[11px] font-medium text-muted-foreground">
                  {timeLine(row.at)}
                </div>
              );
            case "bubble":
              return (
                <BubbleView
                  key={row.bubble.key}
                  row={row}
                  mate={mate}
                  reaction={reactions[row.bubble.key]}
                  onReact={(kind) => onReact(row.bubble, kind)}
                  onReply={() =>
                    onReply({ from: row.bubble.side === "me" ? "you" : mate.name, text: row.bubble.text })
                  }
                />
              );
            case "typing":
              return <Typing key={row.key} step={row.step} />;
            case "steps":
              return (
                <div key={row.key} className="mt-1 px-5 text-[11px] text-muted-foreground">
                  {row.text}
                </div>
              );
            case "learned":
              return <LearnedLine key={row.key} chat={mate.chatId} turnEntry={row.turn.entryId} className="mt-1 px-4" />;
            case "memory":
              return (
                <div key={row.key} className="mt-1 px-4">
                  <span className="inline-flex items-center rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
                    Saved to memory
                  </span>
                </div>
              );
            case "error":
              return (
                <div key={row.key} className="mt-2 px-4">
                  <TurnError error={row.turn.error!} provider={row.turn.meta.provider} prompt={row.turn.prompt || undefined} />
                </div>
              );
            case "delivered":
              return (
                <div key={row.key} className="mt-0.5 px-5 text-right text-[11px] text-muted-foreground">
                  Delivered
                </div>
              );
            default:
              return null;
          }
        })}
        {/* Messages the mate has not read yet (FR-49): faded, with when it will. */}
        {(queued ?? []).map((message) => (
          <div key={message.entryId} className="mt-1 flex flex-col items-end px-4">
            <div className="max-w-[72%] rounded-[20px] rounded-br-[6px] bg-brand/50 px-3.5 py-2 text-[14px] leading-5 whitespace-pre-wrap text-brand-foreground">
              {parsePrompt(message.text).text}
            </div>
            <span className="mt-0.5 px-1 text-[11px] text-muted-foreground">
              {message.deliverAs === "steer" ? "Seen while working" : "Next"}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

export type { Bubble };
