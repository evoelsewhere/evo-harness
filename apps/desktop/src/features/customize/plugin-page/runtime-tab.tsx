// What of a plugin runs, and how (docs/features/plugin-settings/spec.md,
// FR-4, FR-8, FR-10 – FR-13, FR-18): its runtime getting ready, its servers'
// state with Start / Stop / Restart, when it starts, and its log.

import * as React from "react";
import { PlayIcon, RotateCwIcon, SquareIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { plugins, type InstalledPlugin, type PluginRuntime, type PluginWaiting } from "@/lib/plugins";
import { cn } from "@/lib/utils";
import { usePluginRuntime } from "@/features/plugin-ui/backend-stage";
import { Properties } from "../customize-parts";
import { ErrorNote, Section, useBusy } from "./parts";

const MODES: { value: PluginRuntime["startup"]; label: string }[] = [
  { value: "onDemand", label: "When it is needed" },
  { value: "withApp", label: "When evo opens" },
  { value: "off", label: "Never" },
];

const PARTS: Record<PluginWaiting["part"], string> = {
  setup: "Its code",
  hooks: "Its hooks",
  runtime: "Its backend and connector servers",
};

const STATUS: Record<string, string> = {
  connected: "Running",
  connecting: "Starting",
  sleeping: "Asleep — starts when needed",
  off: "Not running",
  failed: "Could not start",
  crashed: "Stopped after crashing",
  "needs-auth": "Needs sign-in",
};

function waitingFor(plugin: InstalledPlugin, waiting: PluginWaiting): string {
  if (waiting.part === "setup") {
    const titles = waiting.needs.map((key) => plugin.settings.find((s) => s.key === key)?.title ?? key);
    return `settings to fill in: ${titles.join(", ")}`;
  }
  const labels = waiting.needs.map((id) => plugin.permissions.find((p) => p.id === id)?.label.toLowerCase() ?? id);
  return `your approval: ${labels.join(", ")}`;
}

export function RuntimeTab({ plugin }: { plugin: InstalledPlugin }) {
  const [state, reload] = usePluginRuntime(plugin.id, plugin.name);
  const { busy, error, run } = useBusy();
  const [idle, setIdle] = React.useState(String(plugin.idleStopMinutes ?? plugin.runtime?.idleStopMinutes ?? 10));
  const runs = plugin.runtime || plugin.connectors.length > 0 || plugin.hooks.length > 0;
  if (!runs) {
    return <p className="text-xs text-muted-foreground">{plugin.name} runs no code of its own.</p>;
  }
  const mode = plugin.startMode ?? plugin.runtime?.startup ?? "onDemand";
  const preparation = state?.preparation;
  const servers = state?.servers ?? [];
  const log = servers.flatMap((s) => s.log.map((line) => `${s.name.replace(`plugin_${plugin.name}_`, "")}: ${line}`)).slice(-40);
  const control = (action: "start" | "stop" | "restart") => run(action, async () => {
    await plugins.runtimeControl(plugin.id, plugin.name, action);
    reload();
  });
  return (
    <div className="flex flex-col gap-5">
      {plugin.waiting.length > 0 && (
        <Section title="Not running yet">
          <ul className="flex flex-col gap-1.5 text-[13px]">
            {plugin.waiting.map((waiting) => (
              <li key={`${waiting.part}-${waiting.needs.join()}`}>
                {PARTS[waiting.part]} waits for {waitingFor(plugin, waiting)}.
              </li>
            ))}
          </ul>
        </Section>
      )}
      {preparation && preparation.phase !== "ready" && (
        <div className={cn("flex items-start gap-2 rounded-lg px-3 py-2 text-xs", preparation.phase === "failed" ? "bg-destructive/8" : "bg-brand/8")}>
          {preparation.phase !== "failed" && <Spinner className="mt-px size-3.5" />}
          <span className="min-w-0 flex-1 whitespace-pre-wrap break-words">
            {preparation.phase === "failed" ? "Its backend could not be prepared:\n" : ""}
            {preparation.message}
          </span>
          {preparation.phase === "failed" && (
            <Button size="xs" disabled={busy !== null} onClick={() => void control("start")}>
              Try again
            </Button>
          )}
        </div>
      )}
      <Section
        title="Running"
        aside={
          <div className="flex gap-1">
            <Button size="xs" variant="ghost" disabled={busy !== null} onClick={() => void control("start")}>
              <PlayIcon /> Start
            </Button>
            <Button size="xs" variant="ghost" disabled={busy !== null} onClick={() => void control("stop")}>
              <SquareIcon /> Stop
            </Button>
            <Button size="xs" variant="ghost" disabled={busy !== null} onClick={() => void control("restart")}>
              <RotateCwIcon /> Restart
            </Button>
          </div>
        }
      >
        {servers.length === 0 ? (
          <p className="text-xs text-muted-foreground">Nothing of it runs now.</p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {servers.map((server) => (
              <li key={server.key} className="flex items-start gap-2 text-[13px]">
                <span
                  className={cn(
                    "mt-1.5 size-2 shrink-0 rounded-full",
                    server.status === "connected" ? "bg-status-ok" : server.status === "failed" || server.status === "crashed" ? "bg-destructive" : "bg-muted-foreground/40",
                  )}
                />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span>
                    {server.name.replace(`plugin_${plugin.name}_`, "")}
                    <span className="ml-2 text-[11px] text-muted-foreground">
                      {STATUS[server.status] ?? server.status} · {server.tools} tools
                    </span>
                  </span>
                  {server.error && <span className="text-[11px] break-words text-destructive">{server.error}</span>}
                </span>
              </li>
            ))}
          </ul>
        )}
        <ErrorNote error={error} />
      </Section>
      <Section title="When it runs">
        <div className="flex flex-wrap items-center gap-3 text-[13px]">
          <span className="text-muted-foreground">Starts</span>
          <Select
            value={mode}
            items={MODES}
            onValueChange={(next) => void run("mode", () => plugins.runtimeMode(plugin.id, next as PluginRuntime["startup"]))}
          >
            <SelectTrigger className="w-48">
              <SelectValue />
            </SelectTrigger>
            <SelectContent alignItemWithTrigger={false}>
              {MODES.map((m) => (
                <SelectItem key={m.value} value={m.value}>
                  {m.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {mode === "onDemand" && (
            <>
              <span className="text-muted-foreground">stops after</span>
              <Input
                type="number"
                min={1}
                max={1440}
                value={idle}
                onChange={(e) => setIdle(e.target.value)}
                onBlur={() => void run("idle", () => plugins.runtimeMode(plugin.id, mode, Number(idle) || null))}
                className="w-20"
              />
              <span className="text-muted-foreground">minutes without use</span>
            </>
          )}
        </div>
        {plugin.runtime && (
          <div className="@container border-t border-border/60 pt-3">
            <Properties
              items={[
                ["Backend", `${plugin.runtime.language === "python" ? "Python, run by uv" : `${plugin.runtime.language === "typescript" ? "TypeScript" : "JavaScript"}, run by Deno`} · ${plugin.runtime.entry}`],
                ["Limits", plugin.runtime.language === "python" ? "Its network and files are not enforced" : "Only the sites and folders you allowed"],
              ]}
            />
          </div>
        )}
      </Section>
      {log.length > 0 && (
        <Section title="Log">
          <pre className="max-h-56 overflow-auto font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-muted-foreground">{log.join("\n")}</pre>
        </Section>
      )}
    </div>
  );
}
