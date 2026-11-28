// The quick switcher (docs/features/evo-mates-ui-v2/spec.md, FR-7): Ctrl+K
// over the window. Mates, channels, pages and actions, ranked as the person
// types; from two characters, the messages that match too (evo-mates FR-76).

import * as React from "react";
import {
  ArrowLeftRightIcon,
  FolderIcon,
  HashIcon,
  HouseIcon,
  KeyboardIcon,
  MoonIcon,
  PanelLeftIcon,
  PauseIcon,
  PlayIcon,
  PlusIcon,
  SettingsIcon,
  UploadIcon,
  UserPlusIcon,
} from "lucide-react";
import { PluginsIcon } from "@/components/icons/tool-icons";
import { useTheme } from "@/components/theme-provider";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { openApp } from "@/lib/apps";
import { formatRelative } from "@/lib/format";
import { matesApi, type MessageHit } from "@/lib/mates";
import { modKey } from "@/lib/platform";
import { useMatesStore } from "@/stores/mates-store";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { useSessionStore } from "@/stores/session-store";
import { useUiStore } from "@/stores/ui-store";
import { MatePresenceFace } from "./presence-face";
import { rankSwitcher, type SwitcherItem } from "./switcher-items";

interface Action {
  id: string;
  label: string;
  keywords: string[];
  icon: React.ComponentType<{ className?: string }>;
  shortcut?: string;
  run(): void;
}

function useActions(): Action[] {
  const { resolved, setTheme } = useTheme();
  const allPaused = useMatesStore((s) => s.mates.length > 0 && s.mates.every((m) => m.paused));
  const ui = useMatesUiStore.getState;
  const store = useMatesStore.getState;
  return [
    { id: "new-mate", label: "New mate", keywords: ["add", "hire", "gallery", "role"], icon: UserPlusIcon, shortcut: `${modKey} N`, run: () => store().showGallery() },
    { id: "new-channel", label: "New channel", keywords: ["add", "create", "group"], icon: PlusIcon, run: () => ui().setDialog("new-channel") },
    { id: "import", label: "Import a team", keywords: ["team", "file", "open"], icon: UploadIcon, run: () => ui().setDialog("import") },
    { id: "today", label: "Today", keywords: ["home", "inbox", "needs you"], icon: HouseIcon, run: () => store().showToday() },
    { id: "files", label: "Files", keywords: ["documents", "pictures"], icon: FolderIcon, run: () => store().showFiles() },
    { id: "settings", label: "Settings", keywords: ["preferences", "options"], icon: SettingsIcon, run: () => useUiStore.getState().openSettings("mates") },
    { id: "customize", label: "Customize", keywords: ["skills", "apps", "connectors", "plugins"], icon: PluginsIcon, run: () => useUiStore.getState().openCustomize() },
    {
      id: "pause-all",
      label: allPaused ? "Resume all mates" : "Pause all mates",
      keywords: ["stop", "pause", "resume"],
      icon: allPaused ? PlayIcon : PauseIcon,
      run: () => void matesApi.pauseAll(!allPaused),
    },
    { id: "theme", label: resolved === "dark" ? "Light theme" : "Dark theme", keywords: ["theme", "dark", "light", "appearance"], icon: MoonIcon, run: () => setTheme(resolved === "dark" ? "light" : "dark") },
    { id: "fold", label: "Fold or unfold the sidebar", keywords: ["sidebar", "rail", "collapse"], icon: PanelLeftIcon, shortcut: `${modKey} B`, run: () => ui().toggleSidebar() },
    { id: "shortcuts", label: "Keyboard shortcuts", keywords: ["keys", "help"], icon: KeyboardIcon, shortcut: `${modKey} /`, run: () => ui().setShortcutsOpen(true) },
    { id: "harness", label: "Switch to Evo Harness", keywords: ["harness", "app"], icon: ArrowLeftRightIcon, run: () => void openApp("harness") },
  ];
}

function useMessageHits(query: string): MessageHit[] {
  const [hits, setHits] = React.useState<MessageHit[]>([]);
  React.useEffect(() => {
    const text = query.trim();
    if (text.length < 2) {
      setHits([]);
      return;
    }
    let gone = false;
    const timer = window.setTimeout(() => {
      void matesApi
        .search(text)
        .then((found) => !gone && setHits(found.slice(0, 8)))
        .catch(() => !gone && setHits([]));
    }, 180);
    return () => {
      gone = true;
      window.clearTimeout(timer);
    };
  }, [query]);
  return hits;
}

export function QuickSwitcher() {
  const open = useMatesUiStore((s) => s.switcherOpen);
  const setOpen = useMatesUiStore((s) => s.setSwitcherOpen);
  const mates = useMatesStore((s) => s.mates);
  const channels = useMatesStore((s) => s.channels);
  const rows = useSessionStore((s) => s.sessions);
  const actions = useActions();
  const [query, setQuery] = React.useState("");
  const hits = useMessageHits(open ? query : "");

  React.useEffect(() => {
    if (!open) setQuery("");
  }, [open]);

  const at = React.useMemo(() => new Map(rows.map((r) => [r.id, r.updatedAt])), [rows]);
  const items = React.useMemo<SwitcherItem[]>(
    () => [
      ...mates.filter((m) => !m.hidden).map((m) => ({ key: `mate:${m.id}`, kind: "mate" as const, label: m.name, detail: m.role, at: at.get(m.chatId) })),
      ...channels
        .filter((c) => !c.hidden)
        .map((c) => ({
          key: `channel:${c.id}`,
          kind: "channel" as const,
          label: c.name,
          detail: c.members.map((x) => mates.find((m) => m.id === x.mateId)?.name).filter(Boolean).join(", "),
          at: at.get(c.chatId),
        })),
      ...actions.map((a) => ({ key: `action:${a.id}`, kind: "action" as const, label: a.label, keywords: a.keywords })),
    ],
    [mates, channels, actions, at],
  );
  const ranked = rankSwitcher(items, query);
  const places = ranked.filter((i) => i.kind !== "action");
  const shownActions = ranked.filter((i) => i.kind === "action");

  const go = (key: string) => {
    setOpen(false);
    const [kind, id] = [key.slice(0, key.indexOf(":")), key.slice(key.indexOf(":") + 1)];
    const store = useMatesStore.getState();
    if (kind === "mate") void store.open(id);
    else if (kind === "channel") void store.openChannel(id);
    else actions.find((a) => a.id === id)?.run();
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogHeader className="sr-only">
        <DialogTitle>Go to</DialogTitle>
        <DialogDescription>Open a mate, a channel or an action</DialogDescription>
      </DialogHeader>
      <DialogContent className="top-1/4 translate-y-0 overflow-hidden rounded-3xl! p-0 sm:max-w-xl" showCloseButton={false}>
      {/* The list is ranked here (switcher-items.ts), not by cmdk. */}
      <Command shouldFilter={false} loop>
      <CommandInput placeholder="Go to a mate, a channel or an action…" value={query} onValueChange={setQuery} />
      <CommandList className="max-h-[min(60vh,440px)]">
        <CommandEmpty>Nothing matches “{query.trim()}”.</CommandEmpty>
        {places.length > 0 && (
          <CommandGroup heading={query.trim() ? "Mates and channels" : "Recent"}>
            {places.map((item) => {
              const mate = item.kind === "mate" ? mates.find((m) => `mate:${m.id}` === item.key) : undefined;
              return (
                <CommandItem key={item.key} value={item.key} onSelect={() => go(item.key)} className="gap-2.5 py-2">
                  {mate ? (
                    <MatePresenceFace mate={mate} size={26} ring="var(--popover)" />
                  ) : (
                    <span className="flex size-[26px] items-center justify-center rounded-full bg-muted text-muted-foreground">
                      <HashIcon className="size-3.5" />
                    </span>
                  )}
                  <span className="min-w-0 flex-1 truncate">
                    <span className="font-medium text-foreground">{item.label}</span>
                    {item.detail && <span className="ml-2 text-xs text-muted-foreground">{item.detail}</span>}
                  </span>
                  <span className="text-[11px] text-muted-foreground">{item.kind === "mate" ? "Mate" : "Channel"}</span>
                </CommandItem>
              );
            })}
          </CommandGroup>
        )}
        {shownActions.length > 0 && (
          <CommandGroup heading="Actions">
            {shownActions.map((item) => {
              const action = actions.find((a) => `action:${a.id}` === item.key)!;
              return (
                <CommandItem key={item.key} value={item.key} onSelect={() => go(item.key)}>
                  <action.icon />
                  {action.label}
                  {action.shortcut && <CommandShortcut>{action.shortcut}</CommandShortcut>}
                </CommandItem>
              );
            })}
          </CommandGroup>
        )}
        {hits.length > 0 && (
          <CommandGroup heading="Messages">
            {hits.map((hit) => {
              const mate = hit.kind === "mate" ? mates.find((m) => m.chatId === hit.chatId) : undefined;
              const channel = hit.kind === "channel" ? channels.find((c) => c.chatId === hit.chatId) : undefined;
              if (!mate && !channel) return null;
              const key = mate ? `mate:${mate.id}` : `channel:${channel!.id}`;
              return (
                <CommandItem key={`hit:${hit.chatId}`} value={`hit:${hit.chatId}`} onSelect={() => go(key)} className="items-start gap-2.5 py-2">
                  {mate ? (
                    <MatePresenceFace mate={mate} size={22} ring="var(--popover)" />
                  ) : (
                    <HashIcon className="mt-0.5 text-muted-foreground" />
                  )}
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="flex items-center gap-2 text-[13px] font-medium">
                      <span className="truncate">{mate ? mate.name : `#${channel!.name}`}</span>
                      <span className="ml-auto shrink-0 text-[11px] font-normal text-muted-foreground">{formatRelative(hit.updatedAt)}</span>
                    </span>
                    <span className="line-clamp-2 text-xs text-muted-foreground">{hit.snippet}</span>
                  </span>
                </CommandItem>
              );
            })}
          </CommandGroup>
        )}
      </CommandList>
      </Command>
      </DialogContent>
    </Dialog>
  );
}
