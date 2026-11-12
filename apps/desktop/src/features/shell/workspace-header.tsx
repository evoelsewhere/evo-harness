import * as React from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  BotIcon,
  ChevronDownIcon,
  EyeIcon,
  ChevronRightIcon,
  FolderOpenIcon,
  FolderIcon,
  GhostIcon,
  GitBranchIcon,
  LaptopIcon,
} from "lucide-react";
import { roleTitle } from "@/components/ai-elements/session-proposal";
import { WorktreeReview } from "@/components/ai-elements/worktree-review";
import { worktrees } from "@/lib/worktrees";
import { SidePanelIcon, SummaryIcon } from "@/components/icons/tool-icons";
import { ContextPanel } from "@/features/chat/context-panel";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useSidebar } from "@/components/ui/sidebar";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { inTauri, isMac, modKey } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { chatWindowId } from "@/lib/windows";
import { useEngineStore, useWorktreeSession } from "@/stores/engine-store";
import { useSessionStore, type SessionSummary } from "@/stores/session-store";
import { useUiStore } from "@/stores/ui-store";
import { TitleBarActions } from "./title-bar";
import { NotificationCenter } from "@/features/notifications/notification-center";
import { OpenInMenu } from "./open-in-menu";
import { WindowControls } from "./window-controls";

const normalPath = (path: string) => path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
const samePath = (a: string, b: string) => normalPath(a) === normalPath(b);

/**
 * The branch of the managed worktree at `path` (`evo/<name>` when the agent
 * named it: docs/features/session-roles-and-mail/spec.md, FR-9).
 */
function useWorktreeBranch(repo: string | undefined, path: string | undefined): string | undefined {
  const [branch, setBranch] = React.useState<string | undefined>();
  React.useEffect(() => {
    setBranch(undefined);
    if (!repo || !path || !inTauri) return;
    let live = true;
    worktrees
      .list(repo)
      .then((list) => {
        if (live) setBranch(list.find((w) => samePath(w.path, path))?.branch);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [repo, path]);
  return branch;
}

/**
 * A chat working in a worktree: review its changes, apply or discard them.
 * The chat's engine has its folder there, so it is stopped before the
 * worktree is removed and starts again in the workspace (or, when nothing
 * was applied or discarded, where it was).
 */
function WorktreeChip() {
  const chat = useSessionStore((s) => s.sessions.find((x) => x.id === s.currentId));
  const workspace = useSessionStore((s) => s.workspaces.find((w) => w.id === chat?.workspaceId));
  const session = useWorktreeSession(chat?.worktreePath ? chat.id : undefined);
  const [open, setOpen] = React.useState(false);
  const branch = useWorktreeBranch(workspace?.path, chat?.worktreePath ?? undefined);
  const id = chat?.id;
  // The chat left the screen with its review open: its engine starts again.
  React.useEffect(() => {
    if (!id) return;
    return () => void useEngineStore.getState().resumeEngine(id);
  }, [id]);
  if (!chat?.worktreePath || !workspace) return null;
  const name = chat.worktreePath.replace(/\\/g, "/").split("/").filter(Boolean).at(-1);
  return (
    <>
      <Button
        variant="ghost"
        size="sm"
        className="no-drag min-w-0 gap-1.5 text-muted-foreground"
        title={`Works in the worktree ${chat.worktreePath}${branch ? ` on the branch ${branch}` : ""}`}
        onClick={() => setOpen(true)}
      >
        <GitBranchIcon />
        <span className="max-w-40 truncate">{branch ?? `Worktree ${name}`}</span>
      </Button>
      <WorktreeReview
        open={open}
        onOpenChange={(next) => {
          setOpen(next);
          if (!next) session.onClosed();
        }}
        repo={workspace.path}
        path={chat.worktreePath}
        branch={branch}
        busy={session.busy}
        beforeSettle={session.beforeSettle}
        onSettled={session.onSettled}
      />
    </>
  );
}

/**
 * A nested session that runs as a role: its agent type, model, thinking and
 * access, with where the type came from in the tooltip
 * (docs/features/session-roles-and-mail/spec.md, FR-3, NFR-4).
 */
function RoleChip() {
  const role = useSessionStore((s) => s.sessions.find((x) => x.id === s.currentId)?.role);
  if (!role) return null;
  const hint = `Runs as a role: ${roleTitle(role)}`;
  // The name only (read-only shows as the eye); the rest is in the tooltip,
  // so the session's own title keeps its room.
  const label = role.name ?? role.model ?? "role";
  return (
    <span
      className="flex min-w-0 shrink-[2] items-center gap-1.5 px-2 text-sm text-muted-foreground"
      title={hint}
      aria-label={hint}
    >
      {role.access === "read-only" ? <EyeIcon className="size-4 shrink-0" /> : <BotIcon className="size-4 shrink-0" />}
      <span className="max-w-32 truncate">{label}</span>
    </span>
  );
}

/** Says that the chat on screen is incognito: no memory, and gone when it ends. */
function IncognitoChip() {
  const incognito = useSessionStore((s) => Boolean(s.sessions.find((x) => x.id === s.currentId)?.incognito));
  if (!incognito) return null;
  const hint = "Incognito chat: it uses no memory, cannot be found by search, and is removed when you end it or quit evo";
  return (
    <span className="flex min-w-0 items-center gap-1.5 px-2 text-sm text-muted-foreground" title={hint} aria-label={hint}>
      <GhostIcon className="size-4" />
      <span className="truncate">Incognito</span>
    </span>
  );
}

/**
 * In a nested session: "Parent title › Child title". The parent part opens
 * the parent (an archived one too). A chat window shows only its own chat, so
 * there the parent is plain text.
 */
function ParentCrumb() {
  const find = (s: { sessions: SessionSummary[]; archived: SessionSummary[] }, id: string | null | undefined) =>
    id ? (s.sessions.find((x) => x.id === id) ?? s.archived.find((x) => x.id === id)) : undefined;
  const child = useSessionStore((s) => find(s, s.currentId));
  const parentId = child?.parentChatId ?? null;
  const parent = useSessionStore((s) => find(s, parentId));
  const loaded = useSessionStore((s) => s.loaded);
  const openParent = useEngineStore((s) => s.openParent);
  // A parent that is not among the live chats is archived: read its title.
  React.useEffect(() => {
    if (parentId && loaded && !parent) void useSessionStore.getState().loadArchived();
  }, [parentId, loaded, parent === undefined]);
  if (!child || !parentId) return null;
  const title = parent?.title ?? "Archived chat";
  const archived = !parent || parent.archivedAt != null;
  return (
    <span className="flex min-w-0 items-center text-sm">
      {chatWindowId ? (
        <span className="max-w-48 truncate px-2 text-muted-foreground" title={`Started from ${title}`}>
          {title}
        </span>
      ) : (
        <Button
          variant="ghost"
          size="sm"
          className="no-drag min-w-0 text-muted-foreground"
          title={`Open ${title}${archived ? " (archived)" : ""}`}
          onClick={() => void openParent(child.id)}
        >
          <span className="max-w-48 truncate">{title}</span>
        </Button>
      )}
      <ChevronRightIcon aria-hidden className="size-3.5 shrink-0 text-muted-foreground" />
      <span className="max-w-48 min-w-12 truncate px-2 font-medium" title={child.title}>
        {child.title}
      </span>
    </span>
  );
}

/** The workspace of the current chat, and a way to switch or open another folder. */
function WorkspaceSwitcher() {
  const cwd = useEngineStore((s) => s.cwd);
  const openWorkspace = useEngineStore((s) => s.openWorkspace);
  const newChat = useEngineStore((s) => s.newChat);
  const workspace = useSessionStore((s) => s.workspaceOf(s.currentId));
  const workspaces = useSessionStore((s) => s.workspaces);
  const Icon = workspace ? FolderIcon : LaptopIcon;
  const others = workspaces.filter((w) => w.id !== workspace?.id);

  async function chooseFolder() {
    const folder = await open({ directory: true, multiple: false, title: "Open a workspace folder" });
    if (typeof folder === "string") await openWorkspace(folder);
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button variant="ghost" size="sm" className="no-drag min-w-0 gap-2 font-medium" title={cwd} />}
      >
        <Icon className="text-muted-foreground" />
        <span className="truncate">{workspace?.name ?? "This computer"}</span>
        <ChevronDownIcon className="text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-72">
        <DropdownMenuGroup>
          <DropdownMenuLabel className="truncate">{cwd ?? "Home folder"}</DropdownMenuLabel>
        </DropdownMenuGroup>
        {(others.length > 0 || workspace) && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuGroup>
              <DropdownMenuLabel>Switch to</DropdownMenuLabel>
              {others.map((w) => (
                <DropdownMenuItem key={w.id} onClick={() => void newChat(w.id)}>
                  <FolderIcon />
                  <span className="truncate">{w.name}</span>
                </DropdownMenuItem>
              ))}
              {workspace && (
                <DropdownMenuItem onClick={() => void newChat(null)}>
                  <LaptopIcon />
                  This computer
                </DropdownMenuItem>
              )}
            </DropdownMenuGroup>
          </>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => void chooseFolder()}>
          <FolderOpenIcon />
          Open folder…
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function HeaderIcon({
  label,
  pressed,
  onClick,
  children,
}: {
  label: string;
  pressed?: boolean;
  onClick(): void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={label}
            aria-pressed={pressed}
            onClick={onClick}
            // Pressed in the accent colour (Settings → Appearance), like the side panel's tool rail.
            className={cn(
              "no-drag size-8 text-muted-foreground",
              pressed && "bg-brand/15 text-brand hover:bg-brand/20 hover:text-brand",
            )}
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

const GroupDivider = () => <span aria-hidden className="mx-0.5 h-4 w-px shrink-0 bg-border" />;

/**
 * The window's title bar, right of the sidebar: the title bar buttons while
 * the sidebar is hidden, then — at the right — the notification bell and the
 * window controls. Everything else is empty space that drags the window.
 */
export function WindowTitleBar() {
  const { state, isMobile } = useSidebar();
  const sidebarHidden = state === "collapsed" || isMobile;
  // macOS traffic lights sit over the title bar once the sidebar is out of the way.
  const clearTrafficLights = isMac && inTauri && sidebarHidden;

  return (
    <header
      data-tauri-drag-region
      className={cn(
        "drag-region flex h-10 shrink-0 items-center gap-1 pl-2",
        clearTrafficLights && "pl-20",
        // No custom window controls at the edge (macOS, browser preview).
        (isMac || !inTauri) && "pr-2",
      )}
    >
      {/* While the sidebar is out of view its title bar buttons (menu, sidebar, back/forward) live here. */}
      {sidebarHidden && !chatWindowId && <TitleBarActions />}
      <div data-tauri-drag-region className="h-full min-w-4 flex-1" />
      <NotificationCenter />
      <WindowControls />
    </header>
  );
}

/**
 * The top row of the chat card: the workspace on the left; on the right one
 * group with Open in, context used, the summary and side panel toggles. Labels drop to
 * icons as the card narrows.
 */
export function WorkspaceToolbar() {
  const cwd = useEngineStore((s) => s.cwd);
  const context = useEngineStore((s) => s.state.context);
  const rightPanelOpen = useUiStore((s) => s.rightPanelOpen);
  const toggleRightPanel = useUiStore((s) => s.toggleRightPanel);
  const summaryShown = useUiStore((s) => (s.summaryFits ? s.summaryOpen : s.summaryPeek));
  const toggleSummary = useUiStore((s) => s.toggleSummary);
  const hasContext = (context?.window ?? 0) > 0;

  return (
    <div className="@container/header flex h-12 shrink-0 items-center gap-1 px-2">
      <WorkspaceSwitcher />
      <IncognitoChip />
      <ParentCrumb />
      <RoleChip />
      <WorktreeChip />
      <div className="min-w-4 flex-1" />

      {/* Workspace tools in one group: Open in, context used, summary, side panel. */}
      <div className="no-drag flex h-9 shrink-0 items-center gap-0.5 rounded-lg p-0.5 shadow-border-small">
        {cwd && (
          <>
            <OpenInMenu cwd={cwd} />
            <GroupDivider />
          </>
        )}
        {hasContext && (
          <>
            <ContextPanel />
            <GroupDivider />
          </>
        )}
        {/* The overlay summary ignores presses here, so this button can close it. */}
        <span data-summary-toggle className="contents">
          <HeaderIcon label={summaryShown ? "Hide summary" : "Show summary"} pressed={summaryShown} onClick={() => toggleSummary()}>
            <SummaryIcon open={summaryShown} className="size-[18px]" />
          </HeaderIcon>
        </span>
        <HeaderIcon label={`Side panel (${modKey} J)`} pressed={rightPanelOpen} onClick={() => toggleRightPanel()}>
          <SidePanelIcon open={rightPanelOpen} className="size-[18px]" />
        </HeaderIcon>
      </div>

    </div>
  );
}
