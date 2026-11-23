import * as React from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { FolderOpenIcon, FolderPlusIcon, SmileIcon, SquarePenIcon, Trash2Icon } from "lucide-react";
import { StatusDot } from "@/components/status-dot";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { DropdownMenuItem, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import { Spinner } from "@/components/ui/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { LOCAL_GROUP, useSessionStore, type SessionSummary } from "@/stores/session-store";
import { ConfirmDialog } from "./confirm-dialog";
import { rollUp, type ChatState, type RollUp } from "./sidebar-model";
import { useChatStates } from "./use-chat-states";
import { ChangeIconDialog, WorkspaceIcon } from "./workspace-icon";

// The workspace rail: one icon per workspace at the sidebar's left edge, the
// one shown marked, each with a dot for the most urgent state of its chats;
// a right-click opens the workspace's menu
// (docs/features/workspace-rail/spec.md, FR-1 – FR-3).

export const LOCAL_NAME = "This computer";
/** "This computer" under its rail icon, where the full name does not fit. */
const LOCAL_SHORT = "Local";

export interface RailGroup {
  id: string;
  name: string;
  path?: string;
}

/** A workspace's actions, for the rail's right-click menu and the list header's "…". */
export function WorkspaceMenuItems({
  group,
  onRemove,
  onChangeIcon,
}: {
  group: RailGroup;
  onRemove(): void;
  onChangeIcon(): void;
}) {
  const newChat = useEngineStore((s) => s.newChat);
  const path = group.path;
  return (
    <>
      <DropdownMenuItem onClick={() => void newChat(group.id === LOCAL_GROUP ? null : group.id)}>
        <SquarePenIcon />
        New chat
      </DropdownMenuItem>
      {path && (
        <>
          <DropdownMenuItem onClick={() => void revealItemInDir(path).catch(() => undefined)}>
            <FolderOpenIcon />
            Show in folder
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onChangeIcon}>
            <SmileIcon />
            Change icon…
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem variant="destructive" onClick={onRemove}>
            <Trash2Icon />
            Remove workspace…
          </DropdownMenuItem>
        </>
      )}
    </>
  );
}

/** Asks before removing a workspace from Evo; the folder stays on disk. */
export function RemoveWorkspaceDialog({
  group,
  open,
  onOpenChange,
}: {
  group: RailGroup;
  open: boolean;
  onOpenChange(open: boolean): void;
}) {
  const removeWorkspace = useSessionStore((s) => s.removeWorkspace);
  const forgetChats = useEngineStore((s) => s.forgetChats);
  return (
    <ConfirmDialog
      open={open}
      onOpenChange={onOpenChange}
      title={`Remove ${group.name}?`}
      description="The workspace and its chats are removed from Evo. The folder and its files stay on disk."
      action="Remove"
      onConfirm={() => void removeWorkspace(group.id).then((ids) => forgetChats(ids))}
    />
  );
}

function summary(total: RollUp): string {
  const parts = [
    total.waiting && `${total.waiting} waiting`,
    total.running && `${total.running} running`,
    total.unread && `${total.unread} unread`,
  ].filter(Boolean);
  return parts.join(" · ");
}

function RailDot({ total }: { total: RollUp }) {
  const ring = "absolute -top-0.5 -right-0.5 rounded-full ring-2 ring-sidebar";
  if (total.waiting) return <StatusDot tone="warn" pulse className={ring} label="Waiting for you" />;
  if (total.running)
    return (
      <span className={cn(ring, "flex size-3.5 items-center justify-center bg-sidebar")}>
        <Spinner className="size-3 text-muted-foreground" aria-label="Running" />
      </span>
    );
  if (total.unread) return <StatusDot tone="info" className={ring} label="Unread replies" />;
  return null;
}

function RailItem({
  group,
  chats,
  states,
  shown,
}: {
  group: RailGroup;
  chats: SessionSummary[];
  states: Record<string, ChatState>;
  shown: boolean;
}) {
  const showWorkspace = useSessionStore((s) => s.showWorkspace);
  const [confirmRemove, setConfirmRemove] = React.useState(false);
  const [changeIcon, setChangeIcon] = React.useState(false);
  const total = rollUp(chats, states);
  const state = summary(total);
  return (
    <ContextMenu>
      <ContextMenuTrigger render={<div className="flex w-full flex-col items-center gap-1" />}>
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                aria-label={state ? `${group.name} (${state})` : group.name}
                aria-current={shown ? "true" : undefined}
                onClick={() => showWorkspace(group.id)}
                className={cn(
                  "no-drag relative flex size-9 items-center justify-center rounded-[10px] transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  // The shown workspace takes a tint of the accent picked in
                  // Settings → Appearance (`--brand`) with a hairline of it;
                  // the others have no fill until hovered.
                  shown
                    ? "bg-brand/15 text-brand shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--brand)_35%,transparent)]"
                    : "text-muted-foreground hover:bg-sidebar-accent/70 hover:text-sidebar-accent-foreground",
                )}
              />
            }
          >
            <WorkspaceIcon group={group} />
            <RailDot total={total} />
          </TooltipTrigger>
          <TooltipContent side="right" className="max-w-72">
            <div className="font-medium">{group.name}</div>
            {group.path && <div className="truncate opacity-70">{group.path}</div>}
            {state && <div className="opacity-70">{state}</div>}
          </TooltipContent>
        </Tooltip>
        {/* The name, so a workspace reads without its tooltip (FR-12). */}
        <span
          aria-hidden
          className={cn(
            "w-[64px] truncate text-center text-[11px] leading-tight",
            shown ? "font-medium text-sidebar-accent-foreground" : "text-muted-foreground",
          )}
        >
          {group.id === LOCAL_GROUP ? LOCAL_SHORT : group.name}
        </span>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-52">
        <WorkspaceMenuItems group={group} onRemove={() => setConfirmRemove(true)} onChangeIcon={() => setChangeIcon(true)} />
      </ContextMenuContent>
      {group.path && <RemoveWorkspaceDialog group={group} open={confirmRemove} onOpenChange={setConfirmRemove} />}
      {group.path && <ChangeIconDialog group={group} open={changeIcon} onOpenChange={setChangeIcon} />}
    </ContextMenu>
  );
}

/**
 * The rail: workspaces A–Z, then "This computer", then Add workspace; the
 * app's own buttons (`footer`: settings, theme) stay at its foot.
 */
export function WorkspaceRail({ onAddWorkspace, footer }: { onAddWorkspace(): void; footer?: React.ReactNode }) {
  const workspaces = useSessionStore((s) => s.workspaces);
  const sessions = useSessionStore((s) => s.sessions);
  const shown = useSessionStore((s) => s.shownWorkspace);
  const states = useChatStates();
  const known = new Set(workspaces.map((w) => w.id));
  const groupOf = (chat: SessionSummary) => (chat.workspaceId && known.has(chat.workspaceId) ? chat.workspaceId : LOCAL_GROUP);
  const live = sessions.filter((chat) => !chat.draft);
  const groups: RailGroup[] = [
    ...workspaces.map((w) => ({ id: w.id, name: w.name, path: w.path })),
    { id: LOCAL_GROUP, name: LOCAL_NAME },
  ];
  return (
    <div className="flex w-[72px] shrink-0 flex-col">
      <nav aria-label="Workspaces" className="flex min-h-0 flex-1 flex-col items-center gap-3 overflow-y-auto pt-1 pb-2">
        {groups.map((group) => (
          <RailItem
            key={group.id}
            group={group}
            chats={live.filter((chat) => groupOf(chat) === group.id)}
            states={states}
            shown={shown === group.id}
          />
        ))}
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                aria-label="Add workspace"
                onClick={onAddWorkspace}
                className="no-drag flex size-9 items-center justify-center rounded-[10px] text-muted-foreground transition-colors outline-none hover:bg-sidebar-accent/70 hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-ring"
              />
            }
          >
            <FolderPlusIcon className="size-4" />
          </TooltipTrigger>
          <TooltipContent side="right">Add workspace</TooltipContent>
        </Tooltip>
      </nav>
      {footer &&<div className="flex flex-col items-center gap-1 pt-1 pb-2">{footer}</div>}
    </div>
  );
}
