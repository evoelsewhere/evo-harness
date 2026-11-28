// A mate's header (docs/features/evo-mates-ui-v2/spec.md, FR-9; evo-mates
// ux.md 2): its live face with presence, its name, and its state word or
// role; on the right Find, the model pill, Resume when paused, the profile
// toggle and the ⋯ menu.

import * as React from "react";
import { PanelRightIcon, PlayIcon, SearchIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { Mate } from "@/lib/mates";
import { modKey } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { MatePresenceFace, usePresence } from "./presence-face";
import { MateMenuButton } from "./roster";
import { currentStep } from "./thread-rows";

export function HeaderButton({ label, onClick, active, children }: { label: string; onClick(): void; active?: boolean; children: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={label}
            aria-pressed={active}
            onClick={onClick}
            className={cn("no-drag rounded-full text-muted-foreground hover:text-foreground", active && "bg-accent text-foreground")}
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function MateHeader({ mate, modelPill }: { mate: Mate; modelPill?: React.ReactNode }) {
  const { presence, word } = usePresence(mate);
  const step = useEngineStore((s) => {
    const turn = s.runtimes[mate.chatId]?.transcript.turns.at(-1);
    return turn && turn.status === "running" ? currentStep(turn) : null;
  });
  const panelOpen = useMatesStore((s) => s.panelOpen);
  const setPanelOpen = useMatesStore((s) => s.setPanelOpen);
  const findOpen = useMatesUiStore((s) => s.findOpen);
  const line = presence === "working" ? (step ?? word) : (word ?? mate.role);
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2 pr-2 pl-4">
      <button
        type="button"
        aria-label={`${mate.name}: profile`}
        onClick={() => setPanelOpen(!panelOpen)}
        className="no-drag group flex min-w-0 items-center gap-2.5 rounded-xl py-1 pr-2 pl-1 transition-colors hover:bg-accent/60"
      >
        <MatePresenceFace mate={mate} size={32} live ring="var(--background)" />
        <span className="flex min-w-0 flex-col text-left">
          <span className="truncate text-[14px] leading-[18px] font-semibold text-foreground">{mate.name}</span>
          <span
            className={cn(
              "truncate text-xs leading-4",
              presence === "working" ? "text-brand" : presence === "waiting" ? "font-medium text-foreground" : "text-muted-foreground",
            )}
          >
            {line}
          </span>
        </span>
      </button>
      <div className="flex-1" />
      <div className="flex shrink-0 items-center gap-0.5">
        {mate.paused && (
          <Button
            size="sm"
            variant="secondary"
            className="no-drag mr-1 h-7 gap-1 rounded-full px-2.5 text-xs"
            onClick={() => void useMatesStore.getState().update(mate.id, { paused: false })}
          >
            <PlayIcon className="size-3.5" />
            Resume
          </Button>
        )}
        <HeaderButton label={`Find (${modKey} F)`} active={findOpen} onClick={() => useMatesUiStore.getState().setFindOpen(!findOpen)}>
          <SearchIcon />
        </HeaderButton>
        {modelPill}
        <HeaderButton label={`${panelOpen ? "Close profile" : "Profile"} (${modKey} I)`} active={panelOpen} onClick={() => setPanelOpen(!panelOpen)}>
          <PanelRightIcon />
        </HeaderButton>
        <MateMenuButton mate={mate} className="size-8" />
      </div>
    </div>
  );
}
