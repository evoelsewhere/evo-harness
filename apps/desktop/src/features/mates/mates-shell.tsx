// Evo Mates' frame (docs/features/evo-mates/ux.md section 2): the sidebar —
// search, New, the roster, Channels, a footer with the switch to Evo
// Harness — and the empty state when no mate is open.

import * as React from "react";
import { ArrowLeftRightIcon, MoonIcon, PlusIcon, SearchIcon, SunIcon } from "lucide-react";
import { useTheme } from "@/components/theme-provider";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { openApp } from "@/lib/apps";
import { modKey } from "@/lib/platform";
import { useMatesStore } from "@/stores/mates-store";
import { Roster } from "./roster";

function IconButton({ label, onClick, children }: { label: string; onClick(): void; children: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={label}
            onClick={onClick}
            className="no-drag text-sidebar-foreground"
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

/** The 280 px sidebar on the window's material. */
export function MatesSidebar({ titleBar }: { titleBar: React.ReactNode }) {
  const { resolved, setTheme } = useTheme();
  const showGallery = useMatesStore((s) => s.showGallery);
  const [query, setQuery] = React.useState("");
  const search = React.useRef<HTMLInputElement>(null);

  // Ctrl+K searches the roster, Ctrl+N adds a mate (ux.md 5, FR-38).
  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const mod = modKey === "⌘" ? event.metaKey : event.ctrlKey;
      if (!mod) return;
      if (event.key.toLowerCase() === "k") {
        event.preventDefault();
        search.current?.focus();
      } else if (event.key.toLowerCase() === "n") {
        event.preventDefault();
        showGallery();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [showGallery]);

  return (
    <aside className="flex w-[280px] shrink-0 flex-col bg-sidebar text-sidebar-foreground" aria-label="Mates">
      {titleBar}
      <div className="flex items-center gap-1 px-2">
        <label className="flex h-8 flex-1 items-center gap-2 rounded-lg bg-sidebar-accent/60 px-2.5 text-sm text-muted-foreground">
          <SearchIcon className="size-4 shrink-0" />
          <input
            ref={search}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") setQuery("");
            }}
            placeholder="Search mates…"
            className="no-drag min-w-0 flex-1 bg-transparent text-sidebar-foreground outline-none placeholder:text-muted-foreground"
          />
        </label>
        <IconButton label={`New mate (${modKey} N)`} onClick={showGallery}>
          <PlusIcon />
        </IconButton>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto pb-2">
        <Roster query={query} />
        <section className="px-2 pt-3">
          <h2 className="px-2 pb-1 text-xs text-muted-foreground">Channels</h2>
          <p className="px-2 py-1 text-[13px] text-muted-foreground/80">No channels yet</p>
        </section>
      </div>
      <footer className="flex items-center gap-1 p-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void openApp("harness")}
          className="no-drag h-8 gap-2 px-2 text-[13px] text-sidebar-foreground"
        >
          <ArrowLeftRightIcon className="size-4" />
          Switch to Evo Harness
        </Button>
        <div className="flex-1" />
        <IconButton
          label={resolved === "dark" ? "Light theme" : "Dark theme"}
          onClick={() => setTheme(resolved === "dark" ? "light" : "dark")}
        >
          {resolved === "dark" ? <SunIcon /> : <MoonIcon />}
        </IconButton>
      </footer>
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
