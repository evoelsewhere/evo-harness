// A channel (docs/features/evo-mates/spec.md, FR-14 – FR-19; ux.md 2, 3):
// the bar (name, its mates' faces, who answers, Stop, settings), the thread
// of the person's and the mates' messages with the channel's own lines, the
// mates at work, and the message box with an `@` list of the channel's mates.

import * as React from "react";
import { ArrowRightIcon, ArrowUpIcon, ChevronRightIcon, PlusIcon, SquareIcon, XIcon } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as pickFiles } from "@tauri-apps/plugin-dialog";
import type { PromptAttachment } from "@/components/ai-elements/prompt-input";
import { attachmentsFromPaths, attachmentsFromTransfer, isLongPaste, releaseAttachment, textAttachment } from "@/features/chat";
import { Markdown } from "@/components/markdown";
import { openFileRef } from "@/components/open-file-ref";
import { MateFace } from "@/components/mate-face/mate-face";
import { log } from "@/lib/db";
import { openChatWindow } from "@/lib/windows";
import type { Channel, Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useChannelStore } from "@/stores/channel-store";
import { useEngineStore } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";
import { boldMentions, channelRows, handoffSummary, handoffVerb, insertMention, mentionAt, mentionParts, type ChannelRow } from "./channel-rows";
import { useMembers } from "./channel-header";
import { FindBar } from "./find-bar";
import { MatePresenceFace } from "./presence-face";
import { clockTime, dayLabel } from "./thread-rows";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { EmojiButton, useComposerAssist } from "./composer-assist";
import { replaceShortcodes } from "./emoji";
import type { SlashCommand } from "./slash-commands";
import { BUBBLE_TEXT } from "./bubble-style";
import { channelMessage, splitMessage } from "./attachments";
import { NamedFiles, SentByMe } from "./attachment-views";
import { RetroCard } from "./retro-card";
import { TypingLine } from "./typing-line";
import { ForwardDialog, type Forwarding } from "./forward-dialog";
import { LineBar, LinkCard, ReactionChips, useChannelReactions } from "./line-tools";
import { composePrompt, excerpt, parsePrompt, type ReplyTo } from "./message-markup";
import { jumpToMessage, PinsBar } from "./pins-bar";
import { channelsApi, matesApi, type ChannelReaction, type ReactionKind } from "@/lib/mates";
import { useMatesPrefs } from "@/stores/mates-prefs-store";

const EMPTY: never[] = [];

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
interface LineActions {
  reactions: Map<string, ChannelReaction[]>;
  react(message: string, kind: ReactionKind | null): void;
  reply(to: ReplyTo): void;
  pinned: Set<string>;
  pin(key: string, text: string): void;
  forward(from: string, text: string): void;
  nameOf(id: string): string;
}
const LineContext = React.createContext<LineActions | null>(null);

/** A line that mentions the person (`@you`, FR-18). */
const MENTIONS_YOU = /(^|[^\p{L}\p{N}@])@you(?![\p{L}\p{N}_])/iu;

function RowView({ row, faceOf, names, folder }: { row: ChannelRow; faceOf(id: string): Mate | undefined; names: string[]; folder: string | null }) {
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
    case "retro":
      return <RetroCard card={row.card} />;
    case "bubble":
      return <BubbleLine row={row} faceOf={faceOf} names={names} folder={folder} />;
  }
}

function BubbleLine({ row, faceOf, names, folder }: { row: Extract<ChannelRow, { kind: "bubble" }>; faceOf(id: string): Mate | undefined; names: string[]; folder: string | null }) {
  const actions = React.useContext(LineContext);
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
                      <React.Fragment key={i}>{part.text}</React.Fragment>
                    ),
                  )}
                </span>
              ) : (
                <Markdown
                  blocks
                  onLocalLink={(reference) => {
                    // A seat works in the channel's folder, else in its mate's (rich-replies FR-37).
                    const author = faceOf(row.author);
                    void openFileRef(reference, folder ?? author?.projectFolder ?? author?.folder ?? null, "mates");
                  }}
                >
                  {boldMentions(row.text, [...names, "you"])}
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
              />
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

/** The mates of the channel whose seats are in `state` now. */
function useSeats(channel: Channel, members: Mate[], state: "working" | "waiting"): Mate[] {
  const work = useSessionStore((s) => s.work);
  return members.filter((mate) => {
    const seat = channel.members.find((m) => m.mateId === mate.id)?.seatChatId;
    return seat ? work[seat] === state : false;
  });
}

function seatOf(channel: Channel, mate: string): string | null {
  return channel.members.find((m) => m.mateId === mate)?.seatChatId ?? null;
}

function Composer({ channel, members, reply, onReplyDone }: { channel: Channel; members: Mate[]; reply: ReplyTo | null; onReplyDone(): void }) {
  // The box's text is a draft kept per channel (evo-mates-messaging FR-11).
  const [text, setTextState] = React.useState(() => useMatesUiStore.getState().drafts[channel.chatId] ?? "");
  const setText = React.useCallback(
    (next: string) => {
      setTextState(next);
      useMatesUiStore.getState().setDraft(channel.chatId, next);
    },
    [channel.chatId],
  );
  React.useEffect(() => setTextState(useMatesUiStore.getState().drafts[channel.chatId] ?? ""), [channel.chatId]);
  const [files, setFiles] = React.useState<PromptAttachment[]>([]);
  const [mention, setMention] = React.useState<{ start: number; query: string } | null>(null);
  const [pick, setPick] = React.useState(0);
  const busy = useEngineStore((s) => s.activeId === channel.chatId && s.transcript.busy);
  const field = React.useRef<HTMLTextAreaElement>(null);

  const matches = mention
    ? [{ id: "everyone", name: "everyone" }, ...members.map((m) => ({ id: m.id, name: m.name }))].filter((m) =>
        m.name.toLowerCase().startsWith(mention.query.toLowerCase()),
      )
    : [];

  React.useLayoutEffect(() => {
    const el = field.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(el.scrollHeight, 6 * 20 + 16)}px`;
  }, [text]);

  const choose = (name: string) => {
    const el = field.current;
    if (!el || !mention) return;
    const next = insertMention(text, mention.start, el.selectionStart, name);
    setText(next.text);
    setMention(null);
    requestAnimationFrame(() => el.setSelectionRange(next.caret, next.caret));
  };

  const addFiles = (added: PromptAttachment[]) => setFiles((current) => [...current, ...added]);

  // Slash commands in a channel (FR-13): /stop, /handoff @mate, /clear-quote.
  const run = (command: SlashCommand, rest: string) => {
    if (command.id === "clear-quote") onReplyDone();
    if (command.id === "stop") void useEngineStore.getState().abort();
    else if (command.id === "handoff") {
      const next = rest.startsWith("@") ? `${rest} ` : `@${rest}`;
      setText(next);
      requestAnimationFrame(() => field.current?.focus());
    }
  };
  const assist = useComposerAssist({ where: "channel", text, setText, field, onCommand: run });

  // Files dropped on the window attach to the message (FR-83).
  React.useEffect(() => {
    let stop: (() => void) | undefined;
    let live = true;
    let webview: ReturnType<typeof getCurrentWebview>;
    try {
      webview = getCurrentWebview();
    } catch {
      return;
    }
    void webview
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop" && event.payload.paths.length > 0) {
          void attachmentsFromPaths(event.payload.paths).then(addFiles);
          field.current?.focus();
        }
      })
      .then((unlisten) => {
        if (live) stop = unlisten;
        else unlisten();
      })
      .catch(() => undefined);
    return () => {
      live = false;
      stop?.();
    };
  }, []);

  const send = async () => {
    if (!text.trim() && files.length === 0) return;
    // Members read the message as text: pictures are saved and every attachment goes by path (FR-83).
    // A reply carries its quote (evo-mates-messaging FR-5).
    const quoted = composePrompt(replaceShortcodes(text), reply, []);
    onReplyDone();
    const message = await channelMessage(quoted, files, async (blob, name) =>
      invoke<string>("fs_save_paste", {
        name,
        data: await new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(String(reader.result).replace(/^data:[^,]*,/, ""));
          reader.onerror = () => reject(reader.error ?? new Error("could not read the picture"));
          reader.readAsDataURL(blob);
        }),
      }),
    ).catch((error) => {
      log("warn", "mates", `could not attach: ${String(error)}`);
      return null;
    });
    if (!message) return;
    files.forEach(releaseAttachment);
    setFiles([]);
    setText("");
    // While the mates answer, a message for only a mate at work steers it;
    // any other waits its turn (FR-56).
    if (busy) {
      if (!(await useEngineStore.getState().steerChannel(message))) useChannelStore.getState().enqueue(channel.chatId, message);
      return;
    }
    try {
      await useEngineStore.getState().sendPrompt(message);
    } catch (error) {
      log("warn", "mates", `could not send to the channel: ${String(error)}`);
    }
  };

  // The next waiting message goes out once the channel is free.
  React.useEffect(() => {
    if (busy) return;
    const next = useChannelStore.getState().dequeue(channel.chatId);
    if (next) void useEngineStore.getState().sendPrompt(next).catch(() => undefined);
  }, [busy, channel.chatId]);

  return (
    <div className="relative mx-auto w-full max-w-3xl px-4 pb-4">
      {!mention && assist.menuView}
      {reply && (
        <div className="mb-1.5 flex items-center gap-2 rounded-2xl bg-muted/70 px-3 py-1.5 text-xs text-muted-foreground">
          <span className="min-w-0 flex-1 truncate">
            Replying to <span className="font-medium text-foreground">{reply.from === "you" ? "yourself" : reply.from}</span>: {reply.text}
          </span>
          <button type="button" aria-label="Cancel reply" onClick={onReplyDone} className="rounded-full p-0.5 hover:bg-accent">
            <XIcon className="size-3.5" />
          </button>
        </div>
      )}
      {mention && matches.length > 0 && (
        <div className="absolute bottom-full left-4 mb-1 w-72 overflow-hidden rounded-2xl bg-popover p-1 shadow-lg ring-1 ring-border" role="listbox" aria-label="Mention a mate">
          {matches.map((m, i) => {
            const mate = members.find((x) => x.id === m.id);
            return (
              <button
                key={m.id}
                type="button"
                role="option"
                aria-selected={i === pick}
                onMouseDown={(event) => {
                  event.preventDefault();
                  choose(m.name);
                }}
                className={cn("flex w-full items-center gap-2.5 rounded-xl px-2 py-1.5 text-left text-[13px]", i === pick && "bg-accent")}
              >
                {mate ? (
                  <MatePresenceFace mate={mate} size={26} ring="var(--popover)" />
                ) : (
                  <span className="flex size-[26px] items-center justify-center rounded-full bg-muted text-muted-foreground">@</span>
                )}
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium text-foreground">{m.name}</span>
                  <span className="block truncate text-xs text-muted-foreground">{mate ? mate.role || "Mate" : "Every mate in the channel"}</span>
                </span>
              </button>
            );
          })}
        </div>
      )}
      {files.length > 0 && (
        <div className="mb-1.5 flex flex-wrap gap-1.5">
          {files.map((file) => (
            <span key={file.id} className="flex items-center gap-1 rounded-full bg-muted px-2.5 py-1 text-xs text-foreground">
              {file.preview && <img src={file.preview} alt="" className="-ml-1.5 size-6 rounded-full object-cover" />}
              <span className="max-w-40 truncate">{file.kind === "text" ? "Pasted text" : file.name}</span>
              <button
                type="button"
                aria-label={`Remove ${file.name}`}
                onClick={() => {
                  releaseAttachment(file);
                  setFiles((current) => current.filter((f) => f.id !== file.id));
                }}
                className="rounded-full hover:text-destructive"
              >
                <XIcon className="size-3" />
              </button>
            </span>
          ))}
        </div>
      )}
      <div className="flex items-end gap-2">
      <button
        type="button"
        aria-label="Attach files"
        onClick={() =>
          void pickFiles({ multiple: true, title: `Send to #${channel.name}` }).then((picked) => {
            const paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
            if (paths.length) void attachmentsFromPaths(paths).then(addFiles);
          })
        }
        className="mb-0.5 flex size-8 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground transition-colors hover:text-foreground"
      >
        <PlusIcon className="size-4" />
      </button>
      <div className="flex min-w-0 flex-1 items-end rounded-[20px] border border-border bg-background pr-1 pl-3.5 transition-colors focus-within:border-brand/70 focus-within:ring-2 focus-within:ring-brand/15">
        <textarea
          onPaste={(event) => {
            const data = event.clipboardData;
            if (data.files.length > 0 || [...data.items].some((item) => item.kind === "file")) {
              event.preventDefault();
              void attachmentsFromTransfer(data).then(addFiles);
              return;
            }
            const pasted = data.getData("text/plain");
            if (pasted && isLongPaste(pasted)) {
              event.preventDefault();
              addFiles([textAttachment(pasted)]);
            }
          }}
          ref={field}
          rows={1}
          value={text}
          placeholder={busy ? "Mates are answering… a message now goes next" : `Message #${channel.name} · @ to mention`}
          onChange={(event) => {
            setText(event.target.value);
            setMention(mentionAt(event.target.value, event.target.selectionStart));
            assist.update(event.target.value, event.target.selectionStart);
            setPick(0);
          }}
          onKeyDown={(event) => {
            if (mention && matches.length > 0) {
              if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                setPick((p) => (p + (event.key === "ArrowDown" ? 1 : matches.length - 1)) % matches.length);
                return;
              }
              if (event.key === "Enter" || event.key === "Tab") {
                event.preventDefault();
                choose(matches[pick]?.name ?? matches[0].name);
                return;
              }
              if (event.key === "Escape") {
                event.preventDefault();
                setMention(null);
                return;
              }
            }
            if (assist.onKeyDown(event)) return;
            if (event.key === "Escape" && reply) {
              event.preventDefault();
              onReplyDone();
              return;
            }
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              void send();
            }
          }}
          className="max-h-[136px] min-h-9 min-w-0 flex-1 resize-none appearance-none border-0 bg-transparent py-2 text-[14px] leading-5 shadow-none outline-none placeholder:text-muted-foreground focus-visible:outline-none"
        />
        <EmojiButton onPick={assist.insert} />
        {busy && !text.trim() && files.length === 0 ? (
          <button
            type="button"
            aria-label="Stop"
            title="Stop every mate in this channel"
            onClick={() => void useEngineStore.getState().abort()}
            className="my-1 flex size-7 shrink-0 items-center justify-center rounded-full bg-foreground text-background"
          >
            <SquareIcon className="size-3 fill-current" />
          </button>
        ) : (
          <button
            type="button"
            aria-label="Send"
            disabled={!text.trim() && files.length === 0}
            onClick={() => void send()}
            className={cn(
              "my-1 flex size-7 shrink-0 items-center justify-center rounded-full bg-brand text-brand-foreground transition-all",
              text.trim() || files.length ? "scale-100 opacity-100" : "pointer-events-none scale-75 opacity-0",
            )}
          >
            <ArrowUpIcon className="size-4" />
          </button>
        )}
      </div>
      </div>
    </div>
  );
}

export function ChannelView({ channel }: { channel: Channel }) {
  const members = useMembers(channel);
  const entries = useChannelStore((s) => s.entries[channel.chatId] ?? EMPTY);
  const queued = useChannelStore((s) => s.queued[channel.chatId] ?? EMPTY);
  const shown = useEngineStore((s) => s.activeId === channel.chatId);
  const working = useSeats(channel, members, "working");
  const waiting = useSeats(channel, members, "waiting");
  const nameOf = React.useCallback((id: string) => members.find((m) => m.id === id)?.name ?? id, [members]);
  const rows = React.useMemo(() => channelRows(entries, nameOf), [entries, nameOf]);
  const bottom = React.useRef<HTMLDivElement>(null);
  const reactions = useChannelReactions(channel.id);
  const [reply, setReply] = React.useState<ReplyTo | null>(null);
  const [forwarding, setForwarding] = React.useState<Forwarding | null>(null);
  const pins = useMatesPrefs((s) => s.pins[channel.chatId]);
  // Opening the channel reads its mentions of the person (FR-18).
  React.useEffect(() => {
    void matesApi.mentionsRead(channel.chatId).catch(() => undefined);
  }, [channel.chatId]);
  const actions = React.useMemo<LineActions>(
    () => ({
      reactions,
      react: (message, kind) => void channelsApi.react(channel.id, message, kind).catch(() => undefined),
      reply: setReply,
      pinned: new Set((pins ?? []).map((p) => p.key)),
      pin: (key, text) => useMatesPrefs.getState().togglePin(channel.chatId, { key, excerpt: excerpt(text, 120) }),
      forward: (from, text) => setForwarding({ from, text, chat: channel.chatId }),
      nameOf: (id) => members.find((m) => m.id === id)?.name ?? id,
    }),
    [reactions, pins, channel.id, channel.chatId, members],
  );

  React.useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [rows.length, working.length, waiting.length, queued.length]);

  // One section per day so each day's label sticks over its own lines (FR-11).
  const days = React.useMemo(() => {
    const out: { key: string; at: number | null; rows: ChannelRow[] }[] = [];
    for (const row of rows) {
      if (row.kind === "day") out.push({ key: row.key, at: row.at, rows: [] });
      else {
        if (!out.length) out.push({ key: "start", at: null, rows: [] });
        out[out.length - 1].rows.push(row);
      }
    }
    return out;
  }, [rows]);

  if (!shown) return null;
  return (
    <LineContext.Provider value={actions}>
    <div className="flex h-full min-h-0 flex-col">
      <PinsBar chat={channel.chatId} />
      <ForwardDialog item={forwarding} onClose={() => setForwarding(null)} />
      <div className="relative min-h-0 flex-1">
      <FindBar scopeId={`channel-${channel.chatId}`} version={rows.length} />
      <div id={`channel-${channel.chatId}`} className="h-full overflow-y-auto pt-2 pb-3">
        {rows.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
            <div className="flex -space-x-2">
              {members.slice(0, 5).map((mate) => (
                <span key={mate.id} className="rounded-full ring-4 ring-background">
                  <MateFace avatar={mate.avatar} size={44} live plate={false} label={mate.name} />
                </span>
              ))}
            </div>
            <h2 className="text-lg font-semibold text-foreground">#{channel.name}</h2>
            <p className="max-w-sm text-sm text-muted-foreground">
              {members.map((m) => m.name).join(", ")} {members.length === 1 ? "is" : "are"} here. Say what you need, or @mention a
              mate.
            </p>
          </div>
        ) : (
          days.map((day) => (
            <section key={day.key} className="flex flex-col">
              {day.at !== null && (
                <div className="sticky top-1 z-[5] my-2 flex justify-center">
                  <span className="rounded-full bg-background px-3 py-1 text-[11.5px] font-medium text-foreground shadow-[0_0_0_1px_var(--border),0_2px_8px_-4px_rgb(0_0_0/0.2)]">
                    {dayLabel(day.at)}
                  </span>
                </div>
              )}
              {day.rows.map((row) => (
                <RowView
                  key={row.key}
                  row={row}
                  names={members.map((m) => m.name)}
                  faceOf={(id) => members.find((m) => m.id === id)}
                  folder={channel.folder ?? null}
                />
              ))}
            </section>
          ))
        )}
        {/* Who is typing (evo-mates-messaging FR-1); none while a mate waits on the person. */}
        <TypingLine channel={channel} working={waiting.length ? [] : working} />
        {queued.map((text, index) => (
          <div key={`queued:${index}`} className="mt-2 flex flex-col items-end gap-0.5 px-4 opacity-60">
            <div className="max-w-[75%] rounded-[20px] rounded-br-[6px] bg-brand px-3.5 py-2 text-[14px] leading-[20px] whitespace-pre-wrap text-brand-foreground">
              {text}
            </div>
            <button
              type="button"
              onClick={() => useChannelStore.getState().unqueue(channel.chatId, index)}
              className="px-1 text-[11px] text-muted-foreground hover:text-foreground"
            >
              Queued · Unsend
            </button>
          </div>
        ))}
        {waiting.map((mate) => (
          <div key={mate.id} className="flex justify-center px-6 py-1.5">
            <span className="flex items-center gap-2 rounded-full bg-amber-500/10 py-1 pr-1 pl-2 text-[12px] text-amber-700 dark:text-amber-300">
              <MateFace avatar={mate.avatar} size={18} mood="waiting" plate={false} label={mate.name} />
              {mate.name} needs you
              <button
                type="button"
                onClick={() => {
                  const seat = seatOf(channel, mate.id);
                  if (seat) void openChatWindow(seat, `${mate.name} in #${channel.name}`);
                }}
                className="rounded-full bg-amber-500 px-2.5 py-0.5 text-[11px] font-medium text-white hover:bg-amber-600"
              >
                Answer
              </button>
            </span>
          </div>
        ))}
        <div ref={bottom} />
      </div>
      </div>
      <Composer channel={channel} members={members} reply={reply} onReplyDone={() => setReply(null)} />
    </div>
    </LineContext.Provider>
  );
}
