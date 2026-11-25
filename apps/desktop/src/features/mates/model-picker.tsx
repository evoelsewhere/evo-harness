// A mate's model picker (docs/features/evo-mates/ux.md 2): a pill in the
// mate's bar opening a two-pane card — a rail of the providers that have
// models (Cloud, then Local; a dot when one needs signing in again) and the
// chosen provider's models, the suggested ones first, with a search when
// there are many, and the effort at the bottom. Models and providers are the
// engine's (models.dev); nothing here is hard-coded.

import * as React from "react";
import { CheckIcon, ChevronDownIcon, SearchIcon } from "lucide-react";
import { ProviderIcon } from "@/components/provider-icon";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { effortName, modelKey } from "@/components/ai-elements/model-selector";
import { UNCONFIGURED_PROVIDER, type ModelDescriptor, type ProviderRow } from "@/engine/protocol";
import { cn } from "@/lib/utils";
import { useEngineStore, usePaneEngine, useRestoring } from "@/stores/engine-store";
import { useProviderStore } from "@/stores/provider-store";
import { useUiStore } from "@/stores/ui-store";

/** Models shown before "All models". */
const SUGGESTED = 5;
/** A provider with more models than this gets a search field. */
const SEARCH_FROM = 8;

interface Group {
  id: string;
  label: string;
  local: boolean;
  /** Its sign-in is missing or expired. */
  attention: boolean;
  models: ModelDescriptor[];
}

/** The providers that have models, Cloud then Local, each in the engine's order. */
export function providerGroups(models: ModelDescriptor[], rows: ProviderRow[]): Group[] {
  const byId = new Map(rows.map((row) => [row.id, row]));
  const groups = new Map<string, Group>();
  for (const model of models) {
    let group = groups.get(model.provider);
    if (!group) {
      const row = byId.get(model.provider);
      group = {
        id: model.provider,
        label: row?.label ?? model.provider,
        local: row?.kind === "local",
        attention: row?.status === "expired" || row?.status === "missing",
        models: [],
      };
      groups.set(model.provider, group);
    }
    group.models.push(model);
  }
  const all = [...groups.values()];
  return [...all.filter((g) => !g.local), ...all.filter((g) => g.local)];
}

/** A provider's suggested models: the current, the default, pinned ones, then its first. */
export function suggested(
  group: Group,
  current?: string,
  defaultModel?: string,
  pinned: string[] = [],
): ModelDescriptor[] {
  const byKey = new Map(group.models.map((m) => [modelKey(m), m]));
  const keys: string[] = [];
  const add = (key?: string) => {
    if (key && byKey.has(key) && !keys.includes(key) && keys.length < SUGGESTED) keys.push(key);
  };
  add(defaultModel);
  add(current);
  pinned.forEach(add);
  group.models.forEach((m) => add(modelKey(m)));
  return keys.map((key) => byKey.get(key)!);
}

export function MateModelPicker({ mateName }: { mateName: string }) {
  const status = usePaneEngine((s) => s.status);
  const busy = usePaneEngine((s) => s.transcript.busy);
  const models = usePaneEngine((s) => s.models);
  const state = usePaneEngine((s) => s.state);
  const thinkingLevels = usePaneEngine((s) => s.thinkingLevels);
  const defaultModel = usePaneEngine((s) => s.defaultModel);
  const pinned = useUiStore((s) => s.pinnedModels);
  const rows = useProviderStore((s) => s.providers);
  const restoring = useRestoring();
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");

  const unconfigured = state.provider === UNCONFIGURED_PROVIDER;
  const provider = unconfigured ? undefined : state.provider;
  const modelId = unconfigured ? undefined : (state.modelId ?? state.model);
  const current = models.find((m) => m.provider === provider && m.id === modelId);
  const currentKey = current ? modelKey(current) : undefined;
  const loading =
    status === "starting" || (!unconfigured && !provider && !modelId && (restoring || status === "ready"));
  const label = current?.name || modelId || (loading ? "Loading…" : "Choose a model");

  const groups = React.useMemo(() => providerGroups(models, rows), [models, rows]);
  const [shown, setShown] = React.useState<string | undefined>(provider);
  React.useEffect(() => {
    if (open) {
      setShown(provider ?? groups[0]?.id);
      setQuery("");
      if (rows.length === 0) void useProviderStore.getState().load();
    }
    // Only when it opens: a click in the rail changes `shown` after.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
  const group = groups.find((g) => g.id === shown) ?? groups[0];

  const needle = query.trim().toLowerCase();
  const top = group ? suggested(group, currentKey, defaultModel, pinned) : [];
  const rest = group
    ? group.models.filter((m) => !top.includes(m) && (!needle || `${m.name} ${m.id}`.toLowerCase().includes(needle)))
    : [];
  const topShown = needle ? top.filter((m) => `${m.name} ${m.id}`.toLowerCase().includes(needle)) : top;

  const showEffort = thinkingLevels.length > 1 && (current?.reasoning ?? false);
  const { setModel, setThinkingLevel } = useEngineStore.getState();

  const choose = (model: ModelDescriptor) => {
    const key = modelKey(model);
    useUiStore.getState().setPinnedModels(pinned.includes(key) ? pinned : [key, ...pinned]);
    if (key !== currentKey) void setModel(model);
  };

  const row = (model: ModelDescriptor) => {
    const key = modelKey(model);
    const active = key === currentKey;
    return (
      <button
        key={key}
        type="button"
        role="menuitemradio"
        aria-checked={active}
        onClick={() => choose(model)}
        className={cn(
          "flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-[13px] text-foreground transition-colors hover:bg-accent",
          active && "bg-accent",
        )}
      >
        <span className="min-w-0 truncate">{model.name || model.id}</span>
        {key === defaultModel && (
          <span className="shrink-0 rounded-md bg-muted px-1.5 py-px text-[10px] text-muted-foreground">Default</span>
        )}
        <span className="ml-auto flex w-4 shrink-0 justify-center">
          {active && <CheckIcon className="size-4 text-brand" />}
        </span>
      </button>
    );
  };

  const cloud = groups.filter((g) => !g.local);
  const local = groups.filter((g) => g.local);
  const railButton = (g: Group) => (
    <button
      key={g.id}
      type="button"
      aria-label={g.label}
      title={g.label}
      aria-pressed={g.id === group?.id}
      onClick={() => {
        setShown(g.id);
        setQuery("");
      }}
      className={cn(
        "relative flex size-10 items-center justify-center rounded-xl transition-colors hover:bg-background/70",
        g.id === group?.id && "bg-background shadow-[0_0_0_1px_var(--border)]",
      )}
    >
      <ProviderIcon id={g.id} label={g.label} size="sm" />
      {g.attention && (
        <span
          aria-label="Needs signing in"
          className="absolute right-1 bottom-1 size-2 rounded-full bg-amber-500 ring-2 ring-muted"
        />
      )}
    </button>
  );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        disabled={status !== "ready" || busy || models.length === 0}
        render={
          <button
            type="button"
            aria-label={`Model: ${label}`}
            className={cn(
              "no-drag flex h-8 max-w-56 items-center gap-1.5 rounded-full bg-card px-2.5 text-[13px] text-foreground shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent disabled:opacity-60",
              open && "shadow-[0_0_0_1.5px_var(--brand)]",
            )}
          />
        }
      >
        {current && <ProviderIcon id={current.provider} size="xs" />}
        <span className="truncate">{label}</span>
        <ChevronDownIcon
          className={cn("size-3.5 shrink-0 text-muted-foreground transition-transform", open && "rotate-180")}
        />
      </PopoverTrigger>
      <PopoverContent align="end" className="flex h-[440px] w-[400px] gap-0 overflow-hidden rounded-2xl p-0">
        <nav
          className="flex w-14 shrink-0 flex-col items-center gap-1 overflow-y-auto bg-muted/60 py-2"
          aria-label="Providers"
        >
          {cloud.length > 0 && (
            <span className="pb-0.5 text-[9px] font-medium tracking-wider text-muted-foreground uppercase">Cloud</span>
          )}
          {cloud.map(railButton)}
          {local.length > 0 && (
            <span className="pt-2 pb-0.5 text-[9px] font-medium tracking-wider text-muted-foreground uppercase">
              Local
            </span>
          )}
          {local.map(railButton)}
        </nav>
        <div className="flex min-w-0 flex-1 flex-col">
          {group && (
            <div className="flex flex-col gap-0.5 px-4 pt-3.5 pb-2">
              <div className="flex items-center gap-2">
                <span className="truncate text-[15px] font-semibold text-foreground">{group.label}</span>
                <span className="ml-auto shrink-0 rounded-full bg-brand/10 px-2 py-0.5 text-[11px] text-brand">
                  {group.models.length} {group.models.length === 1 ? "model" : "models"}
                </span>
              </div>
              <span className="text-xs text-muted-foreground">Choose a model for {mateName}.</span>
              {group.attention && (
                <span className="text-xs text-amber-600 dark:text-amber-400">
                  Its sign-in has expired: sign in again in Evo Harness → Settings → Providers.
                </span>
              )}
            </div>
          )}
          {group && group.models.length > SEARCH_FROM && (
            <label className="mx-3 mb-1 flex h-8 items-center gap-2 rounded-lg bg-muted/60 px-2.5 text-[13px] text-muted-foreground">
              <SearchIcon className="size-3.5 shrink-0" />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={`Search ${group.label}…`}
                className="min-w-0 flex-1 bg-transparent text-foreground outline-none placeholder:text-muted-foreground"
              />
            </label>
          )}
          <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2" role="menu">
            {topShown.length > 0 && (
              <>
                <div className="px-2.5 pt-1.5 pb-1 text-[10px] font-medium tracking-wider text-muted-foreground uppercase">
                  Suggested
                </div>
                {topShown.map(row)}
              </>
            )}
            {rest.length > 0 && (
              <>
                <div className="px-2.5 pt-3 pb-1 text-[10px] font-medium tracking-wider text-muted-foreground uppercase">
                  All models
                </div>
                {rest.map(row)}
              </>
            )}
            {topShown.length === 0 && rest.length === 0 && (
              <p className="px-2.5 py-6 text-center text-xs text-muted-foreground">No model matches “{query}”.</p>
            )}
          </div>
          {showEffort && (
            <div className="flex items-center gap-2 border-t border-border px-3 py-2">
              <span className="text-xs text-muted-foreground">Effort</span>
              <div className="ml-auto flex rounded-full bg-muted p-0.5" role="radiogroup" aria-label="Effort">
                {thinkingLevels.map((level) => (
                  <button
                    key={level}
                    type="button"
                    role="radio"
                    aria-checked={state.thinkingLevel === level}
                    onClick={() => void setThinkingLevel(level)}
                    className={cn(
                      "rounded-full px-2 py-0.5 text-[11px] text-muted-foreground transition-colors hover:text-foreground",
                      state.thinkingLevel === level && "bg-background text-foreground shadow-[0_0_0_1px_var(--border)]",
                    )}
                  >
                    {effortName(level)}
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
