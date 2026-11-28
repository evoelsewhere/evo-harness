// A mate's conversation as a phone messaging app shows it
// (docs/features/evo-mates/spec.md, FR-58 – FR-61; ux.md 3), refined by
// Evo Mates UI v2 (docs/features/evo-mates-ui-v2/spec.md, FR-10 – FR-18):
// day separators that stick, a "New" line where the person left off, a
// "Jump to latest" pill, time on hover, folded long messages, reply quotes
// that jump to their message, steps as a chip that opens, a Stopped line,
// and no typing dots while the mate waits on a card.

import * as React from "react";
import { LearnedLine } from "@/components/learned-line";
import { ArrowDownIcon, CopyIcon, EyeOffIcon, InfoIcon, PencilIcon, ReplyIcon, RotateCcwIcon, SquareIcon, XIcon } from "lucide-react";
import { Markdown } from "@/components/markdown";
import { openFileRef } from "@/components/open-file-ref";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { DropdownMenuItem, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { TurnError, type Turn } from "@/features/chat";
import { notify } from "@/features/notifications/notify";
import { formatCost, formatDuration } from "@/lib/format";
import type { Mate, ReactionKind } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { SentByMate, SentByMe } from "./attachment-views";
import { BUBBLE_TEXT } from "./bubble-style";
import { useEngineStore } from "@/stores/engine-store";
import { takeOpenedUnread, useMatesStore } from "@/stores/mates-store";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { useSessionStore } from "@/stores/session-store";
import { emojiOf, parsePrompt, REACTIONS, type ReplyTo } from "./message-markup";
import { replaceShortcodes } from "./emoji";
import { StepsChip } from "./steps-chip";
import { clockTime, dayLabel, isHeartbeat, isLong, threadRows, type Bubble, type Row } from "./thread-rows";

function metaParts(turn: Turn): string[] {
  const { meta } = turn;
  return [meta.model, meta.durationMs ? formatDuration(meta.durationMs) : null, meta.cost ? formatCost(meta.cost) : null].filter(
    (x): x is string => Boolean(x),
  );
}

function BarButton({ label, onClick, children }: { label: string; onClick(): void; children: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            aria-label={label}
            onClick={onClick}
            className="flex size-7 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

/** What else a bubble offers (evo-mates-messaging FR-7 - FR-9). */
interface MoreActions {
  edit?(): void;
  retry?(): void;
  hide(): void;
}

/** The hover bar (FR-13): reactions, Reply, Copy and, on a mate's reply, its details. */
function BubbleActions({
  bubble,
  reaction,
  onReact,
  onReply,
  details,
  more,
}: {
  bubble: Bubble;
  reaction?: ReactionKind;
  onReact(kind: ReactionKind | null): void;
  onReply(): void;
  details: string[];
  more: MoreActions;
}) {
  return (
    <div
      className={cn(
        "absolute -top-4 z-10 hidden items-center gap-0.5 rounded-full bg-popover p-0.5 shadow-md ring-1 ring-foreground/10 group-focus-within/bubble:flex group-hover/bubble:flex",
        bubble.side === "me" ? "right-2" : "left-2",
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
            "flex size-7 items-center justify-center rounded-full text-[13px] transition-transform motion-safe:hover:scale-125",
            reaction === r.kind && "bg-brand/15",
          )}
        >
          {r.emoji}
        </button>
      ))}
      <span className="mx-0.5 h-4 w-px bg-border" />
      <BarButton label="Reply" onClick={onReply}>
        <ReplyIcon className="size-4" />
      </BarButton>
      <BarButton label="Copy" onClick={() => void navigator.clipboard.writeText(bubble.text)}>
        <CopyIcon className="size-3.5" />
      </BarButton>
      {more.edit && (
        <BarButton label="Edit" onClick={more.edit}>
          <PencilIcon className="size-3.5" />
        </BarButton>
      )}
      {more.retry && (
        <BarButton label="Try again" onClick={more.retry}>
          <RotateCcwIcon className="size-3.5" />
        </BarButton>
      )}
      {details.length > 0 && (
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                aria-label={`Details: ${details.join(", ")}`}
                className="flex size-7 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
              />
            }
          >
            <InfoIcon className="size-3.5" />
          </TooltipTrigger>
          <TooltipContent>{details.join(" · ")}</TooltipContent>
        </Tooltip>
      )}
    </div>
  );
}

/** Text that folds past 16 lines or 1,200 characters (FR-14). */
function Folding({ long, children, me }: { long: boolean; children: React.ReactNode; me: boolean }) {
  const [open, setOpen] = React.useState(false);
  if (!long) return <>{children}</>;
  return (
    <>
      <div className={cn("relative", !open && "max-h-[320px] overflow-hidden")}>
        {children}
        {!open && (
          <div
            aria-hidden
            className={cn("pointer-events-none absolute inset-x-0 bottom-0 h-16 bg-linear-to-t", me ? "from-brand" : "from-muted", "to-transparent")}
          />
        )}
      </div>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={cn("mt-1 text-[12px] font-medium hover:underline", me ? "text-brand-foreground" : "text-brand")}
      >
        {open ? "Show less" : "Show more"}
      </button>
    </>
  );
}

function BubbleView({
  row,
  mate,
  reaction,
  onReact,
  onReply,
  onQuote,
  more,
  editing,
  edited,
  onSaveEdit,
  onCancelEdit,
}: {
  row: Extract<Row, { kind: "bubble" }>;
  mate: Mate;
  reaction?: ReactionKind;
  onReact(kind: ReactionKind | null): void;
  onReply(): void;
  onQuote(quote: ReplyTo): void;
  more: MoreActions;
  /** The person edits this bubble now (FR-7). */
  editing: boolean;
  edited: boolean;
  onSaveEdit(text: string): void;
  onCancelEdit(): void;
}) {
  const { bubble, first, last } = row;
  const me = bubble.side === "me";
  // Files the mate names open from its working folder (rich-replies FR-37).
  const root = mate.projectFolder ?? mate.folder;
  const openRef = React.useCallback((reference: string) => void openFileRef(reference, root, "mates"), [root]);
  const details = !me && last && !bubble.streaming ? metaParts(bubble.turn) : [];
  return (
    <div className={cn("flex w-full px-4", me ? "justify-end" : "justify-start", first ? "mt-2" : "mt-0.5")}>
      <div data-key={bubble.key} className={cn("group/bubble relative flex max-w-[78%] flex-col pt-1", me ? "items-end" : "items-start")}>
        {bubble.reply && (
          <button
            type="button"
            onClick={() => onQuote(bubble.reply!)}
            title="Go to the message"
            className={cn(
              "mb-0.5 flex max-w-full items-stretch gap-2 rounded-2xl bg-muted/50 py-1 pr-3 pl-1.5 text-left text-xs text-muted-foreground transition-colors hover:bg-muted",
              me ? "mr-2" : "ml-2",
            )}
          >
            <span aria-hidden className="w-0.5 shrink-0 rounded-full bg-brand/60" />
            <span className="min-w-0 truncate">
              <span className="font-medium text-foreground/80">{bubble.reply.from === "you" ? "You" : bubble.reply.from}</span> {bubble.reply.text}
            </span>
          </button>
        )}
        <div className={cn("flex max-w-full min-w-0 items-end gap-1.5", me && "flex-row-reverse")}>
          <ContextMenu>
            <ContextMenuTrigger
              render={
                <div
                  data-bubble
                  className={cn(
                    "relative rounded-[20px] px-3.5 py-2 text-[14px] leading-[20px]",
                    BUBBLE_TEXT,
                    me ? "bg-brand text-brand-foreground" : "bg-muted text-foreground",
                    last && (me ? "rounded-br-[6px]" : "rounded-bl-[6px]"),
                    // Replies are markdown: tighter blocks in a bubble.
                    "[&_p]:my-0 [&_p+p]:mt-2 [&_ul]:my-1 [&_ol]:my-1 [&_li]:my-0 [&_li]:leading-5 [&_li_p]:my-0 [&_ul]:pl-4 [&_ol]:pl-4",
                    me && "[&_a]:text-brand-foreground [&_a]:underline [&_code]:bg-white/15",
                  )}
                />
              }
            >
              {/* The person's pictures, files and pasted text (FR-81). */}
              <SentByMe images={bubble.images} files={bubble.files} pasted={bubble.pasted} />
              {editing ? (
                <EditField text={bubble.text} onSave={onSaveEdit} onCancel={onCancelEdit} />
              ) : (
              <Folding long={!bubble.streaming && isLong(bubble.text)} me={me}>
                <div data-find-text>
                  {me ? (
                    <span className="whitespace-pre-wrap break-words">{bubble.text}</span>
                  ) : (
                    <Markdown streaming={bubble.streaming} blocks onLocalLink={openRef}>
                      {bubble.text}
                    </Markdown>
                  )}
                </div>
              </Folding>
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
              {more.edit && (
                <DropdownMenuItem onClick={more.edit}>
                  <PencilIcon />
                  Edit
                </DropdownMenuItem>
              )}
              {more.retry && (
                <DropdownMenuItem onClick={more.retry}>
                  <RotateCcwIcon />
                  Try again
                </DropdownMenuItem>
              )}
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={more.hide}>
                <EyeOffIcon />
                Delete for me
              </DropdownMenuItem>
            </ContextMenuContent>
          </ContextMenu>
          {/* Time on hover, inside a run too (FR-13). */}
          <span className="mb-1 hidden shrink-0 text-[10.5px] text-muted-foreground tabular-nums group-hover/bubble:inline">{clockTime(bubble.at)}</span>
        </div>
        {edited && !editing && <span className="mt-0.5 px-2 text-[10.5px] text-muted-foreground">Edited</span>}
        {!editing && <BubbleActions bubble={bubble} reaction={reaction} onReact={onReact} onReply={onReply} details={details} more={more} />}
        <span className="sr-only">{me ? "You" : mate.name}</span>
      </div>
    </div>
  );
}

/** Editing the person's message in place (FR-7): Enter saves, Esc cancels. */
function EditField({ text, onSave, onCancel }: { text: string; onSave(text: string): void; onCancel(): void }) {
  const [value, setValue] = React.useState(text);
  const field = React.useRef<HTMLTextAreaElement>(null);
  React.useEffect(() => {
    const el = field.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);
  React.useLayoutEffect(() => {
    const el = field.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [value]);
  const save = () => (value.trim() && value.trim() !== text.trim() ? onSave(replaceShortcodes(value.trim())) : onCancel());
  return (
    <div className="flex min-w-60 flex-col gap-1.5">
      <textarea
        ref={field}
        value={value}
        aria-label="Edit your message"
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            onCancel();
          } else if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
            event.preventDefault();
            save();
          }
        }}
        className="min-h-6 w-full resize-none border-0 bg-transparent text-[14px] leading-5 text-brand-foreground shadow-none ring-0 outline-none focus:outline-none focus-visible:ring-0 focus-visible:outline-none"
      />
      <div className="flex justify-end gap-1">
        <button type="button" onClick={onCancel} className="flex items-center gap-1 rounded-full bg-white/15 px-2 py-0.5 text-xs hover:bg-white/25">
          <XIcon className="size-3" />
          Cancel
        </button>
        <button type="button" onClick={save} className="rounded-full bg-white px-2.5 py-0.5 text-xs font-medium text-brand hover:bg-white/90">
          Save
        </button>
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
      {step && <span className="px-1 text-xs text-brand">{step}</span>}
    </div>
  );
}

/** A line across the thread: "New", or a day. */
function Divider({ label, tone }: { label: string; tone: "new" | "day" }) {
  if (tone === "day")
    return (
      <div className="sticky top-1 z-[5] my-2 flex justify-center">
        <span className="rounded-full bg-background px-3 py-1 text-[11.5px] font-medium text-foreground shadow-[0_0_0_1px_var(--border),0_2px_8px_-4px_rgb(0_0_0/0.2)]">
          {label}
        </span>
      </div>
    );
  return (
    <div role="separator" aria-label="New messages" className="my-2 flex items-center gap-2 px-4">
      <span className="h-px flex-1 bg-brand/50" />
      <span className="text-[11px] font-semibold text-brand">{label}</span>
      <span className="h-px flex-1 bg-brand/50" />
    </div>
  );
}

/** The first mate bubble after the person's last message: where "New" goes (FR-12, A-2). */
export function firstNewKey(rows: Row[]): string | null {
  let lastMine = -1;
  rows.forEach((row, i) => {
    if (row.kind === "bubble" && row.bubble.side === "me") lastMine = i;
  });
  const next = rows.slice(lastMine + 1).find((row) => row.kind === "bubble" && row.bubble.side === "mate");
  return next?.kind === "bubble" ? next.bubble.key : null;
}

/** The bubble a reply quotes: the one whose text starts as the quote does (FR-16). */
export function quotedKey(rows: Row[], quote: ReplyTo): string | null {
  const flat = (t: string) => t.replace(/\s+/g, " ").trim().toLowerCase();
  const head = flat(quote.text.replace(/…$/, "")).slice(0, 80);
  if (!head) return null;
  const hit = [...rows].reverse().find((row) => row.kind === "bubble" && flat(row.bubble.text).startsWith(head));
  return hit?.kind === "bubble" ? hit.bubble.key : null;
}

/** The thread of `mate`'s conversation, shown when the engine store shows its chat. */
export function MateThread({
  mate,
  reactions,
  onReact,
  onReply,
  empty,
  waiting,
  scopeId,
}: {
  mate: Mate;
  reactions: Record<string, ReactionKind>;
  onReact(bubble: Bubble, kind: ReactionKind | null): void;
  onReply(reply: ReplyTo): void;
  empty: React.ReactNode;
  /** A question, approval or secret waits on the person: no typing dots (FR-10, QA B5). */
  waiting: boolean;
  /** The element find searches (FR-15). */
  scopeId: string;
}) {
  const turns = useEngineStore((s) => s.transcript.turns);
  const busy = useEngineStore((s) => s.transcript.busy);
  const queued = useEngineStore((s) => s.transcript.queued);
  const shellWorking = useSessionStore((s) => s.work[mate.chatId] === "working");
  // Waiting for room under the running limit (NFR-6): the status says Queued (FR-2).
  const roomQueued = useMatesStore((s) => s.queued.includes(mate.chatId));
  const hiddenKeys = useMatesUiStore((s) => s.hidden[mate.chatId]);
  const editedPrompts = useMatesUiStore((s) => s.edited[mate.chatId]);
  const rows = React.useMemo(() => {
    const all = threadRows(turns, busy, shellWorking && !busy, roomQueued);
    // Deleted for the person (FR-9).
    return hiddenKeys?.length ? all.filter((row) => !(row.kind === "bubble" && hiddenKeys.includes(row.bubble.key))) : all;
  }, [turns, busy, shellWorking, roomQueued, hiddenKeys]);
  const [editing, setEditing] = React.useState<string | null>(null);
  const idle = !busy && !shellWorking;
  const lastMine = [...rows].reverse().find((r) => r.kind === "bubble" && r.bubble.side === "me");
  const lastMate = [...rows].reverse().find((r) => r.kind === "bubble" && r.bubble.side === "mate");
  const lastTurn = turns.at(-1);
  // Up in an empty box edits the last message (FR-7, FR-12).
  React.useEffect(() => {
    const onEdit = (event: Event) => {
      if ((event as CustomEvent<string>).detail !== mate.chatId) return;
      if (idle && lastMine?.kind === "bubble" && lastMine.bubble.turn === lastTurn) setEditing(lastMine.bubble.key);
    };
    window.addEventListener("mates:edit-last", onEdit);
    return () => window.removeEventListener("mates:edit-last", onEdit);
  }, [idle, lastMine, lastTurn, mate.chatId]);
  const saveEdit = (bubble: Bubble, text: string) => {
    setEditing(null);
    const turn = bubble.turn;
    // The prompt keeps its quote, notes and files: only the person's words change.
    const at = turn.prompt.lastIndexOf(bubble.text);
    const prompt = at >= 0 ? turn.prompt.slice(0, at) + text + turn.prompt.slice(at + bubble.text.length) : text;
    useMatesUiStore.getState().markEdited(mate.chatId, text);
    void useEngineStore
      .getState()
      .editMessage(turn.id, prompt)
      .catch((error) => notify({ source: "mates", level: "warning", title: "Couldn't edit the message", body: String(error), system: false }));
  };
  const retry = (bubble: Bubble) =>
    void useEngineStore
      .getState()
      .editMessage(bubble.turn.id, bubble.turn.prompt)
      .catch((error) => notify({ source: "mates", level: "warning", title: "Couldn't try again", body: String(error), system: false }));
  const moreFor = (bubble: Bubble): MoreActions => ({
    edit:
      idle && bubble.side === "me" && lastMine?.kind === "bubble" && lastMine.bubble.key === bubble.key && bubble.turn === lastTurn
        ? () => setEditing(bubble.key)
        : undefined,
    retry:
      idle &&
      bubble.side === "mate" &&
      lastMate?.kind === "bubble" &&
      lastMate.bubble.key === bubble.key &&
      bubble.turn === lastTurn &&
      !isHeartbeat(bubble.turn.prompt)
        ? () => retry(bubble)
        : undefined,
    hide: () => useMatesUiStore.getState().hide(mate.chatId, bubble.key),
  });
  const scroller = React.useRef<HTMLDivElement>(null);
  const pinned = React.useRef(true);
  const [away, setAway] = React.useState(false);
  const [unseen, setUnseen] = React.useState(0);
  const bubbleCount = rows.filter((r) => r.kind === "bubble").length;
  const seenCount = React.useRef(bubbleCount);

  // Where "New" goes: decided once, as the conversation opens unread (FR-12, A-2).
  const [openedUnread] = React.useState(() => ({ current: takeOpenedUnread(mate.chatId) }));
  const [newKey, setNewKey] = React.useState<string | null>(null);
  const placed = React.useRef(false);
  React.useLayoutEffect(() => {
    if (placed.current || rows.length === 0) return;
    placed.current = true;
    const key = openedUnread.current ? firstNewKey(rows) : null;
    setNewKey(key);
    const el = scroller.current;
    if (!el) return;
    if (key) {
      // The line renders with the next frame.
      requestAnimationFrame(() => {
        (el.querySelector("[data-new-line]") as HTMLElement | null)?.scrollIntoView({ block: "start" });
        pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
        setAway(!pinned.current);
      });
    } else el.scrollTop = el.scrollHeight;
  }, [rows]);

  // Follow new messages while the person is at the bottom; else count them (FR-12).
  React.useLayoutEffect(() => {
    const el = scroller.current;
    if (!el || !placed.current) return;
    if (pinned.current) {
      el.scrollTop = el.scrollHeight;
      seenCount.current = bubbleCount;
      setUnseen(0);
    } else setUnseen(Math.max(0, bubbleCount - seenCount.current));
  }, [rows, bubbleCount]);

  const toBottom = () => {
    const el = scroller.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
    pinned.current = true;
    seenCount.current = bubbleCount;
    setUnseen(0);
  };

  const jumpTo = (quote: ReplyTo) => {
    const key = quotedKey(rows, quote);
    const el = key ? scroller.current?.querySelector(`[data-key="${CSS.escape(key)}"] [data-bubble]`) : null;
    if (!el) {
      notify({ source: "mates", title: "Not in this part of the conversation", system: false });
      return;
    }
    el.scrollIntoView({ block: "center", behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
    el.classList.remove("mates-flash");
    void (el as HTMLElement).offsetWidth;
    el.classList.add("mates-flash");
    window.setTimeout(() => el.classList.remove("mates-flash"), 1600);
  };

  const days = React.useMemo(() => {
    const out: { key: string; at: number | null; rows: Row[] }[] = [];
    for (const row of rows) {
      if (row.kind === "day") out.push({ key: row.key, at: row.at, rows: [] });
      else {
        if (!out.length) out.push({ key: "start", at: null, rows: [] });
        out[out.length - 1].rows.push(row);
      }
    }
    return out;
  }, [rows]);

  if (rows.length === 0) return <div className="flex h-full items-center justify-center">{empty}</div>;
  return (
    <div className="relative h-full">
      <div
        ref={scroller}
        id={scopeId}
        data-thread
        onScroll={(event) => {
          const el = event.currentTarget;
          pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
          setAway(el.scrollHeight - el.scrollTop - el.clientHeight > 240);
          if (pinned.current) {
            seenCount.current = bubbleCount;
            setUnseen(0);
          }
        }}
        className="h-full overflow-y-auto pt-2 pb-6"
      >
        <div className="mx-auto flex max-w-3xl flex-col">
          {/* One section per day, so each day's label sticks only over its own messages (FR-11). */}
          {days.map((day) => (
            <section key={day.key} className="flex flex-col">
              {day.at !== null && <Divider label={dayLabel(day.at)} tone="day" />}
              {day.rows.map((row) => {
            switch (row.kind) {
              case "day":
                return null;
              case "time":
                return (
                  <div key={row.key} className="mt-3 mb-0.5 text-center text-[11px] text-muted-foreground tabular-nums">
                    {clockTime(row.at)}
                  </div>
                );
              case "bubble":
                return (
                  <React.Fragment key={row.bubble.key}>
                    {row.bubble.key === newKey && (
                      <div data-new-line>
                        <Divider label="New" tone="new" />
                      </div>
                    )}
                    <BubbleView
                      row={row}
                      mate={mate}
                      reaction={reactions[row.bubble.key]}
                      onReact={(kind) => onReact(row.bubble, kind)}
                      onReply={() => onReply({ from: row.bubble.side === "me" ? "you" : mate.name, text: row.bubble.text })}
                      onQuote={jumpTo}
                      more={moreFor(row.bubble)}
                      editing={editing === row.bubble.key}
                      edited={row.bubble.side === "me" && Boolean(editedPrompts?.includes(row.bubble.text))}
                      onSaveEdit={(text) => saveEdit(row.bubble, text)}
                      onCancelEdit={() => setEditing(null)}
                    />
                  </React.Fragment>
                );
              case "files":
                // What the turn sent and showed, on the mate's side (FR-82).
                return (
                  <div key={row.key} className="mt-1 flex px-4">
                    <SentByMate files={row.files} pictures={row.pictures} />
                  </div>
                );
              case "typing":
                return waiting ? null : <Typing key={row.key} step={row.step} />;
              case "steps":
                return (
                  <div key={row.key} className="mt-1 px-4">
                    <StepsChip text={row.text} turn={row.turn} />
                  </div>
                );
              case "stopped":
                return (
                  <div key={row.key} className="mt-1 flex items-center gap-1.5 px-5 text-[11.5px] text-muted-foreground">
                    <SquareIcon className="size-2.5 fill-current" />
                    Stopped
                  </div>
                );
              case "learned":
                return <LearnedLine key={row.key} chat={mate.chatId} turnEntry={row.turn.entryId} className="mt-1 px-4" />;
              case "memory":
                return (
                  <div key={row.key} className="mt-1 px-4">
                    <span className="inline-flex items-center rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">Saved to memory</span>
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
                  <div key={row.key} className="mt-0.5 px-5 text-right text-[11px] text-muted-foreground" aria-live="polite">
                    {row.word}
                  </div>
                );
              default:
                return null;
            }
              })}
            </section>
          ))}
          {/* Messages the mate has not read yet (FR-49): faded, with when it will. */}
          {(queued ?? []).map((message) => (
            <div key={message.entryId} className="mt-1 flex flex-col items-end px-4">
              <div className="max-w-[72%] rounded-[20px] rounded-br-[6px] bg-brand/50 px-3.5 py-2 text-[14px] leading-5 whitespace-pre-wrap text-brand-foreground">
                {parsePrompt(message.text).text}
              </div>
              <span className="mt-0.5 flex items-center gap-1.5 px-1 text-[11px] text-muted-foreground">
                {message.deliverAs === "steer" ? "Queued · read at the next step" : "Queued · after this turn"}
                <button
                  type="button"
                  onClick={() => void useEngineStore.getState().cancelQueued(message.entryId)}
                  className="rounded-full px-1.5 hover:bg-muted hover:text-foreground"
                >
                  Unsend
                </button>
              </span>
            </div>
          ))}
        </div>
      </div>
      {/* Jump to latest (FR-12). */}
      {(away || unseen > 0) && !pinned.current && (
        <button
          type="button"
          onClick={toBottom}
          className="absolute bottom-3 left-1/2 z-10 flex -translate-x-1/2 items-center gap-1.5 rounded-full bg-popover px-3 py-1.5 text-[12px] font-medium text-foreground shadow-lg ring-1 ring-foreground/10 animate-in fade-in slide-in-from-bottom-1 duration-150 hover:bg-accent"
        >
          <ArrowDownIcon className="size-3.5" />
          Jump to latest
          {unseen > 0 && <span className="rounded-full bg-brand px-1.5 text-[10.5px] font-semibold text-brand-foreground">{unseen} new</span>}
        </button>
      )}
    </div>
  );
}

export type { Bubble };
