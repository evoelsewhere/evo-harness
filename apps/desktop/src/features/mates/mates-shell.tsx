// Evo Mates' sidebar (docs/features/evo-mates-ui-v2/spec.md, FR-1 – FR-3;
// evo-mates ux.md 2): full (280 px) with search, Today and Files, foldable
// Pinned, Mates and Channels sections and a footer; or a rail of faces
// (68 px) in a narrow window or when the person folds it (QA H4).

import * as React from "react";
import {
  ArrowLeftRightIcon,
  ChevronRightIcon,
  FolderIcon,
  HouseIcon,
  MoonIcon,
  PanelLeftCloseIcon,
  PanelLeftOpenIcon,
  PauseIcon,
  PlayIcon,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
  SunIcon,
  UploadIcon,
  XIcon,
} from "lucide-react";
import { useTheme } from "@/components/theme-provider";
import { Button } from "@/components/ui/button";
import { PluginsIcon } from "@/components/icons/tool-icons";
import { Kbd } from "@/components/ui/kbd";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { openApp } from "@/lib/apps";
import { matesApi, type Mate } from "@/lib/mates";
import { modKey } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useMatesStore } from "@/stores/mates-store";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { useSessionStore } from "@/stores/session-store";
import { useUiStore } from "@/stores/ui-store";
import { ChannelSettings } from "./channel-settings";
import { EmptyState } from "./empty-state";
import { MessageHits } from "./message-hits";
import { ChannelRow, ChannelTile, MateRow, MateTile, PinnedTile } from "./roster";
import type { SidebarForm } from "./sidebar-layout";
import { ImportTeam } from "./team-import";

function IconButton({
  label,
  onClick,
  children,
  side = "top",
  className,
}: {
  label: string;
  onClick(): void;
  children: React.ReactNode;
  side?: "top" | "right" | "bottom";
  className?: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button variant="ghost" size="icon-sm" aria-label={label} onClick={onClick} className={cn("no-drag text-muted-foreground hover:text-foreground", className)} />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent side={side}>{label}</TooltipContent>
    </Tooltip>
  );
}

/** How many places wait on the person, and which conversations are unread. */
function useAttention() {
  const mates = useMatesStore((s) => s.mates);
  const channels = useMatesStore((s) => s.channels);
  const work = useSessionStore((s) => s.work);
  const sessions = useSessionStore((s) => s.sessions);
  return React.useMemo(() => {
    const unread = new Set(sessions.filter((c) => c.unread).map((c) => c.id));
    const waiting = (chat: string) => work[chat] === "waiting";
    const channelWaits = (c: (typeof channels)[number]) => waiting(c.chatId) || c.members.some((m) => m.seatChatId != null && waiting(m.seatChatId));
    return {
      needsYou: mates.filter((m) => !m.hidden && waiting(m.chatId)).length + channels.filter((c) => !c.hidden && channelWaits(c)).length,
      unread,
    };
  }, [mates, channels, work, sessions]);
}

/** Today and Files, the sidebar's first rows (evo-mates FR-73, FR-84). */
function HomeRows() {
  const view = useMatesStore((s) => s.view.kind);
  const { needsYou } = useAttention();
  const rows = [
    { id: "today", label: "Today", icon: HouseIcon, active: view === "today" || view === "empty", run: () => useMatesStore.getState().showToday(), badge: needsYou },
    { id: "files", label: "Files", icon: FolderIcon, active: view === "files", run: () => useMatesStore.getState().showFiles(), badge: 0 },
  ];
  return (
    <div className="flex flex-col gap-0.5 px-2 pt-1">
      {rows.map((row) => (
        <button
          key={row.id}
          type="button"
          onClick={row.run}
          aria-current={row.active ? "page" : undefined}
          className={cn(
            "no-drag flex h-8 w-full items-center gap-2.5 rounded-xl px-2.5 text-left text-[13px] font-medium text-sidebar-foreground transition-colors hover:bg-sidebar-accent/70",
            row.active && "bg-sidebar-accent hover:bg-sidebar-accent",
          )}
        >
          <row.icon className="size-4 shrink-0 text-muted-foreground" />
          <span className="flex-1">{row.label}</span>
          {row.badge > 0 && (
            <span
              aria-label={`${row.badge} need you`}
              className="flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-amber-500 px-1.5 text-[11px] font-semibold text-white"
            >
              {row.badge}
            </span>
          )}
        </button>
      ))}
    </div>
  );
}

/** A foldable section with its header (FR-3). */
function Section({
  id,
  title,
  count,
  unread,
  actions,
  children,
}: {
  id: string;
  title: string;
  count: number;
  unread: number;
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  const folded = useMatesUiStore((s) => s.foldedSections.includes(id));
  const toggle = useMatesUiStore((s) => s.toggleSection);
  return (
    <section className="px-2 pt-4" aria-label={title}>
      <div className="group/section flex h-6 items-center pr-1">
        <button
          type="button"
          onClick={() => toggle(id)}
          aria-expanded={!folded}
          className="no-drag flex min-w-0 flex-1 items-center gap-1 rounded-md px-2 text-[11px] font-medium tracking-wide text-muted-foreground uppercase hover:text-foreground"
        >
          <ChevronRightIcon className={cn("size-3 shrink-0 transition-transform", !folded && "rotate-90")} />
          {title}
          <span className="font-normal tracking-normal normal-case opacity-70">{count}</span>
          {folded && unread > 0 && (
            <span className="ml-1 rounded-full bg-brand px-1.5 text-[10px] font-semibold tracking-normal text-brand-foreground normal-case">{unread} new</span>
          )}
        </button>
        <div className="flex items-center opacity-0 transition-opacity group-focus-within/section:opacity-100 group-hover/section:opacity-100">{actions}</div>
      </div>
      {!folded && <div className="mt-0.5 flex flex-col gap-0.5">{children}</div>}
    </section>
  );
}

function matches(mate: Mate, needle: string) {
  return !needle || mate.name.toLowerCase().includes(needle) || mate.role.toLowerCase().includes(needle);
}

/** The full sidebar's lists. */
function FullLists({ query }: { query: string }) {
  const mates = useMatesStore((s) => s.mates);
  const channels = useMatesStore((s) => s.channels);
  const view = useMatesStore((s) => s.view);
  const { unread } = useAttention();
  const [showHidden, setShowHidden] = React.useState(false);
  const needle = query.trim().toLowerCase();
  const active = view.kind === "mate" ? view.id : null;
  const activeChannel = view.kind === "channel" ? view.id : null;
  const shown = mates.filter((m) => !m.hidden && matches(m, needle));
  const pinned = needle ? [] : shown.filter((m) => m.pinned);
  const others = needle ? shown : shown.filter((m) => !m.pinned);
  const hidden = mates.filter((m) => m.hidden && matches(m, needle));
  const shownChannels = channels.filter((c) => !c.hidden && (!needle || c.name.toLowerCase().includes(needle)));
  const ui = useMatesUiStore.getState;
  return (
    <>
      {pinned.length > 0 && (
        <Section id="pinned" title="Pinned" count={pinned.length} unread={pinned.filter((m) => unread.has(m.chatId)).length}>
          <div className="flex flex-wrap gap-0.5">
            {pinned.map((mate) => (
              <PinnedTile key={mate.id} mate={mate} active={mate.id === active} />
            ))}
          </div>
        </Section>
      )}
      <Section
        id="mates"
        title="Mates"
        count={others.length}
        unread={others.filter((m) => unread.has(m.chatId)).length}
        actions={
          <IconButton label={`New mate (${modKey} N)`} onClick={() => useMatesStore.getState().showGallery()} className="size-6">
            <PlusIcon className="size-3.5" />
          </IconButton>
        }
      >
        {others.length === 0 ? (
          needle ? (
            <p className="px-2.5 py-1 text-[13px] text-muted-foreground">No mate matches “{query.trim()}”.</p>
          ) : pinned.length === 0 ? (
            <EmptyState compact text="No mates yet." action="New mate" onAction={() => useMatesStore.getState().showGallery()} />
          ) : null
        ) : (
          others.map((mate) => <MateRow key={mate.id} mate={mate} active={mate.id === active} />)
        )}
        {hidden.length > 0 && (
          <>
            <button
              type="button"
              onClick={() => setShowHidden(!showHidden)}
              className="no-drag flex items-center gap-1 px-2.5 pt-1 text-xs text-muted-foreground hover:text-foreground"
            >
              <ChevronRightIcon className={cn("size-3 transition-transform", showHidden && "rotate-90")} />
              Hidden ({hidden.length})
            </button>
            {showHidden && hidden.map((mate) => <MateRow key={mate.id} mate={mate} active={mate.id === active} />)}
          </>
        )}
      </Section>
      <MessageHits query={query} />
      <Section
        id="channels"
        title="Channels"
        count={shownChannels.length}
        unread={shownChannels.filter((c) => unread.has(c.chatId)).length}
        actions={
          <>
            <IconButton label="Import a team" onClick={() => ui().setDialog("import")} className="size-6">
              <UploadIcon className="size-3.5" />
            </IconButton>
            {mates.length > 0 && (
              <IconButton label="New channel" onClick={() => ui().setDialog("new-channel")} className="size-6">
                <PlusIcon className="size-3.5" />
              </IconButton>
            )}
          </>
        }
      >
        {shownChannels.length === 0 ? (
          needle ? (
            <p className="px-2.5 py-1 text-[13px] text-muted-foreground">No channel matches.</p>
          ) : (
            <EmptyState
              compact
              text={mates.length > 1 ? "Put mates together in a channel." : "Channels need two mates or more."}
              action={mates.length > 1 ? "New channel" : undefined}
              onAction={() => ui().setDialog("new-channel")}
            />
          )
        ) : (
          shownChannels.map((channel) => <ChannelRow key={channel.id} channel={channel} active={channel.id === activeChannel} />)
        )}
      </Section>
    </>
  );
}

/** The rail's tiles (FR-2). */
function RailLists() {
  const mates = useMatesStore((s) => s.mates);
  const channels = useMatesStore((s) => s.channels);
  const view = useMatesStore((s) => s.view);
  const { needsYou } = useAttention();
  const shown = mates.filter((m) => !m.hidden);
  const ordered = [...shown.filter((m) => m.pinned), ...shown.filter((m) => !m.pinned)];
  const homes = [
    { id: "today", label: "Today", icon: HouseIcon, active: view.kind === "today" || view.kind === "empty", run: () => useMatesStore.getState().showToday(), badge: needsYou },
    { id: "files", label: "Files", icon: FolderIcon, active: view.kind === "files", run: () => useMatesStore.getState().showFiles(), badge: 0 },
  ];
  return (
    <div className="flex flex-col items-center gap-1 px-2.5 py-1">
      {homes.map((home) => (
        <Tooltip key={home.id}>
          <TooltipTrigger
            render={
              <button
                type="button"
                onClick={home.run}
                aria-label={home.badge ? `${home.label}, ${home.badge} need you` : home.label}
                aria-current={home.active ? "page" : undefined}
                className={cn(
                  "no-drag relative flex size-10 items-center justify-center rounded-xl text-muted-foreground transition-colors hover:bg-sidebar-accent/70 hover:text-foreground",
                  home.active && "bg-sidebar-accent text-foreground",
                )}
              />
            }
          >
            <home.icon className="size-[18px]" />
            {home.badge > 0 && (
              <span className="absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-amber-500 px-1 text-[10px] font-semibold text-white ring-2 ring-sidebar">
                {home.badge}
              </span>
            )}
          </TooltipTrigger>
          <TooltipContent side="right">{home.label}</TooltipContent>
        </Tooltip>
      ))}
      <span aria-hidden className="my-1.5 h-px w-8 bg-border" />
      {ordered.map((mate) => (
        <MateTile key={mate.id} mate={mate} active={view.kind === "mate" && view.id === mate.id} />
      ))}
      {channels.some((c) => !c.hidden) && <span aria-hidden className="my-1.5 h-px w-8 bg-border" />}
      {channels
        .filter((c) => !c.hidden)
        .map((channel) => (
          <ChannelTile key={channel.id} channel={channel} active={view.kind === "channel" && view.id === channel.id} />
        ))}
    </div>
  );
}

function Footer({ form, forced }: { form: SidebarForm; forced: boolean }) {
  const { resolved, setTheme } = useTheme();
  const mates = useMatesStore((s) => s.mates);
  const allPaused = mates.length > 0 && mates.every((m) => m.paused);
  const rail = form === "rail";
  const side = rail ? "right" : "top";
  const buttons = (
    <>
      <IconButton label="Settings" side={side} onClick={() => useUiStore.getState().openSettings("mates")}>
        <SettingsIcon />
      </IconButton>
      <IconButton label="Customize" side={side} onClick={() => useUiStore.getState().openCustomize()}>
        <PluginsIcon />
      </IconButton>
      {mates.length > 0 && (
        <IconButton label={allPaused ? "Resume all mates" : "Pause all mates"} side={side} onClick={() => void matesApi.pauseAll(!allPaused)}>
          {allPaused ? <PlayIcon /> : <PauseIcon />}
        </IconButton>
      )}
      <IconButton label={resolved === "dark" ? "Light theme" : "Dark theme"} side={side} onClick={() => setTheme(resolved === "dark" ? "light" : "dark")}>
        {resolved === "dark" ? <SunIcon /> : <MoonIcon />}
      </IconButton>
    </>
  );
  if (rail)
    return (
      <footer className="flex flex-col items-center gap-0.5 p-2">
        <IconButton label="Switch to Evo Harness" side="right" onClick={() => void openApp("harness")}>
          <ArrowLeftRightIcon />
        </IconButton>
        {buttons}
        {!forced && (
          <IconButton label={`Unfold the sidebar (${modKey} B)`} side="right" onClick={() => useMatesUiStore.getState().toggleSidebar()}>
            <PanelLeftOpenIcon />
          </IconButton>
        )}
      </footer>
    );
  return (
    <footer className="flex items-center gap-0.5 p-2">
      <Button variant="ghost" size="sm" onClick={() => void openApp("harness")} className="no-drag h-8 gap-2 px-2 text-[13px] text-sidebar-foreground">
        <ArrowLeftRightIcon className="size-4" />
        Evo Harness
      </Button>
      <div className="flex-1" />
      {buttons}
    </footer>
  );
}

/** The sidebar in its form (FR-1): full or rail. */
export function MatesSidebar({ titleBar, form, forced }: { titleBar: React.ReactNode; form: SidebarForm; forced: boolean }) {
  const [query, setQuery] = React.useState("");
  const search = React.useRef<HTMLInputElement>(null);
  const dialog = useMatesUiStore((s) => s.dialog);
  const setDialog = useMatesUiStore((s) => s.setDialog);
  const rail = form === "rail";

  return (
    <aside
      className={cn("flex shrink-0 flex-col bg-sidebar text-sidebar-foreground transition-[width] duration-200", rail ? "w-[68px]" : "w-[280px]")}
      aria-label="Mates"
    >
      {titleBar}
      {rail ? (
        <div className="flex flex-col items-center gap-1 pb-1">
          <IconButton label={`Go to… (${modKey} K)`} side="right" onClick={() => useMatesUiStore.getState().setSwitcherOpen(true)}>
            <SearchIcon />
          </IconButton>
          <IconButton label={`New mate (${modKey} N)`} side="right" onClick={() => useMatesStore.getState().showGallery()}>
            <PlusIcon />
          </IconButton>
        </div>
      ) : (
        <div className="flex items-center gap-1 px-2 pb-1">
          <label className="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg bg-sidebar-accent/60 pr-1 pl-2.5 text-sm text-muted-foreground focus-within:bg-sidebar-accent">
            <SearchIcon className="size-4 shrink-0" />
            <input
              ref={search}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  event.preventDefault();
                  setQuery("");
                  search.current?.blur();
                }
              }}
              aria-label="Filter mates, channels and messages"
              placeholder="Filter…"
              className="no-drag min-w-0 flex-1 bg-transparent text-[13px] text-sidebar-foreground outline-none placeholder:text-muted-foreground"
            />
            {query ? (
              <button type="button" aria-label="Clear search" onClick={() => setQuery("")} className="no-drag rounded-full p-0.5 hover:text-foreground">
                <XIcon className="size-3.5" />
              </button>
            ) : (
              <button
                type="button"
                aria-label={`Go to… (${modKey} K)`}
                onClick={() => useMatesUiStore.getState().setSwitcherOpen(true)}
                className="no-drag rounded-md"
              >
                <Kbd className="pointer-events-auto bg-background/70">{modKey} K</Kbd>
              </button>
            )}
          </label>
          <IconButton label={`New mate (${modKey} N)`} onClick={() => useMatesStore.getState().showGallery()}>
            <PlusIcon />
          </IconButton>
          <IconButton label={`Fold the sidebar (${modKey} B)`} onClick={() => useMatesUiStore.getState().toggleSidebar()}>
            <PanelLeftCloseIcon />
          </IconButton>
        </div>
      )}
      <div className={cn("min-h-0 flex-1 overflow-x-hidden overflow-y-auto pb-2", rail && "[scrollbar-width:none]")}>
        {rail ? (
          <RailLists />
        ) : (
          <>
            {!query && <HomeRows />}
            <FullLists query={query} />
          </>
        )}
      </div>
      <Footer form={form} forced={forced} />
      <ChannelSettings open={dialog === "new-channel"} onOpenChange={(open) => setDialog(open ? "new-channel" : null)} />
      <ImportTeam open={dialog === "import"} onOpenChange={(open) => setDialog(open ? "import" : null)} />
    </aside>
  );
}

/** The conversation area when no mate is open. */
export function MatesEmpty() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      <img src="/mates.png" alt="" className="size-20 rounded-2xl" draggable={false} />
      <h1 className="text-xl font-semibold text-foreground">Pick a mate</h1>
      <p className="max-w-sm text-sm text-muted-foreground">Choose a mate on the left to talk to, or add a new one.</p>
    </div>
  );
}
