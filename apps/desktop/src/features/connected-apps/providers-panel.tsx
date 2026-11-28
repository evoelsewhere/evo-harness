import * as React from "react";
import { ChevronRightIcon, ExternalLinkIcon } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { APP_SERVER_KEY, appsApi } from "@/lib/connected-apps";
import { mcp } from "@/lib/mcp";
import { cn } from "@/lib/utils";
import { useConnectedAppsStore } from "@/stores/connected-apps-store";
import { useConnectorsStore } from "@/stores/connectors-store";
import { ServiceTile } from "./apps-parts";
import { problemOf, TokenDialog, useServer } from "./official-parts";

// Providers (docs/features/connected-apps/spec.md, FR-1, FR-2, FR-17, FR-24):
// the services that bring apps to Evo — Composio with the person's project
// key, Zapier with their Zapier account — each one card of the same shape: its
// state, one main action, what passes through it, and a readable problem.
// Official servers need no provider. Advanced holds one switch: Composio's
// sandbox for chats.

const open = (url: string) => void openUrl(url).catch(() => undefined);
const KEYS_PAGE = "https://platform.composio.dev/";
const SANDBOX = ["COMPOSIO_REMOTE_BASH_TOOL", "COMPOSIO_REMOTE_WORKBENCH"];

type Tone = "ok" | "warn" | "bad" | "off";
const tone: Record<Tone, string> = {
  ok: "bg-status-ok/12 text-status-ok",
  warn: "bg-amber-500/12 text-amber-700 dark:text-amber-400",
  bad: "bg-destructive/10 text-destructive",
  off: "bg-muted text-muted-foreground dark:bg-accent",
};

function Pill({ kind, children }: { kind: Tone; children: React.ReactNode }) {
  return <span className={cn("inline-flex h-5 shrink-0 items-center rounded-full px-2 text-[11px] font-medium", tone[kind])}>{children}</span>;
}

/** One provider: tile, name, state, actions; then what it does, a problem, and its own body. */
function ProviderCard({
  slug,
  name,
  state,
  actions,
  summary,
  problem,
  children,
}: {
  slug: string;
  name: string;
  state: React.ReactNode;
  actions?: React.ReactNode;
  summary: React.ReactNode;
  problem?: React.ReactNode;
  children?: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-3 rounded-2xl bg-card p-4 shadow-[0_0_0_1px_var(--border)]">
      <div className="flex items-center gap-3">
        <ServiceTile slug={slug} name={name} className="size-10 rounded-xl" />
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <h3 className="text-[14px] font-semibold">{name}</h3>
          {state}
        </div>
        {actions && <div className="flex shrink-0 items-center gap-1">{actions}</div>}
      </div>
      <p className="text-xs leading-relaxed text-muted-foreground">{summary}</p>
      {problem}
      {children}
    </section>
  );
}

function Problem({ text, raw, action }: { text: string; raw?: string | null; action?: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5 rounded-xl bg-amber-500/8 px-3 py-2.5 text-xs shadow-[0_0_0_1px_rgb(245_158_11/0.25)]">
      <div className="flex items-center gap-2">
        <p className="flex-1 font-medium text-foreground">{text}</p>
        {action}
      </div>
      {raw && (
        <details className="text-muted-foreground">
          <summary className="cursor-default select-none hover:text-foreground">Details</summary>
          <p className="mt-1 font-mono break-words">{raw}</p>
        </details>
      )}
    </div>
  );
}

function ComposioCard() {
  const key = useConnectedAppsStore((s) => s.key);
  const status = useConnectedAppsStore((s) => s.status);
  const reason = useConnectedAppsStore((s) => s.reason);
  const accounts = useConnectedAppsStore((s) => s.accounts);
  const [editing, setEditing] = React.useState(false);
  const [value, setValue] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [confirm, setConfirm] = React.useState(false);
  const saved = key === "saved";
  const showForm = !saved || editing;
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await useConnectedAppsStore.getState().setKey(value);
      setValue("");
      setEditing(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const state = !saved ? (
    key === "invalid" ? <Pill kind="bad">Key refused</Pill> : <Pill kind="off">Not set up</Pill>
  ) : status === "unavailable" ? (
    <Pill kind="warn">Unavailable</Pill>
  ) : (
    <Pill kind="ok">Connected</Pill>
  );
  const services = new Set(accounts.map((a) => a.toolkit)).size;
  return (
    <ProviderCard
      slug="composio"
      name="Composio"
      state={state}
      actions={
        saved && !editing ? (
          <>
            <Button size="sm" variant="ghost" onClick={() => setEditing(true)}>
              Replace key
            </Button>
            {confirm ? (
              <Button size="sm" variant="destructive" onBlur={() => setConfirm(false)} onClick={() => void useConnectedAppsStore.getState().removeKey().catch((e) => setError(String(e)))}>
                Remove key
              </Button>
            ) : (
              <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => setConfirm(true)}>
                Remove
              </Button>
            )}
          </>
        ) : null
      }
      summary={
        <>
          2,000+ apps, each with several labelled accounts, with your own Composio project key — kept in the system keychain.
          Composio keeps the services' sign-ins, and every tool call and its result go through Composio; Evo never sees a
          service's password.
          {saved && services > 0 && ` ${services} service${services === 1 ? "" : "s"} connected.`}
        </>
      }
      problem={
        status === "unavailable" && saved ? (
          <Problem
            text="Composio does not answer right now. Chats keep their other tools."
            raw={reason}
            action={
              <Button size="xs" variant="outline" onClick={() => void useConnectedAppsStore.getState().retry().catch(() => undefined)}>
                Try again
              </Button>
            }
          />
        ) : null
      }
    >
      {showForm && (
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (value.trim()) void save();
          }}
        >
          <div className="flex gap-2">
            <Input
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder="Project key, ak_…"
              aria-label="Composio project key"
              value={value}
              onChange={(e) => setValue(e.target.value)}
              className="h-8 min-w-0 flex-1 font-mono text-xs"
            />
            <Button size="sm" type="submit" disabled={busy || !value.trim()}>
              {busy ? "Checking…" : "Save"}
            </Button>
            {editing && (
              <Button size="sm" variant="ghost" type="button" onClick={() => setEditing(false)}>
                Cancel
              </Button>
            )}
          </div>
          <button type="button" onClick={() => open(KEYS_PAGE)} className="flex w-fit items-center gap-1 text-xs text-brand hover:underline">
            Get a project key: Composio → your project → Settings → API keys <ExternalLinkIcon className="size-3" />
          </button>
        </form>
      )}
      {(error || (key === "invalid" && reason)) && (
        <p role="alert" className="text-xs text-destructive">
          {error ?? reason}
        </p>
      )}
    </ProviderCard>
  );
}

function ZapierProvider({ onOpen }: { onOpen?(): void }) {
  const zapier = useConnectedAppsStore((s) => s.providers?.zapier);
  const server = useServer(zapier?.key);
  const [tokenOpen, setTokenOpen] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [confirm, setConfirm] = React.useState(false);
  if (!zapier) return null;
  const refresh = () => {
    void useConnectedAppsStore.getState().load();
    void useConnectorsStore.getState().load();
  };
  const signIn = async () => {
    setBusy(true);
    setError(null);
    try {
      await appsApi.connectZapier();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      refresh();
    }
  };
  const problem = zapier.connected ? problemOf(server, "Zapier") : null;
  const working = zapier.connected && server?.status === "connected";
  const state = !zapier.connected ? (
    <Pill kind="off">Not set up</Pill>
  ) : working ? (
    <Pill kind="ok">Connected · {server!.tools.length} actions</Pill>
  ) : server?.status === "connecting" ? (
    <Pill kind="warn">Connecting…</Pill>
  ) : (
    <Pill kind="warn">{problem?.endsWith("sign in.") ? "Needs sign-in" : "Not working"}</Pill>
  );
  return (
    <ProviderCard
      slug="zapier"
      name="Zapier"
      state={state}
      actions={
        !zapier.connected ? (
          <Button size="sm" disabled={busy} onClick={() => void signIn()}>
            {busy ? "Signing in…" : "Connect"}
          </Button>
        ) : (
          <>
            {working ? (
              <Button size="sm" variant="outline" onClick={() => open(zapier.actions)}>
                Choose actions <ExternalLinkIcon />
              </Button>
            ) : null}
            {onOpen && working && (
              <Button size="sm" variant="ghost" onClick={onOpen}>
                Open
              </Button>
            )}
            {confirm ? (
              <Button
                size="sm"
                variant="destructive"
                onBlur={() => setConfirm(false)}
                onClick={async () => {
                  await appsApi.disconnectZapier().catch(() => undefined);
                  refresh();
                }}
              >
                Disconnect
              </Button>
            ) : (
              <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => setConfirm(true)}>
                Disconnect
              </Button>
            )}
          </>
        )
      }
      summary={
        <>
          Thousands of apps through your Zapier account: its tools are the actions you enable for your server on Zapier. Each
          successful call uses {zapier.tasksPerCall} tasks of your Zapier plan, at Zapier's rate; failed calls use none.
        </>
      }
      problem={
        problem ? (
          <Problem
            text={problem}
            raw={server?.error}
            action={
              <Button size="xs" disabled={busy} onClick={() => void signIn()}>
                {problem.endsWith("sign in.") ? (busy ? "Signing in…" : "Sign in") : "Try again"}
              </Button>
            }
          />
        ) : null
      }
    >
      {!working && (
        <button type="button" onClick={() => setTokenOpen(true)} className="w-fit text-xs text-muted-foreground hover:text-foreground">
          Or use a connection token from mcp.zapier.com
        </button>
      )}
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
      <TokenDialog
        title="Connect Zapier with a token"
        hint="A connection token from mcp.zapier.com → your server → Connect."
        docs={zapier.docs}
        open={tokenOpen}
        onOpenChange={setTokenOpen}
        onSubmit={async (token) => {
          await appsApi.connectZapier(token);
          refresh();
        }}
      />
    </ProviderCard>
  );
}

/** Advanced: whether chats may use Composio's sandbox (its remote bash and workbench). */
function Advanced() {
  const server = useServer(APP_SERVER_KEY);
  const [open, setOpen] = React.useState(false);
  if (!server) return null;
  const sandbox = server.tools.filter((t) => SANDBOX.includes(t.name));
  const on = sandbox.length > 0 && sandbox.every((t) => t.permission !== "block");
  return (
    <section className="flex flex-col gap-2">
      <button type="button" onClick={() => setOpen((o) => !o)} className="flex w-fit items-center gap-1 text-xs font-medium text-muted-foreground hover:text-foreground">
        <ChevronRightIcon className={cn("size-3.5 transition-transform", open && "rotate-90")} />
        Advanced
      </button>
      {open && (
        <div className="flex items-start gap-3 rounded-2xl bg-card p-4 shadow-[0_0_0_1px_var(--border)]">
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <p className="text-[13px] font-medium">Let chats run code at Composio</p>
            <p className="text-xs text-muted-foreground">
              Composio's remote workbench and bash run code with every connected account, so they are off by default. When on,
              each run asks first; mates limited to some apps never get them.
            </p>
          </div>
          <Switch
            aria-label="Let chats run code at Composio"
            checked={on}
            disabled={sandbox.length === 0}
            onCheckedChange={(next) => {
              for (const tool of SANDBOX) void mcp.setPermission(APP_SERVER_KEY, tool, next ? "ask" : "block").catch(() => undefined);
            }}
          />
        </div>
      )}
    </section>
  );
}

/** The providers that bring apps to Evo, as one page. */
export function ProvidersPanel({ onOpenZapier, className }: { onOpenZapier?(): void; className?: string }) {
  React.useEffect(() => {
    void useConnectedAppsStore.getState().load();
  }, []);
  return (
    <div className={cn("flex flex-col gap-4", className)}>
      <header className="flex flex-col gap-1">
        <h2 className="text-base font-semibold">Providers</h2>
        <p className="text-[13px] text-muted-foreground">
          Providers bring many apps to Evo with one sign-in of yours. Services with their own official server — GitHub, Notion,
          Linear and others — need no provider: connect them from Browse apps.
        </p>
      </header>
      <ComposioCard />
      <ZapierProvider onOpen={onOpenZapier} />
      <Advanced />
    </div>
  );
}
