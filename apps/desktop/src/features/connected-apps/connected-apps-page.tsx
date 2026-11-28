import * as React from "react";
import { ArrowLeftIcon, ChevronRightIcon, RefreshCwIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useConnectedAppsStore } from "@/stores/connected-apps-store";
import { AppsCatalogue, CatalogueSearch, ServiceDetail, ServiceTile, serviceLine, statusTone, useConnectedServices } from "./apps-parts";
import { KeyCard } from "./key-card";
import { OfficialBadge, OfficialHeader, serverTone, useServer, ZapierHeader } from "./official-parts";

// Connected apps on one page, for Evo Mates (docs/features/connected-apps/
// spec.md, FR-1 – FR-6, FR-17): the services you connected, a service's
// accounts, the catalogue, and the key. In Evo Harness the same parts live in
// Customize → Connectors next to the MCP servers.

export { ServiceTile, useServiceName } from "./apps-parts";

/** The unavailable notice with Retry (FR-17). */
export function UnavailableNotice() {
  const status = useConnectedAppsStore((s) => s.status);
  const reason = useConnectedAppsStore((s) => s.reason);
  if (status !== "unavailable") return null;
  return (
    <div className="flex items-center gap-2 rounded-xl bg-amber-500/10 px-3 py-2 text-xs">
      <span className="flex-1">Composio is unavailable{reason ? `: ${reason}` : ""}. Chats keep their other tools.</span>
      <Button size="xs" variant="ghost" onClick={() => void useConnectedAppsStore.getState().retry().catch(() => undefined)}>
        <RefreshCwIcon /> Retry
      </Button>
    </div>
  );
}

/** A connected official server or Zapier in the list. */
function ServerRow({ serverKey, slug, name, official, onOpen }: { serverKey: string; slug: string; name: string; official?: boolean; onOpen(): void }) {
  const server = useServer(serverKey);
  return (
    <li>
      <button type="button" onClick={onOpen} className="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-accent/50">
        <ServiceTile slug={slug} name={name} />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="flex items-center gap-1.5 truncate text-[13px] font-medium">
            {name} {official && <OfficialBadge className="h-4 px-1 text-[10px]" />}
          </span>
          <span className="flex items-center gap-1.5 truncate text-xs text-muted-foreground">
            <span className={cn("size-1.5 shrink-0 rounded-full", serverTone(server?.status))} />
            {server?.status === "connected" ? `${server.tools.length} tools` : server?.status === "failed" ? "Failed" : "Needs sign-in"}
          </span>
        </span>
        <ChevronRightIcon className="size-4 text-muted-foreground" />
      </button>
    </li>
  );
}

export function ConnectedAppsPage({ className }: { className?: string }) {
  const loaded = useConnectedAppsStore((s) => s.loaded);
  const key = useConnectedAppsStore((s) => s.key);
  const services = useConnectedServices();
  const providers = useConnectedAppsStore((s) => s.providers);
  const officials = (providers?.official ?? []).filter((o) => o.connected);
  const zapier = providers?.zapier.connected ? providers.zapier : null;
  const [open, setOpen] = React.useState<string | null>(null);
  const [browsing, setBrowsing] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const [keyOpen, setKeyOpen] = React.useState(false);
  const keyShown = keyOpen || key !== "saved";

  React.useEffect(() => {
    void useConnectedAppsStore.getState().load();
  }, []);
  React.useEffect(() => {
    if (key === "saved") void useConnectedAppsStore.getState().loadCatalog();
  }, [key]);

  if (!loaded) return null;

  if (open)
    return (
      <div className={cn("flex flex-col gap-4", className)}>
        <Button size="sm" variant="ghost" className="w-fit gap-1.5 text-muted-foreground" onClick={() => setOpen(null)}>
          <ArrowLeftIcon /> All apps
        </Button>
        {open.startsWith("svc:") ? (
          <ServiceDetail toolkit={open.slice(4)} />
        ) : open === "zapier" ? (
          <ZapierHeader />
        ) : (
          <OfficialHeader id={open.slice(4)} />
        )}
      </div>
    );

  return (
    <div className={cn("flex flex-col gap-4", className)}>
      <UnavailableNotice />
      {!browsing && (
        <section className="flex flex-col gap-2">
          <div className="flex items-center justify-between">
            <h3 className="text-xs font-medium text-muted-foreground">Your apps</h3>
            <Button size="xs" variant="outline" onClick={() => setBrowsing(true)}>
              Browse apps
            </Button>
          </div>
          {(officials.length > 0 || zapier) && (
            <ul className="divide-y divide-border/60 overflow-hidden rounded-xl shadow-[0_0_0_1px_var(--border)]">
              {officials.map((o) => (
                <ServerRow key={o.id} serverKey={o.key} slug={o.composio[0] ?? o.id} name={o.name} official onOpen={() => setOpen(`off:${o.id}`)} />
              ))}
              {zapier && <ServerRow serverKey={zapier.key} slug="zapier" name="Zapier" onOpen={() => setOpen("zapier")} />}
            </ul>
          )}
          {services.length === 0 && (officials.length > 0 || zapier) ? null : services.length === 0 ? (
            <p className="rounded-xl px-3 py-6 text-center text-xs text-muted-foreground shadow-[0_0_0_1px_var(--border)]">
              Nothing connected yet. Browse apps to connect Gmail, Slack, Notion and more.
            </p>
          ) : (
            <ul className="divide-y divide-border/60 overflow-hidden rounded-xl shadow-[0_0_0_1px_var(--border)]">
              {services.map((service) => (
                <li key={service.toolkit}>
                  <button type="button" onClick={() => setOpen(`svc:${service.toolkit}`)} className="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-accent/50">
                    <ServiceTile slug={service.toolkit} name={service.name} />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-[13px] font-medium">{service.name}</span>
                      <span className="flex items-center gap-1.5 truncate text-xs text-muted-foreground">
                        <span className={cn("size-1.5 shrink-0 rounded-full", statusTone[service.status])} />
                        {serviceLine(service)}
                      </span>
                    </span>
                    <ChevronRightIcon className="size-4 text-muted-foreground" />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
      {browsing && (
        <section className="flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <Button size="sm" variant="ghost" className="gap-1.5 text-muted-foreground" onClick={() => setBrowsing(false)}>
              <ArrowLeftIcon /> Your apps
            </Button>
            <div className="flex-1">
              <CatalogueSearch value={query} onChange={setQuery} />
            </div>
          </div>
          <AppsCatalogue
            query={query}
            onOpen={(view) => {
              setBrowsing(false);
              setOpen(view);
            }}
          />
        </section>
      )}
      <section className="flex flex-col gap-2">
        <button type="button" onClick={() => setKeyOpen((o) => !o)} className="flex w-fit items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
          <ChevronRightIcon className={cn("size-3.5 transition-transform", keyOpen && "rotate-90")} />
          Composio key
        </button>
        {keyShown && <KeyCard compact={key === "saved"} />}
      </section>
    </div>
  );
}
