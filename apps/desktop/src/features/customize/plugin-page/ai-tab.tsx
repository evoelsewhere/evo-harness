// A plugin's Models tab: its AI (docs/features/plugin-ai/spec.md, FR-14, FR-15, FR-19, FR-21):
// what it spent, its runs with Stop, its threads, and the person's rules — which
// providers and models it may use, a default, a model per tier, and caps
// per day and month (none by default).

import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { SquareIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { formatRelative } from "@/lib/format";
import { inTauri } from "@/lib/platform";
import { plugins, type AiModelInfo, type AiModelRef, type AiSettings, type AiState, type InstalledPlugin } from "@/lib/plugins";
import { Properties } from "../customize-parts";
import { ThreadsSection } from "./ai-threads";
import { ErrorNote, Section, useBusy } from "./parts";

const AUTO = "__auto__";
const TIERS: { key: "fast" | "balanced" | "best"; label: string }[] = [
  { key: "fast", label: "Fast" },
  { key: "balanced", label: "Balanced" },
  { key: "best", label: "Best" },
];

const keyOf = (m: AiModelRef) => `${m.provider}/${m.model}`;
const usd = (n: number) => (n === 0 ? "$0" : n < 0.0001 ? "< $0.0001" : n < 0.01 ? `$${n.toFixed(4)}` : `$${n.toFixed(2)}`);

function useAi(plugin: InstalledPlugin): [AiState | null, () => void] {
  const [state, setState] = React.useState<AiState | null>(null);
  const load = React.useCallback(() => {
    if (!inTauri) return;
    void plugins.aiState(plugin.id).then(setState).catch(() => undefined);
  }, [plugin.id]);
  React.useEffect(() => {
    load();
    if (!inTauri) return;
    const offs = [listen("plugin-ai://runs", load), listen("plugins://changed", load)];
    const timer = window.setInterval(load, 5_000);
    return () => {
      offs.forEach((off) => void off.then((stop) => stop()));
      window.clearInterval(timer);
    };
  }, [load]);
  return [state, load];
}

function ModelSelect({ value, models, auto, onChange, disabled }: { value: AiModelRef | null | undefined; models: AiModelInfo[]; auto: string; onChange(m: AiModelRef | null): void; disabled?: boolean }) {
  const items = [{ value: AUTO, label: auto }, ...models.map((m) => ({ value: keyOf(m), label: `${m.name} · ${m.provider}` }))];
  return (
    <Select
      value={value ? keyOf(value) : AUTO}
      items={items}
      disabled={disabled}
      onValueChange={(next) => {
        const found = models.find((m) => keyOf(m) === next);
        onChange(found ? { provider: found.provider, model: found.model } : null);
      }}
    >
      <SelectTrigger className="w-64">
        <SelectValue />
      </SelectTrigger>
      <SelectContent alignItemWithTrigger={false}>
        {items.map((item) => (
          <SelectItem key={item.value} value={item.value}>
            {item.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

export function AiTab({ plugin }: { plugin: InstalledPlugin }) {
  const [state, reload] = useAi(plugin);
  const { busy, error, run } = useBusy();
  const [models, setModels] = React.useState<AiModelInfo[] | null>(null);
  const [modelsError, setModelsError] = React.useState<string | null>(null);
  const allowed = plugin.approvedPermissions.includes("ai");
  React.useEffect(() => {
    if (!inTauri || !allowed) return;
    void plugins.aiModels(plugin.id).then(setModels, (e) => setModelsError(String(e)));
  }, [plugin.id, allowed]);
  const settings: AiSettings = state?.settings ?? {};
  const [day, setDay] = React.useState("");
  const [month, setMonth] = React.useState("");
  React.useEffect(() => {
    setDay(settings.capDayUsd != null ? String(settings.capDayUsd) : "");
    setMonth(settings.capMonthUsd != null ? String(settings.capMonthUsd) : "");
  }, [settings.capDayUsd, settings.capMonthUsd]);

  const save = (next: AiSettings) => run("save", async () => {
    await plugins.aiSaveSettings(plugin.id, next);
    reload();
  });
  const cap = (text: string) => {
    const n = Number(text);
    return text.trim() === "" || !Number.isFinite(n) || n < 0 ? null : n;
  };
  const providers = [...new Set((models ?? []).map((m) => m.provider))];
  const usable = (models ?? []).filter((m) => !settings.providers || settings.providers.includes(m.provider));

  if (!allowed) {
    return <p className="text-xs text-muted-foreground">{plugin.name} has not been allowed to use your AI models. Allow “Use your AI models” on its Permissions tab first.</p>;
  }
  const going = state?.runs.going ?? [];
  const recent = state?.runs.recent ?? [];
  const noCap = settings.capDayUsd == null && settings.capMonthUsd == null;
  return (
    <div className="flex flex-col gap-5">
      <Section title="Use">
        <div className="@container">
          <Properties
            items={[
              ["Today", usd(state?.spentToday ?? 0)],
              ["This month", `${usd(state?.spentMonth ?? 0)} · ${state?.monthCalls ?? 0} calls`],
            ]}
          />
        </div>
        {noCap && <p className="text-[11px] text-muted-foreground">No cap: {plugin.name} may spend money on your providers. Set one below.</p>}
      </Section>

      <Section title="Running">
        {going.length === 0 ? (
          <p className="text-xs text-muted-foreground">Nothing runs now.</p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {going.map((r) => (
              <li key={r.id} className="flex items-center gap-2 text-[13px]">
                <span className="size-2 shrink-0 rounded-full bg-status-ok" />
                <span className="min-w-0 flex-1 truncate">{r.model ?? "Choosing a model…"}</span>
                <span className="text-[11px] text-muted-foreground">
                  {r.seconds}s · {usd(r.costUsd)}
                </span>
                <Button size="xs" variant="ghost" onClick={() => void run("stop", () => plugins.aiCancel(r.id))}>
                  <SquareIcon /> Stop
                </Button>
              </li>
            ))}
          </ul>
        )}
        {recent.length > 0 && (
          <ul className="flex flex-col gap-1 border-t border-border/60 pt-2">
            {recent.slice(0, 8).map((r) => (
              <li key={r.id} className="flex items-center gap-2 text-[12px] text-muted-foreground">
                <span className={r.status === "error" ? "text-destructive" : undefined}>{r.status === "done" ? "Done" : r.status === "cancelled" ? "Stopped" : r.status === "error" ? "Failed" : "Running"}</span>
                <span className="min-w-0 flex-1 truncate">
                  {r.modelId ?? "—"}
                  {r.threadId ? ` · ${r.threadId}` : ""}
                </span>
                <span>{usd(r.costUsd)}</span>
                <span className="w-20 text-right">{formatRelative(r.startedAtMs)}</span>
              </li>
            ))}
          </ul>
        )}
      </Section>

      <ThreadsSection plugin={plugin} />

      <Section title="Models it may use">
        {modelsError && <ErrorNote error={modelsError} />}
        {!models && !modelsError && <p className="text-xs text-muted-foreground">Reading your connected models…</p>}
        {models && models.length === 0 && <p className="text-xs text-muted-foreground">No provider is connected. Add one in Settings → Providers.</p>}
        {models && models.length > 0 && (
          <div className="flex flex-col gap-3 text-[13px]">
            <label className="flex items-center gap-2">
              <Switch
                checked={!settings.providers}
                disabled={busy !== null}
                onCheckedChange={(all) => void save({ ...settings, providers: all ? null : providers })}
              />
              Every connected provider
            </label>
            {settings.providers && (
              <div className="flex flex-wrap gap-2">
                {providers.map((p) => {
                  const on = settings.providers?.includes(p) ?? false;
                  return (
                    <button
                      key={p}
                      type="button"
                      disabled={busy !== null}
                      onClick={() => void save({ ...settings, providers: on ? (settings.providers ?? []).filter((x) => x !== p) : [...(settings.providers ?? []), p] })}
                      className={on ? "rounded-full border border-brand/40 bg-brand/10 px-2.5 py-0.5 text-[12px]" : "rounded-full border border-border/70 px-2.5 py-0.5 text-[12px] text-muted-foreground hover:bg-muted"}
                    >
                      {p}
                    </button>
                  );
                })}
              </div>
            )}
            <div className="flex flex-wrap items-center gap-3">
              <span className="w-28 text-muted-foreground">Default</span>
              <ModelSelect value={settings.defaultModel} models={usable} auto="Let evo choose" disabled={busy !== null} onChange={(m) => void save({ ...settings, defaultModel: m })} />
            </div>
            {TIERS.map((tier) => (
              <div key={tier.key} className="flex flex-wrap items-center gap-3">
                <span className="w-28 text-muted-foreground">{tier.label}</span>
                <ModelSelect
                  value={settings.tiers?.[tier.key]}
                  models={usable}
                  auto="Automatic"
                  disabled={busy !== null}
                  onChange={(m) => {
                    const tiers = { ...(settings.tiers ?? {}) };
                    if (m) tiers[tier.key] = m;
                    else delete tiers[tier.key];
                    void save({ ...settings, tiers });
                  }}
                />
              </div>
            ))}
            <p className="text-[11px] text-muted-foreground">A plugin asks for fast, balanced or best; evo picks among these by price and ability unless you pin one.</p>
          </div>
        )}
      </Section>

      <Section title="Budget">
        <div className="flex flex-wrap items-center gap-3 text-[13px]">
          <span className="w-28 text-muted-foreground">Per day ($)</span>
          <Input className="w-28" inputMode="decimal" placeholder="No cap" value={day} onChange={(e) => setDay(e.target.value)} onBlur={() => void save({ ...settings, capDayUsd: cap(day) })} />
          <span className="w-28 text-muted-foreground">Per month ($)</span>
          <Input className="w-28" inputMode="decimal" placeholder="No cap" value={month} onChange={(e) => setMonth(e.target.value)} onBlur={() => void save({ ...settings, capMonthUsd: cap(month) })} />
        </div>
        <p className="text-[11px] text-muted-foreground">When a cap is reached, its AI calls are refused and a running one stops.</p>
      </Section>
      <ErrorNote error={error} />
    </div>
  );
}
