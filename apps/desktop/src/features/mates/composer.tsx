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
import { attachmentsFromPaths, buildPrompt, releaseAttachment } from "@/features/chat";
import { log } from "@/lib/db";
import { matesApi, type Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";
import { composePrompt, type ReplyTo } from "./message-markup";

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

export function MateComposer({ mate, reply, onReplyDone }: { mate: Mate; reply: ReplyTo | null; onReplyDone(): void }) {
  const [text, setText] = React.useState("");
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

  // A new reply puts the caret in the box.
  React.useEffect(() => {
    if (reply) field.current?.focus();
  }, [reply]);

  const send = async (next = false) => {
    const typed = text.trim();
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

  if (mate.paused) {
    return (
      <div className="mx-auto w-full max-w-3xl px-4 pb-4">
        <div className="flex items-center justify-center gap-3 rounded-[20px] bg-muted px-4 py-2.5 text-sm text-muted-foreground">
          <span>{mate.name} is paused. It does no work until you resume it.</span>
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
    <div className="mx-auto w-full max-w-3xl px-4 pb-4">
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
              <span className="max-w-40 truncate">{file.name}</span>
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
            onChange={(event) => setText(event.target.value)}
            onKeyDown={(event) => {
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
