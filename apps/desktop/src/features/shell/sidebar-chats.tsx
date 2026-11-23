import * as React from "react";
import { useShallow } from "zustand/react/shallow";
import { ArchiveIcon, ChevronRightIcon, GhostIcon, MoreHorizontalIcon, PinIcon, PlusIcon } from "lucide-react";
import { StatusDot } from "@/components/status-dot";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import {
  SidebarContent,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
} from "@/components/ui/sidebar";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { startDrag } from "@/features/split/drag";
import { useDisplayTitle } from "@/hooks/use-display-title";
import { plainText } from "@/lib/plain-text";
import { LOCAL_GROUP, UNTITLED, useSessionStore, type SessionSummary } from "@/stores/session-store";
import { useSchedulerStore } from "@/stores/scheduler-store";
import { ChatMenuItems, useChatMenuKeys } from "./chat-menu";
import { ConfirmDialog } from "./confirm-dialog";
import { foldRuns, groupByTime, needsYou, TIME_GROUP_LABEL } from "./sidebar-model";
import { useChatStates } from "./use-chat-states";
import { ChangeIconDialog, WorkspaceIcon } from "./workspace-icon";
import { RemoveWorkspaceDialog, WorkspaceMenuItems, type RailGroup } from "./workspace-rail";

// The sidebar's chat lists: a row per chat with what it is doing (running,
// waiting for approval, unread, how long ago), and its actions (see
// `chat-menu.tsx`) from a right-click or the "…" button.

/** "now", "5m", "3h", "2d", "4w", then the date. */
function age(at: number, now = Date.now()): string {
  const minutes = Math.floor((now - at) / 60_000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  if (days < 35) return `${Math.floor(days / 7)}w`;
  return new Date(at).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** Re-render once a minute so ages stay current. */
function useMinuteTick() {
  const [, setTick] = React.useState(0);
  React.useEffect(() => {
    const timer = window.setInterval(() => setTick((t) => t + 1), 60_000);
    return () => window.clearInterval(timer);
  }, []);
}

function RenameInput({ chat, onDone }: { chat: SessionSummary; onDone(): void }) {
  const rename = useSessionStore((s) => s.rename);
  const [value, setValue] = React.useState(chat.title);
  const done = React.useRef(false);
  const commit = (save: boolean) => {
    if (done.current) return;
    done.current = true;
    if (save && value.trim() && value.trim() !== chat.title) void rename(chat.id, value);
    onDone();
  };
  return (
    <input
      autoFocus
      value={value}
      aria-label="Chat title"
      onChange={(event) => setValue(event.target.value)}
      onFocus={(event) => event.target.select()}
      onBlur={() => commit(true)}
      onKeyDown={(event) => {
        if (event.key === "Enter") commit(true);
        else if (event.key === "Escape") commit(false);
      }}
      className="h-7 w-full rounded-lg border border-sidebar-ring bg-sidebar px-2 text-[13px] text-sidebar-accent-foreground outline-none"
    />
  );
}

/**
 * One chat row: right-click (or "…") for its actions; the title shimmers while
 * it runs. A chat the shell runs with no window shows as working or waiting too
 * (docs/features/shell-hosted-engines/spec.md, NFR-5).
 */
export function ChatItem({ chat, active, tag }: { chat: SessionSummary; active: boolean; tag?: React.ReactNode }) {
  const shellWork = useSessionStore((s) => s.work[chat.id]);
  const busy =
    useEngineStore((s) => Boolean(s.runtimes[chat.id]?.transcript.busy || s.busyElsewhere[chat.id])) ||
    shellWork === "working";
  const asking = useEngineStore((s) => Boolean(s.runtimes[chat.id]?.transcript.turns.at(-1)?.permission));
  const waiting = asking || shellWork === "waiting";
  // A scheduled run shows its date (ux-polish FR-5); its preview reads as plain text (FR-9).
  const shownTitle = useDisplayTitle(chat, UNTITLED);
  const preview = chat.preview ? plainText(chat.preview) : "";
  const failed = useEngineStore((s) => s.runtimes[chat.id]?.status === "exited");
  const { selectSession, closeChat } = useEngineStore.getState();
  const [menuOpen, setMenuOpen] = React.useState(false);
  const [contextOpen, setContextOpen] = React.useState(false);
  const [renaming, setRenaming] = React.useState(false);
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const archived = chat.archivedAt != null;
  const menu = {
    chat,
    active,
    onRename: () => setRenaming(true),
    onDelete: () => setConfirmDelete(true),
    onClose: () => {
      setMenuOpen(false);
      setContextOpen(false);
    },
  };
  const onMenuKey = useChatMenuKeys(menu);

  if (renaming) {
    return (
      <SidebarMenuItem>
        <RenameInput chat={chat} onDone={() => setRenaming(false)} />
      </SidebarMenuItem>
    );
  }

  // A run that waits for an approval is still running: the amber dot wins.
  const status = waiting ? (
    <StatusDot tone="warn" pulse label={asking ? "Waiting for approval" : "Waiting for you"} />
  ) : busy ? (
    <Spinner className="size-3.5 text-muted-foreground" aria-label="Running" />
  ) : failed ? (
    <StatusDot tone="error" label="Engine stopped" />
  ) : chat.unread ? (
    <StatusDot tone="info" label="Unread reply" />
  ) : chat.draft ? null : (
    <span className="text-[11px] text-muted-foreground tabular-nums">{age(chat.updatedAt)}</span>
  );

  const row = (
    <>
      <SidebarMenuButton
        isActive={active || contextOpen}
        onClick={() => void selectSession(chat.id)}
        // Drag a saved chat onto the chat area to show it beside another
        // (docs/features/chat-split-view/spec.md, FR-2).
        onPointerDown={(event) => {
          if (!chat.draft && !archived) startDrag(event, { kind: "chat", chat: chat.id, title: chat.title });
        }}
        title={preview ? `${shownTitle}

${preview}` : shownTitle}
        className="h-7 pr-8 text-[13px]"
      >
        {chat.pinned && !archived && <PinIcon className="size-3! text-muted-foreground" />}
        {/* An incognito chat: no memory, and gone when it ends. */}
        {chat.incognito && <GhostIcon className="size-3! text-muted-foreground" aria-label="Incognito chat" />}
        <span
          className={cn(
            "truncate",
            chat.draft && "text-muted-foreground italic",
            chat.unread && "font-medium",
            busy && "shimmer motion-reduce:shimmer-none",
          )}
        >
          {shownTitle}
        </span>
        {/* Needs you: the chat's workspace (docs/features/workspace-rail/spec.md, FR-7). */}
        {tag && <span className="ml-auto flex shrink-0 items-center text-muted-foreground">{tag}</span>}
      </SidebarMenuButton>
      {status && (
        <span
          className={cn(
            "pointer-events-none absolute top-1/2 right-2 flex -translate-y-1/2 items-center",
            !chat.draft && "group-hover/menu-item:opacity-0 group-focus-within/menu-item:opacity-0",
            menuOpen && "opacity-0",
          )}
        >
          {status}
        </span>
      )}
      {!chat.draft && (
        <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
          <DropdownMenuTrigger
            render={<SidebarMenuAction showOnHover className="top-1 size-5" aria-label={`Actions for ${chat.title}`} />}
          >
            <MoreHorizontalIcon />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" side="right" className="w-52" onKeyDown={onMenuKey}>
            <ChatMenuItems {...menu} />
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      <ConfirmDialog
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        title={chat.incognito ? "End this incognito chat?" : "Delete this chat?"}
        description={
          chat.incognito
            ? "The chat and its whole conversation are removed from this computer now. This cannot be undone."
            : `"${chat.title}" and its whole conversation are removed from this computer. This cannot be undone.`
        }
        action={chat.incognito ? "End chat" : "Delete chat"}
        onConfirm={() => void closeChat(chat.id, "delete")}
      />
    </>
  );

  // A draft has no saved chat to act on yet.
  if (chat.draft) return <SidebarMenuItem>{row}</SidebarMenuItem>;
  return (
    <ContextMenu open={contextOpen} onOpenChange={setContextOpen}>
      <ContextMenuTrigger render={<SidebarMenuItem />}>{row}</ContextMenuTrigger>
      <ContextMenuContent className="w-52" onKeyDown={onMenuKey}>
        <ChatMenuItems {...menu} />
      </ContextMenuContent>
    </ContextMenu>
  );
}

/** Nested sessions shown before the rest fold into the footer. */
const NESTED_SHOWN = 3;

const NO_CHILDREN: SessionSummary[] = [];
let indexedSessions: SessionSummary[] | null = null;
let childIndex = new Map<string, SessionSummary[]>();

/** The live chats each chat started, by parent id: built once per change of the chat list. */
function childrenByParent(sessions: SessionSummary[]): Map<string, SessionSummary[]> {
  if (sessions !== indexedSessions) {
    const index = new Map<string, SessionSummary[]>();
    for (const chat of sessions) {
      if (!chat.parentChatId) continue;
      const list = index.get(chat.parentChatId);
      if (list) list.push(chat);
      else index.set(chat.parentChatId, [chat]);
    }
    indexedSessions = sessions;
    childIndex = index;
  }
  return childIndex;
}

/**
 * The sessions a chat started, under its row with a guide line: running,
 * waiting and unread ones first, then the most recent; the rest fold into
 * "N more nested sessions · M running · K unread".
 */
export function NestedChats({ parentId, currentId }: { parentId: string; currentId: string | null }) {
  const children = useSessionStore(useShallow((s) => childrenByParent(s.sessions).get(parentId) ?? NO_CHILDREN));
  // Working with no window counts too (NFR-5).
  const work = useSessionStore((s) => s.work);
  const attention = useEngineStore((s) => {
    const marks: Record<string, number> = {};
    for (const child of children) {
      const rt = s.runtimes[child.id];
      marks[child.id] =
        rt?.transcript.turns.at(-1)?.permission || work[child.id] === "waiting"
          ? 3
          : rt?.transcript.busy || s.busyElsewhere[child.id] || work[child.id] === "working"
            ? 2
            : 0;
    }
    return JSON.stringify(marks);
  });
  const [expanded, setExpanded] = React.useState(false);
  if (children.length === 0) return null;
  const marks = JSON.parse(attention) as Record<string, number>;
  const rank = (chat: SessionSummary) => (marks[chat.id] ?? 0) + (chat.unread ? 1 : 0);
  const sorted = [...children].sort((a, b) => rank(b) - rank(a) || b.updatedAt - a.updatedAt);
  // The open chat stays visible even when it would fold away.
  const shown = expanded
    ? sorted
    : sorted.filter((chat, index) => index < NESTED_SHOWN || chat.id === currentId);
  const hidden = sorted.filter((chat) => !shown.includes(chat));
  const unread = hidden.filter((chat) => chat.unread).length;
  const running = hidden.filter((chat) => (marks[chat.id] ?? 0) > 0).length;
  return (
    <SidebarMenuSub className="mr-0 ml-3.5 gap-0 pr-0 pl-1.5">
      {shown.map((chat) => (
        <ChatItem key={chat.id} chat={chat} active={chat.id === currentId} />
      ))}
      {(hidden.length > 0 || expanded) && sorted.length > NESTED_SHOWN && (
        <li>
          <button
            type="button"
            onClick={() => setExpanded((value) => !value)}
            className="flex h-6 w-full items-center gap-1 rounded-md px-2 text-left text-xs text-muted-foreground hover:text-sidebar-accent-foreground"
          >
            {expanded ? (
              "Show fewer"
            ) : (
              <>
                {hidden.length} more nested {hidden.length === 1 ? "session" : "sessions"}
                {running > 0 && <span> · {running} running</span>}
                {unread > 0 && <span className="text-link"> · {unread} unread</span>}
              </>
            )}
          </button>
        </li>
      )}
    </SidebarMenuSub>
  );
}

/**
 * A list of chat rows. With `nested` (the default) each row is followed by the
 * live sessions it started; the archive lists its chats flat, so a live child
 * of an archived parent shows once, at the top level.
 */
export function ChatList({ chats, empty, nested = true }: { chats: SessionSummary[]; empty?: string; nested?: boolean }) {
  const currentId = useEngineStore((s) => s.activeId);
  useMinuteTick();
  return (
    <SidebarMenu>
      {chats.length === 0 && empty && <p className="px-2 py-1 text-xs text-muted-foreground">{empty}</p>}
      {chats.map((chat) => (
        <React.Fragment key={chat.id}>
          <ChatItem chat={chat} active={chat.id === currentId} />
          {nested && <NestedChats parentId={chat.id} currentId={currentId} />}
        </React.Fragment>
      ))}
    </SidebarMenu>
  );
}

/** Rows shown in "Needs you" before the rest fold into "N more" (FR-7). */
const NEEDS_YOU_SHOWN = 6;

/**
 * Chats across every workspace that wait for the person, have an unread reply
 * or run — most urgent first, each with its workspace's initials. Hidden when
 * empty (docs/features/workspace-rail/spec.md, FR-7).
 */
export function NeedsYou() {
  const sessions = useSessionStore((s) => s.sessions);
  const workspaces = useSessionStore((s) => s.workspaces);
  const currentId = useEngineStore((s) => s.activeId);
  const states = useChatStates();
  const [expanded, setExpanded] = React.useState(false);
  useMinuteTick();
  const chats = needsYou(sessions, states);
  if (chats.length === 0) return null;
  const byId = new Map(workspaces.map((w) => [w.id, w]));
  // The workspace's own icon (FR-15), its name in the tooltip.
  const tag = (chat: SessionSummary) => {
    const workspace = chat.workspaceId ? byId.get(chat.workspaceId) : undefined;
    return workspace ? (
      <span title={workspace.name} className="flex size-4 items-center justify-center">
        <WorkspaceIcon group={{ id: workspace.id, name: workspace.name, path: workspace.path }} size="sm" />
      </span>
    ) : undefined;
  };
  const shown = expanded ? chats : chats.filter((chat, index) => index < NEEDS_YOU_SHOWN || chat.id === currentId);
  const hidden = chats.length - shown.length;
  return (
    <SidebarGroup className="max-h-[40%] shrink-0 overflow-y-auto py-0.5">
      <SidebarGroupLabel className="h-7 text-xs font-normal text-muted-foreground">Needs you</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {shown.map((chat) => (
            <ChatItem key={chat.id} chat={chat} active={chat.id === currentId} tag={tag(chat)} />
          ))}
          {(hidden > 0 || expanded) && chats.length > NEEDS_YOU_SHOWN && (
            <li>
              <FoldButton onClick={() => setExpanded((value) => !value)}>{expanded ? "Show fewer" : `${hidden} more`}</FoldButton>
            </li>
          )}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

function FoldButton({ onClick, children, open }: { onClick(): void; children: React.ReactNode; open?: boolean }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-expanded={open}
      className="flex h-6 w-full items-center gap-1 rounded-md px-2 text-left text-xs text-muted-foreground hover:text-sidebar-accent-foreground"
    >
      {open !== undefined && <ChevronRightIcon className={cn("size-3 shrink-0 transition-transform", open && "rotate-90")} />}
      {children}
    </button>
  );
}

/**
 * A chat row with the sessions it started and, for a scheduled task's latest
 * run, its earlier runs behind "N earlier runs" (FR-10, FR-11).
 */
function ChatWithRuns({ chat, runs, currentId }: { chat: SessionSummary; runs: SessionSummary[]; currentId: string | null }) {
  const [open, setOpen] = React.useState(false);
  const holdsCurrent = runs.some((run) => run.id === currentId);
  const expanded = open || holdsCurrent;
  return (
    <>
      <ChatItem chat={chat} active={chat.id === currentId} />
      <NestedChats parentId={chat.id} currentId={currentId} />
      {runs.length > 0 && (
        <SidebarMenuSub className="mr-0 ml-3.5 gap-0 pr-0 pl-1.5">
          {expanded && runs.map((run) => <ChatItem key={run.id} chat={run} active={run.id === currentId} />)}
          <li>
            <FoldButton open={expanded} onClick={() => setOpen(!expanded)}>
              {runs.length} earlier {runs.length === 1 ? "run" : "runs"}
            </FoldButton>
          </li>
        </SidebarMenuSub>
      )}
    </>
  );
}

/**
 * The shown workspace's chats: a header (name, New chat, menu), its pinned
 * chats, then Today / Yesterday / Previous 7 days / Older, Older folded and
 * scheduled runs folded (FR-3, FR-8 – FR-10).
 */
export function WorkspaceChats({
  group,
  chats,
  children,
}: {
  group: RailGroup;
  chats: SessionSummary[];
  /** Shown after the chats, in the same scrolling area (the archive). */
  children?: React.ReactNode;
}) {
  const currentId = useEngineStore((s) => s.activeId);
  const newChat = useEngineStore((s) => s.newChat);
  const tasks = useSchedulerStore((s) => s.tasks);
  const [olderOpen, setOlderOpen] = React.useState(false);
  const [confirmRemove, setConfirmRemove] = React.useState(false);
  const [changeIcon, setChangeIcon] = React.useState(false);
  useMinuteTick();
  // Another workspace starts with Older folded again.
  React.useEffect(() => setOlderOpen(false), [group.id]);

  const names = new Set(tasks.map((task) => task.name));
  const isRun = (chat: SessionSummary) => !chat.draft && (chat.scheduled || names.has(chat.title));
  const byUpdated = (a: SessionSummary, b: SessionSummary) => b.updatedAt - a.updatedAt;
  const pinned = chats.filter((chat) => chat.pinned).sort(byUpdated);
  const rest = foldRuns(chats.filter((chat) => !chat.pinned).sort(byUpdated), isRun);
  const groups = groupByTime(rest, (item) => item.chat.updatedAt);

  return (
    <>
      {/* The header stays put while the chats scroll under it. */}
      <SidebarGroup className="group/workspace shrink-0 py-0.5">
        {/* The workspace's name is on the rail already (FR-3). */}
        <SidebarGroupLabel title={group.path ?? group.name} className="h-7 pr-14 text-xs font-normal text-muted-foreground">
          Chats
        </SidebarGroupLabel>
        {group.path && (
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <SidebarGroupAction
                  className="top-1.5 right-8"
                  title={`${group.name} actions`}
                />
              }
            >
              <MoreHorizontalIcon />
              <span className="sr-only">{group.name} actions</span>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-52">
              <WorkspaceMenuItems group={group} onRemove={() => setConfirmRemove(true)} onChangeIcon={() => setChangeIcon(true)} />
            </DropdownMenuContent>
          </DropdownMenu>
        )}
        <SidebarGroupAction
          title={`New chat in ${group.name}`}
          onClick={() => void newChat(group.id === LOCAL_GROUP ? null : group.id)}
          className="top-1.5"
        >
          <PlusIcon />
          <span className="sr-only">New chat in {group.name}</span>
        </SidebarGroupAction>
        {group.path && <RemoveWorkspaceDialog group={group} open={confirmRemove} onOpenChange={setConfirmRemove} />}
        {group.path && <ChangeIconDialog group={group} open={changeIcon} onOpenChange={setChangeIcon} />}
      </SidebarGroup>
      <SidebarContent className="gap-0">
        <SidebarGroup className="pt-0 pb-0.5">
          <SidebarGroupContent>
            {chats.length === 0 && <p className="px-2 py-1 text-xs text-muted-foreground">No chats yet</p>}
            {pinned.length > 0 && (
              <>
                <TimeLabel>Pinned</TimeLabel>
                <ChatList chats={pinned} />
              </>
            )}
            {groups.map(({ group: time, items }) => {
              const older = time === "older";
              const holdsCurrent = items.some((item) => item.chat.id === currentId || item.runs.some((run) => run.id === currentId));
              const open = !older || olderOpen || holdsCurrent;
              return (
                <React.Fragment key={time}>
                  {older ? (
                    <FoldButton open={open} onClick={() => setOlderOpen(!open)}>
                      {TIME_GROUP_LABEL.older} · {items.length}
                    </FoldButton>
                  ) : (
                    <TimeLabel>{TIME_GROUP_LABEL[time]}</TimeLabel>
                  )}
                  {open && (
                    <SidebarMenu>
                      {items.map((item) => (
                        <ChatWithRuns key={item.chat.id} chat={item.chat} runs={item.runs} currentId={currentId} />
                      ))}
                    </SidebarMenu>
                  )}
                </React.Fragment>
              );
            })}
          </SidebarGroupContent>
        </SidebarGroup>
        {children}
      </SidebarContent>
    </>
  );
}

function TimeLabel({ children }: { children: React.ReactNode }) {
  return <div className="px-2.5 pt-2 pb-0.5 text-[11px] text-muted-foreground">{children}</div>;
}

/** Archived chats, loaded when opened. */
export function ArchivedGroup() {
  const archived = useSessionStore((s) => s.archived);
  const loadArchived = useSessionStore((s) => s.loadArchived);
  const [open, setOpen] = React.useState(false);
  return (
    <Collapsible
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (next) void loadArchived();
      }}
    >
      <SidebarGroup className="py-0.5">
        <SidebarGroupLabel
          render={<CollapsibleTrigger />}
          className="h-7 w-full gap-1 text-[13px] text-muted-foreground hover:bg-sidebar-accent"
        >
          <ArchiveIcon className="size-3.5!" />
          <span className="truncate">Archived</span>
          <ChevronRightIcon className={cn("size-3.5 shrink-0 transition-transform", open && "rotate-90")} />
        </SidebarGroupLabel>
        <CollapsibleContent>
          <SidebarGroupContent>
            <ChatList chats={archived} empty="Nothing archived" nested={false} />
          </SidebarGroupContent>
        </CollapsibleContent>
      </SidebarGroup>
    </Collapsible>
  );
}
