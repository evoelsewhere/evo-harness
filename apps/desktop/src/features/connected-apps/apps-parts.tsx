import * as React from "react";
import { CheckIcon, ChevronRightIcon, ExternalLinkIcon, PlusIcon, RefreshCwIcon, SearchIcon, ZapIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { appsApi, statusText, type Account, type AppTool, type PendingSignIn, type Toolkit } from "@/lib/connected-apps";
import { formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useConnectedAppsStore } from "@/stores/connected-apps-store";
import { OfficialBadge, PreviewBadge, useConnectOfficial, useOfficial, WaysDialog } from "./official-parts";

// The pieces of Connected apps that Customize → Connectors and Evo Mates'
// page are built from (docs/features/connected-apps/spec.md, FR-3 – FR-6,
// FR-16): a service's logo tile, the services you connected, the catalogue,
// a service's page with its accounts and tools, and the Connect dialog.

/** A service's tile: its logo (fetched and cached by the shell), else its initial. */
export function ServiceTile({ slug, name, className }: { slug: string; name: string; className?: string }) {
  const logo = useConnectedAppsStore((s) => s.logos[slug]);
  React.useEffect(() => {
    if (slug !== "zapier") useConnectedAppsStore.getState().wantLogos([slug]);
  }, [slug]);
  // Composio's logo host has no usable Zapier logo: Zapier's own orange and bolt.
  if (slug === "zapier")
    return (
      <span aria-hidden className={cn("flex size-8 shrink-0 items-center justify-center rounded-lg bg-[#ff4f00] text-white", className)}>
        <ZapIcon className="size-1/2 fill-current" />
      </span>
    );
  return (
    <span
      aria-hidden
      className={cn(
        "flex size-8 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-muted text-[13px] font-medium text-muted-foreground dark:bg-accent",
        logo && "bg-white p-[3px] shadow-[0_0_0_1px_rgb(0_0_0/0.08)] dark:bg-white",
        className,
      )}
    >
      {logo ? <img src={logo} alt="" className="size-full object-contain" draggable={false} /> : name.trim().charAt(0).toUpperCase() || "?"}
    </span>
  );
}

export const statusTone: Record<Account["status"], string> = {
  active: "bg-status-ok",
  pending: "bg-amber-500",
  "needs-sign-in": "bg-amber-500",
  expired: "bg-amber-500",
  failed: "bg-destructive",
};

/** The service name for a toolkit slug. */
export function useServiceName(): (slug: string) => string {
  const toolkits = useConnectedAppsStore((s) => s.toolkits);
  return React.useCallback(
    (slug: string) => toolkits.find((t) => t.slug === slug)?.name ?? slug.charAt(0).toUpperCase() + slug.slice(1),
    [toolkits],
  );
}

/** One service you connected: its accounts and sign-ins under way. */
export interface ConnectedService {
  toolkit: string;
  name: string;
  accounts: Account[];
  pending: PendingSignIn[];
  /** Active when any account is; else the state that needs the person. */
  status: Account["status"];
}

/** The services you connected, in the order you connected them. */
export function useConnectedServices(): ConnectedService[] {
  const accounts = useConnectedAppsStore((s) => s.accounts);
  const pending = useConnectedAppsStore((s) => s.pending);
  const name = useServiceName();
  return React.useMemo(() => {
    const order: string[] = [];
    for (const a of accounts) if (!order.includes(a.toolkit)) order.push(a.toolkit);
    for (const p of pending) if (!order.includes(p.toolkit)) order.push(p.toolkit);
    return order.map((toolkit) => {
      const mine = accounts.filter((a) => a.toolkit === toolkit && !pending.some((p) => p.account === a.id));
      const waiting = pending.filter((p) => p.toolkit === toolkit);
      const status: Account["status"] = mine.some((a) => a.status === "active")
        ? "active"
        : waiting.length
          ? "pending"
          : (mine.find((a) => a.status === "failed")?.status ?? mine[0]?.status ?? "pending");
      return { toolkit, name: name(toolkit), accounts: mine, pending: waiting, status };
    });
  }, [accounts, pending, name]);
}

/** "work, personal" or "Signing in…": a service's line under its name. */
export function serviceLine(service: ConnectedService): string {
  if (!service.accounts.length && service.pending.length) return "Signing in…";
  const labels = service.accounts.map((a) => a.label).join(", ");
  const attention = service.accounts.filter((a) => a.status !== "active").length;
  return attention ? `${labels} · ${attention} need${attention === 1 ? "s" : ""} you` : labels;
}

// ── Connect ─────────────────────────────────────────────────────────────────

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
          <div className="flex items-center gap-3">
            <ServiceTile slug={toolkit.slug} name={toolkit.name} className="size-10 rounded-xl" />
            <div className="flex min-w-0 flex-col gap-0.5">
              <DialogTitle>{first ? `Connect ${toolkit.name}` : `Add a ${toolkit.name} account`}</DialogTitle>
              <DialogDescription>The sign-in opens in your browser; Evo never sees your password.</DialogDescription>
            </div>
          </div>
        </DialogHeader>
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <label className="text-xs text-muted-foreground" htmlFor="connected-app-label">
            Label — tells this account apart, such as “work” or “personal”{first ? "" : " (required)"}
          </label>
          <Input id="connected-app-label" value={label} maxLength={32} onChange={(e) => setLabel(e.target.value)} autoFocus />
          {mine.length > 0 && <p className="text-xs text-muted-foreground">Already connected: {mine.map((a) => a.label).join(", ")}</p>}
          {!toolkit.managedAuth && !toolkit.noAuth && (
            <p className="rounded-lg bg-muted/60 px-3 py-2 text-xs text-muted-foreground dark:bg-accent/40">
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
              {busy ? "Opening the browser…" : "Continue in browser"}
              {!busy && <ExternalLinkIcon />}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ── The catalogue ───────────────────────────────────────────────────────────

/** "developer tools" → "Developer tools". */
const ACRONYMS: Record<string, string> = { crm: "CRM", ai: "AI", seo: "SEO", hr: "HR", erp: "ERP", sms: "SMS", it: "IT", api: "API" };
const firstUpper = (text: string) =>
  text
    .split(" ")
    .map((word, i) => ACRONYMS[word.toLowerCase()] ?? (i === 0 ? word.charAt(0).toUpperCase() + word.slice(1) : word))
    .join(" ");

/** Cards shown before "Show more". */
const PAGE = 48;
/** Category chips shown (the most used ones). */
const CHIPS = 10;

/**
 * Every service Composio offers, most used first, as a grid of cards; search
 * by name or category and a row of category chips. A card opens the
 * service when it is connected, else the Connect dialog.
 */
/**
 * `onOpen` gets what to show for a connected service: `svc:<toolkit>` (a
 * Composio service), `off:<id>` (an official server) or `zapier`.
 */
export function AppsCatalogue({ query, onOpen }: { query: string; onOpen?(view: string): void }) {
  const toolkits = useConnectedAppsStore((s) => s.toolkits);
  const loading = useConnectedAppsStore((s) => s.catalogLoading);
  const error = useConnectedAppsStore((s) => s.catalogError);
  const accounts = useConnectedAppsStore((s) => s.accounts);
  const officials = useConnectedAppsStore((s) => s.providers?.official);
  const [category, setCategory] = React.useState("");
  const [shown, setShown] = React.useState(PAGE);
  const [connecting, setConnecting] = React.useState<Toolkit | null>(null);
  const [choosing, setChoosing] = React.useState<Toolkit | null>(null);
  React.useEffect(() => {
    void useConnectedAppsStore.getState().loadCatalog();
  }, []);
  React.useEffect(() => setShown(PAGE), [query, category]);

  const chips = React.useMemo(() => {
    const count = new Map<string, number>();
    for (const t of toolkits) for (const c of t.categories) if (c.toLowerCase() !== "model context protocol") count.set(c, (count.get(c) ?? 0) + 1);
    return [...count.entries()].sort((a, b) => b[1] - a[1]).slice(0, CHIPS).map(([c]) => c);
  }, [toolkits]);
  const q = query.trim().toLowerCase();
  const found = toolkits.filter(
    (t) =>
      (!q || t.name.toLowerCase().includes(q) || t.slug.includes(q) || t.categories.some((c) => c.toLowerCase().includes(q))) &&
      (!category || t.categories.some((c) => c.toLowerCase() === category.toLowerCase())),
  );

  if (loading && !toolkits.length)
    return (
      <div className="flex justify-center py-10">
        <Spinner className="size-4 text-muted-foreground" />
      </div>
    );
  if (error && !toolkits.length) return <p className="py-8 text-center text-xs text-muted-foreground">The catalogue could not be read: {error}</p>;
  return (
    <div className="flex flex-col gap-3">
      <div className="scrollbar-none -mx-1 flex gap-1 overflow-x-auto px-1">
        {["", ...chips].map((c) => (
          <button
            key={c || "all"}
            type="button"
            onClick={() => setCategory(c)}
            className={cn(
              "h-7 shrink-0 rounded-full px-3 text-xs transition-colors",
              category === c ? "bg-foreground text-background" : "bg-muted text-muted-foreground hover:text-foreground dark:bg-accent",
            )}
          >
            {c ? firstUpper(c.toLowerCase()) : "All"}
          </button>
        ))}
      </div>
      {found.length === 0 ? (
        <p className="py-8 text-center text-xs text-muted-foreground">No service matches{q ? ` “${query.trim()}”` : ""}.</p>
      ) : (
        <ul className="grid grid-cols-[repeat(auto-fill,minmax(210px,1fr))] gap-2" aria-label="Services">
          {found.slice(0, shown).map((toolkit) => {
            const count = accounts.filter((a) => a.toolkit === toolkit.slug && a.status === "active").length;
            const official = toolkit.official ? officials?.find((o) => o.id === toolkit.official) : undefined;
            const direct = Boolean(official?.connected);
            const open = () => {
              if (direct && onOpen) return onOpen(`off:${official!.id}`);
              if (count && onOpen) return onOpen(`svc:${toolkit.slug}`);
              if (toolkit.official) return setChoosing(toolkit);
              setConnecting(toolkit);
            };
            return (
              <li key={toolkit.slug}>
                <button
                  type="button"
                  onClick={open}
                  className="group flex h-full w-full items-center gap-3 rounded-xl bg-card px-3 py-2.5 text-left shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent/50"
                >
                  <ServiceTile slug={toolkit.slug} name={toolkit.name} className="size-9 rounded-[10px]" />
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="flex min-w-0 items-center gap-1.5">
                      <span className="truncate text-[13px] font-medium">{toolkit.name}</span>
                      {toolkit.official && <OfficialBadge className="h-4 px-1 text-[10px]" />}
                      {official?.preview && <PreviewBadge />}
                    </span>
                    <span className="truncate text-xs text-muted-foreground">
                      {direct ? (
                        <span className="inline-flex items-center gap-1 text-foreground/80">
                          <CheckIcon className="size-3 text-status-ok" />
                          Connected directly
                        </span>
                      ) : count ? (
                        <span className="inline-flex items-center gap-1 text-foreground/80">
                          <CheckIcon className="size-3 text-status-ok" />
                          {count === 1 ? "Connected" : `${count} accounts`}
                        </span>
                      ) : (
                        [firstUpper(toolkit.categories[0] ?? ""), toolkit.toolsCount ? `${toolkit.toolsCount} tools` : ""].filter(Boolean).join(" · ")
                      )}
                    </span>
                  </span>
                  {!count && !direct && <PlusIcon className="size-4 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {found.length > shown && (
        <Button variant="ghost" size="sm" className="self-center" onClick={() => setShown((n) => n + PAGE * 2)}>
          Show more ({found.length - shown})
        </Button>
      )}
      <ConnectDialog toolkit={connecting} onOpenChange={(open) => !open && setConnecting(null)} />
      <WaysDialog toolkit={choosing} onOpenChange={(open) => !open && setChoosing(null)} />
    </div>
  );
}

/** A search box for the catalogue where the page has none of its own. */
export function CatalogueSearch({ value, onChange }: { value: string; onChange(value: string): void }) {
  return (
    <div className="relative">
      <SearchIcon className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <Input value={value} onChange={(e) => onChange(e.target.value)} placeholder="Search 2,000+ apps" aria-label="Search apps" className="h-8 pl-8 text-[13px]" />
    </div>
  );
}

// ── One service ─────────────────────────────────────────────────────────────

function Tools({ toolkit }: { toolkit: string }) {
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
  if (error) return <p className="px-3.5 py-3 text-xs text-muted-foreground">The tools could not be read: {error}</p>;
  if (!tools)
    return (
      <div className="flex justify-center py-4">
        <Spinner className="size-4 text-muted-foreground" />
      </div>
    );
  return (
    <ul className="divide-y divide-border/60">
      {tools.map((tool) => (
        <li key={tool.slug} className="flex items-start gap-3 px-3.5 py-2">
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-[13px] font-medium">{tool.name}</span>
            {tool.description && <span className="line-clamp-2 text-xs text-muted-foreground">{tool.description}</span>}
          </span>
          <span
            className={cn(
              "mt-0.5 shrink-0 rounded-md px-1.5 py-0.5 text-[11px]",
              tool.read ? "bg-muted text-muted-foreground dark:bg-accent" : "bg-amber-500/12 text-amber-700 dark:text-amber-400",
            )}
          >
            {tool.read ? "Reads" : "Asks first"}
          </span>
        </li>
      ))}
    </ul>
  );
}

/** A connected service: its accounts (Reconnect, Disconnect, Add account) and its tools. */
export function ServiceDetail({ toolkit }: { toolkit: string }) {
  const services = useConnectedServices();
  const toolkits = useConnectedAppsStore((s) => s.toolkits);
  const name = useServiceName();
  const service = services.find((s) => s.toolkit === toolkit);
  const info = toolkits.find((t) => t.slug === toolkit);
  const [adding, setAdding] = React.useState<Toolkit | null>(null);
  const [busy, setBusy] = React.useState<string | null>(null);
  const [confirm, setConfirm] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [toolsOpen, setToolsOpen] = React.useState(false);
  const official = useOfficial(info?.official);
  const direct = useConnectOfficial();
  const act = async (id: string, run: () => Promise<void>) => {
    setBusy(id);
    setError(null);
    try {
      await run();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
      setConfirm(null);
    }
  };
  const title = info?.name ?? name(toolkit);
  const addAccount = () =>
    setAdding(info ?? { slug: toolkit, name: title, logo: "", categories: [], managedAuth: true, noAuth: false, description: "", toolsCount: 0, composio: true });

  return (
    <div className="flex flex-col gap-5">
      <header className="flex items-start gap-3">
        <ServiceTile slug={toolkit} name={title} className="size-11 rounded-xl" />
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <h2 className="truncate text-base font-semibold">{title}</h2>
          {info?.description && <p className="line-clamp-2 text-[13px] text-muted-foreground">{info.description}</p>}
        </div>
        <Button size="sm" variant="outline" onClick={addAccount}>
          <PlusIcon />
          {service?.accounts.length ? "Add account" : "Connect"}
        </Button>
      </header>

      {official && !official.connected && (
        <div className="flex items-center gap-3 rounded-xl bg-brand/6 px-3.5 py-2.5 shadow-[0_0_0_1px_var(--brand)]">
          <OfficialBadge />
          <p className="min-w-0 flex-1 text-xs text-muted-foreground">
            {title} also has its own MCP server: data goes only to {title}, and Evo adds no cost.
          </p>
          <Button size="xs" disabled={direct.busy} onClick={() => void direct.connect(official)}>
            Connect directly
          </Button>
          {direct.dialog}
        </div>
      )}
      <section className="flex flex-col gap-2">
        <h3 className="text-xs font-medium text-muted-foreground">Accounts through Composio</h3>
        {error && (
          <p role="alert" className="text-xs text-destructive">
            {error}
          </p>
        )}
        <ul className="divide-y divide-border/60 overflow-hidden rounded-xl shadow-[0_0_0_1px_var(--border)]">
          {service?.pending.map((p) => (
            <li key={p.id} className="flex items-center gap-3 px-3.5 py-2.5">
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-[13px] font-medium">{p.label}</span>
                <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <span className="size-1.5 animate-pulse rounded-full bg-amber-500" />
                  Signing in — finish it in your browser
                </span>
              </span>
              <Button size="xs" variant="outline" disabled={busy === p.id} onClick={() => void act(p.id, () => useConnectedAppsStore.getState().reopen(p.id))}>
                Open again
              </Button>
            </li>
          ))}
          {service?.accounts.map((account) => (
            <li key={account.id} className="flex items-center gap-3 px-3.5 py-2.5">
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-[13px] font-medium">{account.label}</span>
                <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <span className={cn("size-1.5 rounded-full", statusTone[account.status])} />
                  {statusText[account.status]}
                  {account.connectedAt && ` · connected ${formatRelative(Date.parse(account.connectedAt))}`}
                </span>
              </span>
              {account.status !== "active" && (
                <Button size="xs" disabled={busy === account.id} onClick={() => void act(account.id, () => useConnectedAppsStore.getState().reconnect(account.id))}>
                  Reconnect
                </Button>
              )}
              {confirm === account.id ? (
                <Button size="xs" variant="destructive" disabled={busy === account.id} onBlur={() => setConfirm(null)} onClick={() => void act(account.id, () => useConnectedAppsStore.getState().disconnect(account.id))}>
                  Disconnect {account.label}
                </Button>
              ) : (
                <Button size="xs" variant="ghost" className="text-muted-foreground" onClick={() => setConfirm(account.id)}>
                  Disconnect
                </Button>
              )}
            </li>
          ))}
          {!service && <li className="px-3.5 py-3 text-xs text-muted-foreground">No account yet.</li>}
        </ul>
        <p className="text-xs text-muted-foreground">
          Disconnect revokes that account's access at {title}; other accounts stay. Mates on “Only these” use {title} once you
          allow it in their Apps tab.
        </p>
      </section>

      <section className="flex flex-col gap-2">
        <button type="button" onClick={() => setToolsOpen((o) => !o)} className="flex items-center gap-1 text-xs font-medium text-muted-foreground hover:text-foreground">
          <ChevronRightIcon className={cn("size-3.5 transition-transform", toolsOpen && "rotate-90")} />
          Tools{info?.toolsCount ? ` (${info.toolsCount})` : ""} — those that send, post or delete always ask first
        </button>
        {toolsOpen && (
          <div className="max-h-96 overflow-y-auto rounded-xl shadow-[0_0_0_1px_var(--border)]">
            <Tools toolkit={toolkit} />
          </div>
        )}
      </section>
      <ConnectDialog toolkit={adding} onOpenChange={(open) => !open && setAdding(null)} />
    </div>
  );
}

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
