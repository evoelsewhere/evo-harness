// Plugin views in the app's four slots (docs/features/plugin-ui/spec.md,
// FR-4 – FR-7, FR-15): a sidebar page in the chat area, a panel in the side
// panel, a card in the transcript, and composer actions in a popover.

import * as React from "react";
import { PuzzleIcon, RotateCwIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { InputGroupButton } from "@/components/ui/input-group";
import { Popover as PopoverPrimitive } from "@base-ui/react/popover";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { findView, usePluginUiStore, type PluginViewRef } from "@/stores/plugin-ui-store";
import { useUiStore } from "@/stores/ui-store";
import { PluginSlot } from "./plugin-slot";
import { pluginIcon } from "./plugin-icon";

// ── A page or panel with its header ─────────────────────────────────────────

function ViewFrame({ view, children }: { view: PluginViewRef; children(generation: number): React.ReactNode }) {
  const [generation, setGeneration] = React.useState(0);
  const Icon = pluginIcon(view.icon);
  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border/60 px-3">
        <Icon className="size-4 text-muted-foreground" />
        <span className="truncate text-[13px] font-medium">{view.view.title}</span>
        <span className="truncate text-xs text-muted-foreground">{view.pluginName}</span>
        <span className="flex-1" />
        <Tooltip>
          <TooltipTrigger
            render={<Button variant="ghost" size="icon-sm" aria-label="Reload" onClick={() => setGeneration((n) => n + 1)} />}
          >
            <RotateCwIcon />
          </TooltipTrigger>
          <TooltipContent>Reload</TooltipContent>
        </Tooltip>
      </div>
      <div className="min-h-0 flex-1">{children(generation)}</div>
    </div>
  );
}

function Gone({ title, onBack }: { title: string; onBack?(): void }) {
  return (
    <Empty className="h-full">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <PuzzleIcon />
        </EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        <EmptyDescription>Its plugin is off or was removed. Customize → Plugins turns it on again.</EmptyDescription>
      </EmptyHeader>
      {onBack && (
        <Button size="sm" variant="outline" onClick={onBack}>
          Back to the chat
        </Button>
      )}
    </Empty>
  );
}

/** A plugin's sidebar page, in the chat area (FR-4). */
export function PluginPage() {
  const key = useUiStore((s) => s.pluginPage);
  const views = usePluginUiStore((s) => s.views);
  const loaded = usePluginUiStore((s) => s.loaded);
  const view = key ? findView(views, key, "sidebar") : undefined;
  if (!view) return loaded ? <Gone title="This page is not available" onBack={() => useUiStore.getState().closeCustomize()} /> : null;
  return (
    <ViewFrame view={view}>
      {(generation) => (
        <PluginSlot
          key={view.key}
          plugin={view.plugin}
          pluginName={view.pluginName}
          slot="sidebar"
          view={view.view.id}
          title={view.view.title}
          icon={view.icon}
          generation={generation}
          keepAlive
        />
      )}
    </ViewFrame>
  );
}

/** A plugin's panel, in the side panel (FR-5). */
export function PluginPanel({ panel }: { panel: string }) {
  const views = usePluginUiStore((s) => s.views);
  const loaded = usePluginUiStore((s) => s.loaded);
  const view = findView(views, panel, "panels");
  if (!view) return loaded ? <Gone title="This panel is not available" /> : null;
  return (
    <ViewFrame view={view}>
      {(generation) => (
        <PluginSlot
          key={view.key}
          plugin={view.plugin}
          pluginName={view.pluginName}
          slot="panels"
          view={view.view.id}
          title={view.view.title}
          icon={view.icon}
          generation={generation}
          keepAlive
        />
      )}
    </ViewFrame>
  );
}

// ── Cards (FR-7) ────────────────────────────────────────────────────────────

const CARD_HEIGHT = 240;

/** A card the agent showed (`plugin_show_card`), drawn from the call's result. */
export function PluginCard({ plugin, card, title, data }: { plugin: string; card: string; title?: string; data: unknown }) {
  const views = usePluginUiStore((s) => s.views);
  const loaded = usePluginUiStore((s) => s.loaded);
  const view = views.find((v) => v.plugin === plugin && v.slot === "cards" && v.view.id === card);
  const [height, setHeight] = React.useState(CARD_HEIGHT);
  const name = plugin.split("@")[0];
  if (!view) {
    return (
      <div className="flex w-[36rem] max-w-full items-center gap-2 rounded-lg px-3 py-2 text-xs text-muted-foreground ring-1 ring-border">
        <PuzzleIcon className="size-3.5 shrink-0" />
        {loaded ? `${title ?? "A card"} from ${name}: the plugin is off or was removed.` : (title ?? "Plugin card")}
      </div>
    );
  }
  const Icon = pluginIcon(view.icon);
  return (
    <div className="flex w-[36rem] max-w-full flex-col overflow-hidden rounded-lg ring-1 ring-border">
      <div className="flex h-8 items-center gap-2 border-b border-border/60 px-3 text-xs">
        <Icon className="size-3.5 text-muted-foreground" />
        <span className="truncate font-medium">{view.view.title}</span>
        <span className="truncate text-muted-foreground">{view.pluginName}</span>
      </div>
      <div style={{ height }}>
        <PluginSlot
          plugin={view.plugin}
          pluginName={view.pluginName}
          slot="cards"
          view={view.view.id}
          title={view.view.title}
          icon={view.icon}
          data={data}
          unloadOffscreen
          onHeight={setHeight}
        />
      </div>
    </div>
  );
}

// ── Composer actions (FR-6) ─────────────────────────────────────────────────

const POPOVER = { width: 380, height: 300 };

/** The view of one action, in a popover above the composer, anchored on its button. */
function ComposerPopover({ view, anchor, onClose }: { view: PluginViewRef; anchor: React.RefObject<HTMLElement | null>; onClose(): void }) {
  const [height, setHeight] = React.useState(POPOVER.height);
  return (
    <PopoverPrimitive.Root open onOpenChange={(next) => !next && onClose()}>
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Positioner anchor={anchor} side="top" align="start" sideOffset={6} className="isolate z-50 outline-none">
          <PopoverPrimitive.Popup
            data-slot="popover-content"
            style={{ width: POPOVER.width }}
            className="z-50 overflow-hidden rounded-2xl bg-popover text-popover-foreground shadow-lg ring-1 ring-foreground/5 outline-none dark:ring-foreground/10"
          >
            <div className="flex h-8 items-center gap-2 border-b border-border/60 px-3 text-xs">
              <span className="truncate font-medium">{view.view.title}</span>
              <span className="truncate text-muted-foreground">{view.pluginName}</span>
            </div>
            <div style={{ height }}>
              <PluginSlot
                plugin={view.plugin}
                pluginName={view.pluginName}
                slot="composer"
                view={view.view.id}
                title={view.view.title}
                icon={view.icon}
                onHeight={setHeight}
              />
            </div>
          </PopoverPrimitive.Popup>
        </PopoverPrimitive.Positioner>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  );
}

/**
 * The composer's plugin buttons: one per action, or a "Plugins" menu when
 * there are several. Each opens its view in a popover above the composer.
 */
export function ComposerActions() {
  const views = usePluginUiStore((s) => s.views);
  const actions = React.useMemo(() => views.filter((view) => view.slot === "composer"), [views]);
  const [open, setOpen] = React.useState<string | null>(null);
  const anchor = React.useRef<HTMLSpanElement>(null);
  // A press on the button closes the popover as an outside press first: the click then must not reopen it.
  const closedAt = React.useRef(0);
  const close = () => {
    closedAt.current = performance.now();
    setOpen(null);
  };
  if (actions.length === 0) return null;
  const current = actions.find((view) => view.key === open);
  const single = actions.length === 1 ? actions[0] : null;
  const SingleIcon = single ? pluginIcon(single.icon) : PuzzleIcon;
  return (
    <span ref={anchor} className="inline-flex">
      {single ? (
        <InputGroupButton
          size="sm"
          variant="ghost"
          aria-label={single.view.title}
          aria-expanded={!!current}
          title={`${single.view.title} (${single.pluginName})`}
          onClick={() => {
            if (open) close();
            else if (performance.now() - closedAt.current > 250) setOpen(single.key);
          }}
        >
          <SingleIcon />
        </InputGroupButton>
      ) : (
        <DropdownMenu>
          <DropdownMenuTrigger render={<InputGroupButton size="sm" variant="ghost" aria-label="Plugins" title="Plugins" />}>
            <PuzzleIcon />
          </DropdownMenuTrigger>
          <DropdownMenuContent side="top" align="start" className="w-56">
            <DropdownMenuGroup>
              <DropdownMenuLabel>Plugins</DropdownMenuLabel>
              {actions.map((view) => {
                const Icon = pluginIcon(view.icon);
                return (
                  <DropdownMenuItem key={view.key} onClick={() => setOpen(view.key)}>
                    <Icon className="text-muted-foreground" />
                    <span className="truncate">{view.view.title}</span>
                    <span className="ml-auto truncate text-xs text-muted-foreground">{view.pluginName}</span>
                  </DropdownMenuItem>
                );
              })}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      {current && <ComposerPopover key={current.key} view={current} anchor={anchor} onClose={close} />}
    </span>
  );
}
