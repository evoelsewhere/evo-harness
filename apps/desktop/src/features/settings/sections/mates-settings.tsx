import { useMatesPrefs } from "@/stores/mates-prefs-store";
import * as React from "react";
import { ChevronRightIcon, UsersRoundIcon } from "lucide-react";
import { modelKey } from "@/components/ai-elements/model-selector";
import { ProviderIcon } from "@/components/provider-icon";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { db, log } from "@/lib/db";
import { channelsApi } from "@/lib/mates";
import { inTauri } from "@/lib/platform";
import { useEngineStore } from "@/stores/engine-store";
import { SettingsGroup, SettingsHeader, SettingsRow } from "../settings-controls";
import { LearningSettings } from "./learning-settings";

// Settings → Evo Mates (docs/features/evo-mates/spec.md, FR-18): how many
// times mates in a channel may hand work to each other for one message
// before every mate stops, and the model that picks who answers under Auto
// (FR-28). Saved in the app settings; every channel reads them again at
// once (`channels_reload`, `src-tauri/src/mates/channels.rs`).

const HANDOFF_LIMIT = "mates.handoffLimit";
const DEFAULT = 12;
const MAX_RUNNING = "mates.maxRunning";
const RUNNING_DEFAULT = 4;
const ROUTER_MODEL = "mates.routerModel";
/** Dollars a mate may spend a day (FR-80); none: no limit. */
const DAILY_LIMIT = "mates.dailyLimitUsd";

type RouterChoice = { provider: string; model: string } | null;

let reloadTimer: ReturnType<typeof setTimeout> | undefined;

/** Channels read the settings again once a slider rests; a failure is only logged. */
function reloadChannels() {
  clearTimeout(reloadTimer);
  reloadTimer = setTimeout(() => {
    void channelsApi.reload().catch((error) => log("warn", "ui", `could not apply to channels: ${String(error)}`));
  }, 600);
}

/** The daily spending limit per mate (FR-80): off by default; a mate that reaches it pauses until you resume it. */
function DailyLimit() {
  const [limit, setLimit] = React.useState<number | null>(null);
  const [draft, setDraft] = React.useState("2.00");
  React.useEffect(() => {
    if (!inTauri) return;
    void db
      .settings()
      .then((all) => {
        const saved = all[DAILY_LIMIT];
        if (typeof saved === "number" && saved > 0) {
          setLimit(saved);
          setDraft(saved.toFixed(2));
        }
      })
      .catch(() => undefined);
  }, []);
  const save = (value: number | null) => {
    setLimit(value);
    void db.setSetting(DAILY_LIMIT, value).catch((error) => log("error", "ui", `could not save the daily limit: ${String(error)}`));
  };
  const fromDraft = () => {
    const value = Number.parseFloat(draft.replace(/[^0-9.]/g, ""));
    return Number.isFinite(value) && value > 0 ? Math.round(value * 100) / 100 : null;
  };
  return (
    <SettingsRow
      labelId="mates-daily-limit"
      label="Daily spending limit per mate"
      description="When a mate's cost today reaches it, the mate pauses and tells you. Resume lets it go on for the rest of the day."
      control={
        <div className="flex items-center gap-2">
          {limit !== null && (
            <Input
              aria-label="Dollars a day"
              className="h-8 w-20 text-right"
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              onBlur={() => {
                const value = fromDraft();
                if (value) save(value);
                else setDraft(limit.toFixed(2));
              }}
            />
          )}
          <Switch aria-labelledby="mates-daily-limit" checked={limit !== null} onCheckedChange={(on) => save(on ? (fromDraft() ?? 2) : null)} />
        </div>
      }
    />
  );
}

/** Link previews in Evo Mates (evo-mates-messaging FR-17): off by default. */
function LinkPreviews() {
  const on = useMatesPrefs((s) => s.linkPreviews);
  React.useEffect(() => {
    void useMatesPrefs.getState().load();
  }, []);
  return (
    <SettingsRow
      labelId="mates-link-previews"
      label="Link previews"
      description="Show a card with the title and picture of a web page a mate links to. Turning this on makes Evo contact the linked sites to read them."
      control={<Switch aria-labelledby="mates-link-previews" checked={on} onCheckedChange={(next) => useMatesPrefs.getState().setLinkPreviews(next)} />}
    />
  );
}

export function MatesSettings() {
  const [limit, setLimit] = React.useState(DEFAULT);
  const [running, setRunning] = React.useState(RUNNING_DEFAULT);
  const [router, setRouter] = React.useState<RouterChoice>(null);

  React.useEffect(() => {
    if (!inTauri) return;
    void db
      .settings()
      .then((all) => {
        const saved = all[HANDOFF_LIMIT];
        if (typeof saved === "number" && saved >= 1 && saved <= 50) setLimit(saved);
        const max = all[MAX_RUNNING];
        if (typeof max === "number" && max >= 1 && max <= 16) setRunning(max);
        const chosen = all[ROUTER_MODEL] as RouterChoice | undefined;
        if (chosen && typeof chosen.provider === "string" && typeof chosen.model === "string") setRouter(chosen);
      })
      .catch((error) => log("warn", "ui", `could not read the Evo Mates settings: ${String(error)}`));
  }, []);

  const save = (value: number) => {
    setLimit(value);
    void db
      .setSetting(HANDOFF_LIMIT, value === DEFAULT ? null : value)
      .then(reloadChannels)
      .catch((error) => {
        log("error", "ui", `could not save the hand-off limit: ${String(error)}`);
      });
  };

  const saveRouter = (value: RouterChoice) => {
    setRouter(value);
    void db
      .setSetting(ROUTER_MODEL, value)
      .then(reloadChannels)
      .catch((error) => {
        log("error", "ui", `could not save the router model: ${String(error)}`);
      });
  };

  const saveRunning = (value: number) => {
    setRunning(value);
    void db.setSetting(MAX_RUNNING, value === RUNNING_DEFAULT ? null : value).catch((error) => {
      log("error", "ui", `could not save the running limit: ${String(error)}`);
    });
  };

  return (
    <div className="flex flex-col gap-8">
      <SettingsHeader
        icon={UsersRoundIcon}
        title="Evo Mates"
        tag="Preview"
        description="How your mates work together in channels."
      />
      <SettingsGroup title="Mates">
        <SettingsRow
          labelId="mates-max-running"
          label={`Mates working at once: ${running}`}
          description="When more mates have work, the others wait their turn and show Queued. Your own messages never wait."
          control={
            <Slider
              aria-labelledby="mates-max-running"
              className="w-40"
              min={1}
              max={16}
              step={1}
              value={running}
              onValueChange={(value) => saveRunning(Math.round(Array.isArray(value) ? value[0] : value))}
            />
          }
        />
        <DailyLimit />
        <LinkPreviews />
      </SettingsGroup>
      <SettingsGroup title="Channels">
        <SettingsRow
          labelId="mates-handoff-limit"
          label={`Hand-offs per message: ${limit}`}
          description="How many times mates may hand work to each other for one of your messages. Past it, every mate in the channel stops and asks you how to go on."
          control={
            <Slider
              aria-labelledby="mates-handoff-limit"
              className="w-40"
              min={1}
              max={20}
              step={1}
              value={limit}
              onValueChange={(value) => save(Math.round(Array.isArray(value) ? value[0] : value))}
            />
          }
        />
        <SettingsRow
          labelId="mates-router-model"
          label="Picks who answers"
          description="Under Auto, this model reads a message that mentions nobody and picks the mate. By default the cheapest connected model fit for tools. Its cost shows in Usage under Evo Mates."
          control={<RouterModelPicker value={router} onChange={saveRouter} />}
        />
      </SettingsGroup>
      <LearningSettings app="mates" />
    </div>
  );
}

/** The router model, from the models of the connected providers; none chosen is the cheapest. */
function RouterModelPicker({ value, onChange }: { value: RouterChoice; onChange(value: RouterChoice): void }) {
  const models = useEngineStore((s) => s.models);
  const ready = useEngineStore((s) => s.status === "ready");
  const chosenKey = value ? `${value.provider}/${value.model}` : "";
  const current = models.find((m) => modelKey(m) === chosenKey);
  const groups = React.useMemo(() => {
    const byProvider = new Map<string, typeof models>();
    for (const model of models) byProvider.set(model.provider, [...(byProvider.get(model.provider) ?? []), model]);
    return [...byProvider.entries()];
  }, [models]);

  return (
    <div className="flex items-center gap-2">
      {value && (
        <Button variant="ghost" size="sm" onClick={() => onChange(null)}>
          Cheapest
        </Button>
      )}
      <DropdownMenu>
        <DropdownMenuTrigger disabled={!ready || models.length === 0} render={<Button variant="outline" size="sm" className="max-w-64" />}>
          {value && <ProviderIcon id={value.provider} size="xs" />}
          <span className="truncate">{value ? (current?.name || value.model) : "Cheapest connected"}</span>
          <ChevronRightIcon className="rotate-90" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="max-h-96 w-64">
          <DropdownMenuRadioGroup
            value={chosenKey}
            onValueChange={(key) => {
              const model = models.find((m) => modelKey(m) === key);
              if (model) onChange({ provider: model.provider, model: model.id });
            }}
          >
            {groups.map(([provider, items], index) => (
              <React.Fragment key={provider}>
                {index > 0 && <DropdownMenuSeparator />}
                <DropdownMenuGroup>
                  <DropdownMenuLabel className="flex items-center gap-2">
                    <ProviderIcon id={provider} size="xs" />
                    {provider}
                  </DropdownMenuLabel>
                  {items.map((model) => (
                    <DropdownMenuRadioItem key={modelKey(model)} value={modelKey(model)}>
                      <span className="truncate">{model.name || model.id}</span>
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuGroup>
              </React.Fragment>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
