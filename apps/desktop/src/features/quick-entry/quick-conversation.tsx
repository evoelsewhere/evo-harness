// The quick entry box once it shows a chat: a header, the chat's turns drawn
// as in the main window (compact steps, every reply block), what the chat
// waits for, and the follow-up field the page passes in
// (docs/features/quick-entry/spec.md, FR-11 – FR-14, FR-17, FR-20 – FR-22, NFR-3).

import * as React from "react";
import { ExternalLinkIcon, PinIcon, PinOffIcon, SquarePenIcon, XIcon } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ThemeProvider } from "@/components/theme-provider";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { Kbd } from "@/components/ui/kbd";
import { StatusDot } from "@/components/status-dot";
import { cn } from "@/lib/utils";
import { AssistantMessage, ApprovalPanel, QuestionsPanel, UserMessage, type PermissionRequest, type Turn } from "@/features/chat";
import { useEngineStore } from "@/stores/engine-store";
import { useSessionStore } from "@/stores/session-store";
import { answerable } from "./answerable";
import { openInEvo } from "./box";

/** "This computer", as the sidebar calls chats outside a workspace. */
const LOCAL_NAME = "This computer";
const NO_TURNS: Turn[] = [];

/** The header, the field and the card's padding around the turns (px). */
const CHROME = 150;

function HeaderButton({
  label,
  keys,
  onClick,
  children,
}: {
  label: string;
  keys?: string;
  onClick(): void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            aria-label={label}
            onClick={onClick}
            className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring [&_svg]:size-4"
          >
            {children}
          </button>
        }
      />
      <TooltipContent side="bottom">
        {label}
        {keys && <Kbd className="ml-1.5">{keys}</Kbd>}
      </TooltipContent>
    </Tooltip>
  );
}

function BoxHeader({
  chat,
  pinned,
  onNewChat,
  onPin,
  onClose,
}: {
  chat: string;
  pinned: boolean;
  onNewChat(): void;
  onPin(): void;
  onClose(): void;
}) {
  const mod = navigator.platform.toLowerCase().includes("mac") ? "⌘" : "Ctrl+";
  const title = useSessionStore((s) => s.sessions.find((c) => c.id === chat)?.title ?? "New chat");
  const workspace = useSessionStore((s) => {
    const row = s.sessions.find((c) => c.id === chat);
    const id = row?.workspaceId ?? null;
    return id ? (s.workspaces.find((w) => w.id === id)?.name ?? LOCAL_NAME) : LOCAL_NAME;
  });
  return (
    <div
      // The box is moved by its header; a pinned one opens where it was left (FR-17).
      data-tauri-drag-region=""
      className="flex items-center gap-2 border-b border-border/60 px-3 py-1.5"
    >
      <span data-tauri-drag-region="" className="shrink-0 rounded-md bg-brand/12 px-1.5 py-0.5 text-[11px] font-medium text-brand">
        {workspace}
      </span>
      <span data-tauri-drag-region="" className="min-w-0 flex-1 truncate text-[13px] text-muted-foreground">
        {title}
      </span>
      <HeaderButton label="New chat" keys={`${mod}N`} onClick={onNewChat}>
        <SquarePenIcon />
      </HeaderButton>
      <HeaderButton label="Open in Evo" keys={`${mod}O`} onClick={() => openInEvo(chat)}>
        <ExternalLinkIcon />
      </HeaderButton>
      <HeaderButton label={pinned ? "Unpin" : "Pin"} onClick={onPin}>
        {pinned ? <PinOffIcon /> : <PinIcon />}
      </HeaderButton>
      <HeaderButton label="Close" keys="Esc" onClick={onClose}>
        <XIcon />
      </HeaderButton>
    </div>
  );
}

const BoxTurn = React.memo(function BoxTurn({ turn, chat }: { turn: Turn; chat: string }) {
  return (
    <div className="flex flex-col gap-3">
      <UserMessage turn={turn} />
      <AssistantMessage turn={turn} compact onOpenInEvo={() => openInEvo(chat, { kind: "turn", id: turn.id })} />
    </div>
  );
});

/** What the chat waits for: answered in the box, or sent to Evo (FR-14). */
function BoxQuestion({ chat, permission }: { chat: string; permission: PermissionRequest }) {
  const where = answerable(permission);
  const review = () => openInEvo(chat, { kind: "question", id: permission.requestId });
  if (where === "elsewhere") {
    return (
      <div className="flex items-center gap-2 rounded-lg px-2 py-2 text-sm ring-1 ring-border" role="alert">
        <StatusDot tone="warn" pulse />
        <span className="min-w-0 flex-1 truncate">Evo needs you · {permission.title}</span>
        <button
          type="button"
          onClick={review}
          className="flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-[13px] font-medium text-brand hover:bg-brand/10"
        >
          Open in Evo <ExternalLinkIcon className="size-3.5" />
        </button>
      </div>
    );
  }
  if (permission.kind === "questions") return <QuestionsPanel key={permission.requestId} permission={permission} />;
  return (
    <ApprovalPanel
      permission={permission}
      compact
      extra={
        where === "inBoxWithReview" ? (
          <button type="button" onClick={review} className="ml-2 text-brand underline-offset-4 hover:underline">
            Review in Evo
          </button>
        ) : undefined
      }
    />
  );
}

/** The turns, following the latest text while the person is near the end (FR-11). */
function BoxTurns({ chat, fill }: { chat: string; fill: boolean }) {
  // A stable empty list while the store has not shown this chat yet: a new
  // array on every read would make React render again and again.
  const turns = useEngineStore((s) => (s.activeId === chat ? s.transcript.turns : NO_TURNS));
  const scroller = React.useRef<HTMLDivElement | null>(null);
  const follow = React.useRef(true);
  const maxHeight = Math.max(160, Math.floor((window.screen?.availHeight ?? 900) * 0.6) - CHROME);
  React.useLayoutEffect(() => {
    const element = scroller.current;
    if (element && follow.current) element.scrollTop = element.scrollHeight;
  });
  return (
    <div
      ref={scroller}
      onScroll={(event) => {
        const element = event.currentTarget;
        follow.current = element.scrollHeight - element.scrollTop - element.clientHeight < 48;
      }}
      // A box the person resized fills its window; otherwise it grows up to its limit (FR-11, FR-23).
      style={fill ? undefined : { maxHeight }}
      className={cn("scrollbar-slim flex flex-col gap-5 overflow-y-auto px-4 py-3 [--chat-width:100%]", fill && "min-h-0 flex-1")}
    >
      {turns.map((turn) => (
        <BoxTurn key={turn.id} turn={turn} chat={chat} />
      ))}
    </div>
  );
}

export function QuickConversation({
  chat,
  pinned,
  visible,
  fill = false,
  field,
  onNewChat,
}: {
  chat: string;
  pinned: boolean;
  /** The person resized the box: fill it (FR-23). */
  fill?: boolean;
  /** Hidden: the stores keep the streamed text, nothing is drawn (NFR-3). */
  visible: boolean;
  /** The follow-up field, owned by the box's page (FR-22). */
  field: React.ReactNode;
  onNewChat(): void;
}) {
  const permission = useEngineStore((s) => (s.activeId === chat ? s.transcript.permissions?.[0] : undefined));
  const [pin, setPin] = React.useState(pinned);
  React.useEffect(() => setPin(pinned), [pinned]);
  const togglePin = () => {
    const next = !pin;
    setPin(next);
    void invoke("quick_entry_pin", { pinned: next }).catch(() => setPin(!next));
  };
  const inPlace = permission && answerable(permission) !== "elsewhere";
  return (
    <ThemeProvider>
      <TooltipProvider delay={400}>
        <div className={cn("flex min-h-0 flex-col", fill && "flex-1")}>
          <BoxHeader
            chat={chat}
            pinned={pin}
            onNewChat={onNewChat}
            onPin={togglePin}
            onClose={() => void invoke("quick_entry_hide").catch(() => undefined)}
          />
          <React.Activity mode={visible ? "visible" : "hidden"}>
            <BoxTurns chat={chat} fill={fill} />
          </React.Activity>
          <div className="flex flex-col gap-2 border-t border-border/60 px-3 py-2.5">
            {permission && <BoxQuestion chat={chat} permission={permission} />}
            {/* A request answered in place takes the field's place, as in the main window. */}
            {!inPlace && field}
          </div>
        </div>
      </TooltipProvider>
    </ThemeProvider>
  );
}
