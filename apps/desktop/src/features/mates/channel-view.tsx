// A channel (docs/features/evo-mates/spec.md, FR-14 – FR-19; ux.md 2, 3):
// the bar (name, its mates' faces, who answers, Stop, settings), the thread
// of the person's and the mates' messages with the channel's own lines, the
// mates at work, and the message box with an `@` list of the channel's mates.

import * as React from "react";
import { ArrowUpIcon, ClipboardCheckIcon, DownloadIcon, HashIcon, PlusIcon, SettingsIcon, SquareIcon, Trash2Icon, XIcon } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as pickFiles } from "@tauri-apps/plugin-dialog";
import type { PromptAttachment } from "@/components/ai-elements/prompt-input";
import { attachmentsFromPaths, attachmentsFromTransfer, isLongPaste, releaseAttachment, textAttachment } from "@/features/chat";
import { Markdown } from "@/components/markdown";
import { openFileRef } from "@/components/open-file-ref";
import { MateFace } from "@/components/mate-face/mate-face";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { log } from "@/lib/db";
import { openChatWindow } from "@/lib/windows";
import { channelsApi, type Channel, type Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useChannelStore } from "@/stores/channel-store";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";
import {
  boldMentions,
  channelRows,
  handoffLine,
  insertMention,
  mentionAt,
  mentionParts,
  type ChannelRow,
} from "./channel-rows";
import { ChannelSettings, RULES } from "./channel-settings";
import { ConfirmDelete } from "./confirm";
import { channelMessage, splitMessage } from "./attachments";
import { NamedFiles, SentByMe } from "./attachment-views";
import { RetroCard } from "./retro-card";
import { exportTeam } from "./team-import";

const EMPTY: never[] = [];

function useMembers(channel: Channel): Mate[] {
  const mates = useMatesStore((s) => s.mates);
  return React.useMemo(
    () => channel.members.map((m) => mates.find((mate) => mate.id === m.mateId)).filter((m): m is Mate => Boolean(m)),
    [channel.members, mates],
  );
}

function IconButton({ label, onClick, children }: { label: string; onClick(): void; children: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button variant="ghost" size="icon-sm" aria-label={label} onClick={onClick} className="no-drag rounded-full text-muted-foreground" />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function ChannelBar({ channel }: { channel: Channel }) {
  const members = useMembers(channel);
  const removeChannel = useMatesStore((s) => s.removeChannel);
  const [settings, setSettings] = React.useState(false);
  const [confirm, setConfirm] = React.useState(false);
  const rule = RULES.find((r) => r.id === channel.replyRule);
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2 pl-3">
      <button
        type="button"
        onClick={() => setSettings(true)}
        aria-label={`${channel.name}: settings`}
        className="no-drag flex h-8 min-w-0 items-center gap-1.5 rounded-full bg-card pr-2.5 pl-2 shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent"
      >
        <HashIcon className="size-4 shrink-0 text-muted-foreground" />
        <span className="truncate text-[13px] font-semibold text-foreground">{channel.name}</span>
        <span className="flex -space-x-1.5 pl-1">
          {members.slice(0, 5).map((mate) => (
            <span key={mate.id} className="rounded-full ring-2 ring-card">
              <MateFace avatar={mate.avatar} size={18} plate={false} label={mate.name} />
            </span>
          ))}
        </span>
      </button>
      {rule && (
        <span className="hidden rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground sm:inline" title={rule.hint}>
          {rule.label}
        </span>
      )}
      <div className="flex-1" />
      <IconButton label="Export channel" onClick={() => void exportTeam({ channel: channel.id }, channel.name)}>
        <DownloadIcon />
      </IconButton>
      <IconButton
        label="Run retro"
        onClick={() => void channelsApi.runRetro(channel.chatId).catch((error) => log("warn", "mates", `the retro did not run: ${String(error)}`))}
      >
        <ClipboardCheckIcon />
      </IconButton>
      <IconButton label="Channel settings" onClick={() => setSettings(true)}>
        <SettingsIcon />
      </IconButton>
      <IconButton label="Delete channel" onClick={() => setConfirm(true)}>
        <Trash2Icon />
      </IconButton>
      <ChannelSettings open={settings} onOpenChange={setSettings} channel={channel} />
      <ConfirmDelete
        open={confirm}
        onOpenChange={setConfirm}
        title={`Delete ${channel.name}?`}
        description="Its messages go. The mates stay, with their own conversations and memory."
        onConfirm={() => void removeChannel(channel.id)}
      />
    </div>
  );
}

function Line({ children, tone = "muted" }: { children: React.ReactNode; tone?: "muted" | "warn" }) {
  return (
    <div className="flex justify-center px-6 py-1.5">
      <span
        className={cn(
          "max-w-[85%] rounded-full px-3 py-1 text-center text-[11.5px]",
          tone === "warn" ? "bg-destructive/10 text-destructive" : "bg-muted/60 text-muted-foreground",
        )}
      >
        {children}
      </span>
    </div>
  );
}

function RowView({ row, faceOf, names, folder }: { row: ChannelRow; faceOf(id: string): Mate | undefined; names: string[]; folder: string | null }) {
  switch (row.kind) {
    case "time":
      return <div className="py-2 text-center text-[11px] font-medium text-muted-foreground">{row.text}</div>;
    case "route":
      return <Line>{row.text}</Line>;
    case "handoff":
      return <Line>{handoffLine(row)}</Line>;
    case "notice":
      return <Line tone={row.stop ? "warn" : "muted"}>{row.text}</Line>;
    case "retro":
      return <RetroCard card={row.card} />;
    case "bubble": {
      const mate = row.me ? undefined : faceOf(row.author);
      // The person's pictures, files and pasted text show as chips (FR-83).
      const said = row.me ? splitMessage(row.text) : null;
      const root = folder ?? mate?.projectFolder ?? mate?.folder ?? null;
      return (
        <div className={cn("flex items-end gap-2 px-4", row.me ? "justify-end" : "justify-start", row.first ? "mt-2" : "mt-0.5")}>
          {!row.me && (
            <span className="w-7 shrink-0">
              {row.last && mate && <MateFace avatar={mate.avatar} size={28} plate={false} label={mate.name} />}
            </span>
          )}
          <div className={cn("flex max-w-[75%] min-w-0 flex-col", row.me ? "items-end" : "items-start")}>
            {!row.me && row.first && <span className="mb-0.5 ml-2 text-[11px] font-medium text-muted-foreground">{row.name}</span>}
            <div
              className={cn(
                "min-w-0 rounded-[20px] px-3.5 py-2 text-[14px] leading-[20px]",
                row.me ? "bg-brand text-brand-foreground" : "bg-muted text-foreground",
                row.last && (row.me ? "rounded-br-[6px]" : "rounded-bl-[6px]"),
                row.failed && "opacity-70",
                "[&_p]:my-0 [&_p+p]:mt-2 [&_ul]:my-1 [&_ol]:my-1 [&_li]:my-0 [&_ul]:pl-4 [&_ol]:pl-4",
              )}
            >
              {said && <SentByMe files={said.files} pasted={said.pasted} />}
              {row.me ? (
                <span className="whitespace-pre-wrap break-words">
                  {mentionParts(said?.text ?? row.text, names).map((part, i) =>
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
                  {boldMentions(row.text, names)}
                </Markdown>
              )}
            </div>
            {/* The files a mate's line names that exist, under it (FR-83). */}
            {!row.me && <NamedFiles text={row.text} root={root} />}
            {row.steers && <span className="mt-0.5 mr-2 text-[11px] text-muted-foreground">Seen by {row.steers} while working</span>}
          </div>
        </div>
      );
    }
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

function Composer({ channel, members }: { channel: Channel; members: Mate[] }) {
  const [text, setText] = React.useState("");
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
    const message = await channelMessage(text, files, async (blob, name) =>
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
      {mention && matches.length > 0 && (
        <div className="absolute bottom-full left-4 mb-1 w-56 overflow-hidden rounded-2xl bg-popover p-1 shadow-lg ring-1 ring-border" role="listbox">
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
                className={cn("flex w-full items-center gap-2 rounded-xl px-2 py-1.5 text-left text-[13px]", i === pick && "bg-accent")}
              >
                {mate ? (
                  <MateFace avatar={mate.avatar} size={20} plate={false} label={mate.name} />
                ) : (
                  <span className="flex size-5 items-center justify-center text-muted-foreground">@</span>
                )}
                {m.name}
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
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              void send();
            }
          }}
          className="max-h-[136px] min-h-9 min-w-0 flex-1 resize-none appearance-none border-0 bg-transparent py-2 text-[14px] leading-5 shadow-none outline-none placeholder:text-muted-foreground focus-visible:outline-none"
        />
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

  React.useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [rows.length, working.length, waiting.length, queued.length]);

  if (!shown) return null;
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto pt-2 pb-3">
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
          rows.map((row) => (
            <RowView
              key={row.key}
              row={row}
              names={members.map((m) => m.name)}
              faceOf={(id) => members.find((m) => m.id === id)}
              folder={channel.folder ?? null}
            />
          ))
        )}
        {working.map((mate) => (
          <div key={mate.id} className="mt-2 flex items-end gap-2 px-4" aria-live="polite">
            <MateFace avatar={mate.avatar} size={28} mood="working" live plate={false} label={mate.name} />
            <div className="flex flex-col items-start gap-0.5">
              <div className="flex items-center gap-1 rounded-[20px] rounded-bl-[6px] bg-muted px-3.5 py-3">
                {[0, 150, 300].map((delay) => (
                  <span
                    key={delay}
                    className="size-2 rounded-full bg-muted-foreground/70 motion-safe:animate-bounce"
                    style={{ animationDelay: `${delay}ms`, animationDuration: "1s" }}
                  />
                ))}
              </div>
              <span className="flex items-center gap-1.5 px-1 text-xs text-muted-foreground">
                {mate.name} is working…
                <button
                  type="button"
                  onClick={() => void channelsApi.stopMember(channel.id, mate.id).catch(() => undefined)}
                  className="rounded-full px-1.5 text-[11px] text-muted-foreground hover:bg-muted hover:text-foreground"
                  aria-label={`Stop ${mate.name}`}
                >
                  Stop
                </button>
              </span>
            </div>
          </div>
        ))}
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
              Next · Remove
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
      <Composer channel={channel} members={members} />
    </div>
  );
}
