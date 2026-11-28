// The message box of a mate's conversation (docs/features/evo-mates/spec.md,
// FR-61, FR-62; ux.md 3): a rounded pill with an attach button, a field
// that grows to six lines and a round send button in the accent colour (a
// stop button while the mate works). Enter sends, Shift+Enter starts a new
// line. A reply shows its quote above the box; Esc drops it. Files and
// pictures go through Harness's prompt building.

import * as React from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowUpIcon, ClockIcon, PlusIcon, SquareIcon, XIcon } from "lucide-react";
import type { PromptAttachment } from "@/components/ai-elements/prompt-input";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  attachmentsFromPaths,
  attachmentsFromTransfer,
  buildPrompt,
  composerDraft,
  isLongPaste,
  releaseAttachment,
  textAttachment,
} from "@/features/chat";
import { db, log } from "@/lib/db";
import { matesApi, type Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";
import { composePrompt, type ReplyTo } from "./message-markup";
import { notify } from "@/features/notifications/notify";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { EmojiButton, useComposerAssist } from "./composer-assist";
import { replaceShortcodes } from "./emoji";
import { parseCheckin, type SlashCommand } from "./slash-commands";

const MAX_LINES = 6;

/** Whether the mate's chat works now: by its engine's events or the shell's work list. */
export function useMateWorking(chat: string): boolean {
  const busy = useEngineStore((s) => s.activeId === chat && s.transcript.busy);
  const work = useSessionStore((s) => s.work[chat]);
  return busy || work === "working";
}

function working(chat: string): boolean {
  return useSessionStore.getState().work[chat] === "working";
}
const LINE_PX = 20;

/** The paused line (FR-45), naming the daily limit when that is why (FR-80). */
export function pausedLine(name: string, overLimit: number | null): string {
  return overLimit !== null
    ? `Paused: ${name} reached today's limit of $${overLimit.toFixed(2)}. Resume lets it go on today.`
    : `${name} is paused. It does no work until you resume it.`;
}

/** The daily limit, when a paused mate's cost today reached it. */
function useOverLimit(mate: Mate): number | null {
  const [over, setOver] = React.useState<number | null>(null);
  React.useEffect(() => {
    if (!mate.paused) {
      setOver(null);
      return;
    }
    let gone = false;
    void Promise.all([db.settings(), matesApi.today()])
      .then(([all, today]) => {
        const limit = all["mates.dailyLimitUsd"];
        const spent = today.team.find((t) => t.mateId === mate.id)?.costUsd ?? 0;
        if (!gone) setOver(typeof limit === "number" && limit > 0 && spent >= limit ? limit : null);
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [mate.id, mate.paused]);
  return over;
}

export function MateComposer({ mate, reply, onReplyDone }: { mate: Mate; reply: ReplyTo | null; onReplyDone(): void }) {
  // The box's text is a draft kept per conversation (evo-mates-messaging FR-11).
  const [text, setTextState] = React.useState(() => useMatesUiStore.getState().drafts[mate.chatId] ?? "");
  const setText = React.useCallback(
    (next: string) => {
      setTextState(next);
      useMatesUiStore.getState().setDraft(mate.chatId, next);
    },
    [mate.chatId],
  );
  React.useEffect(() => setTextState(useMatesUiStore.getState().drafts[mate.chatId] ?? ""), [mate.chatId]);
  // An edit that could not run comes back to the box (review 1a #10).
  React.useEffect(() => {
    const back = (event: Event) => {
      if ((event as CustomEvent<string>).detail === mate.chatId) setTextState(useMatesUiStore.getState().drafts[mate.chatId] ?? "");
    };
    window.addEventListener("mates:draft", back);
    return () => window.removeEventListener("mates:draft", back);
  }, [mate.chatId]);
  const [files, setFiles] = React.useState<PromptAttachment[]>([]);
  // Working by the engine store or by the shell: a run the harness retries
  // after a failed attempt reports no new start, but the shell sees it work.
  const busy = useMateWorking(mate.chatId);
  const status = useEngineStore((s) => s.status);
  const field = React.useRef<HTMLTextAreaElement>(null);

  // Grow with the text up to six lines, then scroll.
  React.useLayoutEffect(() => {
    const el = field.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(el.scrollHeight, MAX_LINES * LINE_PX + 16)}px`;
  }, [text]);

  // A suggestion's Set it up fills the box (learning-layers FR-14).
  React.useEffect(() => {
    const fill = (next: string) => {
      setText(next);
      field.current?.focus();
    };
    composerDraft.listeners.add(fill);
    return () => void composerDraft.listeners.delete(fill);
  }, []);

  // Pictures and files from the clipboard; long text becomes a chip (FR-81).
  const onPaste = (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const data = event.clipboardData;
    if (data.files.length > 0 || [...data.items].some((item) => item.kind === "file")) {
      event.preventDefault();
      void attachmentsFromTransfer(data)
        .then((added) => setFiles((current) => [...current, ...added]))
        .catch((error) => log("warn", "mates", `could not attach: ${String(error)}`));
      return;
    }
    const pasted = data.getData("text/plain");
    if (pasted && isLongPaste(pasted)) {
      event.preventDefault();
      setFiles((current) => [...current, textAttachment(pasted)]);
    }
  };

  // Files dropped on the window attach to the message (FR-81).
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
          void attachmentsFromPaths(event.payload.paths).then((added) => setFiles((current) => [...current, ...added]));
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

  // A new reply puts the caret in the box.
  React.useEffect(() => {
    if (reply) field.current?.focus();
  }, [reply]);

  // Slash commands (FR-13).
  const run = (command: SlashCommand, rest: string) => {
    const store = useMatesStore.getState();
    switch (command.id) {
      case "remind": {
        // While it works, the request joins the run (review 1a #7).
        const engine = useEngineStore.getState();
        const ask = `Please remind me: ${rest}`;
        void (engine.transcript.busy || working(mate.chatId) ? engine.queueMessage(ask, [], "followUp") : engine.sendPrompt(ask)).catch(() => undefined);
        break;
      }
      case "checkin": {
        const parsed = parseCheckin(rest);
        if (!parsed) {
          notify({ source: "mates", level: "warning", title: "Say when", body: "For example: /checkin every weekday 9:00 check the inbox", system: false });
          setText(`/checkin ${rest}`);
          return;
        }
        void matesApi
          .addCheckin(mate.id, parsed.text, parsed.repeat)
          .then(() => notify({ source: "mates", title: "Check-in added", body: parsed.text, system: false }))
          .catch((error) => log("warn", "mates", `could not add a check-in: ${String(error)}`));
        break;
      }
      case "pause":
      case "resume":
        void store.update(mate.id, { paused: command.id === "pause" });
        break;
      case "stop":
        void useEngineStore.getState().abort();
        break;
      case "memory":
        store.openPanel("memory");
        break;
      case "clear-quote":
        onReplyDone();
        break;
      default:
        break;
    }
  };
  const assist = useComposerAssist({
    where: "mate",
    text,
    setText,
    field,
    onCommand: run,
    // ↑ in an empty box edits the last message (FR-7, FR-12).
    onEditLast: () => window.dispatchEvent(new CustomEvent("mates:edit-last", { detail: mate.chatId })),
  });

  const send = async (next = false) => {
    const typed = replaceShortcodes(text.trim());
    if (!typed && files.length === 0) return;
    const engine = useEngineStore.getState();
    const prompt = await buildPrompt(typed, files);
    // Reactions since the mate's last turn travel with this message (FR-60).
    const reactions = await matesApi.takeReactions(mate.chatId).catch(() => []);
    const full = composePrompt(prompt.text, reply, reactions);
    setText("");
    setFiles([]);
    onReplyDone();
    files.forEach(releaseAttachment);
    try {
      // While it works (a retry included), the message joins the run instead
      // of starting one the engine would refuse ("a run is already active").
      // Enter: read at the mate's next step ("Seen while working"); Send as
      // next (Alt+Enter): after its turn (FR-49).
      if (engine.transcript.busy || working(mate.chatId)) await engine.queueMessage(full, prompt.images, next ? "followUp" : "steer");
      else await engine.sendPrompt(full, prompt.images);
    } catch (error) {
      log("warn", "mates", `could not send: ${String(error)}`);
    }
  };

  const attach = async () => {
    const picked = await open({ multiple: true, title: `Send to ${mate.name}` });
    const paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    if (paths.length) {
      const added = await attachmentsFromPaths(paths);
      setFiles((current) => [...current, ...added]);
    }
  };

  // A chat with no engine yet starts one on its first message.
  const ready = status !== "starting";
  const hasText = text.trim().length > 0 || files.length > 0;

  const overLimit = useOverLimit(mate);
  if (mate.paused) {
    return (
      <div className="mx-auto w-full max-w-3xl px-4 pb-4">
        <div className="flex items-center justify-center gap-3 rounded-[20px] bg-muted px-4 py-2.5 text-sm text-muted-foreground">
          <span>{pausedLine(mate.name, overLimit)}</span>
          <button
            type="button"
            onClick={() => void useMatesStore.getState().update(mate.id, { paused: false })}
            className="rounded-full bg-brand px-3 py-1 text-xs font-medium text-brand-foreground"
          >
            Resume
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="relative mx-auto w-full max-w-3xl px-4 pb-4">
      {assist.menuView}
      {reply && (
        <div className="mb-1.5 flex items-center gap-2 rounded-2xl bg-muted/70 px-3 py-1.5 text-xs text-muted-foreground">
          <span className="min-w-0 flex-1 truncate">
            Replying to <span className="font-medium text-foreground">{reply.from === "you" ? "yourself" : reply.from}</span>:{" "}
            {reply.text}
          </span>
          <button type="button" aria-label="Cancel reply" onClick={onReplyDone} className="rounded-full p-0.5 hover:bg-accent">
            <XIcon className="size-3.5" />
          </button>
        </div>
      )}
      {files.length > 0 && (
        <div className="mb-1.5 flex flex-wrap gap-1.5">
          {files.map((file) => (
            <span key={file.id} className="flex items-center gap-1 rounded-full bg-muted px-2.5 py-1 text-xs text-foreground">
              {file.preview && <img src={file.preview} alt="" className="-ml-1.5 size-6 rounded-full object-cover" />}
              <span className="max-w-40 truncate">
                {file.kind === "text" ? `Pasted text · ${file.text?.split(/\r?\n/).length ?? 0} lines` : file.name}
              </span>
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
          onClick={() => void attach()}
          className="mb-0.5 flex size-8 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground transition-colors hover:text-foreground"
        >
          <PlusIcon className="size-4" />
        </button>
        <div className="flex min-w-0 flex-1 items-end rounded-[20px] border border-border bg-background pr-1 pl-3.5 transition-colors focus-within:border-brand/70 focus-within:ring-2 focus-within:ring-brand/15">
          <textarea
            ref={field}
            rows={1}
            value={text}
            disabled={!ready}
            placeholder={ready ? `Message ${mate.name}` : "Starting…"}
            onChange={(event) => {
              setText(event.target.value);
              assist.update(event.target.value, event.target.selectionStart);
            }}
            onPaste={onPaste}
            onKeyDown={(event) => {
              if (assist.onKeyDown(event)) return;
              if (event.key === "Escape" && reply) {
                event.preventDefault();
                onReplyDone();
              } else if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
                event.preventDefault();
                void send(event.altKey && busy);
              }
            }}
            // The pill shows focus; the field itself draws no ring or border.
            className="max-h-[136px] min-h-9 min-w-0 flex-1 resize-none appearance-none border-0 bg-transparent py-2 text-[14px] leading-5 shadow-none ring-0 outline-none placeholder:text-muted-foreground focus:outline-none focus-visible:shadow-none focus-visible:ring-0 focus-visible:outline-none"
          />
          <EmojiButton onPick={assist.insert} />
          {busy && hasText && (
            <button
              type="button"
              aria-label="Send as next"
              title="Send as next (Alt+Enter): after this turn"
              onClick={() => void send(true)}
              className="my-1 mr-1 flex h-7 shrink-0 items-center gap-1 rounded-full px-2 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
            >
              <ClockIcon className="size-3.5" />
              Next
            </button>
          )}
          {busy && !hasText ? (
            <button
              type="button"
              aria-label="Stop"
              onClick={() => void useEngineStore.getState().abort()}
              className="my-1 flex size-7 shrink-0 items-center justify-center rounded-full bg-foreground text-background"
            >
              <SquareIcon className="size-3 fill-current" />
            </button>
          ) : (
            <button
              type="button"
              aria-label="Send"
              disabled={!hasText}
              onClick={() => void send()}
              className={cn(
                "my-1 flex size-7 shrink-0 items-center justify-center rounded-full bg-brand text-brand-foreground transition-all",
                hasText ? "scale-100 opacity-100" : "pointer-events-none scale-75 opacity-0",
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
