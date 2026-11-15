// What a plugin is and adds: its details, parts, views and Developer mode.

import { openUrl } from "@tauri-apps/plugin-opener";
import { Switch } from "@/components/ui/switch";
import { plugins, type InstalledPlugin } from "@/lib/plugins";
import { cn } from "@/lib/utils";
import { Properties } from "../customize-parts";
import { ReviewBanner } from "./permissions";
import { ErrorNote, Section, useBusy } from "./parts";

const SLOT_NAMES: { key: keyof InstalledPlugin["ui"]; label: string }[] = [
  { key: "sidebar", label: "Sidebar page" },
  { key: "panels", label: "Side panel tool" },
  { key: "cards", label: "Chat card" },
  { key: "composer", label: "Composer action" },
];

function Contents({ plugin }: { plugin: InstalledPlugin }) {
  const rows: [string, React.ReactNode][] = [
    ["Skills", plugin.skills.length ? plugin.skills.map((s) => `/${plugin.name}:${s}`).join("  ") : undefined],
    ["Commands", plugin.commands || undefined],
    ["Subagents", plugin.agents.length ? plugin.agents.map((a) => `${plugin.name}:${a}`).join(", ") : undefined],
    ["Hooks", plugin.hooks.length ? plugin.hooks.map((h) => `${h.event} (${h.count})`).join(", ") : undefined],
    ["Connectors", plugin.connectors.length ? plugin.connectors.join(", ") : undefined],
    ["Backend", plugin.runtime ? `${plugin.runtime.language} · ${plugin.runtime.entry}` : undefined],
    ["Accounts", plugin.accounts.length ? plugin.accounts.map((a) => a.service).join(", ") : undefined],
    ["Commands on PATH", plugin.bin ? "bin/" : undefined],
  ];
  const any = rows.some(([, v]) => v !== undefined);
  return any ? <Properties items={rows} /> : <p className="text-xs text-muted-foreground">This plugin adds nothing evo can use.</p>;
}

export function OverviewTab({ plugin }: { plugin: InstalledPlugin }) {
  const { busy, error, run } = useBusy();
  const views = SLOT_NAMES.flatMap(({ key, label }) => plugin.ui[key].map((view) => ({ ...view, slot: label })));
  return (
    <div className="flex flex-col gap-5">
      <ReviewBanner plugin={plugin} />
      {plugin.problems.length > 0 && (
        <div className="flex flex-col gap-1 rounded-xl border border-amber-500/30 bg-amber-500/8 px-3.5 py-3 text-xs">
          <p className="font-medium">Some parts could not be read</p>
          {plugin.problems.map((p) => (
            <p key={p} className="font-mono break-words text-muted-foreground">
              {p}
            </p>
          ))}
        </div>
      )}
      <div className="@container rounded-xl bg-muted/40 px-4 py-3 dark:bg-accent/30">
        <Properties
          items={[
            ["Author", plugin.author],
            ["License", plugin.license],
            [
              "Homepage",
              plugin.homepage && (
                <button type="button" className="text-link hover:underline" onClick={() => void openUrl(plugin.homepage!).catch(() => undefined)}>
                  {plugin.homepage}
                </button>
              ),
            ],
            ["Repository", plugin.repository],
            ["Keywords", plugin.keywords.length ? plugin.keywords.join(", ") : undefined],
          ]}
        />
      </div>
      <section className="flex flex-col gap-2">
        <h3 className="text-xs font-medium text-muted-foreground">What it adds</h3>
        <div className={cn("@container rounded-xl border border-border/70 px-4 py-3", !plugin.enabled && "opacity-60")}>
          <Contents plugin={plugin} />
        </div>
      </section>
      {views.length > 0 && (
        <Section title="Views">
          <ul className="flex flex-col gap-1 text-[13px]">
            {views.map((view) => (
              <li key={`${view.slot}-${view.id}`} className="flex items-center gap-2">
                <span className="min-w-0 truncate" title={view.title}>
                  {view.title}
                </span>
                <span className="shrink-0 text-[11px] text-muted-foreground">{view.slot}</span>
              </li>
            ))}
          </ul>
          <label className="flex items-center gap-3 border-t border-border/60 pt-3">
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="text-[13px]">Developer mode</span>
              <span className="text-[11px] text-muted-foreground">Reload its views when its files change.</span>
            </span>
            <Switch
              checked={plugin.developerMode}
              disabled={busy !== null}
              onCheckedChange={(on) => void run("dev", () => plugins.setDeveloperMode(plugin.id, on))}
            />
          </label>
          <ErrorNote error={error} />
        </Section>
      )}
      <p className="truncate font-mono text-[11px] text-muted-foreground" title={plugin.path}>
        {plugin.path}
      </p>
    </div>
  );
}
