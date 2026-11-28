// A channel (docs/features/evo-mates/spec.md, FR-14 – FR-19; ux.md 2, 3):
// the bar (name, its mates' faces, who answers, Stop, settings), the thread
// of the person's and the mates' messages with the channel's own lines, the
// mates at work, and the message box with an `@` list of the channel's mates.

import * as React from "react";
import { ArrowUpIcon, BellIcon, BellOffIcon, CheckCircle2Icon, Columns3Icon, MessagesSquareIcon, PlusIcon, Share2Icon, SquareIcon, XIcon } from "lucide-react";
import { useThreadsStore } from "@/stores/threads-store";
import { ShareDialog, type Sharing } from "./share-dialog";
import { copyMessageLink } from "./app-links";
import type { Place } from "./links-logic";
import { useMatesStore } from "@/stores/mates-store";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as pickFiles } from "@tauri-apps/plugin-dialog";
import type { PromptAttachment } from "@/components/ai-elements/prompt-input";
import { attachmentsFromPaths, attachmentsFromTransfer, isLongPaste, releaseAttachment, textAttachment } from "@/features/chat";
import { MateFace } from "@/components/mate-face/mate-face";
import { log } from "@/lib/db";
import { notify } from "@/features/notifications/notify";
import { openChatWindow } from "@/lib/windows";
import type { Channel, ChannelEntry, Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useChannelStore } from "@/stores/channel-store";
import { useEngineStore } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";
import { channelRows, insertMention, mentionAt, openJob, type ChannelRow } from "./channel-rows";
import { useMembers } from "./channel-header";
import { FindBar } from "./find-bar";
import { MatePresenceFace } from "./presence-face";
import { dayLabel } from "./thread-rows";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { EmojiButton, useComposerAssist } from "./composer-assist";
import { replaceShortcodes } from "./emoji";
import type { SlashCommand } from "./slash-commands";
import { channelMessage } from "./attachments";
import { LineContext, RowView, type LineActions } from "./channel-lines";
import { TypingLine } from "./typing-line";
import { ForwardDialog, type Forwarding } from "./forward-dialog";
import { useChannelReactions } from "./line-tools";
import { composePrompt, excerpt, parsePrompt, threadMark, withThread, type ReplyTo } from "./message-markup";
import { PinsBar } from "./pins-bar";
import { useBoardStore } from "@/stores/board-store";
import { channelsApi, matesApi } from "@/lib/mates";
import { useMatesPrefs } from "@/stores/mates-prefs-store";

const EMPTY: never[] = [];

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

function Composer({
  channel,
  members,
  reply,
  onReplyDone,
  thread = null,
  canStop = true,
  acceptDrops = true,
}: {
  channel: Channel;
  members: Mate[];
  reply: ReplyTo | null;
  onReplyDone(): void;
  /** The thread this box replies in (evo-mates-threads-board FR-1). */
  thread?: string | null;
  /** Stop shows only where the running job is (FR-1). */
  canStop?: boolean;
  /** Files dropped on the window come to this box: the thread's while it is open (review A #7). */
  acceptDrops?: boolean;
}) {
  // The box's text is a draft kept per channel, and per thread (evo-mates-messaging FR-11).
  const draftKey = thread ? `${channel.chatId}#${thread}` : channel.chatId;
  const [text, setTextState] = React.useState(() => useMatesUiStore.getState().drafts[draftKey] ?? "");
  const setText = React.useCallback(
    (next: string) => {
      setTextState(next);
      useMatesUiStore.getState().setDraft(draftKey, next);
    },
    [draftKey],
  );
  React.useEffect(() => setTextState(useMatesUiStore.getState().drafts[draftKey] ?? ""), [draftKey]);
  const [files, setFiles] = React.useState<PromptAttachment[]>([]);
  const [mention, setMention] = React.useState<{ start: number; query: string } | null>(null);
  const [pick, setPick] = React.useState(0);
  // **Also send to channel** under a thread's box (evo-mates-threads-board FR-22).
  const [also, setAlso] = React.useState(false);
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
    // /stop stops the job only where it runs (review A #8).
    if (command.id === "stop") {
      if (canStop) void useEngineStore.getState().abort();
      else notify({ source: "mates", title: "The running job is in a thread", body: "Stop it from that thread.", system: false });
    }
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
    if (!acceptDrops) return;
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
  }, [acceptDrops]);

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
    // A thread's box marks its message for the coordinator (FR-1).
    const sent = thread ? withThread(thread, message, also) : message;
    setAlso(false);
    // While the mates answer, a message for only a mate at work steers it;
    // any other waits its turn (FR-56; one job at a time, D1).
    if (busy) {
      if (!(await useEngineStore.getState().steerChannel(sent))) useChannelStore.getState().enqueue(channel.chatId, sent);
      return;
    }
    try {
      await useEngineStore.getState().sendPrompt(sent);
    } catch (error) {
      log("warn", "mates", `could not send to the channel: ${String(error)}`);
    }
  };

  // The next waiting message goes out once the channel is free.
  React.useEffect(() => {
    // Only the channel's own box sends the queue on.
    if (busy || thread) return;
    const next = useChannelStore.getState().dequeue(channel.chatId);
    if (next) void useEngineStore.getState().sendPrompt(next).catch(() => undefined);
  }, [busy, channel.chatId, thread]);

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
      {thread && (
        <label className="mb-1.5 flex w-fit cursor-pointer items-center gap-1.5 px-1 text-[12px] text-muted-foreground select-none">
          <input type="checkbox" checked={also} onChange={(e) => setAlso(e.target.checked)} className="size-3.5 accent-[var(--brand)]" />
          Also send to #{channel.name}
        </label>
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
          placeholder={
            thread
              ? busy
                ? "Mates are answering… your reply goes next"
                : "Reply in thread…"
              : busy
                ? "Mates are answering… a message now goes next"
                : `Message #${channel.name} · @ to mention`
          }
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
        {busy && canStop && !text.trim() && files.length === 0 ? (
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

/** A channel's thread beside its feed (evo-mates-threads-board FR-1 – FR-6). */
function ThreadPanel({
  channel,
  members,
  root,
  entries,
  actions,
  working,
  open,
  onClose,
}: {
  channel: Channel;
  members: Mate[];
  root: string;
  entries: ChannelEntry[];
  actions: LineActions;
  working: Mate[];
  open: { thread: string | null } | null;
  onClose(): void;
}) {
  const nameOf = actions.nameOf;
  const rows = React.useMemo(() => channelRows(entries, nameOf, Date.now(), root), [entries, nameOf, root]);
  const [reply, setReply] = React.useState<ReplyTo | null>(null);
  const queued = useChannelStore((s) => s.queued[channel.chatId] ?? EMPTY);
  const bottom = React.useRef<HTMLDivElement>(null);
  const here = open?.thread === root;
  // Lines in a thread reply in the same thread: no thread action (D3).
  const inThread = React.useMemo<LineActions>(() => ({ ...actions, openThread: undefined, reply: setReply }), [actions]);
  React.useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [rows.length, here, working.length]);
  React.useEffect(() => {
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) onClose();
    };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [onClose]);
  const names = members.map((m) => m.name);
  const replies = rows.filter((r) => r.kind === "bubble").length - 1;
  // The thread's card, or Track on board (evo-mates-threads-board FR-15).
  const board = useBoardStore((s) => s.views[channel.id]);
  const card = board?.cards.find((c) => c.threadRoot === root);
  const column = card ? board?.board?.columns.find((c) => c.id === card.columnId)?.name : undefined;
  const parent = entries.find((e) => e.kind === "message" && e.id === root);
  // Follow, resolve, and reading it (FR-20, FR-21).
  const state = useThreadsStore((s) => s.byChat[channel.chatId]?.[root]);
  React.useEffect(() => {
    void useThreadsStore.getState().read(channel.chatId, root);
  }, [channel.chatId, root, rows.length, state?.unread]);
  const track = () => {
    const words = parent && parent.kind === "message" ? threadMark(parent.text).text : "";
    void useBoardStore
      .getState()
      .create(channel.id, { title: excerpt(words.replace(/<[^>]+>/g, " "), 80) || "From a thread", threadRoot: root, links: [{ kind: "message", target: root, label: "" }] })
      .catch(() => undefined);
  };
  return (
    <aside
      aria-label="Thread"
      className="absolute inset-y-0 right-0 z-20 flex w-full max-w-[420px] min-w-0 flex-col border-l border-border bg-background shadow-xl min-[1200px]:static min-[1200px]:w-[380px] min-[1200px]:max-w-none min-[1200px]:shrink-0 min-[1200px]:shadow-none"
    >
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
        <MessagesSquareIcon className="size-4 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13.5px] font-semibold text-foreground">Thread</div>
          <div className="truncate text-[11.5px] text-muted-foreground">
            #{channel.name} · {replies <= 0 ? "No replies yet" : `${replies} ${replies === 1 ? "reply" : "replies"}`}
          </div>
        </div>
        {card ? (
          <button
            type="button"
            onClick={() => useBoardStore.getState().showCard(card.key)}
            title={card.title}
            className="flex h-7 max-w-44 min-w-0 shrink items-center gap-1 rounded-full bg-muted px-2.5 text-[12px] whitespace-nowrap hover:bg-accent"
          >
            <Columns3Icon className="size-3.5 shrink-0 text-muted-foreground" />
            <span className="shrink-0 font-mono font-medium text-foreground">{card.key}</span>
            {column && <span className="min-w-0 truncate text-muted-foreground">· {column}</span>}
          </button>
        ) : (
          board?.board?.enabled && (
            <button type="button" onClick={track} className="flex h-7 shrink-0 items-center gap-1 rounded-full px-2.5 text-[12px] text-muted-foreground hover:bg-muted hover:text-foreground">
              <Columns3Icon className="size-3.5" />
              Track on board
            </button>
          )
        )}
        <button
          type="button"
          aria-label={state?.following ? "Unfollow thread" : "Follow thread"}
          title={state?.following ? "Following: replies show under Threads. Click to unfollow." : "Follow: replies show under Threads"}
          onClick={() => void useThreadsStore.getState().set(channel.chatId, root, { following: !state?.following })}
          className={cn("rounded-full p-1.5 hover:bg-muted", state?.following ? "text-brand" : "text-muted-foreground hover:text-foreground")}
        >
          {state?.following ? <BellIcon className="size-4" /> : <BellOffIcon className="size-4" />}
        </button>
        <button
          type="button"
          aria-label={state?.resolved ? "Unresolve thread" : "Resolve thread"}
          title={state?.resolved ? "Unresolve" : "Resolve: fold it in the channel"}
          onClick={() => void useThreadsStore.getState().set(channel.chatId, root, { resolved: !state?.resolved })}
          className={cn("rounded-full p-1.5 hover:bg-muted", state?.resolved ? "text-emerald-600 dark:text-emerald-400" : "text-muted-foreground hover:text-foreground")}
        >
          <CheckCircle2Icon className="size-4" />
        </button>
        <button type="button" aria-label="Close thread" onClick={onClose} className="rounded-full p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground">
          <XIcon className="size-4" />
        </button>
      </header>
      <LineContext.Provider value={inThread}>
        <div className="min-h-0 flex-1 overflow-y-auto pt-2 pb-3">
          {rows.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">This thread's first line is no longer here.</p>
          ) : (
            rows.map((row, index) =>
              row.kind === "day" ? null : (
                <React.Fragment key={row.key}>
                  <RowView row={row} names={names} faceOf={(id) => members.find((m) => m.id === id)} folder={channel.folder ?? null} />
                  {index === rows.findIndex((r) => r.kind === "bubble") && replies > 0 && (
                    <div className="my-2 flex items-center gap-2 px-4 text-[11px] text-muted-foreground">
                      <span className="h-px flex-1 bg-border" />
                      {replies} {replies === 1 ? "reply" : "replies"}
                      <span className="h-px flex-1 bg-border" />
                    </div>
                  )}
                </React.Fragment>
              ),
            )
          )}
          {here && <TypingLine channel={channel} working={working} />}
          {queued.map((text, index) => {
            const mark = threadMark(text);
            if (mark.root !== root) return null;
            return <QueuedBubble key={`queued:${index}`} text={mark.text} onUnsend={() => useChannelStore.getState().unqueue(channel.chatId, index)} />;
          })}
          <div ref={bottom} />
        </div>
      </LineContext.Provider>
      {state?.resolved && (
        <div className="mx-4 mb-1 flex items-center gap-2 rounded-xl bg-emerald-500/10 px-3 py-1.5 text-[12px] text-emerald-800 dark:text-emerald-200">
          <CheckCircle2Icon className="size-3.5" />
          Resolved. A new reply opens it again.
          <button type="button" onClick={() => void useThreadsStore.getState().set(channel.chatId, root, { resolved: false })} className="ml-auto font-medium hover:underline">
            Unresolve
          </button>
        </div>
      )}
      <Composer channel={channel} members={members} reply={reply} onReplyDone={() => setReply(null)} thread={root} canStop={here} />
    </aside>
  );
}

/** A message waiting for the channel (FR-56): Queued · Unsend. */
function QueuedBubble({ text, onUnsend }: { text: string; onUnsend(): void }) {
  return (
    <div className="mt-2 flex flex-col items-end gap-0.5 px-4 opacity-60">
      <div className="max-w-[75%] rounded-[20px] rounded-br-[6px] bg-brand px-3.5 py-2 text-[14px] leading-[20px] whitespace-pre-wrap text-brand-foreground">
        {text}
      </div>
      <button type="button" onClick={onUnsend} className="px-1 text-[11px] text-muted-foreground hover:text-foreground">
        Queued · Unsend
      </button>
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
  // The open thread, by its parent's id (evo-mates-threads-board FR-1).
  const [thread, setThread] = React.useState<string | null>(null);
  React.useEffect(() => setThread(null), [channel.chatId]);
  // A card's "Open its thread" lands here (FR-15).
  const asked = useBoardStore((s) => s.openThread);
  React.useEffect(() => {
    if (asked && asked.chat === channel.chatId) {
      setThread(asked.root);
      useBoardStore.getState().showThread(null);
    }
  }, [asked, channel.chatId]);
  // Threads' state, read again as the feed changes (FR-20, FR-21).
  const threadRows = useThreadsStore((s) => s.byChat[channel.chatId]);
  React.useEffect(() => {
    const timer = window.setTimeout(() => void useThreadsStore.getState().loadChat(channel.chatId), 400);
    return () => window.clearTimeout(timer);
  }, [channel.chatId, entries.length]);
  const resolved = React.useMemo(() => new Set(Object.values(threadRows ?? {}).filter((t) => t.resolved).map((t) => t.root)), [threadRows]);
  // Select mode for sharing (FR-16).
  const [selecting, setSelecting] = React.useState<Set<string> | null>(null);
  const [sharing, setSharing] = React.useState<Sharing | null>(null);
  React.useEffect(() => setSelecting(null), [channel.chatId]);
  const allMates = useMatesStore((s) => s.mates);
  const allChannels = useMatesStore((s) => s.channels);
  const places = React.useMemo<Place[]>(
    () => [
      ...allChannels.filter((c) => !c.hidden).map((c) => ({ word: `#${c.name}`, path: `channel/${c.id}` })),
      ...allMates.filter((m) => !m.hidden).map((m) => ({ word: `@${m.name}`, path: `mate/${m.id}` })),
    ],
    [allChannels, allMates],
  );
  const placeOfChat = React.useCallback(
    (chat: string) => {
      const c = allChannels.find((x) => x.chatId === chat);
      if (c) return `#${c.name}`;
      return allMates.find((m) => m.chatId === chat)?.name ?? null;
    },
    [allChannels, allMates],
  );
  const shareSelected = () => {
    if (!selecting?.size) return;
    const lines = entries
      .filter((e): e is Extract<ChannelEntry, { kind: "message" }> => e.kind === "message" && selecting.has(e.id))
      .map((e) => ({ id: e.id, author: e.author === "person" ? "You" : e.name || nameOf(e.author), text: parsePrompt(threadMark(e.text).text).text }));
    setSharing({ chat: channel.chatId, place: `#${channel.name}`, lines });
    setSelecting(null);
  };
  const selectToEnd = () => {
    if (!selecting?.size) return;
    const feed = rows.filter((r) => r.kind === "bubble").map((r) => r.key);
    const first = feed.findIndex((k) => selecting.has(k));
    if (first >= 0) setSelecting(new Set(feed.slice(first)));
  };
  // A board line from the engine (a card made or moved) reads the board again (FR-12, FR-13).
  const boardLines = React.useMemo(() => entries.filter((e) => e.kind === "board").length, [entries]);
  React.useEffect(() => {
    if (boardLines) void useBoardStore.getState().load(channel.id);
  }, [boardLines, channel.id]);
  const busy = useEngineStore((s) => s.activeId === channel.chatId && s.transcript.busy);
  const open = React.useMemo(() => (busy ? openJob(entries) : null), [busy, entries]);
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
      openThread: setThread,
      share: (line) => setSelecting(new Set([line])),
      selecting,
      toggleSelect: (line) =>
        setSelecting((current) => {
          const next = new Set(current ?? []);
          if (next.has(line)) next.delete(line);
          else next.add(line);
          return next.size ? next : null;
        }),
      copyLink: (line) => copyMessageLink(channel.chatId, line),
      resolved,
      places,
      placeOfChat,
    }),
    [reactions, pins, channel.id, channel.chatId, members, selecting, resolved, places, placeOfChat],
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
    <div className="relative flex h-full min-h-0">
    <LineContext.Provider value={actions}>
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col">
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
        {/* Who is typing (evo-mates-messaging FR-1); none while a mate waits on the person.
            A thread's job types in its panel (evo-mates-threads-board FR-1). */}
        <TypingLine channel={channel} working={waiting.length || open?.thread ? [] : working} />
        {queued.map((text, index) =>
          threadMark(text).root ? null : (
            <QueuedBubble key={`queued:${index}`} text={text} onUnsend={() => useChannelStore.getState().unqueue(channel.chatId, index)} />
          ),
        )}
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
      {selecting ? (
        <div className="mx-auto flex w-full max-w-3xl flex-wrap items-center gap-2 px-4 pb-4">
          <span className="text-[13px] font-medium text-foreground">
            {selecting.size} selected
          </span>
          <span className="text-[12px] text-muted-foreground">Click messages to add or remove them.</span>
          <span className="flex-1" />
          <button type="button" onClick={selectToEnd} className="h-8 rounded-full px-3 text-[12.5px] text-muted-foreground hover:bg-muted hover:text-foreground">
            Select to the end
          </button>
          <button type="button" onClick={() => setSelecting(null)} className="h-8 rounded-full px-3 text-[12.5px] text-muted-foreground hover:bg-muted hover:text-foreground">
            Cancel
          </button>
          <button type="button" onClick={shareSelected} className="flex h-8 items-center gap-1.5 rounded-full bg-brand px-3.5 text-[12.5px] font-medium text-brand-foreground">
            <Share2Icon className="size-3.5" />
            Share…
          </button>
        </div>
      ) : (
        <Composer
          channel={channel}
          members={members}
          reply={reply}
          onReplyDone={() => setReply(null)}
          // Stop and /stop only where the job runs; drops go to an open thread (review A #7, #8).
          canStop={!open || open.thread === null}
          acceptDrops={!thread}
        />
      )}
      <ShareDialog item={sharing} onClose={() => setSharing(null)} />
    </div>
    </LineContext.Provider>
    {thread && (
      <ThreadPanel
        // A thread's own box: no quote, files or mention list carried from another (review A #6).
        key={thread}
        channel={channel}
        members={members}
        root={thread}
        entries={entries}
        actions={actions}
        working={waiting.length ? [] : working}
        open={open}
        onClose={() => setThread(null)}
      />
    )}
    </div>
  );
}
