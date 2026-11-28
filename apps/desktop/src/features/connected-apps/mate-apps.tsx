import * as React from "react";
import { Checkbox } from "@/components/ui/checkbox";
import { appsApi, SERVICE_PREFIX, type AppTool, type ServiceGrant } from "@/lib/connected-apps";
import type { Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useConnectedAppsStore } from "@/stores/connected-apps-store";
import { useMatesStore } from "@/stores/mates-store";
import { openConnectedApps } from "./apps-dialog";
import { ServiceTile, useServiceName } from "./connected-apps-page";

// A mate's connected apps (docs/features/connected-apps/spec.md, FR-10;
// Evo Mates FR-39): All connected apps, or Only these with, per service, All
// tools / These tools / None and the accounts it may use. Kept in the mate's
// `apps` as `{service: "composio:<toolkit>", tools, accounts}`; other entries
// of `apps` are left as they are.

type ToolsChoice = "all" | "these" | "none";

const isServiceGrant = (entry: unknown): entry is ServiceGrant =>
  !!entry && typeof entry === "object" && typeof (entry as ServiceGrant).service === "string" && (entry as ServiceGrant).service.startsWith(SERVICE_PREFIX);

function choiceOf(grant: ServiceGrant | undefined): ToolsChoice {
  if (!grant || grant.tools === "none") return "none";
  if (Array.isArray(grant.tools)) return "these";
  return "all";
}

function ToolList({ toolkit, chosen, onChange }: { toolkit: string; chosen: string[]; onChange(next: string[]): void }) {
  const [tools, setTools] = React.useState<AppTool[] | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    let live = true;
    appsApi
      .tools(toolkit)
      .then((list) => live && setTools(list))
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [toolkit]);
  if (error) return <p className="text-xs text-muted-foreground">The tools could not be read: {error}</p>;
  if (!tools) return <p className="text-xs text-muted-foreground">Reading tools…</p>;
  return (
    <ul className="flex max-h-56 flex-col gap-1 overflow-y-auto pr-1">
      {tools.map((tool) => (
        <li key={tool.slug}>
          <label className="flex items-start gap-2 text-xs">
            <Checkbox
              checked={chosen.includes(tool.slug)}
              onCheckedChange={(on) => onChange(on ? [...chosen, tool.slug] : chosen.filter((t) => t !== tool.slug))}
              className="mt-0.5"
            />
            <span className="min-w-0">
              <span className="font-medium">{tool.name}</span>
              <span className="text-muted-foreground"> · {tool.read ? "reads" : "may send or change"}</span>
            </span>
          </label>
        </li>
      ))}
    </ul>
  );
}

export function MateConnectedApps({ mate }: { mate: Mate }) {
  const key = useConnectedAppsStore((s) => s.key);
  const accounts = useConnectedAppsStore((s) => s.accounts);
  const name = useServiceName();
  React.useEffect(() => {
    void useConnectedAppsStore.getState().load();
  }, []);
  React.useEffect(() => {
    if (key === "saved") void useConnectedAppsStore.getState().loadCatalog();
  }, [key]);
  if (key !== "saved")
    return (
      <section className="flex flex-col gap-1">
        <h3 className="text-[13px] font-medium">Connected apps</h3>
        <p className="text-xs text-muted-foreground">
          Gmail, Slack, Notion and hundreds more, through Composio with your own key.{" "}
          <button type="button" onClick={openConnectedApps} className="text-brand hover:underline">
            Set up connected apps
          </button>
        </p>
      </section>
    );

  const services = [...new Set(accounts.map((a) => a.toolkit))].sort();
  const all = mate.apps === null;
  const entries = (mate.apps ?? []).filter(isServiceGrant);
  const others = (mate.apps ?? []).filter((e) => !isServiceGrant(e));
  const grantOf = (toolkit: string) => entries.find((e) => e.service === `${SERVICE_PREFIX}${toolkit}`);
  const save = (next: ServiceGrant[] | null) =>
    void useMatesStore.getState().update(mate.id, { apps: next === null ? null : [...others, ...next] });
  const setGrant = (toolkit: string, grant: ServiceGrant | null) => {
    const service = `${SERVICE_PREFIX}${toolkit}`;
    const rest = entries.filter((e) => e.service !== service);
    save(grant ? [...rest, grant] : rest);
  };

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <h3 className="text-[13px] font-medium">Connected apps</h3>
        <button type="button" onClick={openConnectedApps} className="text-xs text-brand hover:underline">
          Manage
        </button>
      </div>
      <div role="radiogroup" aria-label="Connected apps for this mate" className="flex gap-1 rounded-full bg-muted/70 p-1 text-[12.5px]">
        {[
          { id: "all", label: "All connected apps" },
          { id: "only", label: "Only these" },
        ].map((option) => {
          const on = (option.id === "all") === all;
          return (
            <button
              key={option.id}
              type="button"
              role="radio"
              aria-checked={on}
              onClick={() => (option.id === "all" ? save(null) : all && save([]))}
              className={cn("flex-1 rounded-full px-3 py-1", on ? "bg-background shadow-[0_1px_2px_rgb(0_0_0/0.08)]" : "text-muted-foreground")}
            >
              {option.label}
            </button>
          );
        })}
      </div>
      <p className="text-xs text-muted-foreground">
        {all
          ? `${mate.name} may use every connected app, and any you connect later.`
          : `${mate.name} may use only the services below; one you connect later stays off until you turn it on.`}
      </p>
      {!all && services.length === 0 && <p className="text-xs text-muted-foreground">No service connected yet.</p>}
      {!all &&
        services.map((toolkit) => {
          const grant = grantOf(toolkit);
          const choice = choiceOf(grant);
          const labels = accounts.filter((a) => a.toolkit === toolkit && a.status === "active").map((a) => a.label);
          const allowed = grant?.accounts ?? labels;
          return (
            <div key={toolkit} className="flex flex-col gap-2 rounded-2xl bg-muted/40 px-3 py-2">
              <div className="flex items-center gap-2">
                <ServiceTile slug={toolkit} name={name(toolkit)} className="size-6 rounded-md text-xs" />
                <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{name(toolkit)}</span>
                <select
                  aria-label={`${name(toolkit)} tools`}
                  value={choice}
                  onChange={(e) => {
                    const next = e.target.value as ToolsChoice;
                    if (next === "none") setGrant(toolkit, { service: `${SERVICE_PREFIX}${toolkit}`, tools: "none" });
                    else setGrant(toolkit, { service: `${SERVICE_PREFIX}${toolkit}`, tools: next === "all" ? "all" : [], accounts: grant?.accounts });
                  }}
                  className="h-7 rounded-md border border-input bg-background px-1.5 text-xs"
                >
                  <option value="all">All tools</option>
                  <option value="these">These tools</option>
                  <option value="none">None</option>
                </select>
              </div>
              {choice === "these" && grant && (
                <ToolList
                  toolkit={toolkit}
                  chosen={Array.isArray(grant.tools) ? grant.tools : []}
                  onChange={(tools) => setGrant(toolkit, { ...grant, tools })}
                />
              )}
              {choice !== "none" && grant && labels.length >= 2 && (
                <div className="flex flex-col gap-1">
                  <span className="text-xs text-muted-foreground">Accounts {mate.name} may use</span>
                  <div className="flex flex-wrap gap-3">
                    {labels.map((label) => (
                      <label key={label} className="flex items-center gap-1.5 text-xs">
                        <Checkbox
                          checked={allowed.includes(label)}
                          onCheckedChange={(on) => {
                            const next = on ? [...allowed, label] : allowed.filter((l) => l !== label);
                            setGrant(toolkit, { ...grant, accounts: next });
                          }}
                        />
                        {label}
                      </label>
                    ))}
                  </div>
                </div>
              )}
            </div>
          );
        })}
    </section>
  );
}
