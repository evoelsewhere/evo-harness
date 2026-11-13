import { AppWindowIcon, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { StatusDot } from "@/components/status-dot";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { UNTITLED, useSessionStore } from "@/stores/session-store";
import type { PaneIndex } from "@/stores/split-rules";
import { useSplitStore } from "@/stores/split-store";
import { startDrag } from "./drag";

/** A chat's title and whether it works or waits, for a pane header or the switcher. */
function useChatLine(chat: string) {
  const title = useSessionStore((s) => s.sessions.find((x) => x.id === chat)?.title) || UNTITLED;
  const work = useSessionStore((s) => s.work[chat]);
  const busy = useEngineStore((s) => Boolean(s.runtimes[chat]?.transcript.busy || s.busyElsewhere[chat])) || work === "working";
  const waiting = useEngineStore((s) => Boolean(s.runtimes[chat]?.transcript.permissions?.length)) || work === "waiting";
  return { title, busy, waiting };
}

function ChatState({ busy, waiting }: { busy: boolean; waiting: boolean }) {
  if (waiting) return <StatusDot tone="warn" pulse label="Waiting for you" />;
  if (busy) return <Spinner className="size-3 text-muted-foreground" aria-label="Running" />;
  return null;
}

function HeaderButton({ label, onClick, children }: { label: string; onClick(): void; children: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={label}
            onClick={onClick}
            // A press on a button is not the start of a pane drag.
            onPointerDown={(event) => event.stopPropagation()}
            className="shrink-0 text-muted-foreground"
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

/** One chat in the narrow switcher (FR-13). */
function SwitcherItem({ chat, index, current }: { chat: string; index: PaneIndex; current: boolean }) {
  const { title, busy, waiting } = useChatLine(chat);
  return (
    <button
      type="button"
      aria-pressed={current}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={() => useSplitStore.getState().focusIndex(index)}
      className={cn(
        "flex min-w-0 max-w-48 items-center gap-1.5 rounded-md px-2 py-0.5 text-xs transition-colors",
        current ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground",
      )}
    >
      <span className="truncate">{title}</span>
      <ChatState busy={busy} waiting={waiting} />
    </button>
  );
}

/**
 * A pane's slim header, shown while two panes are open (spec Q6): the chat's
 * title and state, open in a window of its own, close. Dragging it to the
 * other half swaps the panes (FR-7). Narrow, it switches between the two
 * chats instead (FR-13).
 */
export function PaneHeader({ chat, index, narrow, onClose }: { chat: string; index: PaneIndex; narrow: boolean; onClose(): void }) {
  const { title, busy, waiting } = useChatLine(chat);
  const panes = useSplitStore((s) => s.panes);
  return (
    <div
      onPointerDown={(event) => !narrow && startDrag(event, { kind: "pane", index })}
      className={cn(
        "flex h-8 shrink-0 items-center gap-1.5 border-b border-border/60 pr-1 pl-3 select-none",
        !narrow && "cursor-grab active:cursor-grabbing",
      )}
    >
      {narrow && panes ? (
        <div className="flex min-w-0 flex-1 items-center gap-0.5" role="group" aria-label="Chats in this window">
          <SwitcherItem chat={panes[0]} index={0} current={index === 0} />
          <SwitcherItem chat={panes[1]} index={1} current={index === 1} />
        </div>
      ) : (
        <>
          <span className="min-w-0 truncate text-xs font-medium">{title}</span>
          <ChatState busy={busy} waiting={waiting} />
          <span className="flex-1" />
        </>
      )}
      <HeaderButton label="Open in a new window" onClick={() => void useEngineStore.getState().openInNewWindow(chat)}>
        <AppWindowIcon />
      </HeaderButton>
      <HeaderButton label="Close pane" onClick={onClose}>
        <XIcon />
      </HeaderButton>
    </div>
  );
}
