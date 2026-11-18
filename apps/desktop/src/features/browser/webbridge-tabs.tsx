// A chat's tabs in the person's own Chrome or Edge (Evo Webbridge), shown in
// the Browser panel with Take back (docs/features/browser-bridge/spec.md
// FR-7, FR-8). The pages themselves are in the person's browser, not here.

import * as React from "react";
import { GlobeIcon, UndoIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useEngineStore } from "@/stores/engine-store";
import { useWebbridgeStore } from "@/stores/webbridge-store";

export function WebbridgeTabs() {
  const chat = useEngineStore((s) => s.activeId);
  const tabs = useWebbridgeStore((s) => (chat ? s.chatTabs[chat] : undefined));
  const loadChatTabs = useWebbridgeStore((s) => s.loadChatTabs);
  const release = useWebbridgeStore((s) => s.release);

  React.useEffect(() => {
    if (chat) void loadChatTabs(chat);
  }, [chat, loadChatTabs]);

  if (!chat || !tabs || tabs.length === 0) return null;
  return (
    <div className="shrink-0 border-b border-border/60 px-2 py-1.5">
      <p className="px-1 pb-1 text-[11px] text-muted-foreground">In your browser</p>
      <ul className="flex flex-col gap-0.5">
        {tabs.map((tab) => (
          <li key={tab.id} className="flex items-center gap-2 rounded-md px-1 py-0.5 text-xs hover:bg-muted/60">
            <GlobeIcon className="size-3.5 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate" title={tab.url}>
              {tab.title || tab.url}
            </span>
            <span className="shrink-0 text-[11px] text-muted-foreground">{tab.taken ? `${tab.browser} · you have control` : tab.browser}</span>
            <Tooltip>
              <TooltipTrigger
                render={<Button variant="ghost" size="icon-xs" aria-label={`Take back ${tab.title || tab.url}`} onClick={() => void release(chat, tab.id)} />}
              >
                <UndoIcon />
              </TooltipTrigger>
              <TooltipContent>Take back: the agent can no longer use this tab</TooltipContent>
            </Tooltip>
          </li>
        ))}
      </ul>
    </div>
  );
}
