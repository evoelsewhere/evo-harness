import * as React from "react";
import { PlusIcon, RefreshCwIcon, SearchIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { statusText, type Account, type Toolkit } from "@/lib/connected-apps";
import { formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useConnectedAppsStore } from "@/stores/connected-apps-store";
import { KeyCard } from "./key-card";

// The Connected apps page (docs/features/connected-apps/spec.md, FR-1 – FR-6,
// FR-16, FR-17): the key, the catalogue with search by name and category, and
// the Connected tab with every account as its own row. Used by Customize, the
// sidebar's More menu, Settings and Evo Mates.

/** Rows shown before the person narrows the search. */
const SHOWN = 120;

/** A service's tile: its initial on a quiet square (logos would need the page to load remote images). */
export function ServiceTile({ name, className }: { name: string; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted text-[13px] font-medium text-muted-foreground dark:bg-accent",
        className,
      )}
    >
      {name.trim().charAt(0).toUpperCase() || "?"}
    </span>
  );
}

const statusTone: Record<Account["status"], string> = {
  active: "bg-emerald-500",
  pending: "bg-amber-500",
  "needs-sign-in": "bg-amber-500",
  expired: "bg-amber-500",
  failed: "bg-red-500",
};

/** The service name for a toolkit slug. */
export function useServiceName(): (slug: string) => string {
  const toolkits = useConnectedAppsStore((s) => s.toolkits);
  return React.useCallback(
    (slug: string) => toolkits.find((t) => t.slug === slug)?.name ?? slug.charAt(0).toUpperCase() + slug.slice(1),
    [toolkits],
  );
}

/** Ask for a label and start the sign-in (FR-4, FR-16). */
export function ConnectDialog({ toolkit, onOpenChange }: { toolkit: Toolkit | null; onOpenChange(open: boolean): void }) {
  const accounts = useConnectedAppsStore((s) => s.accounts);
  const [label, setLabel] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const mine = toolkit ? accounts.filter((a) => a.toolkit === toolkit.slug) : [];
  const first = !mine.some((a) => a.status === "active");
  React.useEffect(() => {
    setLabel(first ? "personal" : "");
    setError(null);
  }, [toolkit?.slug, first]);
  if (!toolkit) return null;
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await useConnectedAppsStore.getState().connect(toolkit.slug, label.trim() || undefined);
      onOpenChange(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={!!toolkit} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Connect {toolkit.name}</DialogTitle>
          <DialogDescription>
            The sign-in opens in your browser. The label tells this account apart from others of {toolkit.name}, such as
            “work” or “personal”.
          </DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <label className="text-xs text-muted-foreground" htmlFor="connected-app-label">
            Label{first ? "" : " (required: this service already has an account)"}
          </label>
          <Input id="connected-app-label" value={label} maxLength={32} onChange={(e) => setLabel(e.target.value)} autoFocus />
          {mine.length > 0 && (
            <p className="text-xs text-muted-foreground">Already connected: {mine.map((a) => a.label).join(", ")}</p>
          )}
          {!toolkit.managedAuth && !toolkit.noAuth && (
            <p className="text-xs text-muted-foreground">
              Composio has no sign-in of its own for {toolkit.name}: Evo uses your own auth config for it in your Composio
              project, or tells you how to make one.
            </p>
          )}
          {error && (
            <p role="alert" className="text-xs text-destructive">
              {error}
            </p>
          )}
          <DialogFooter>
            <Button variant="ghost" type="button" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || (!first && !label.trim())}>
              {busy ? "Opening…" : "Connect"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function Catalogue({ onConnect }: { onConnect(toolkit: Toolkit): void }) {
  const toolkits = useConnectedAppsStore((s) => s.toolkits);
  const categories = useConnectedAppsStore((s) => s.categories);
  const loading = useConnectedAppsStore((s) => s.catalogLoading);
  const error = useConnectedAppsStore((s) => s.catalogError);
  const accounts = useConnectedAppsStore((s) => s.accounts);
  const [query, setQuery] = React.useState("");
  const [category, setCategory] = React.useState("");
  const q = query.trim().toLowerCase();
  const found = toolkits.filter(
    (t) =>
      (!q || t.name.toLowerCase().includes(q) || t.slug.includes(q)) &&
      (!category || t.categories.some((c) => c.toLowerCase() === category.toLowerCase())),
  );
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-0 flex-1">
          <SearchIcon className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search services"
            aria-label="Search services"
            className="h-8 pl-8 text-[13px]"
          />
        </div>
        <select
          value={category}
          onChange={(e) => setCategory(e.target.value)}
          aria-label="Category"
          className="h-8 max-w-48 rounded-md border border-input bg-background px-2 text-[13px]"
        >
          <option value="">All categories</option>
          {categories.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </div>
      {loading && toolkits.length === 0 ? (
        <div className="flex justify-center py-8">
          <Spinner className="size-4 text-muted-foreground" />
        </div>
      ) : error && toolkits.length === 0 ? (
        <p className="py-6 text-center text-xs text-muted-foreground">The catalogue could not be read: {error}</p>
      ) : (
        <ul className="flex flex-col gap-1" aria-label="Services">
          {found.slice(0, SHOWN).map((toolkit) => {
            const count = accounts.filter((a) => a.toolkit === toolkit.slug && a.status === "active").length;
            return (
              <li key={toolkit.slug} className="flex items-center gap-3 rounded-lg px-2 py-1.5 hover:bg-accent/50">
                <ServiceTile name={toolkit.name} />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-[13px] font-medium">{toolkit.name}</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {[toolkit.categories.slice(0, 2).join(", "), count ? `${count} connected` : ""].filter(Boolean).join(" · ") ||
                      toolkit.slug}
                  </span>
                </span>
                <Button size="xs" variant={count ? "ghost" : "outline"} onClick={() => onConnect(toolkit)}>
                  {count ? (
                    <>
                      <PlusIcon /> Add account
                    </>
                  ) : (
                    "Connect"
                  )}
                </Button>
              </li>
            );
          })}
          {found.length > SHOWN && (
            <li className="px-2 py-2 text-xs text-muted-foreground">
              {found.length - SHOWN} more: search by name or pick a category.
            </li>
          )}
          {found.length === 0 && <li className="px-2 py-6 text-center text-xs text-muted-foreground">No service matches.</li>}
        </ul>
      )}
    </div>
  );
}

function Connected({ onConnect }: { onConnect(toolkit: Toolkit): void }) {
  const accounts = useConnectedAppsStore((s) => s.accounts);
  const pending = useConnectedAppsStore((s) => s.pending);
  const toolkits = useConnectedAppsStore((s) => s.toolkits);
  const name = useServiceName();
  const [busy, setBusy] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const act = async (id: string, run: () => Promise<void>) => {
    setBusy(id);
    setError(null);
    try {
      await run();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };
  if (accounts.length === 0 && pending.length === 0)
    return <p className="py-8 text-center text-xs text-muted-foreground">No account connected yet: pick a service in the catalogue.</p>;
  const shownAccounts = accounts.filter((a) => a.status !== "pending" || !pending.some((p) => p.account === a.id));
  return (
    <div className="flex flex-col gap-2">
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
      <ul className="flex flex-col gap-1" aria-label="Connected accounts">
        {pending.map((p) => (
          <li key={p.id} className="flex items-center gap-3 rounded-lg px-2 py-1.5">
            <ServiceTile name={name(p.toolkit)} />
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-[13px] font-medium">
                {name(p.toolkit)} <span className="font-normal text-muted-foreground">· {p.label}</span>
              </span>
              <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                <span className="size-1.5 rounded-full bg-amber-500" /> Signing in — finish it in your browser
              </span>
            </span>
            <Button size="xs" variant="outline" disabled={busy === p.id} onClick={() => void act(p.id, () => useConnectedAppsStore.getState().reopen(p.id))}>
              Open again
            </Button>
          </li>
        ))}
        {shownAccounts.map((account) => {
          const toolkit = toolkits.find((t) => t.slug === account.toolkit);
          return (
            <li key={account.id} className="flex items-center gap-3 rounded-lg px-2 py-1.5 hover:bg-accent/50">
              <ServiceTile name={name(account.toolkit)} />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-[13px] font-medium">
                  {name(account.toolkit)} <span className="font-normal text-muted-foreground">· {account.label}</span>
                </span>
                <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <span className={cn("size-1.5 rounded-full", statusTone[account.status])} />
                  {statusText[account.status]}
                  {account.connectedAt && ` · connected ${formatRelative(Date.parse(account.connectedAt))}`}
                </span>
              </span>
              {account.status !== "active" && (
                <Button size="xs" variant="outline" disabled={busy === account.id} onClick={() => void act(account.id, () => useConnectedAppsStore.getState().reconnect(account.id))}>
                  Reconnect
                </Button>
              )}
              {account.status === "active" && toolkit && (
                <Button size="xs" variant="ghost" onClick={() => onConnect(toolkit)}>
                  Add account
                </Button>
              )}
              <Button size="xs" variant="ghost" disabled={busy === account.id} onClick={() => void act(account.id, () => useConnectedAppsStore.getState().disconnect(account.id))}>
                Disconnect
              </Button>
            </li>
          );
        })}
      </ul>
      <p className="text-xs text-muted-foreground">Disconnect revokes that account's access at the service; other accounts stay.</p>
    </div>
  );
}

/** The whole page: the key, then the catalogue and the Connected tab. */
export function ConnectedAppsPage({ className }: { className?: string }) {
  const loaded = useConnectedAppsStore((s) => s.loaded);
  const key = useConnectedAppsStore((s) => s.key);
  const status = useConnectedAppsStore((s) => s.status);
  const reason = useConnectedAppsStore((s) => s.reason);
  const count = useConnectedAppsStore((s) => s.accounts.length + s.pending.length);
  const [connecting, setConnecting] = React.useState<Toolkit | null>(null);
  const [tab, setTab] = React.useState<"catalogue" | "connected">("catalogue");

  React.useEffect(() => {
    void useConnectedAppsStore.getState().load();
  }, []);
  React.useEffect(() => {
    if (key === "saved") void useConnectedAppsStore.getState().loadCatalog();
  }, [key]);

  return (
    <div className={cn("flex flex-col gap-4", className)}>
      <KeyCard />
      {!loaded ? null : key === "saved" ? (
        <>
          {status === "unavailable" && (
            <div className="flex items-center gap-2 rounded-lg bg-amber-500/10 px-3 py-2 text-xs">
              <span className="flex-1">Composio is unavailable{reason ? `: ${reason}` : ""}. Chats keep their other tools.</span>
              <Button size="xs" variant="ghost" onClick={() => void useConnectedAppsStore.getState().retry().catch(() => undefined)}>
                <RefreshCwIcon /> Retry
              </Button>
            </div>
          )}
          <Tabs value={tab} onValueChange={(value) => setTab(value as typeof tab)}>
            <TabsList>
              <TabsTrigger value="catalogue">Catalogue</TabsTrigger>
              <TabsTrigger value="connected">
                Connected{count ? <span className="ml-1 text-muted-foreground tabular-nums">{count}</span> : null}
              </TabsTrigger>
            </TabsList>
            <TabsContent value="catalogue" className="pt-3">
              <Catalogue onConnect={setConnecting} />
            </TabsContent>
            <TabsContent value="connected" className="pt-3">
              <Connected onConnect={setConnecting} />
            </TabsContent>
          </Tabs>
        </>
      ) : (
        <p className="text-xs text-muted-foreground">
          {status === "unavailable" ? "Every service shows as unavailable until Composio can be reached." : "Save a key to see the services you can connect."}
        </p>
      )}
      <ConnectDialog toolkit={connecting} onOpenChange={(open) => !open && setConnecting(null)} />
    </div>
  );
}
