import * as React from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderPlusIcon, MoonIcon, MoreHorizontalIcon, SearchIcon, SettingsIcon, SquarePenIcon, SunIcon } from "lucide-react";
import { useTheme } from "@/components/theme-provider";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Kbd } from "@/components/ui/kbd";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { inTauri, isMac, modKey } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { LOCAL_GROUP, useSessionStore, type SessionSummary } from "@/stores/session-store";
import { ArchivedGroup, ChatList, WorkspaceGroup } from "./sidebar-chats";
import { useUiStore } from "@/stores/ui-store";
import { EditSidebarDialog } from "./edit-sidebar-dialog";
import { SIDEBAR_ITEMS, isShown } from "./sidebar-items";
import { SidebarResizer } from "./sidebar-resizer";
import { TitleBarActions } from "./title-bar";

function IconButton({
  label,
  onClick,
  children,
  className,
}: {
  label: string;
  onClick?(): void;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={label}
            onClick={onClick}
            className={cn("no-drag text-sidebar-foreground", className)}
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

/**
 * Navigation on the system material (Mica / vibrancy): brand and new chat,
 * search, tools, chats grouped by workspace, engine status and theme.
 * The right edge drags to resize.
 */
export function AppSidebar() {
  const workspaces = useSessionStore((s) => s.workspaces);
  const sessions = useSessionStore((s) => s.sessions);
  const newChat = useEngineStore((s) => s.newChat);
  const openWorkspace = useEngineStore((s) => s.openWorkspace);
  const setCommandOpen = useUiStore((s) => s.setCommandOpen);
  const openSettings = useUiStore((s) => s.openSettings);
  const { resolved, setTheme } = useTheme();
  // Page rows: the items the person shows; the others are in More (docs/features/sidebar-items/spec.md).
  const saved = useUiStore((s) => s.sidebarShown);
  const activeId = useUiStore((s) => SIDEBAR_ITEMS.find((item) => item.isActive(s))?.id ?? null);
  const [editOpen, setEditOpen] = React.useState(false);
  React.useEffect(() => {
    void useUiStore.getState().loadSidebarItems();
  }, []);
  const shown = SIDEBAR_ITEMS.filter((item) => isShown(item, saved));
  const hidden = SIDEBAR_ITEMS.filter((item) => !isShown(item, saved));
  const hiddenActive = hidden.some((item) => item.id === activeId);

  const byUpdated = (a: SessionSummary, b: SessionSummary) => b.updatedAt - a.updatedAt;
  // Nested sessions show under their parent; at the top level only when it is
  // not among the live chats (archived: the archive lists its chats flat).
  const ids = new Set(sessions.map((s) => s.id));
  const topLevel = sessions.filter((s) => !s.parentChatId || !ids.has(s.parentChatId));
  const pinned = topLevel.filter((s) => s.pinned).sort(byUpdated);
  const unpinned = topLevel.filter((s) => !s.pinned);
  const local = unpinned.filter((s) => s.workspaceId === null).sort(byUpdated);

  async function addWorkspace() {
    const folder = await open({ directory: true, multiple: false, title: "Add a workspace folder" });
    if (typeof folder === "string") await openWorkspace(folder);
  }

  return (
    <Sidebar className="border-r-0 group-data-[side=left]:border-r-0">
      {/* Title bar: the window's own buttons, after the macOS traffic lights. Empty space drags the window. */}
      <div
        data-tauri-drag-region
        className={cn("drag-region flex h-10 shrink-0 items-center px-2", isMac && inTauri && "pl-[76px]")}
      >
        <TitleBarActions />
      </div>
      <SidebarHeader className="gap-2 pt-0">
        <button
          type="button"
          onClick={() => setCommandOpen(true)}
          className="flex h-8 items-center gap-2 rounded-lg bg-sidebar-accent/60 px-2.5 text-sm text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
        >
          <SearchIcon className="size-4 shrink-0" />
          <span className="flex-1 truncate text-left">Search chats…</span>
          <Kbd>{modKey} P</Kbd>
        </button>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton onClick={() => void newChat()} className="h-7 text-[13px]">
              <SquarePenIcon />
              <span>New chat</span>
            </SidebarMenuButton>
            <SidebarMenuBadge className="font-normal text-muted-foreground">{modKey} N</SidebarMenuBadge>
          </SidebarMenuItem>
          {shown.map((item) => (
            <SidebarMenuItem key={item.id}>
              <SidebarMenuButton onClick={item.open} isActive={activeId === item.id} className="h-7 text-[13px]">
                <item.icon />
                <span>{item.label}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          ))}
          <SidebarMenuItem>
            <DropdownMenu>
              <DropdownMenuTrigger
                render={<SidebarMenuButton isActive={hiddenActive} className="h-7 text-[13px] aria-expanded:bg-sidebar-accent" />}
              >
                <MoreHorizontalIcon />
                <span>More</span>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" side="right" className="w-48">
                {hidden.map((item) => (
                  <DropdownMenuItem key={item.id} onClick={item.open}>
                    <item.icon />
                    {item.label}
                  </DropdownMenuItem>
                ))}
                {hidden.length > 0 && <DropdownMenuSeparator />}
                <DropdownMenuItem onClick={() => setEditOpen(true)}>Edit sidebar…</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </SidebarMenuItem>
        </SidebarMenu>
        <EditSidebarDialog open={editOpen} onOpenChange={setEditOpen} />
      </SidebarHeader>

      <SidebarContent className="gap-0">
        {pinned.length > 0 && (
          <SidebarGroup className="py-0.5">
            <SidebarGroupLabel className="h-7 text-xs font-normal text-muted-foreground">Pinned</SidebarGroupLabel>
            <SidebarGroupContent>
              <ChatList chats={pinned} />
            </SidebarGroupContent>
          </SidebarGroup>
        )}
        <div className="flex items-center justify-between px-4 pt-2 pb-1">
          <span className="text-xs text-muted-foreground">Workspaces</span>
          <IconButton label="Add workspace" onClick={() => void addWorkspace()} className="size-6 text-muted-foreground">
            <FolderPlusIcon />
          </IconButton>
        </div>

        {workspaces.map((workspace) => (
          <WorkspaceGroup
            key={workspace.id}
            id={workspace.id}
            name={workspace.name}
            path={workspace.path}
            chats={unpinned.filter((s) => s.workspaceId === workspace.id).sort(byUpdated)}
          />
        ))}
        <WorkspaceGroup id={LOCAL_GROUP} name="This computer" chats={local} />
        <ArchivedGroup />
      </SidebarContent>

      <SidebarFooter className="flex-row items-center gap-1">
        <IconButton label={`Settings (${modKey} ,)`} onClick={() => openSettings()}>
          <SettingsIcon />
        </IconButton>
        <div className="flex-1" />
        <IconButton
          label={resolved === "dark" ? "Light theme" : "Dark theme"}
          onClick={() => setTheme(resolved === "dark" ? "light" : "dark")}
        >
          {resolved === "dark" ? <SunIcon /> : <MoonIcon />}
        </IconButton>
      </SidebarFooter>

      <SidebarResizer />
    </Sidebar>
  );
}
