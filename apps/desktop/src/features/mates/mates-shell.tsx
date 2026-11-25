// Evo Mates' frame (docs/features/evo-mates/ux.md section 2): the sidebar —
// search, New, the Mates and Channels lists, a footer with the switch to Evo
// Harness — and the empty state before any mate exists. Phase 1 of the
// tasks: the roster is empty; creating mates comes with phase 2.

import * as React from "react";
import { ArrowLeftRightIcon, MoonIcon, PlusIcon, SearchIcon, SunIcon } from "lucide-react";
import { useTheme } from "@/components/theme-provider";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { openApp } from "@/lib/apps";

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

function Section({ title, empty }: { title: string; empty: string }) {
  return (
    <section className="px-2 pt-3">
      <h2 className="px-2 pb-1 text-xs text-muted-foreground">{title}</h2>
      <p className="px-2 py-1 text-[13px] text-muted-foreground/80">{empty}</p>
    </section>
  );
}

/** The 280 px sidebar on the window's material. */
export function MatesSidebar({ titleBar }: { titleBar: React.ReactNode }) {
  const { resolved, setTheme } = useTheme();
  return (
    <aside className="flex w-[280px] shrink-0 flex-col bg-sidebar text-sidebar-foreground" aria-label="Mates">
      {titleBar}
      <div className="flex flex-col gap-2 px-2">
        <div className="flex items-center gap-1">
          <div className="flex h-8 flex-1 items-center gap-2 rounded-lg bg-sidebar-accent/60 px-2.5 text-sm text-muted-foreground">
            <SearchIcon className="size-4 shrink-0" />
            <span className="flex-1 truncate">Search mates…</span>
          </div>
          <Tooltip>
            <TooltipTrigger
              render={
                <Button variant="ghost" size="icon-sm" aria-label="New mate" disabled className="text-sidebar-foreground" />
              }
            >
              <PlusIcon />
            </TooltipTrigger>
            <TooltipContent>New mate</TooltipContent>
          </Tooltip>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <Section title="Mates" empty="No mates yet" />
        <Section title="Channels" empty="No channels yet" />
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

/** The conversation area before any mate exists. */
export function MatesEmpty() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      <img src="/mates.png" alt="" className="size-20 rounded-2xl" draggable={false} />
      <h1 className="text-xl font-semibold text-foreground">Meet your mates</h1>
      <p className="max-w-sm text-sm text-muted-foreground">
        AI teammates you talk to like colleagues, each with a role, a face and a memory of their own. Your mates
        will show here.
      </p>
    </div>
  );
}
