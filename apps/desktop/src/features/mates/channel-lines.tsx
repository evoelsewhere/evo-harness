// The lines of a channel (docs/features/evo-mates/spec.md, FR-14 – FR-19):
// bubbles, who answers, hand-off cards, notices and retro cards, with what
// each line can do (react, reply, pin, forward, reply in thread). Shared by
// the channel's feed and its thread panel (evo-mates-threads-board FR-1).

import * as React from "react";
import { ArrowRightIcon, ChevronRightIcon, Columns3Icon } from "lucide-react";
import { useBoardStore } from "@/stores/board-store";
import { linkCards, splitCardKeys } from "./board-logic";
import { Markdown } from "@/components/markdown";
import { openFileRef } from "@/components/open-file-ref";
import { MateFace } from "@/components/mate-face/mate-face";
import { formatRelative } from "@/lib/format";
import type { ChannelReaction, Mate, ReactionKind } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { boldMentions, handoffSummary, handoffVerb, mentionParts, type ChannelRow, type ThreadSummary } from "./channel-rows";
import { MatePresenceFace } from "./presence-face";
import { clockTime } from "./thread-rows";
import { BUBBLE_TEXT } from "./bubble-style";
import { splitMessage } from "./attachments";
import { NamedFiles, SentByMe } from "./attachment-views";
import { RetroCard } from "./retro-card";
import { LineBar, LinkCard, ReactionChips } from "./line-tools";
import { parsePrompt, type ReplyTo } from "./message-markup";
import { jumpToMessage } from "./pins-bar";

/** "3 replies · Writer, Analyst · 2 min ago" under a thread's parent (FR-2). */
function ThreadSummaryButton({
  summary,
  faceOf,
  nameOf,
  onOpen,
}: {
  summary: ThreadSummary;
  faceOf(id: string): Mate | undefined;
  nameOf(id: string): string;
  onOpen(): void;
}) {
  const names = summary.repliers.slice(0, 2).map((id) => (id === "person" ? "You" : nameOf(id)));
  const more = summary.repliers.length - names.length;
  return (
    <button
      type="button"
      onClick={onOpen}
      className="mt-1 flex max-w-full items-center gap-1.5 rounded-full py-0.5 pr-2.5 pl-1 text-[11.5px] text-muted-foreground transition-colors hover:bg-muted"
    >
      <span className="flex shrink-0 -space-x-1">
        {summary.repliers
          .filter((id) => id !== "person")
          .slice(0, 3)
          .map((id) => {
            const mate = faceOf(id);
            return mate ? <MateFace key={id} avatar={mate.avatar} size={16} plate={false} label={mate.name} className="rounded-full ring-1 ring-background" /> : null;
          })}
      </span>
      <span className="shrink-0 font-medium text-brand">
        {summary.count} {summary.count === 1 ? "reply" : "replies"}
      </span>
      <span className="min-w-0 truncate">
        · {names.join(", ")}
        {more > 0 ? ` +${more}` : ""} · {formatRelative(summary.last)}
      </span>
    </button>
  );
}

function Line({ children, tone = "muted" }: { children: React.ReactNode; tone?: "muted" | "warn" }) {
  return (
    <div className="flex justify-center px-6 py-1.5">
      <span
        className={cn(
          "max-w-[85%] rounded-full px-3 py-1 text-center text-[11.5px]",
          // A hand-off's long URL breaks like a bubble's (qa-2026-11-28, H1).
          BUBBLE_TEXT,
          tone === "warn" ? "bg-destructive/10 text-destructive" : "bg-muted/60 text-muted-foreground",
        )}
      >
        {children}
      </span>
    </div>
  );
}

/** A hand-off as a card: sender → receiver, the verb, a summary that opens (FR-20; QA C1). */
function HandoffCard({ row, faceOf, folder }: { row: Extract<ChannelRow, { kind: "handoff" }>; faceOf(id: string): Mate | undefined; folder: string | null }) {
  const [open, setOpen] = React.useState(false);
  const from = faceOf(row.from);
  const to = faceOf(row.to);
  const summary = handoffSummary(row.text);
  const full = handoffSummary(row.text, 4000);
  const long = full !== summary;
  return (
    <div className="flex flex-col items-center px-6 py-1">
      <button
        type="button"
        aria-expanded={long ? open : undefined}
        onClick={() => long && setOpen(!open)}
        className={cn(
          "flex max-w-[min(560px,90%)] min-w-0 items-start gap-2 rounded-2xl bg-muted/50 px-3 py-1.5 text-left text-[12px] text-muted-foreground shadow-[0_0_0_1px_var(--border)]",
          long && "hover:bg-muted",
          BUBBLE_TEXT,
        )}
      >
        <span className="flex shrink-0 items-center gap-1 pt-px">
          {from ? <MatePresenceFace mate={from} size={18} ring="var(--background)" /> : null}
          <ArrowRightIcon className="size-3" />
          {to ? <MatePresenceFace mate={to} size={18} ring="var(--background)" /> : null}
        </span>
        <span className={cn("min-w-0 flex-1", !open && "line-clamp-1")}>
          <span className="font-medium text-foreground">{row.fromName}</span> {handoffVerb(row.handoff)}{" "}
          <span className="font-medium text-foreground">{row.toName}</span>
          {summary && <span data-find-text>: {open ? full : summary}</span>}
        </span>
        {long && <ChevronRightIcon className={cn("mt-0.5 size-3 shrink-0 transition-transform", open && "rotate-90")} />}
      </button>
      {/* Files given with it (evo-mates-messaging FR-6). */}
      {row.files.length > 0 && (
        <div className="mt-1 flex max-w-[min(560px,90%)] justify-center">
          <NamedFiles text="" paths={row.files} root={folder} />
        </div>
      )}
    </div>
  );
}

/** What every line of a channel can do (evo-mates-messaging FR-3, FR-5, FR-10, FR-14). */
export interface LineActions {
  reactions: Map<string, ChannelReaction[]>;
  react(message: string, kind: ReactionKind | null): void;
  reply(to: ReplyTo): void;
  pinned: Set<string>;
  pin(key: string, text: string): void;
  forward(from: string, text: string): void;
  nameOf(id: string): string;
  /** Open the thread under a line; absent inside a thread (evo-mates-threads-board FR-1, D3). */
  openThread?(root: string): void;
}
export const LineContext = React.createContext<LineActions | null>(null);

/** A line that mentions the person (`@you`, FR-18). */
const MENTIONS_YOU = /(^|[^\p{L}\p{N}@])@you(?![\p{L}\p{N}_])/iu;

export function RowView({ row, faceOf, names, folder }: { row: ChannelRow; faceOf(id: string): Mate | undefined; names: string[]; folder: string | null }) {
  switch (row.kind) {
    case "day":
      return null;
    case "time":
      return <div className="mt-2 py-1 text-center text-[11px] text-muted-foreground tabular-nums">{clockTime(row.at)}</div>;
    case "route":
      // Who answers, with their faces (FR-20; evo-mates FR-36).
      return (
        <div className="flex justify-center px-6 py-1">
          <span className={cn("flex max-w-[85%] items-center gap-1.5 rounded-full bg-muted/60 py-0.5 pr-3 pl-1 text-[11.5px] text-muted-foreground", BUBBLE_TEXT)}>
            <span className="flex -space-x-1">
              {row.picked.slice(0, 3).map((id) => {
                const mate = faceOf(id);
                return mate ? <MatePresenceFace key={id} mate={mate} size={18} ring="var(--background)" /> : null;
              })}
            </span>
            {!row.picked.length && <span className="w-1.5" />}
            {row.text}
          </span>
        </div>
      );
    case "handoff":
      return <HandoffCard row={row} faceOf={faceOf} folder={folder ?? faceOf(row.from)?.folder ?? null} />;
    case "notice":
      return <Line tone={row.stop ? "warn" : "muted"}>{row.text}</Line>;
    case "board":
      // A card's change: its key opens the card (evo-mates-threads-board FR-13).
      return (
        <Line>
          <Columns3Icon className="mr-1 inline size-3 -translate-y-px" />
          {row.text.split(row.card).map((part, i, all) => (
            <React.Fragment key={i}>
              {part}
              {i < all.length - 1 && (
                <button type="button" onClick={() => useBoardStore.getState().showCard(row.card)} className="font-medium text-link hover:underline">
                  {row.card}
                </button>
              )}
            </React.Fragment>
          ))}
        </Line>
      );
    case "retro":
      return <RetroCard card={row.card} />;
    case "bubble":
      return <BubbleLine row={row} faceOf={faceOf} names={names} folder={folder} />;
  }
}

function BubbleLine({ row, faceOf, names, folder }: { row: Extract<ChannelRow, { kind: "bubble" }>; faceOf(id: string): Mate | undefined; names: string[]; folder: string | null }) {
  const actions = React.useContext(LineContext);
  const cardKeys = useBoardStore((s) => s.keys);
  {
      const mate = row.me ? undefined : faceOf(row.author);
      // The person's pictures, files and pasted text show as chips (FR-83);
      // a quote they replied with, or a forward, shows above (FR-5, FR-10).
      const split = row.me ? splitMessage(row.text) : null;
      const parsed = split ? parsePrompt(split.text) : null;
      const said = split && parsed ? { ...split, text: parsed.text } : null;
      const quote = row.me ? (parsed?.reply ? { name: parsed.reply.from === "you" ? "You" : parsed.reply.from, text: parsed.reply.text, id: null as string | null } : null) : row.quote ? { ...row.quote } : null;
      const root = folder ?? mate?.projectFolder ?? mate?.folder ?? null;
      const reactions = actions?.reactions.get(row.key) ?? [];
      const mine = reactions.find((r) => r.reactor === "person")?.reaction;
      const plain = said?.text ?? row.text;
      const mentionsYou = !row.me && MENTIONS_YOU.test(row.text);
      const jump = () => {
        if (quote?.id && jumpToMessage(quote.id)) return;
        // A quote of the person names no id: find the line by its words.
        const head = (quote?.text ?? "").replace(/\u2026$/, "").slice(0, 60);
        const found = [...document.querySelectorAll<HTMLElement>("[data-entry]")].find((el) => (el.textContent ?? "").includes(head));
        if (found?.dataset.entry) jumpToMessage(found.dataset.entry);
      };
      return (
        <div
          data-entry={row.key}
          className={cn(
            "group/line relative flex items-end gap-2 px-4 pt-1",
            row.me ? "justify-end" : "justify-start",
            row.first ? "mt-2" : "mt-0.5",
            mentionsYou && "bg-amber-500/[0.06] shadow-[inset_2px_0_0_0_rgb(245_158_11/0.7)]",
          )}
        >
          {!row.me && (
            <span className="w-7 shrink-0">
              {row.last && mate && <MateFace avatar={mate.avatar} size={28} plate={false} label={mate.name} />}
            </span>
          )}
          <div className={cn("relative flex max-w-[75%] min-w-0 flex-col", row.me ? "items-end" : "items-start")}>
            {!row.me && row.first && <span className="mb-0.5 ml-2 text-[11px] font-medium text-muted-foreground">{row.name}</span>}
            {parsed?.forwarded && (
              <span className="mb-0.5 mr-2 text-[11px] text-muted-foreground">Forwarded from {parsed.forwarded.from}</span>
            )}
            {quote && (
              <button
                type="button"
                onClick={jump}
                title="Go to the message"
                className={cn(
                  "mb-0.5 flex max-w-full items-stretch gap-2 rounded-2xl bg-muted/50 py-1 pr-3 pl-1.5 text-left text-xs text-muted-foreground transition-colors hover:bg-muted",
                  row.me ? "mr-2" : "ml-2",
                )}
              >
                <span aria-hidden className="w-0.5 shrink-0 rounded-full bg-brand/60" />
                <span className="min-w-0 truncate">
                  <span className="font-medium text-foreground/80">{quote.name}</span> {quote.text}
                </span>
              </button>
            )}
            <div
              data-bubble
              className={cn(
                "rounded-[20px] px-3.5 py-2 text-[14px] leading-[20px]",
                BUBBLE_TEXT,
                row.me ? "bg-brand text-brand-foreground" : "bg-muted text-foreground",
                row.last && (row.me ? "rounded-br-[6px]" : "rounded-bl-[6px]"),
                row.failed && "opacity-70",
                "[&_p]:my-0 [&_p+p]:mt-2 [&_ul]:my-1 [&_ol]:my-1 [&_li]:my-0 [&_ul]:pl-4 [&_ol]:pl-4",
              )}
            >
              {said && <SentByMe files={said.files} pasted={said.pasted} />}
              {parsed?.forwarded && (
                <div className="mb-1 border-l-2 border-white/40 pl-2 text-[13px] whitespace-pre-wrap opacity-90">{parsed.forwarded.text}</div>
              )}
              <div data-find-text>
              {row.me ? (
                <span className="whitespace-pre-wrap break-words">
                  {mentionParts(plain, names).map((part, i) =>
                    part.mention ? (
                      <span key={i} className="rounded-md bg-white/20 px-0.5 font-semibold">
                        {part.text}
                      </span>
                    ) : (
                      <React.Fragment key={i}>
                        {splitCardKeys(part.text, cardKeys).map((piece, j) =>
                          piece.key ? (
                            <button key={j} type="button" onClick={() => useBoardStore.getState().showCard(piece.key!)} className="rounded bg-white/20 px-0.5 font-medium underline-offset-2 hover:underline">
                              {piece.text}
                            </button>
                          ) : (
                            <React.Fragment key={j}>{piece.text}</React.Fragment>
                          ),
                        )}
                      </React.Fragment>
                    ),
                  )}
                </span>
              ) : (
                <Markdown
                  blocks
                  onCardLink={(key) => useBoardStore.getState().showCard(key)}
                  onLocalLink={(reference) => {
                    // A seat works in the channel's folder, else in its mate's (rich-replies FR-37).
                    const author = faceOf(row.author);
                    void openFileRef(reference, folder ?? author?.projectFolder ?? author?.folder ?? null, "mates");
                  }}
                >
                  {linkCards(boldMentions(row.text, [...names, "you"]), cardKeys)}
                </Markdown>
              )}
              </div>
            </div>
            {/* The files a mate's line names that exist, under it (FR-83). */}
            {!row.me && <NamedFiles text={row.text} root={root} />}
            {!row.me && <LinkCard text={row.text} />}
            {actions && (
              <ReactionChips
                reactions={reactions}
                nameOf={actions.nameOf}
                align={row.me ? "end" : "start"}
                onToggle={(kind) => actions.react(row.key, mine === kind ? null : kind)}
              />
            )}
            {actions && (
              <LineBar
                side={row.me ? "end" : "start"}
                mine={mine}
                pinned={actions.pinned.has(row.key)}
                onReact={(kind) => actions.react(row.key, kind)}
                onReply={() => actions.reply({ from: row.me ? "you" : row.name, text: plain })}
                onPin={() => actions.pin(row.key, plain)}
                onForward={() => actions.forward(row.me ? "You" : row.name, plain)}
                onThread={actions.openThread ? () => actions.openThread?.(row.key) : undefined}
              />
            )}
            {/* Its thread: replies, who, when (evo-mates-threads-board FR-2). */}
            {actions?.openThread && row.thread && (
              <ThreadSummaryButton summary={row.thread} faceOf={faceOf} nameOf={actions.nameOf} onOpen={() => actions.openThread?.(row.key)} />
            )}
            {row.steers && <span className="mt-0.5 mr-2 text-[11px] text-muted-foreground">Seen by {row.steers} while working</span>}
            {row.me && !row.steers && row.seenBy && row.seenBy.length > 0 && (
              <span className="mt-0.5 mr-2 flex items-center gap-1 text-[11px] text-muted-foreground" aria-label={`Seen by ${row.seenBy.map((id) => faceOf(id)?.name ?? id).join(", ")}`}>
                Seen by
                <span className="flex -space-x-1">
                  {row.seenBy.slice(0, 4).map((id) => {
                    const seer = faceOf(id);
                    return seer ? <MateFace key={id} avatar={seer.avatar} size={14} plate={false} label={seer.name} className="rounded-full ring-1 ring-background" /> : null;
                  })}
                </span>
              </span>
            )}
          </div>
        </div>
      );
  }
}
